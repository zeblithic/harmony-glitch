pub mod types;

pub use types::{ActiveBuff, BuffEffect, BuffSpec};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-player container for all active buffs.
/// Keyed by `kind` — same-kind apply overwrites (refresh semantics).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuffState {
    pub active: HashMap<String, ActiveBuff>,
}

impl BuffState {
    /// Apply a buff. If a buff with the same `kind` is already active,
    /// it is replaced in place (refresh semantics).
    ///
    /// Non-finite inputs are a no-op (logged once) so release behavior is
    /// deterministic — the caller never ends up with a NaN/∞ `expires_at`.
    pub fn apply(&mut self, spec: &BuffSpec, game_time: f64, source: String) {
        if !spec.duration_secs.is_finite() || !game_time.is_finite() {
            debug_assert!(
                false,
                "buff apply requires finite game_time and duration_secs"
            );
            eprintln!(
                "[buff] apply skipped: non-finite input (duration_secs={}, game_time={}, kind={})",
                spec.duration_secs, game_time, spec.kind
            );
            return;
        }
        let active = ActiveBuff {
            kind: spec.kind.clone(),
            effect: spec.effect.clone(),
            expires_at: game_time + spec.duration_secs,
            source,
            on_expire: spec.on_expire.clone(),
        };
        self.active.insert(spec.kind.clone(), active);
    }

    /// Rebase `expires_at` to save-relative form (remaining seconds until
    /// expiry at `current_time`). The returned `BuffState` should be placed
    /// into `SaveState.buffs` — its `expires_at` values are NOT valid
    /// against any live game_time clock until `from_save_form` reverses
    /// the transform.
    ///
    /// Why: `game_time` is not persisted in `SaveState`, and a new session
    /// starts with `game_time = 0.0`. Without this rebase, a buff saved at
    /// `game_time = 3600` with `expires_at = 4200` would live 4200 more
    /// seconds after restart instead of the intended 600.
    pub fn to_save_form(&self, current_time: f64) -> Self {
        let active = self
            .active
            .iter()
            .map(|(k, b)| {
                let mut b = b.clone();
                b.expires_at -= current_time;
                (k.clone(), b)
            })
            .collect();
        Self { active }
    }

    /// Inverse of `to_save_form`: shifts `expires_at` from save-relative
    /// (remaining seconds) back to absolute against `current_time`.
    pub fn from_save_form(save: &Self, current_time: f64) -> Self {
        let active = save
            .active
            .iter()
            .map(|(k, b)| {
                let mut b = b.clone();
                b.expires_at += current_time;
                (k.clone(), b)
            })
            .collect();
        Self { active }
    }

    /// Fold all active `MoodDecayMultiplier` effects multiplicatively.
    /// Returns `1.0` when no relevant buffs are active. Future `BuffEffect`
    /// variants are ignored by the `filter_map` — no refactor needed when
    /// adding e.g. `EnergyDecayMultiplier`.
    // Clippy suggests converting to `.map(...)` because there's currently only
    // one BuffEffect variant. Keep filter_map: when new variants are added,
    // they should be silently skipped here, not force this function to change.
    #[allow(clippy::unnecessary_filter_map)]
    pub fn mood_decay_multiplier(&self) -> f64 {
        self.active
            .values()
            .filter_map(|b| match &b.effect {
                BuffEffect::MoodDecayMultiplier { value } => Some(*value),
                // Future variants that don't affect mood decay should be
                // silently skipped by this function. Add them to whichever
                // subsystem cares (e.g., an energy_decay_multiplier helper
                // parallel to this one).
                #[allow(unreachable_patterns)]
                _ => None,
            })
            .fold(1.0, |acc, v| acc * v)
    }

    /// Remove buffs whose `expires_at <= game_time`. For each expired buff
    /// with `on_expire: Some(spec)`, immediately apply the successor.
    /// Bounded to 8 expansion passes to defend against degenerate chains.
    pub fn tick(&mut self, game_time: f64) {
        // Guard against zero-duration on_expire chains that would otherwise
        // loop forever. Each pass handles one level of chain depth; 8 is
        // comfortable for the deepest chains we expect in content data
        // (typical ramp-downs are 2-3 levels).
        const MAX_PASSES: usize = 8;
        for _ in 0..MAX_PASSES {
            // Collect expired kinds in sorted order for determinism.
            let mut expired_kinds: Vec<String> = self
                .active
                .iter()
                .filter(|(_, b)| !b.expires_at.is_finite() || b.expires_at <= game_time)
                .map(|(k, _)| k.clone())
                .collect();
            expired_kinds.sort();
            if expired_kinds.is_empty() {
                return;
            }
            // Two-phase: remove all expired entries FIRST, then apply their
            // on_expire successors. If we interleaved these, an on_expire
            // successor whose `kind` matched a not-yet-processed entry in
            // `expired_kinds` would be overwritten or (worse) removed by the
            // stale list on its own key. Collecting first decouples removal
            // from successor-application, so a freshly applied successor is
            // never a candidate for removal in the same pass.
            let expired: Vec<ActiveBuff> = expired_kinds
                .into_iter()
                .filter_map(|kind| self.active.remove(&kind))
                .collect();
            for buff in expired {
                if let Some(spec) = buff.on_expire {
                    self.apply(&spec, game_time, "on_expire".to_string());
                }
            }
        }
    }
}

/// Apply an item's buff effect (if any) to the player's BuffState.
/// Uses the item's `id` as the buff source for HUD attribution.
pub fn apply_item_buff(buffs: &mut BuffState, item_def: &crate::item::types::ItemDef, game_time: f64) {
    if let Some(spec) = &item_def.buff_effect {
        buffs.apply(spec, game_time, item_def.id.clone());
    }
}

/// Per-buff data shape sent to the frontend each tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuffFrame {
    pub kind: String,
    pub icon: String,
    pub label: String,
    pub remaining_secs: f64,
}

/// Build the list of BuffFrames for the IPC game-state payload.
/// `item_defs` is used to resolve display name and icon.
///
/// Lookup order: first by `source` (set to item id for item-applied buffs),
/// then by `kind` (covers `on_expire` successors whose source is the literal
/// `"on_expire"` string but whose `kind` often matches the originating item).
/// Falls back to the raw `kind` string for both icon and label when neither
/// lookup hits — so entirely system-sourced buffs still render something
/// visible instead of disappearing.
///
/// Result is sorted by kind for stable UI rendering.
pub fn build_buff_frames(
    buffs: &BuffState,
    item_defs: &crate::item::types::ItemDefs,
    game_time: f64,
) -> Vec<BuffFrame> {
    let mut frames: Vec<BuffFrame> = buffs
        .active
        .values()
        .map(|b| {
            let (icon, label) = item_defs
                .get(&b.source)
                .or_else(|| item_defs.get(&b.kind))
                .map(|d| (d.icon.clone(), d.name.clone()))
                .unwrap_or_else(|| (b.kind.clone(), b.kind.clone()));
            BuffFrame {
                kind: b.kind.clone(),
                icon,
                label,
                remaining_secs: (b.expires_at - game_time).max(0.0),
            }
        })
        .collect();
    frames.sort_by(|a, b| a.kind.cmp(&b.kind));
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rookswort_spec(value: f64, duration: f64) -> BuffSpec {
        BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value },
            duration_secs: duration,
            on_expire: None,
        }
    }

    #[test]
    fn apply_inserts_new_buff() {
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 100.0, "rookswort".into());
        assert_eq!(s.active.len(), 1);
        let b = s.active.get("rookswort").unwrap();
        assert!((b.expires_at - 700.0).abs() < 1e-9);
        assert_eq!(b.source, "rookswort");
    }

    #[test]
    fn apply_same_kind_refreshes_expires_at() {
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 100.0, "rookswort".into());
        // 5 minutes later, re-apply
        s.apply(&rookswort_spec(0.5, 600.0), 400.0, "rookswort".into());
        assert_eq!(s.active.len(), 1, "still one buff, not two");
        assert!((s.active["rookswort"].expires_at - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn apply_same_kind_replaces_effect_magnitude() {
        // Tier-upgrade pattern: content layer applies a stronger buff with same kind.
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 0.0, "rookswort".into());
        s.apply(&rookswort_spec(0.25, 600.0), 0.0, "rookswort_tier2".into());
        let b = s.active.get("rookswort").unwrap();
        assert_eq!(b.effect, BuffEffect::MoodDecayMultiplier { value: 0.25 });
        assert_eq!(b.source, "rookswort_tier2");
    }

    #[test]
    fn mood_decay_multiplier_returns_one_when_empty() {
        let s = BuffState::default();
        assert_eq!(s.mood_decay_multiplier(), 1.0);
    }

    #[test]
    fn mood_decay_multiplier_single_buff_returns_its_value() {
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 0.0, "rookswort".into());
        assert!((s.mood_decay_multiplier() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn mood_decay_multiplier_composes_multiplicatively() {
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 0.0, "rookswort".into());
        // A second buff of a different kind — simulate a future campfire warmth.
        let other = BuffSpec {
            kind: "campfire".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.75 },
            duration_secs: 60.0,
            on_expire: None,
        };
        s.apply(&other, 0.0, "campfire".into());
        // 0.5 * 0.75 = 0.375
        assert!((s.mood_decay_multiplier() - 0.375).abs() < 1e-9);
    }

    #[test]
    fn tick_removes_expired_buff_without_on_expire() {
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 0.0, "rookswort".into());
        s.tick(601.0); // past expires_at
        assert!(s.active.is_empty());
    }

    #[test]
    fn tick_does_not_remove_unexpired_buff() {
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 0.0, "rookswort".into());
        s.tick(300.0);
        assert_eq!(s.active.len(), 1);
    }

    #[test]
    fn tick_expired_buff_with_on_expire_applies_successor() {
        let ramp_down = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.75 },
            duration_secs: 180.0,
            on_expire: None,
        };
        let initial = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            duration_secs: 600.0,
            on_expire: Some(Box::new(ramp_down)),
        };
        let mut s = BuffState::default();
        s.apply(&initial, 0.0, "rookswort".into());
        s.tick(601.0);
        // Successor replaces in place (same kind).
        let b = s.active.get("rookswort").expect("successor present");
        assert_eq!(b.effect, BuffEffect::MoodDecayMultiplier { value: 0.75 });
        assert!((b.expires_at - 781.0).abs() < 1e-9);
        assert_eq!(b.source, "on_expire");
    }

    #[test]
    fn tick_chain_terminates_after_bounded_passes() {
        // Degenerate chain: every on_expire has duration 0, so successor
        // is immediately expired. Without a pass bound, infinite loop.
        fn zero_duration(next: Option<Box<BuffSpec>>) -> BuffSpec {
            BuffSpec {
                kind: "loop".into(),
                effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
                duration_secs: 0.0,
                on_expire: next,
            }
        }
        // Build 20 levels of zero-duration chain
        let mut chain = zero_duration(None);
        for _ in 0..20 {
            chain = zero_duration(Some(Box::new(chain)));
        }
        let mut s = BuffState::default();
        s.apply(&chain, 0.0, "test".into());
        // Should not hang. Bound is internal (8 passes).
        s.tick(0.0);
        // Final state: either empty or one buff — but NOT an infinite loop.
        assert!(s.active.len() <= 1);
    }

    #[test]
    fn tick_does_nothing_when_empty() {
        let mut s = BuffState::default();
        s.tick(1000.0);
        assert!(s.active.is_empty());
    }

    #[test]
    fn apply_item_buff_applies_when_item_has_buff_effect() {
        use crate::item::types::ItemDef;
        let rookswort = ItemDef {
            id: "rookswort".into(),
            name: "Rookswort".into(),
            description: "test".into(),
            category: "food".into(),
            stack_limit: 50,
            icon: "rookswort".into(),
            base_cost: None,
            energy_value: None,
            mood_value: None,
            buff_effect: Some(rookswort_spec(0.5, 600.0)),
        };
        let mut s = BuffState::default();
        apply_item_buff(&mut s, &rookswort, 100.0);
        assert_eq!(s.active.len(), 1);
        assert_eq!(s.active["rookswort"].source, "rookswort");
    }

    #[test]
    fn apply_item_buff_noop_when_item_has_no_buff_effect() {
        use crate::item::types::ItemDef;
        let cherry = ItemDef {
            id: "cherry".into(),
            name: "Cherry".into(),
            description: "test".into(),
            category: "food".into(),
            stack_limit: 50,
            icon: "cherry".into(),
            base_cost: None,
            energy_value: Some(10),
            mood_value: None,
            buff_effect: None,
        };
        let mut s = BuffState::default();
        apply_item_buff(&mut s, &cherry, 100.0);
        assert!(s.active.is_empty());
    }

    #[test]
    fn tick_treats_nan_expires_at_as_expired() {
        // Defense-in-depth: if some bug produced a NaN expires_at, the
        // buff should be removed on next tick rather than becoming immortal.
        let mut s = BuffState::default();
        s.active.insert(
            "bad".into(),
            ActiveBuff {
                kind: "bad".into(),
                effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
                expires_at: f64::NAN,
                source: "test".into(),
                on_expire: None,
            },
        );
        s.tick(1000.0);
        assert!(s.active.is_empty());
    }

    #[test]
    fn build_buff_frames_uses_item_name_and_icon_from_catalog() {
        use crate::item::types::{ItemDef, ItemDefs};
        let mut item_defs: ItemDefs = Default::default();
        item_defs.insert(
            "rookswort".into(),
            ItemDef {
                id: "rookswort".into(),
                name: "Rookswort".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 50,
                icon: "rookswort_icon".into(),
                base_cost: None,
                energy_value: None,
                mood_value: None,
                buff_effect: None,
            },
        );
        let mut buffs = BuffState::default();
        buffs.apply(&rookswort_spec(0.5, 600.0), 0.0, "rookswort".into());
        let frames = build_buff_frames(&buffs, &item_defs, 100.0);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].label, "Rookswort");
        assert_eq!(frames[0].icon, "rookswort_icon");
        assert!((frames[0].remaining_secs - 500.0).abs() < 1e-9);
    }

    #[test]
    fn tick_does_not_remove_successor_whose_kind_matches_another_expiring_kind() {
        // Regression: buff "alpha" expires and its on_expire applies a
        // successor with kind "beta". Another buff "beta" was ALSO in the
        // expired list for this pass. The old implementation removed "beta"
        // by kind after alpha's successor had already taken its place,
        // wiping the successor out.
        let alphas_successor = BuffSpec {
            kind: "beta".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.25 },
            duration_secs: 300.0,
            on_expire: None,
        };
        let alpha = BuffSpec {
            kind: "alpha".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            duration_secs: 100.0,
            on_expire: Some(Box::new(alphas_successor)),
        };
        let beta = BuffSpec {
            kind: "beta".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.75 },
            duration_secs: 100.0,
            on_expire: None,
        };
        let mut s = BuffState::default();
        s.apply(&alpha, 0.0, "alpha".into());
        s.apply(&beta, 0.0, "beta".into());
        // Both expire at t=100. Tick at t=101 expires both.
        s.tick(101.0);
        // The successor with kind=beta, value=0.25, expiring at t=401
        // must be present — it was freshly applied at t=101 during alpha's
        // expiry and should NOT be swept away by the stale expired_kinds list.
        let survivor = s.active.get("beta").expect("successor must survive tick");
        assert_eq!(survivor.effect, BuffEffect::MoodDecayMultiplier { value: 0.25 });
        assert!((survivor.expires_at - 401.0).abs() < 1e-9);
        assert_eq!(survivor.source, "on_expire");
    }

    #[test]
    fn save_form_round_trips_remaining_time_across_game_time_reset() {
        // Simulates app-restart semantics: save at game_time=3600 with a
        // buff that had 600s remaining, then load in a fresh session
        // (game_time=0). The buff's remaining lifetime must still be 600s,
        // NOT the 4200s that an unshifted absolute `expires_at` would imply.
        let mut live = BuffState::default();
        live.apply(&rookswort_spec(0.5, 600.0), 3600.0, "rookswort".into());
        assert!((live.active["rookswort"].expires_at - 4200.0).abs() < 1e-9);

        let save = live.to_save_form(3600.0);
        // On disk, expires_at is now the remaining-time, not an absolute.
        assert!((save.active["rookswort"].expires_at - 600.0).abs() < 1e-9);

        // Load in a new session with fresh clock.
        let restored = BuffState::from_save_form(&save, 0.0);
        assert!((restored.active["rookswort"].expires_at - 600.0).abs() < 1e-9);

        // If the new session advances past 600s, the buff expires as expected.
        let mut s = restored.clone();
        s.tick(601.0);
        assert!(s.active.is_empty(), "buff should expire at its rebased time");

        // And before 600s, it survives.
        let mut s = restored;
        s.tick(500.0);
        assert_eq!(s.active.len(), 1);
    }

    #[test]
    fn save_form_round_trips_multiple_buffs() {
        let mut live = BuffState::default();
        live.apply(&rookswort_spec(0.5, 600.0), 1000.0, "rookswort".into());
        let campfire = BuffSpec {
            kind: "campfire".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.75 },
            duration_secs: 60.0,
            on_expire: None,
        };
        live.apply(&campfire, 1000.0, "campfire".into());

        let save = live.to_save_form(1000.0);
        let restored = BuffState::from_save_form(&save, 50000.0);
        assert!((restored.active["rookswort"].expires_at - 50600.0).abs() < 1e-9);
        assert!((restored.active["campfire"].expires_at - 50060.0).abs() < 1e-9);
    }

    #[test]
    fn apply_is_noop_on_non_finite_inputs_in_release() {
        // Release-mode behavior: no-op (not panic). Debug builds still
        // trip the debug_assert, but this test only exercises the
        // release-mode early-return path to verify state is untouched.
        let mut s = BuffState::default();
        // Apply one valid buff first.
        s.apply(&rookswort_spec(0.5, 600.0), 0.0, "rookswort".into());
        // Now try invalid inputs — these must not corrupt state in release.
        let nan_spec = BuffSpec {
            kind: "nan_kind".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            duration_secs: f64::NAN,
            on_expire: None,
        };
        // Debug builds assert; skip the check there. Release-mode behavior
        // is what we want to verify — no panic, no insert.
        if cfg!(not(debug_assertions)) {
            s.apply(&nan_spec, 0.0, "nan".into());
            assert!(!s.active.contains_key("nan_kind"));
            assert_eq!(s.active.len(), 1);
        }
    }

    #[test]
    fn build_buff_frames_falls_back_to_kind_lookup_for_on_expire_successors() {
        // on_expire successors inherit the literal source "on_expire" instead
        // of the original item id. The HUD lookup must therefore also try
        // `kind` as a secondary key so tier-ramp-down successors still show
        // the parent item's icon/label rather than the raw kind string.
        use crate::item::types::{ItemDef, ItemDefs};
        let mut item_defs: ItemDefs = Default::default();
        item_defs.insert(
            "rookswort".into(),
            ItemDef {
                id: "rookswort".into(),
                name: "Rookswort".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 50,
                icon: "rookswort_icon".into(),
                base_cost: None,
                energy_value: None,
                mood_value: None,
                buff_effect: None,
            },
        );
        let mut buffs = BuffState::default();
        // Simulate an on_expire successor — source is "on_expire" but kind
        // matches the catalog entry.
        buffs.active.insert(
            "rookswort".into(),
            ActiveBuff {
                kind: "rookswort".into(),
                effect: BuffEffect::MoodDecayMultiplier { value: 0.75 },
                expires_at: 200.0,
                source: "on_expire".into(),
                on_expire: None,
            },
        );
        let frames = build_buff_frames(&buffs, &item_defs, 100.0);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].label, "Rookswort");
        assert_eq!(frames[0].icon, "rookswort_icon");
    }

    #[test]
    fn build_buff_frames_uses_kind_string_when_neither_source_nor_kind_resolves() {
        // System-sourced buff with no matching catalog entry — falls back
        // to the kind string for both icon and label so the HUD still
        // shows something rather than a blank.
        let mut buffs = BuffState::default();
        buffs.active.insert(
            "mystery".into(),
            ActiveBuff {
                kind: "mystery".into(),
                effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
                expires_at: 200.0,
                source: "environment".into(),
                on_expire: None,
            },
        );
        let item_defs = crate::item::types::ItemDefs::default();
        let frames = build_buff_frames(&buffs, &item_defs, 100.0);
        assert_eq!(frames[0].icon, "mystery");
        assert_eq!(frames[0].label, "mystery");
    }

    #[test]
    fn build_buff_frames_clamps_negative_remaining_to_zero() {
        let mut buffs = BuffState::default();
        buffs.apply(&rookswort_spec(0.5, 10.0), 0.0, "rookswort".into());
        let item_defs = crate::item::types::ItemDefs::default();
        // game_time is past expires_at
        let frames = build_buff_frames(&buffs, &item_defs, 100.0);
        assert_eq!(frames[0].remaining_secs, 0.0);
    }
}
