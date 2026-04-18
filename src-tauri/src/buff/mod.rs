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
    pub fn apply(&mut self, spec: &BuffSpec, game_time: f64, source: String) {
        debug_assert!(
            spec.duration_secs.is_finite() && game_time.is_finite(),
            "buff apply requires finite game_time and duration_secs"
        );
        let active = ActiveBuff {
            kind: spec.kind.clone(),
            effect: spec.effect.clone(),
            expires_at: game_time + spec.duration_secs,
            source,
            on_expire: spec.on_expire.clone(),
        };
        self.active.insert(spec.kind.clone(), active);
    }

    /// Fold all active `MoodDecayMultiplier` effects multiplicatively.
    /// Returns `1.0` when no relevant buffs are active. Future `BuffEffect`
    /// variants are ignored by the `filter_map` — no refactor needed when
    /// adding e.g. `EnergyDecayMultiplier`.
    pub fn mood_decay_multiplier(&self) -> f64 {
        self.active
            .values()
            .filter_map(|b| match b.effect {
                BuffEffect::MoodDecayMultiplier { value } => Some(value),
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
            for kind in expired_kinds {
                let Some(buff) = self.active.remove(&kind) else {
                    continue;
                };
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
}
