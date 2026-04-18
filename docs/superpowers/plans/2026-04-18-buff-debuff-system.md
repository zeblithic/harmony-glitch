# Buff/Debuff System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a data-driven buff engine end-to-end by wiring a single real item (rookswort) through the full pipeline: data authoring → application via `eat_item` → composition into mood decay → expiration with optional `on_expire` chains → persistence across save/load → HUD display.

**Architecture:** New Rust module `src-tauri/src/buff/` with `BuffState` hanging on `SocialState`. `MoodState::tick` loses its `party_bonus: bool` parameter in favor of a generic `decay_modifier: f64` that `SocialState::tick` composes multiplicatively from party state and active buffs. Item catalog (`assets/items.json`) gains an optional `buffEffect` field per item, read by `eat_item` to apply the buff. Frontend gets a new Svelte `BuffHud.svelte` component fed via the existing game-state frame IPC.

**Tech Stack:** Rust + Tauri v2 backend, Svelte 5 frontend, `serde_json` for persistence + item catalog, `vitest` + `@testing-library/svelte` for frontend tests, inline `#[cfg(test)] mod tests` for Rust unit tests.

**Spec:** `docs/superpowers/specs/2026-04-18-buff-debuff-design.md`

---

## File Structure

| Path | Action | Responsibility |
|---|---|---|
| `src-tauri/src/buff/mod.rs` | Create | `BuffState` — apply, tick, compose; helper `apply_item_buff` |
| `src-tauri/src/buff/types.rs` | Create | `BuffEffect` enum, `BuffSpec` template, `ActiveBuff` instance |
| `src-tauri/src/lib.rs` | Modify | `pub mod buff;` declaration; wire buff apply into `eat_item`; expose `activeBuffs` on game-state frame |
| `src-tauri/src/item/types.rs` | Modify | Add `buff_effect: Option<BuffSpec>` field to `ItemDef` |
| `src-tauri/src/mood/mod.rs` | Modify | `tick` signature: `party_bonus: bool` → `decay_modifier: f64`; clamp `.max(0.0)` |
| `src-tauri/src/social/mod.rs` | Modify | Add `buffs: BuffState` to `SocialState`; compose `decay_modifier` in `tick` |
| `src-tauri/src/engine/state.rs` | Modify | Add `buffs` field to `SaveState`; wire through `save_state` and `restore_save` |
| `assets/items.json` | Modify | Add `buffEffect` block to existing `rookswort` entry |
| `src/lib/types.ts` | Modify | Extend `RenderFrame` with `activeBuffs: BuffFrame[]`; add `BuffFrame` type |
| `src/lib/components/BuffHud.svelte` | Create | Horizontal row of buff icons with remaining-time label |
| `src/lib/components/BuffHud.test.ts` | Create | Vitest coverage |
| `src/App.svelte` | Modify | Import and render `BuffHud`; pass `latestFrame?.activeBuffs` |

---

## Task 1: Create buff module with type definitions

**Files:**
- Create: `src-tauri/src/buff/mod.rs`
- Create: `src-tauri/src/buff/types.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod buff;`)

- [ ] **Step 1: Declare the new module in lib.rs**

Open `src-tauri/src/lib.rs` and find the existing `pub mod` declarations near the top (search for `pub mod mood;` or `pub mod social;`). Add alongside them:

```rust
pub mod buff;
```

- [ ] **Step 2: Create the types file with failing tests**

Create `src-tauri/src/buff/types.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Effect kinds. v1 ships one variant. Future types are additive — new variants
/// are ignored by `BuffState::mood_decay_multiplier()` via `filter_map`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BuffEffect {
    /// Multiplies mood decay rate. 0.5 = half-rate. 0.0 = halts decay.
    /// Values > 1.0 accelerate decay (debuff). Negative values are clamped
    /// to 0.0 by the mood tick to prevent effective mood gain.
    MoodDecayMultiplier { value: f64 },
}

/// Static buff template loaded from JSON (e.g. `ItemDef.buff_effect`).
/// Duration is relative; resolved to absolute `expires_at` at application time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BuffSpec {
    pub kind: String,
    pub effect: BuffEffect,
    pub duration_secs: f64,
    #[serde(default)]
    pub on_expire: Option<Box<BuffSpec>>,
}

/// Live buff instance in the `BuffState` map.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveBuff {
    pub kind: String,
    pub effect: BuffEffect,
    pub expires_at: f64,
    pub source: String,
    pub on_expire: Option<Box<BuffSpec>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buff_effect_tagged_serialization_shape() {
        // Locks the JSON shape so item catalog files don't silently break.
        let e = BuffEffect::MoodDecayMultiplier { value: 0.5 };
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, r#"{"type":"moodDecayMultiplier","value":0.5}"#);
    }

    #[test]
    fn buff_spec_roundtrips_json() {
        let spec = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            duration_secs: 600.0,
            on_expire: None,
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: BuffSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn buff_spec_with_on_expire_chain_roundtrips_json() {
        let inner = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.75 },
            duration_secs: 180.0,
            on_expire: None,
        };
        let spec = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            duration_secs: 600.0,
            on_expire: Some(Box::new(inner)),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: BuffSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn active_buff_roundtrips_json() {
        let buff = ActiveBuff {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            expires_at: 1234.5,
            source: "rookswort".into(),
            on_expire: None,
        };
        let json = serde_json::to_string(&buff).unwrap();
        let back: ActiveBuff = serde_json::from_str(&json).unwrap();
        assert_eq!(back, buff);
    }

    #[test]
    fn buff_spec_without_on_expire_field_in_json_defaults_to_none() {
        let json = r#"{
            "kind": "rookswort",
            "effect": { "type": "moodDecayMultiplier", "value": 0.5 },
            "durationSecs": 600.0
        }"#;
        let spec: BuffSpec = serde_json::from_str(json).unwrap();
        assert!(spec.on_expire.is_none());
    }
}
```

- [ ] **Step 3: Create the module stub**

Create `src-tauri/src/buff/mod.rs`:

```rust
pub mod types;

pub use types::{ActiveBuff, BuffEffect, BuffSpec};
```

- [ ] **Step 4: Run tests — all should pass**

```bash
cd src-tauri && cargo test -p harmony-glitch buff::types::tests
```

Expected: 5 tests pass (`buff_effect_tagged_serialization_shape`, `buff_spec_roundtrips_json`, `buff_spec_with_on_expire_chain_roundtrips_json`, `active_buff_roundtrips_json`, `buff_spec_without_on_expire_field_in_json_defaults_to_none`).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/buff/ src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(buffs): add BuffEffect, BuffSpec, ActiveBuff types (ZEB-80)

Data shapes for the v1 buff engine. BuffSpec is the JSON template,
ActiveBuff is the runtime instance with resolved expires_at. on_expire
chains via Option<Box<BuffSpec>>.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: BuffState with apply (refresh semantics)

**Files:**
- Modify: `src-tauri/src/buff/mod.rs`

- [ ] **Step 1: Write the failing tests**

Replace `src-tauri/src/buff/mod.rs` with:

```rust
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
        let active = ActiveBuff {
            kind: spec.kind.clone(),
            effect: spec.effect.clone(),
            expires_at: game_time + spec.duration_secs,
            source,
            on_expire: spec.on_expire.clone(),
        };
        self.active.insert(spec.kind.clone(), active);
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
}
```

- [ ] **Step 2: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p harmony-glitch buff::tests
```

Expected: 3 new tests pass (`apply_inserts_new_buff`, `apply_same_kind_refreshes_expires_at`, `apply_same_kind_replaces_effect_magnitude`). Prior types tests still pass (run `cargo test -p harmony-glitch buff` to see all 8).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/buff/mod.rs
git commit -m "$(cat <<'EOF'
feat(buffs): BuffState apply with refresh semantics (ZEB-80)

Same-kind buffs replace in place — this is the core contract the content
layer relies on for tier-upgrade patterns.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: BuffState::mood_decay_multiplier

**Files:**
- Modify: `src-tauri/src/buff/mod.rs`

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/buff/mod.rs`, inside the existing `#[cfg(test)] mod tests` block, add:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test -p harmony-glitch buff::tests::mood_decay_multiplier
```

Expected: FAIL with "no method named `mood_decay_multiplier` found for struct `BuffState`".

- [ ] **Step 3: Implement the method**

In `src-tauri/src/buff/mod.rs`, inside `impl BuffState`, add:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p harmony-glitch buff::tests
```

Expected: All 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/buff/mod.rs
git commit -m "$(cat <<'EOF'
feat(buffs): compose mood decay multipliers multiplicatively (ZEB-80)

Folds active MoodDecayMultiplier effects into a single factor. filter_map
keeps future variants invisible to this function.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: BuffState::tick with on_expire chains

**Files:**
- Modify: `src-tauri/src/buff/mod.rs`

- [ ] **Step 1: Write the failing tests**

In the existing `#[cfg(test)] mod tests` block, add:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test -p harmony-glitch buff::tests::tick
```

Expected: FAIL with "no method named `tick` found for struct `BuffState`".

- [ ] **Step 3: Implement the method**

In `src-tauri/src/buff/mod.rs`, inside `impl BuffState`, add:

```rust
    /// Remove buffs whose `expires_at <= game_time`. For each expired buff
    /// with `on_expire: Some(spec)`, immediately apply the successor.
    /// Bounded to 8 expansion passes to defend against degenerate chains.
    pub fn tick(&mut self, game_time: f64) {
        const MAX_PASSES: usize = 8;
        for _ in 0..MAX_PASSES {
            // Collect expired kinds in sorted order for determinism.
            let mut expired_kinds: Vec<String> = self
                .active
                .iter()
                .filter(|(_, b)| b.expires_at <= game_time)
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
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p harmony-glitch buff::tests
```

Expected: All 11 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/buff/mod.rs
git commit -m "$(cat <<'EOF'
feat(buffs): tick with on_expire chains and bounded expansion (ZEB-80)

Deterministic expiration order (sorted by kind). on_expire successors
apply in-place if same-kind, otherwise under their own key. Bounded to
8 passes to defend against degenerate circular content data.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Extend ItemDef with buff_effect field

**Files:**
- Modify: `src-tauri/src/item/types.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/item/types.rs`, inside the existing `#[cfg(test)] mod tests` block, add:

```rust
    #[test]
    fn item_def_with_buff_effect_deserializes() {
        let json = r#"{
            "name": "Rookswort",
            "description": "Slows mood loss.",
            "category": "food",
            "stackLimit": 50,
            "icon": "rookswort",
            "buffEffect": {
                "kind": "rookswort",
                "effect": { "type": "moodDecayMultiplier", "value": 0.5 },
                "durationSecs": 600.0
            }
        }"#;
        let def: ItemDef = serde_json::from_str(json).unwrap();
        let be = def.buff_effect.as_ref().expect("buff_effect present");
        assert_eq!(be.kind, "rookswort");
        assert!((be.duration_secs - 600.0).abs() < 1e-9);
    }

    #[test]
    fn item_def_without_buff_effect_defaults_to_none() {
        let json = r#"{
            "name": "Cherry",
            "description": "A cherry.",
            "category": "food",
            "stackLimit": 50,
            "icon": "cherry"
        }"#;
        let def: ItemDef = serde_json::from_str(json).unwrap();
        assert!(def.buff_effect.is_none());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test -p harmony-glitch item::types::tests::item_def_with_buff_effect_deserializes
```

Expected: FAIL with "no field `buff_effect` on type `ItemDef`" (compilation error).

- [ ] **Step 3: Add the field to ItemDef**

In `src-tauri/src/item/types.rs`, modify the `ItemDef` struct:

```rust
/// Item type definition (loaded from JSON at startup).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDef {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub stack_limit: u32,
    pub icon: String,
    #[serde(default)]
    pub base_cost: Option<u32>,
    #[serde(default)]
    pub energy_value: Option<u32>,
    #[serde(default)]
    pub mood_value: Option<u32>,
    #[serde(default)]
    pub buff_effect: Option<crate::buff::BuffSpec>,
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p harmony-glitch item::types::tests
```

Expected: All pre-existing item tests still pass + both new `buff_effect` tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/item/types.rs
git commit -m "$(cat <<'EOF'
feat(items): add optional buff_effect field to ItemDef (ZEB-80)

Content authors attach a BuffSpec to any item; use_item handlers apply
it. Existing items deserialize unchanged thanks to #[serde(default)].

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Add rookswort buffEffect to items.json

**Files:**
- Modify: `assets/items.json` (existing `rookswort` entry, approx line 9712)

- [ ] **Step 1: Write the failing test**

Add a new test to `src-tauri/src/item/types.rs` (inside the existing `#[cfg(test)] mod tests` block):

```rust
    #[test]
    fn items_catalog_loads_rookswort_with_expected_buff_effect() {
        // Loads the actual shipped catalog to guard against JSON regressions.
        let json = std::fs::read_to_string("../assets/items.json")
            .expect("assets/items.json should be readable from src-tauri/");
        let catalog: std::collections::HashMap<String, ItemDef> =
            serde_json::from_str(&json).expect("items.json parses");
        let rookswort = catalog.get("rookswort").expect("rookswort entry exists");
        let be = rookswort
            .buff_effect
            .as_ref()
            .expect("rookswort has buffEffect");
        assert_eq!(be.kind, "rookswort");
        assert!((be.duration_secs - 600.0).abs() < 1e-9);
        assert_eq!(
            be.effect,
            crate::buff::BuffEffect::MoodDecayMultiplier { value: 0.5 }
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd src-tauri && cargo test -p harmony-glitch item::types::tests::items_catalog_loads_rookswort_with_expected_buff_effect
```

Expected: FAIL with `rookswort has buffEffect` panic (current rookswort entry has no `buffEffect`).

- [ ] **Step 3: Add buffEffect to rookswort in items.json**

Open `assets/items.json`, find the `rookswort` entry (search for `"rookswort": {` — the FOOD one, approx line 9712, NOT `herb_seed_rookswort` or `essence_of_rookswort`). Current shape:

```json
  "rookswort": {
    "name": "Rookswort",
    "description": "An exotic bud with a pleasantly lasting aftertaste of danger and chaos.",
    "category": "food",
    "stackLimit": 50,
    "icon": "rookswort",
    "baseCost": 75,
    "energyValue": 75,
    "moodValue": 22
  },
```

Change to:

```json
  "rookswort": {
    "name": "Rookswort",
    "description": "An exotic bud with a pleasantly lasting aftertaste of danger and chaos.",
    "category": "food",
    "stackLimit": 50,
    "icon": "rookswort",
    "baseCost": 75,
    "energyValue": 75,
    "moodValue": 22,
    "buffEffect": {
      "kind": "rookswort",
      "effect": { "type": "moodDecayMultiplier", "value": 0.5 },
      "durationSecs": 600
    }
  },
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd src-tauri && cargo test -p harmony-glitch item::types::tests::items_catalog_loads_rookswort_with_expected_buff_effect
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/item/types.rs assets/items.json
git commit -m "$(cat <<'EOF'
feat(items): rookswort slows mood loss 50% for 10 minutes (ZEB-80)

First real content consumer of the buff engine. Adds a regression test
that loads the actual catalog to catch JSON-shape drift.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Refactor MoodState::tick signature to take decay_modifier

**Files:**
- Modify: `src-tauri/src/mood/mod.rs`
- Modify: `src-tauri/src/social/mod.rs` (caller still passes party-only value; buffs wired in Task 8)

- [ ] **Step 1: Write new failing tests for the decay_modifier signature**

In `src-tauri/src/mood/mod.rs`, inside `#[cfg(test)] mod tests`, REPLACE the existing `tick_with_party_bonus_reduces_decay_by_25_percent` test and add new ones:

```rust
    #[test]
    fn tick_with_decay_modifier_of_one_matches_unmodified_baseline() {
        let mut s = MoodState::default();
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, false, 1.0);
        // Same as prior baseline test (no party, no buffs)
        assert!(s.mood < 100.0);
    }

    #[test]
    fn tick_with_decay_modifier_of_zero_halts_decay() {
        let mut s = MoodState::default();
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, false, 0.0);
        assert_eq!(s.mood, 100.0);
    }

    #[test]
    fn tick_with_decay_modifier_of_0_75_reduces_by_25_percent() {
        let mut base = MoodState::default();
        let mut reduced = MoodState::default();
        let game_time = base.mood_grace_until + 1.0;
        base.tick(60.0, game_time, false, 1.0);
        reduced.tick(60.0, game_time, false, 0.75);
        let base_decay = 100.0 - base.mood;
        let reduced_decay = 100.0 - reduced.mood;
        let ratio = reduced_decay / base_decay;
        assert!((ratio - 0.75).abs() < 1e-9, "got {ratio}");
    }

    #[test]
    fn tick_with_decay_modifier_above_one_accelerates_decay() {
        let mut base = MoodState::default();
        let mut debuffed = MoodState::default();
        let game_time = base.mood_grace_until + 1.0;
        base.tick(60.0, game_time, false, 1.0);
        debuffed.tick(60.0, game_time, false, 2.0);
        let base_decay = 100.0 - base.mood;
        let debuff_decay = 100.0 - debuffed.mood;
        assert!(debuff_decay > base_decay, "debuff should decay faster");
    }

    #[test]
    fn tick_clamps_negative_decay_modifier_to_zero() {
        let mut s = MoodState::default();
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, false, -5.0);
        // Clamped to 0.0, so mood is unchanged (not increased).
        assert_eq!(s.mood, 100.0);
    }
```

ALSO remove or replace the now-invalid tests that used the old bool parameter:
- Find `tick_with_party_bonus_reduces_decay_by_25_percent` — DELETE (replaced above)
- Find all other tests using `tick(..., false)` or `tick(..., true)` — update the last arg to `1.0` or `0.75` respectively. Affected tests: `tick_decays_mood`, `tick_suppressed_during_dialogue`, `tick_suppressed_during_grace_period`, `tick_resumes_after_grace_period`, `tick_ignores_invalid_inputs`.

Example transformation:
```rust
// Before
s.tick(60.0, game_time, false, false);
// After
s.tick(60.0, game_time, false, 1.0);
```

- [ ] **Step 2: Run tests to verify they fail (compilation error expected)**

```bash
cd src-tauri && cargo test -p harmony-glitch mood::tests
```

Expected: Compilation failure — `MoodState::tick` still takes `bool` but tests pass `f64`.

- [ ] **Step 3: Change MoodState::tick signature**

In `src-tauri/src/mood/mod.rs`, replace the `tick` method:

```rust
    /// Advances mood by `dt` seconds of game time.
    /// Decay is suppressed during dialogue or while the grace period is active.
    /// `decay_modifier` scales the effective decay (1.0 = normal, 0.5 = half,
    /// 0.0 = halted, >1.0 accelerates). Negative values are clamped to 0.0.
    pub fn tick(&mut self, dt: f64, game_time: f64, in_dialogue: bool, decay_modifier: f64) {
        if !dt.is_finite() || dt <= 0.0 || !game_time.is_finite() || game_time < 0.0 {
            return;
        }
        if in_dialogue || game_time < self.mood_grace_until {
            return;
        }
        let safe_modifier = decay_modifier.max(0.0);
        let effective_dt = dt * safe_modifier;
        self.mood = decay::mood_decay(self.mood, self.max_mood, effective_dt);
    }
```

Update the doc comment too (the old `party_bonus` comment block at line 39 — replace it).

- [ ] **Step 4: Update the one caller in social/mod.rs**

In `src-tauri/src/social/mod.rs`, locate `SocialState::tick` (around line 42):

```rust
    pub fn tick(&mut self, dt: f64, ctx: &SocialTickContext) {
        self.emotes.check_date_change(ctx.current_date);
        let party_bonus = self.party.has_party_bonus();
        self.mood.tick(dt, ctx.game_time, ctx.in_dialogue, party_bonus);
        // ... rest unchanged
    }
```

Replace the `party_bonus` lines with:

```rust
    pub fn tick(&mut self, dt: f64, ctx: &SocialTickContext) {
        self.emotes.check_date_change(ctx.current_date);
        let party_factor = if self.party.has_party_bonus() { 0.75 } else { 1.0 };
        self.mood.tick(dt, ctx.game_time, ctx.in_dialogue, party_factor);
        // ... rest unchanged
    }
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test -p harmony-glitch
```

Expected: All tests pass — mood tests updated, social tests still green.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/mood/mod.rs src-tauri/src/social/mod.rs
git commit -m "$(cat <<'EOF'
refactor(mood): replace party_bonus bool with decay_modifier f64 (ZEB-80)

Generalized parameter lets callers compose multiple decay sources
(party, buffs, environment) into a single factor. Allows values > 1.0
for future debuff content. Negative values clamped to prevent mood gain.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Integrate BuffState into SocialState

**Files:**
- Modify: `src-tauri/src/social/mod.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/social/mod.rs`, inside `#[cfg(test)] mod tests`, add:

```rust
    #[test]
    fn tick_composes_party_bonus_and_buff_multiplicatively() {
        use crate::buff::{BuffEffect, BuffSpec};

        let mut base = make_social();
        let mut both = make_social();

        // Skip grace period by pushing game_time past mood_grace_until.
        let game_time = base.mood.mood_grace_until + 1.0;

        // "Both" gets a party bonus (2 members) AND a rookswort buff.
        both.party.create_party([1u8; 16], "Me".into(), 0.0);
        both.party
            .party
            .as_mut()
            .unwrap()
            .add_member(PartyMember {
                address_hash: [2u8; 16],
                display_name: "Peer".into(),
                joined_at: 1.0,
            })
            .unwrap();
        assert!(both.party.has_party_bonus()); // sanity: need 2 members

        let spec = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            duration_secs: 600.0,
            on_expire: None,
        };
        both.buffs.apply(&spec, game_time, "rookswort".into());

        let ctx = SocialTickContext {
            current_date: "2026-04-18",
            in_dialogue: false,
            game_time,
        };
        base.tick(60.0, &ctx);
        both.tick(60.0, &ctx);

        let base_decay = 100.0 - base.mood.mood;
        let both_decay = 100.0 - both.mood.mood;
        // Expected: party (0.75) × buff (0.5) = 0.375
        let ratio = both_decay / base_decay;
        assert!((ratio - 0.375).abs() < 1e-9, "got {ratio}");
    }

    #[test]
    fn tick_with_no_buffs_or_party_preserves_baseline() {
        let mut s = make_social();
        let game_time = s.mood.mood_grace_until + 1.0;
        let ctx = SocialTickContext {
            current_date: "2026-04-18",
            in_dialogue: false,
            game_time,
        };
        let before = s.mood.mood;
        s.tick(60.0, &ctx);
        assert!(s.mood.mood < before, "baseline decay still occurs");
    }
```

Note: `PartyState` fields are `pub party: Option<ActiveParty>`. `create_party` starts a solo party; `add_member` on the inner `ActiveParty` brings it to 2 members (which is the threshold for `has_party_bonus`). The test above mirrors the pattern already used in `src-tauri/src/social/party.rs` test module (line 492 `has_party_bonus_requires_two_members`).

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test -p harmony-glitch social::tests::tick_composes_party_bonus_and_buff_multiplicatively
```

Expected: FAIL with "no field `buffs` on type `SocialState`" (compilation error).

- [ ] **Step 3: Add BuffState to SocialState**

In `src-tauri/src/social/mod.rs`:

At the top, add import:

```rust
use crate::buff::BuffState;
```

Extend `SocialState` struct:

```rust
#[derive(Debug, Clone)]
pub struct SocialState {
    pub mood: MoodState,
    pub emotes: EmoteState,
    pub buddies: BuddyState,
    pub party: PartyState,
    pub buffs: BuffState,
}
```

Extend `SocialState::new`:

```rust
    pub fn new(identity: [u8; 16], date: &str) -> Self {
        Self {
            mood: MoodState::default(),
            emotes: EmoteState::new(identity, date),
            buddies: BuddyState::default(),
            party: PartyState::default(),
            buffs: BuffState::default(),
        }
    }
```

Extend `SocialState::tick` to tick buffs and compose the modifier:

```rust
    pub fn tick(&mut self, dt: f64, ctx: &SocialTickContext) {
        self.emotes.check_date_change(ctx.current_date);

        // Expire buffs before reading the modifier so the current frame
        // sees a consistent active set.
        self.buffs.tick(ctx.game_time);

        let party_factor = if self.party.has_party_bonus() { 0.75 } else { 1.0 };
        let buff_factor = self.buffs.mood_decay_multiplier();
        let decay_modifier = party_factor * buff_factor;

        self.mood.tick(dt, ctx.game_time, ctx.in_dialogue, decay_modifier);
        self.buddies.expire_requests(ctx.game_time);
        self.buddies.expire_outgoing_requests(ctx.game_time);
        self.party.expire_invite(ctx.game_time);
        self.party.expire_outgoing_invites(ctx.game_time);
        self.party.expire_pending_join(ctx.game_time);
    }
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test -p harmony-glitch social::tests
```

Expected: Both new tests pass, all prior social tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/social/mod.rs
git commit -m "$(cat <<'EOF'
feat(social): integrate BuffState into SocialState tick (ZEB-80)

Buff modifiers compose multiplicatively with party bonus in a single
place. mood.tick receives the folded decay_modifier value; party and
buff codepaths no longer reach into mood directly.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Wire buff application into eat_item

**Files:**
- Modify: `src-tauri/src/lib.rs` (eat_item handler, around line 3580)
- Modify: `src-tauri/src/buff/mod.rs` (add `apply_item_buff` helper for unit testing)

- [ ] **Step 1: Write a failing unit test for the helper**

In `src-tauri/src/buff/mod.rs`, inside `#[cfg(test)] mod tests`, add:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test -p harmony-glitch buff::tests::apply_item_buff
```

Expected: FAIL with "cannot find function `apply_item_buff` in this scope".

- [ ] **Step 3: Implement the helper**

In `src-tauri/src/buff/mod.rs`, add (outside the `impl BuffState` block but inside the module):

```rust
/// Apply an item's buff effect (if any) to the player's BuffState.
/// Uses the item's `id` as the buff source for HUD attribution.
pub fn apply_item_buff(buffs: &mut BuffState, item_def: &crate::item::types::ItemDef, game_time: f64) {
    if let Some(spec) = &item_def.buff_effect {
        buffs.apply(spec, game_time, item_def.id.clone());
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd src-tauri && cargo test -p harmony-glitch buff::tests
```

Expected: All buff tests (including two new ones) pass.

- [ ] **Step 5: Wire the helper into eat_item**

Open `src-tauri/src/lib.rs` and locate `fn eat_item` (around line 3580). Find the block right after `state.energy = new_energy;` (around line 3595) that applies mood_gained. Right after the mood application block (around line 3599 after `state.social.mood.apply_mood_change(mood_gained);` or right after the `if mood_gained > 0.0` block ends), add:

```rust
    // Apply buff effect (if item has one).
    if let Some(item_def) = state.item_defs.get(&item_id).cloned() {
        let gt = state.game_time;
        crate::buff::apply_item_buff(&mut state.social.buffs, &item_def, gt);
    }
```

Note: `item_id` here is the eaten item (the `use`-site already has it as a local). The `.cloned()` avoids borrow conflicts between immutable ItemDef access and mutable buffs access.

- [ ] **Step 6: Run the full test suite**

```bash
cd src-tauri && cargo test -p harmony-glitch
```

Expected: All tests pass; no regressions in existing `eat_item` coverage.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/buff/mod.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(buffs): apply item buffs on eat_item (ZEB-80)

Small helper apply_item_buff keeps the wiring unit-testable without
constructing a tauri AppHandle. Eating rookswort now produces a 10-min
mood-decay-reduction buff.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Persist BuffState across save/load

**Files:**
- Modify: `src-tauri/src/engine/state.rs` (SaveState struct + save_state + restore_save)

- [ ] **Step 1: Write failing tests**

In `src-tauri/src/engine/state.rs`, inside the existing `#[cfg(test)] mod tests` block, add:

```rust
    #[test]
    fn save_state_with_active_buff_roundtrips() {
        use crate::buff::{ActiveBuff, BuffEffect, BuffState};
        let mut buffs = BuffState::default();
        buffs.active.insert(
            "rookswort".into(),
            ActiveBuff {
                kind: "rookswort".into(),
                effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
                expires_at: 1234.5,
                source: "rookswort".into(),
                on_expire: None,
            },
        );
        let save = SaveState {
            street_id: "test".into(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Right,
            inventory: vec![],
            avatar: Default::default(),
            currants: 0,
            energy: 100.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            upgrades: Default::default(),
            skill_progress: Default::default(),
            quest_progress: Default::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
            buffs,
        };
        let json = serde_json::to_string(&save).unwrap();
        let back: SaveState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.buffs.active.len(), 1);
        assert!((back.buffs.active["rookswort"].expires_at - 1234.5).abs() < 1e-9);
    }

    #[test]
    fn save_state_missing_buffs_field_defaults_to_empty() {
        // Simulates loading a pre-buff save: serialize a full SaveState,
        // strip the buffs field, deserialize, verify default.
        let save = SaveState {
            street_id: "test".into(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Right,
            inventory: vec![],
            avatar: Default::default(),
            currants: 0,
            energy: 100.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            upgrades: Default::default(),
            skill_progress: Default::default(),
            quest_progress: Default::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
            buffs: Default::default(),
        };
        let mut json_value: serde_json::Value = serde_json::to_value(&save).unwrap();
        json_value.as_object_mut().unwrap().remove("buffs");
        let json_str = serde_json::to_string(&json_value).unwrap();
        let back: SaveState = serde_json::from_str(&json_str).unwrap();
        assert!(back.buffs.active.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd src-tauri && cargo test -p harmony-glitch engine::state::tests::save_state_with_active_buff_roundtrips
```

Expected: FAIL — no field `buffs` on type `SaveState`.

- [ ] **Step 3: Add buffs field to SaveState**

In `src-tauri/src/engine/state.rs`, find the `SaveState` struct (around line 58) and add:

```rust
pub struct SaveState {
    // ... existing fields ...
    #[serde(default)]
    pub last_hi_date: Option<String>,
    #[serde(default)]
    pub buffs: crate::buff::BuffState,
}
```

- [ ] **Step 4: Update save_state() to populate buffs**

Find `save_state()` (around line 1118). Inside the `Some(SaveState { ... })` block, add after the `last_hi_date` line:

```rust
            last_hi_date: Some(self.social.emotes.current_date.clone()),
            buffs: self.social.buffs.clone(),
```

- [ ] **Step 5: Update restore_save() to read buffs**

Find `restore_save()` (around line 1158). After the `self.social.buddies.restore_from_save(...)` line, add:

```rust
        self.social.buffs = save.buffs.clone();
```

- [ ] **Step 6: Run tests**

```bash
cd src-tauri && cargo test -p harmony-glitch
```

Expected: New tests pass; all existing SaveState roundtrip tests still pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "$(cat <<'EOF'
feat(buffs): persist BuffState across save/load (ZEB-80)

expires_at is absolute game_time, which rides alongside the game_time
the engine already persists. Pre-buff saves default buffs to empty via
#[serde(default)].

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: Expose activeBuffs on game-state frame IPC

**Files:**
- Modify: `src-tauri/src/lib.rs` (frame builder, around line 898 where `maxMood` is written)

- [ ] **Step 1: Add a BuffFrame serializer helper in Rust**

In `src-tauri/src/buff/mod.rs`, add a frame helper:

```rust
use serde::Serialize;

/// Per-buff data shape sent to the frontend each tick.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuffFrame {
    pub kind: String,
    pub icon: String,
    pub label: String,
    pub remaining_secs: f64,
}

/// Build the list of BuffFrames for the IPC game-state payload.
/// `item_defs` is used to resolve display name and icon by source item id.
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
    // Deterministic ordering for stable UI rendering.
    frames.sort_by(|a, b| a.kind.cmp(&b.kind));
    frames
}
```

- [ ] **Step 2: Write a failing test**

Inside the existing `#[cfg(test)] mod tests` block in `src-tauri/src/buff/mod.rs`, add:

```rust
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
    fn build_buff_frames_clamps_negative_remaining_to_zero() {
        let mut buffs = BuffState::default();
        buffs.apply(&rookswort_spec(0.5, 10.0), 0.0, "rookswort".into());
        let item_defs = crate::item::types::ItemDefs::default();
        // game_time is past expires_at
        let frames = build_buff_frames(&buffs, &item_defs, 100.0);
        assert_eq!(frames[0].remaining_secs, 0.0);
    }
```

- [ ] **Step 3: Run tests**

```bash
cd src-tauri && cargo test -p harmony-glitch buff::tests::build_buff_frames
```

Expected: Both pass after Step 1's code is in place.

- [ ] **Step 4: Wire activeBuffs into the IPC frame in lib.rs**

Open `src-tauri/src/lib.rs`. Find the block that writes the game-state frame JSON (grep for `"maxMood": state.social.mood.max_mood` — around line 898). Immediately after the `"maxMood"` line (and inside the same json! block), add:

```rust
        "activeBuffs": crate::buff::build_buff_frames(
            &state.social.buffs,
            &state.item_defs,
            state.game_time,
        ),
```

- [ ] **Step 5: Run the full suite**

```bash
cd src-tauri && cargo build -p harmony-glitch && cargo test -p harmony-glitch
```

Expected: Clean build + all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/buff/mod.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(buffs): expose activeBuffs on game-state frame IPC (ZEB-80)

build_buff_frames resolves icon and display name from the item catalog
via the buff's source item_id, with fallback to the kind for
system-sourced buffs (none in v1).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: BuffHud.svelte component with tests

**Files:**
- Create: `src/lib/components/BuffHud.svelte`
- Create: `src/lib/components/BuffHud.test.ts`

- [ ] **Step 1: Write failing tests**

Create `src/lib/components/BuffHud.test.ts`:

```typescript
import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import BuffHud from './BuffHud.svelte';

describe('BuffHud', () => {
  it('renders nothing when there are no active buffs', () => {
    const { container } = render(BuffHud, { props: { buffs: [] } });
    // Container should be empty (or contain only the root hud element with no icons).
    const icons = container.querySelectorAll('.buff-icon');
    expect(icons.length).toBe(0);
  });

  it('renders one buff icon with label and remaining time', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: 300 },
        ],
      },
    });
    expect(screen.getByLabelText(/Rookswort.*5:00 remaining/)).toBeInTheDocument();
  });

  it('formats remaining time as mm:ss when above 60 seconds', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: 125 },
        ],
      },
    });
    expect(screen.getByText('2:05')).toBeInTheDocument();
  });

  it('formats remaining time as Ns when below 60 seconds', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: 45 },
        ],
      },
    });
    expect(screen.getByText('45s')).toBeInTheDocument();
  });

  it('renders multiple buffs in given order', () => {
    const { container } = render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: 300 },
          { kind: 'campfire', icon: 'campfire', label: 'Campfire', remainingSecs: 30 },
        ],
      },
    });
    const icons = container.querySelectorAll('.buff-icon');
    expect(icons.length).toBe(2);
  });

  it('clamps negative remainingSecs to 0s in display', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: -5 },
        ],
      },
    });
    expect(screen.getByText('0s')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
npm test -- src/lib/components/BuffHud.test.ts
```

Expected: FAIL — `BuffHud.svelte` does not exist.

- [ ] **Step 3: Create the component**

Create `src/lib/components/BuffHud.svelte`:

```svelte
<script lang="ts">
  interface BuffFrame {
    kind: string;
    icon: string;
    label: string;
    remainingSecs: number;
  }

  let { buffs = [] }: { buffs: BuffFrame[] } = $props();

  function formatRemaining(secs: number): string {
    const safe = Math.max(0, Math.floor(secs));
    if (safe >= 60) {
      const m = Math.floor(safe / 60);
      const s = safe % 60;
      return `${m}:${s.toString().padStart(2, '0')}`;
    }
    return `${safe}s`;
  }
</script>

{#if buffs.length > 0}
  <div class="buff-hud" role="list" aria-label="Active buffs">
    {#each buffs as buff (buff.kind)}
      <div
        class="buff-icon"
        role="listitem"
        aria-label="{buff.label}: {formatRemaining(buff.remainingSecs)} remaining"
      >
        <span class="buff-icon-sprite">{buff.icon}</span>
        <span class="buff-timer">{formatRemaining(buff.remainingSecs)}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .buff-hud {
    position: fixed;
    top: 74px;
    left: 12px;
    display: flex;
    flex-direction: row;
    gap: 6px;
    z-index: 50;
    pointer-events: none;
    user-select: none;
  }

  .buff-icon {
    background: rgba(26, 26, 46, 0.85);
    padding: 4px 8px;
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    min-width: 28px;
  }

  .buff-icon-sprite {
    font-size: 11px;
    color: #fbbf24;
  }

  .buff-timer {
    font-size: 10px;
    font-weight: bold;
    color: #fbbf24;
  }
</style>
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
npm test -- src/lib/components/BuffHud.test.ts
```

Expected: All 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/BuffHud.svelte src/lib/components/BuffHud.test.ts
git commit -m "$(cat <<'EOF'
feat(ui): BuffHud component with remaining-time display (ZEB-80)

Horizontal row of buff icons below the mood bar. mm:ss formatting for
durations over 60s, Ns for shorter. Hidden when there are no active buffs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: Wire BuffHud into App.svelte and RenderFrame type

**Files:**
- Modify: `src/lib/types.ts` (extend `RenderFrame`)
- Modify: `src/App.svelte` (import and render `BuffHud`)

- [ ] **Step 1: Add BuffFrame type and extend RenderFrame**

Open `src/lib/types.ts`. `RenderFrame` starts at line 202 (`export interface RenderFrame {`). Add the new `BuffFrame` interface immediately before it (so it's defined before use):

```typescript
export interface BuffFrame {
  kind: string;
  icon: string;
  label: string;
  remainingSecs: number;
}
```

Then add a field inside the `RenderFrame` body. Search within `RenderFrame` for the `mood:` or `maxMood:` field and add directly after it:

```typescript
  activeBuffs: BuffFrame[];
```

(If `RenderFrame` doesn't currently have `mood` and `maxMood` fields but does have other HUD fields like `energy`, add `activeBuffs` adjacent to those. The placement rule: group with other HUD stats.)

- [ ] **Step 2: Add BuffHud to the imports in App.svelte**

Open `src/App.svelte`. Near the top where `MoodHud` is imported (around line 16), add:

```svelte
  import BuffHud from './lib/components/BuffHud.svelte';
```

And extend the type import near line 35 to include `BuffFrame` if you use it as a local type (optional — not required for rendering).

- [ ] **Step 3: Render BuffHud in the template**

In `src/App.svelte`, locate the `<MoodHud>` usage (around line 1147). Right after it, add:

```svelte
    <MoodHud mood={latestFrame?.mood ?? 100} maxMood={latestFrame?.maxMood ?? 100} />
    <BuffHud buffs={latestFrame?.activeBuffs ?? []} />
```

- [ ] **Step 4: Typecheck and test**

```bash
npm run check
npm test
```

Expected: No type errors; all tests pass.

- [ ] **Step 5: Manual smoke test**

Start the dev environment:

```bash
npm run tauri dev
```

Steps to verify:
1. Start a new game or load an existing save.
2. Open the developer console in the Tauri window (right-click → Inspect).
3. Cheat a rookswort into inventory if needed: `invoke('grant_item', { itemId: 'rookswort', count: 1 })` — or harvest one if you have a Rookswort plant in the current street.
4. Open inventory (I key), click "Use" on the rookswort.
5. Verify: buff icon appears below the mood bar with a timer counting down from 10:00.
6. Join a party with a test peer. Confirm the buff icon still shows and mood decay feels slower (compare to baseline).
7. Save the game, reload it, confirm the buff is still active with the correct remaining time.
8. Wait (or fast-forward via dev tools) until the buff expires. Confirm the icon disappears and mood decay returns to normal.

Document any issues observed. The implementation is complete when all six verification steps pass.

- [ ] **Step 6: Commit**

```bash
git add src/lib/types.ts src/App.svelte
git commit -m "$(cat <<'EOF'
feat(ui): wire BuffHud into App with activeBuffs frame field (ZEB-80)

End-to-end buff pipeline now observable: eating rookswort produces a
HUD indicator with live countdown, and mood decay reflects the composed
modifier from both party state and active buffs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 14: Final verification

**Files:** (none modified — verification only)

- [ ] **Step 1: Run the full Rust test suite**

```bash
cd src-tauri && cargo test -p harmony-glitch
```

Expected: All tests pass, including new buff tests, refactored mood tests, integrated social tests, SaveState roundtrip tests.

- [ ] **Step 2: Run the frontend test suite**

```bash
npm test
```

Expected: All tests pass, including new `BuffHud.test.ts`.

- [ ] **Step 3: Run clippy with deny-warnings**

```bash
cd src-tauri && cargo clippy -p harmony-glitch -- -D warnings
```

Expected: No clippy errors on new or modified code. (Pre-existing warnings in unrelated files are acceptable; document but do not fix.)

- [ ] **Step 4: Build for production**

```bash
npm run tauri build
```

Expected: Build succeeds (smoke test that nothing's broken in the release path).

- [ ] **Step 5: No commit needed**

Verification-only task. The branch is ready to PR.

---

## Appendix: Summary of type signatures introduced or changed

**New (in `src-tauri/src/buff/`):**
- `BuffEffect::MoodDecayMultiplier { value: f64 }`
- `BuffSpec { kind: String, effect: BuffEffect, duration_secs: f64, on_expire: Option<Box<BuffSpec>> }`
- `ActiveBuff { kind: String, effect: BuffEffect, expires_at: f64, source: String, on_expire: Option<Box<BuffSpec>> }`
- `BuffState { active: HashMap<String, ActiveBuff> }`
- `BuffState::apply(spec: &BuffSpec, game_time: f64, source: String)`
- `BuffState::tick(game_time: f64)`
- `BuffState::mood_decay_multiplier() -> f64`
- `apply_item_buff(buffs: &mut BuffState, item_def: &ItemDef, game_time: f64)`
- `BuffFrame { kind, icon, label, remaining_secs }`
- `build_buff_frames(buffs, item_defs, game_time) -> Vec<BuffFrame>`

**Changed:**
- `MoodState::tick(dt, game_time, in_dialogue, decay_modifier: f64)` (was `party_bonus: bool`)
- `SocialState` gains `buffs: BuffState` field
- `SocialState::tick` now ticks buffs and composes `decay_modifier`
- `ItemDef` gains `buff_effect: Option<BuffSpec>` field
- `SaveState` gains `buffs: BuffState` field
- `GameState::save_state` populates `buffs` from `self.social.buffs`
- `GameState::restore_save` reads `save.buffs` into `self.social.buffs`
- `eat_item` applies item buffs via `apply_item_buff` helper
- Game-state frame IPC payload gains `activeBuffs` array
- `RenderFrame` TypeScript type gains `activeBuffs: BuffFrame[]`

**Frontend new:**
- `BuffHud.svelte` — horizontal buff row below `MoodHud`
- `BuffFrame` TS interface
