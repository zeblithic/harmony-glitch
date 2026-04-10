# Social Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the social foundation for Harmony Glitch — mood metabolics, hi emotes with daily viral variants, a trust-integrated buddy system, and ephemeral parties. These four subsystems create the core social loop: mood creates the need for social interaction, emotes and parties are the response, buddies are the persistent relationship layer.

**Architecture:** Three new Rust modules (`mood/`, `emote/`, `social/`) under a `SocialState` aggregator on `GameState`. Mood is an `f64` with three-tier decay. Emotes use BLAKE3-seeded daily variants. Buddies use mutual-witness add with trust integration. Parties are ephemeral leader-authority. All social state flows through `RenderFrame` to the frontend via existing Tauri IPC.

**Tech Stack:** Rust (Tauri v2), Svelte 5 (runes), TypeScript, Vitest, BLAKE3

**Spec:** `docs/superpowers/specs/2026-04-10-social-foundation-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `src-tauri/src/mood/mod.rs` | Create | MoodState struct, tick, apply_mood_change |
| `src-tauri/src/mood/decay.rs` | Create | Three-tier decay function, mood_multiplier |
| `src-tauri/src/emote/mod.rs` | Create | Re-exports |
| `src-tauri/src/emote/types.rs` | Create | HiVariant, EmoteType, EmoteMessage, EmoteState, daily_variant |
| `src-tauri/src/social/mod.rs` | Create | SocialState aggregator, SocialTickContext, re-exports |
| `src-tauri/src/social/buddy.rs` | Create | BuddyEntry, BuddyState, add/remove/block logic |
| `src-tauri/src/social/party.rs` | Create | PartyState, ActiveParty, PartyMember, lifecycle |
| `src-tauri/src/social/types.rs` | Create | SocialMessage enum, BuddySaveEntry |
| `src-tauri/src/item/types.rs` | Modify | Add `mood_value` to ItemDef and ItemStackFrame |
| `src-tauri/src/item/energy.rs` | Modify | Return mood restoration alongside energy |
| `src-tauri/src/engine/state.rs` | Modify | Add mood/max_mood to SaveState/GameState/RenderFrame, wire tick |
| `src-tauri/src/item/interaction.rs` | Modify | Add RemotePlayer variant to NearestInteractable, extend proximity_scan |
| `src-tauri/src/network/types.rs` | Modify | Add Emote/Social variants to NetMessage, ChatChannel to ChatMessage |
| `src-tauri/src/lib.rs` | Modify | Add `pub mod mood/emote/social;`, IPC commands, step-7 augmentation |
| `assets/items.json` | Modify | Add `moodValue` to food items |
| `src/lib/types.ts` | Modify | Add mood, social fields to RenderFrame, RemotePlayerFrame |
| `src/lib/ipc.ts` | Modify | Add social IPC functions |
| `src/lib/components/MoodHud.svelte` | Create | Mood bar display |
| `src/lib/components/MoodHud.test.ts` | Create | Tests for mood bar |
| `src/lib/components/PartyPanel.svelte` | Create | Party member list UI |
| `src/lib/components/PartyPanel.test.ts` | Create | Tests for party panel |
| `src/lib/components/BuddyListPanel.svelte` | Create | Buddy list UI |
| `src/lib/components/BuddyListPanel.test.ts` | Create | Tests for buddy list |
| `src/lib/components/SocialPrompt.svelte` | Create | Contextual interaction menu for remote players |
| `src/lib/components/SocialPrompt.test.ts` | Create | Tests for social prompt |
| `src/lib/components/EmoteAnimation.svelte` | Create | Floating emote variant animation |
| `src/App.svelte` | Modify | Wire MoodHud, PartyPanel, BuddyListPanel, SocialPrompt, EmoteAnimation |
| `src-tauri/Cargo.toml` | Modify | Add `blake3` dependency |

---

### Task 1: Mood module -- types and decay

**Files:**
- Create: `src-tauri/src/mood/mod.rs`
- Create: `src-tauri/src/mood/decay.rs`

- [ ] **Step 1: Add blake3 dependency to Cargo.toml**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
blake3 = "1"
```

- [ ] **Step 2: Create `src-tauri/src/mood/decay.rs` with tests first**

Create `src-tauri/src/mood/decay.rs`:

```rust
/// Three-tier mood decay rates (per-second, converted from per-minute).
/// Faithful to original Glitch asymmetric curve.
const DECAY_HIGH: f64 = 0.015 / 60.0; // 1.5% of max per minute
const DECAY_MID: f64 = 0.005 / 60.0; // 0.5% of max per minute
const DECAY_LOW: f64 = 0.0025 / 60.0; // 0.25% of max per minute

const THRESHOLD_HIGH: f64 = 0.80;
const THRESHOLD_MID: f64 = 0.50;

/// Computes mood decay for a single tick.
/// Returns the new mood value after decay.
pub fn mood_decay(mood: f64, max_mood: f64, dt: f64) -> f64 {
    if max_mood <= 0.0 {
        return mood;
    }
    let pct = mood / max_mood;
    let rate = if pct > THRESHOLD_HIGH {
        DECAY_HIGH
    } else if pct > THRESHOLD_MID {
        DECAY_MID
    } else {
        DECAY_LOW
    };
    (mood - rate * max_mood * dt).max(0.0)
}

/// Returns imagination earning multiplier based on current mood.
/// At 50%+ mood: 1.0 (full earnings)
/// Below 50%: scales from 0.5 at 0% to 1.0 at 50%
pub fn mood_multiplier(mood: f64, max_mood: f64) -> f64 {
    if max_mood <= 0.0 {
        return 1.0;
    }
    let pct = mood / max_mood;
    if pct >= 0.5 {
        1.0
    } else {
        0.5 + pct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_high_tier_applies_fast_rate() {
        // 90% mood => high tier (>80%)
        let mood = 90.0;
        let max_mood = 100.0;
        let dt = 1.0; // 1 second
        let result = mood_decay(mood, max_mood, dt);
        let expected_decay = DECAY_HIGH * max_mood * dt;
        assert!((result - (mood - expected_decay)).abs() < 1e-10);
    }

    #[test]
    fn decay_mid_tier_applies_moderate_rate() {
        // 65% mood => mid tier (50-80%)
        let mood = 65.0;
        let max_mood = 100.0;
        let dt = 1.0;
        let result = mood_decay(mood, max_mood, dt);
        let expected_decay = DECAY_MID * max_mood * dt;
        assert!((result - (mood - expected_decay)).abs() < 1e-10);
    }

    #[test]
    fn decay_low_tier_applies_slow_rate() {
        // 30% mood => low tier (<50%)
        let mood = 30.0;
        let max_mood = 100.0;
        let dt = 1.0;
        let result = mood_decay(mood, max_mood, dt);
        let expected_decay = DECAY_LOW * max_mood * dt;
        assert!((result - (mood - expected_decay)).abs() < 1e-10);
    }

    #[test]
    fn decay_clamped_at_zero() {
        let mood = 0.001;
        let max_mood = 100.0;
        let dt = 100.0; // large dt to force below zero
        let result = mood_decay(mood, max_mood, dt);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn decay_never_negative() {
        let result = mood_decay(0.0, 100.0, 1.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn decay_zero_max_mood_returns_unchanged() {
        let result = mood_decay(50.0, 0.0, 1.0);
        assert_eq!(result, 50.0);
    }

    #[test]
    fn decay_boundary_at_exactly_80_percent() {
        // Exactly 80% should be mid tier (>80% is high, so 80% is mid)
        let mood = 80.0;
        let max_mood = 100.0;
        let dt = 1.0;
        let result = mood_decay(mood, max_mood, dt);
        let expected_decay = DECAY_MID * max_mood * dt;
        assert!((result - (mood - expected_decay)).abs() < 1e-10);
    }

    #[test]
    fn decay_boundary_at_exactly_50_percent() {
        // Exactly 50% should be low tier (>50% is mid, so 50% is low)
        let mood = 50.0;
        let max_mood = 100.0;
        let dt = 1.0;
        let result = mood_decay(mood, max_mood, dt);
        let expected_decay = DECAY_LOW * max_mood * dt;
        assert!((result - (mood - expected_decay)).abs() < 1e-10);
    }

    #[test]
    fn multiplier_full_at_50_percent() {
        assert_eq!(mood_multiplier(50.0, 100.0), 1.0);
    }

    #[test]
    fn multiplier_full_above_50_percent() {
        assert_eq!(mood_multiplier(75.0, 100.0), 1.0);
        assert_eq!(mood_multiplier(100.0, 100.0), 1.0);
    }

    #[test]
    fn multiplier_scales_below_50_percent() {
        // At 25% mood: 0.5 + 0.25 = 0.75
        assert!((mood_multiplier(25.0, 100.0) - 0.75).abs() < 1e-10);
    }

    #[test]
    fn multiplier_minimum_at_zero_mood() {
        // At 0% mood: 0.5 + 0.0 = 0.5
        assert!((mood_multiplier(0.0, 100.0) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn multiplier_zero_max_mood_returns_full() {
        assert_eq!(mood_multiplier(0.0, 0.0), 1.0);
    }
}
```

- [ ] **Step 3: Create `src-tauri/src/mood/mod.rs` with MoodState**

Create `src-tauri/src/mood/mod.rs`:

```rust
pub mod decay;

use serde::{Deserialize, Serialize};

/// Grace period after loading a save (seconds). Mood decay is suppressed during this time.
const MOOD_GRACE_DURATION: f64 = 300.0; // 5 minutes

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoodState {
    pub mood: f64,
    pub max_mood: f64,
    /// Game time at which grace period expires. Decay suppressed until game_time >= this.
    pub mood_grace_until: f64,
}

impl Default for MoodState {
    fn default() -> Self {
        Self {
            mood: 100.0,
            max_mood: 100.0,
            mood_grace_until: MOOD_GRACE_DURATION,
        }
    }
}

impl MoodState {
    /// Create a new MoodState with a grace period starting at the given game time.
    pub fn new_with_grace(mood: f64, max_mood: f64, game_time: f64) -> Self {
        Self {
            mood,
            max_mood,
            mood_grace_until: game_time + MOOD_GRACE_DURATION,
        }
    }

    /// Tick mood decay. Decay is suppressed during dialogue or grace period.
    /// `party_bonus` should be true when in a party with 2+ members on the same street.
    pub fn tick(&mut self, dt: f64, game_time: f64, in_dialogue: bool, party_bonus: bool) {
        // Suppress decay during dialogue
        if in_dialogue {
            return;
        }
        // Suppress decay during grace period
        if game_time < self.mood_grace_until {
            return;
        }
        let mut new_mood = decay::mood_decay(self.mood, self.max_mood, dt);
        // Party bonus: reduce decay by 25%
        if party_bonus {
            let base_decay = self.mood - new_mood;
            new_mood = self.mood - base_decay * 0.75;
            new_mood = new_mood.max(0.0);
        }
        self.mood = new_mood;
    }

    /// Apply a mood change (positive or negative). Clamps to [0.0, max_mood].
    pub fn apply_mood_change(&mut self, delta: f64) {
        self.mood = (self.mood + delta).clamp(0.0, self.max_mood);
    }

    /// Returns the imagination earning multiplier based on current mood.
    pub fn multiplier(&self) -> f64 {
        decay::mood_multiplier(self.mood, self.max_mood)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mood_is_100() {
        let state = MoodState::default();
        assert_eq!(state.mood, 100.0);
        assert_eq!(state.max_mood, 100.0);
    }

    #[test]
    fn apply_positive_change_capped_at_max() {
        let mut state = MoodState::default();
        state.apply_mood_change(50.0);
        assert_eq!(state.mood, 100.0); // capped at max_mood
    }

    #[test]
    fn apply_negative_change_clamped_at_zero() {
        let mut state = MoodState { mood: 5.0, max_mood: 100.0, mood_grace_until: 0.0 };
        state.apply_mood_change(-10.0);
        assert_eq!(state.mood, 0.0);
    }

    #[test]
    fn apply_change_normal() {
        let mut state = MoodState { mood: 50.0, max_mood: 100.0, mood_grace_until: 0.0 };
        state.apply_mood_change(10.0);
        assert_eq!(state.mood, 60.0);
    }

    #[test]
    fn tick_decays_mood() {
        let mut state = MoodState { mood: 90.0, max_mood: 100.0, mood_grace_until: 0.0 };
        let before = state.mood;
        state.tick(1.0, 1.0, false, false);
        assert!(state.mood < before);
    }

    #[test]
    fn tick_suppressed_during_dialogue() {
        let mut state = MoodState { mood: 90.0, max_mood: 100.0, mood_grace_until: 0.0 };
        let before = state.mood;
        state.tick(1.0, 1.0, true, false);
        assert_eq!(state.mood, before);
    }

    #[test]
    fn tick_suppressed_during_grace_period() {
        let mut state = MoodState { mood: 90.0, max_mood: 100.0, mood_grace_until: 300.0 };
        let before = state.mood;
        state.tick(1.0, 100.0, false, false); // game_time 100 < grace_until 300
        assert_eq!(state.mood, before);
    }

    #[test]
    fn tick_resumes_after_grace_period() {
        let mut state = MoodState { mood: 90.0, max_mood: 100.0, mood_grace_until: 300.0 };
        let before = state.mood;
        state.tick(1.0, 301.0, false, false); // game_time 301 >= grace_until 300
        assert!(state.mood < before);
    }

    #[test]
    fn tick_party_bonus_reduces_decay() {
        let mut no_party = MoodState { mood: 90.0, max_mood: 100.0, mood_grace_until: 0.0 };
        let mut with_party = MoodState { mood: 90.0, max_mood: 100.0, mood_grace_until: 0.0 };

        no_party.tick(1.0, 1.0, false, false);
        with_party.tick(1.0, 1.0, false, true);

        // Party member should have less decay (higher mood remaining)
        assert!(with_party.mood > no_party.mood);
        // Party decay should be 75% of normal decay
        let normal_decay = 90.0 - no_party.mood;
        let party_decay = 90.0 - with_party.mood;
        assert!((party_decay - normal_decay * 0.75).abs() < 1e-10);
    }

    #[test]
    fn multiplier_delegates_correctly() {
        let state = MoodState { mood: 25.0, max_mood: 100.0, mood_grace_until: 0.0 };
        assert!((state.multiplier() - 0.75).abs() < 1e-10);
    }

    #[test]
    fn new_with_grace_sets_grace_period() {
        let state = MoodState::new_with_grace(80.0, 100.0, 500.0);
        assert_eq!(state.mood, 80.0);
        assert_eq!(state.max_mood, 100.0);
        assert_eq!(state.mood_grace_until, 800.0); // 500 + 300
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```
cd src-tauri && cargo test mood:: -- --nocapture
```

Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/mood/mod.rs src-tauri/src/mood/decay.rs src-tauri/Cargo.toml
git commit -m "feat(mood): add MoodState with three-tier decay and mood_multiplier"
```

---

### Task 2: Mood integration into GameState

**Files:**
- Modify: `src-tauri/src/engine/state.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing tests for mood on SaveState**

In `src-tauri/src/engine/state.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn save_state_mood_default() {
    let json = r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50}"#;
    let save: SaveState = serde_json::from_str(json).unwrap();
    assert_eq!(save.mood, 100.0);
    assert_eq!(save.max_mood, 100.0);
}

#[test]
fn save_state_mood_round_trip() {
    let json = r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50,"mood":72.5,"maxMood":100.0}"#;
    let save: SaveState = serde_json::from_str(json).unwrap();
    assert!((save.mood - 72.5).abs() < f64::EPSILON);
    assert!((save.max_mood - 100.0).abs() < f64::EPSILON);
    let reserialized = serde_json::to_string(&save).unwrap();
    let restored: SaveState = serde_json::from_str(&reserialized).unwrap();
    assert!((restored.mood - 72.5).abs() < f64::EPSILON);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cd src-tauri && cargo test save_state_mood -- --nocapture
```

Expected: FAIL -- `SaveState` has no field `mood`

- [ ] **Step 3: Add `pub mod mood;` to lib.rs**

In `src-tauri/src/lib.rs`, add to the module declarations (after `pub mod item;`):

```rust
pub mod mood;
```

- [ ] **Step 4: Add mood/max_mood to SaveState**

In `src-tauri/src/engine/state.rs`, add the default function near existing defaults (near `default_energy`, `default_imagination`, etc.):

```rust
fn default_mood() -> f64 {
    100.0
}
```

Add to `SaveState` struct (after `quest_progress` field):

```rust
    #[serde(default = "default_mood")]
    pub mood: f64,
    #[serde(default = "default_mood")]
    pub max_mood: f64,
```

- [ ] **Step 5: Add mood to GameState**

In `src-tauri/src/engine/state.rs`, add to `GameState` struct (after `active_dialogue` field):

```rust
    pub mood: crate::mood::MoodState,
```

- [ ] **Step 6: Add mood/max_mood to RenderFrame**

In `src-tauri/src/engine/state.rs`, add to `RenderFrame` struct (after `quest_progress` field):

```rust
    pub mood: f64,
    pub max_mood: f64,
```

- [ ] **Step 7: Initialize mood in GameState::new()**

In `src-tauri/src/engine/state.rs`, in `GameState::new()`, add to the `Self { ... }` block:

```rust
    mood: crate::mood::MoodState::default(),
```

- [ ] **Step 8: Wire MoodState::tick() into GameState::tick()**

In `src-tauri/src/engine/state.rs`, in the `tick()` method, after the existing passive energy decay line (`self.energy = (self.energy - PASSIVE_ENERGY_DECAY_RATE * dt).max(0.0);`), add:

```rust
    // Mood decay
    let in_dialogue = self.active_dialogue.is_some();
    self.mood.tick(dt, self.game_time, in_dialogue, false);
```

Note: The `party_bonus` parameter is `false` for now; it will be wired in Task 9 when the party system exists.

- [ ] **Step 9: Add mood to RenderFrame construction in tick()**

In `src-tauri/src/engine/state.rs`, in the `tick()` method where `RenderFrame` is constructed, add after the `quest_progress` field:

```rust
    mood: self.mood.mood,
    max_mood: self.mood.max_mood,
```

- [ ] **Step 10: Wire mood into save_state()**

In `src-tauri/src/engine/state.rs`, in `save_state()`, add to the `SaveState { ... }` constructor after `quest_progress`:

```rust
    mood: self.mood.mood,
    max_mood: self.mood.max_mood,
```

- [ ] **Step 11: Wire mood into restore_save()**

In `src-tauri/src/engine/state.rs`, in `restore_save()`, add after existing field restorations:

```rust
    self.mood = crate::mood::MoodState::new_with_grace(save.mood, save.max_mood, self.game_time);
```

- [ ] **Step 12: Run tests to verify they pass**

```
cd src-tauri && cargo test -- --nocapture
```

Expected: ALL PASS (including new save_state_mood tests and all existing tests)

- [ ] **Step 13: Commit**

```bash
git add src-tauri/src/engine/state.rs src-tauri/src/lib.rs
git commit -m "feat(mood): integrate MoodState into GameState, SaveState, RenderFrame"
```

---

### Task 3: mood_value on items + eat integration

**Files:**
- Modify: `src-tauri/src/item/types.rs`
- Modify: `src-tauri/src/item/energy.rs`
- Modify: `src-tauri/src/engine/state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `assets/items.json`

- [ ] **Step 1: Write failing tests for mood_value on ItemDef**

In `src-tauri/src/item/types.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn item_def_with_mood_value() {
    let json = r#"{"name":"Cherry","description":"A cherry.","category":"food","stackLimit":50,"icon":"cherry","baseCost":3,"energyValue":12,"moodValue":3}"#;
    let def: ItemDef = serde_json::from_str(json).unwrap();
    assert_eq!(def.mood_value, Some(3));
}

#[test]
fn item_def_without_mood_value() {
    let json = r#"{"name":"Wood","description":"Wood.","category":"material","stackLimit":50,"icon":"wood","baseCost":4}"#;
    let def: ItemDef = serde_json::from_str(json).unwrap();
    assert_eq!(def.mood_value, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cd src-tauri && cargo test item_def_with_mood_value item_def_without_mood_value -- --nocapture
```

Expected: FAIL -- `ItemDef` has no field `mood_value`

- [ ] **Step 3: Add `mood_value` field to `ItemDef`**

In `src-tauri/src/item/types.rs`, add after `energy_value` in the `ItemDef` struct:

```rust
    #[serde(default)]
    pub mood_value: Option<u32>,
```

- [ ] **Step 4: Update all test helpers that construct `ItemDef` to include `mood_value: None`**

Search the codebase for all `ItemDef { ... }` constructors in test code and add `mood_value: None` (or an appropriate value) to each. This includes helpers in `vendor.rs`, `interaction.rs`, `energy.rs`, `state.rs`, and anywhere else `ItemDef` is constructed in tests. Each construction site needs the new field to compile.

- [ ] **Step 5: Add `mood_value` to `ItemStackFrame`**

In `src-tauri/src/item/types.rs`, add after `energy_value` in the `ItemStackFrame` struct:

```rust
    pub mood_value: Option<u32>,
```

- [ ] **Step 6: Update `build_inventory_frame()` to include mood_value**

In `src-tauri/src/engine/state.rs`, in `build_inventory_frame()`, add to the `ItemStackFrame` constructor after `energy_value`:

```rust
    mood_value: def.and_then(|d| d.mood_value),
```

- [ ] **Step 7: Write failing test for eat returning mood restoration**

In `src-tauri/src/item/energy.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn eat_returns_mood_value() {
    let mut inv = crate::item::inventory::Inventory::new(10);
    let mut defs = std::collections::HashMap::new();
    defs.insert(
        "cherry".to_string(),
        crate::item::types::ItemDef {
            id: "cherry".to_string(),
            name: "Cherry".to_string(),
            description: "A cherry.".to_string(),
            category: "food".to_string(),
            stack_limit: 50,
            icon: "cherry".to_string(),
            base_cost: Some(3),
            energy_value: Some(12),
            mood_value: Some(3),
        },
    );
    inv.add_item("cherry", 5);
    let result = eat("cherry", 500.0, 600.0, &mut inv, &defs);
    assert!(result.is_ok());
    let (new_energy, _energy_gained, mood_gained) = result.unwrap();
    assert_eq!(mood_gained, 3.0);
    assert!((new_energy - 512.0).abs() < f64::EPSILON);
}

#[test]
fn eat_returns_zero_mood_when_no_mood_value() {
    let mut inv = crate::item::inventory::Inventory::new(10);
    let mut defs = std::collections::HashMap::new();
    defs.insert(
        "wood".to_string(),
        crate::item::types::ItemDef {
            id: "wood".to_string(),
            name: "Wood".to_string(),
            description: "Wood.".to_string(),
            category: "material".to_string(),
            stack_limit: 50,
            icon: "wood".to_string(),
            base_cost: Some(4),
            energy_value: Some(10),
            mood_value: None,
        },
    );
    inv.add_item("wood", 1);
    let result = eat("wood", 500.0, 600.0, &mut inv, &defs);
    assert!(result.is_ok());
    let (_new_energy, _energy_gained, mood_gained) = result.unwrap();
    assert_eq!(mood_gained, 0.0);
}
```

- [ ] **Step 8: Run tests to verify they fail**

```
cd src-tauri && cargo test eat_returns_mood -- --nocapture
```

Expected: FAIL -- `eat()` returns `(f64, f64)` not `(f64, f64, f64)`

- [ ] **Step 9: Update `eat()` signature to return mood_gained**

In `src-tauri/src/item/energy.rs`, change the return type of `eat()` from `Result<(f64, f64), String>` to `Result<(f64, f64, f64), String>` where the third element is `mood_gained`:

```rust
pub fn eat(
    item_id: &str,
    energy: f64,
    max_energy: f64,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
) -> Result<(f64, f64, f64), String> {
    let def = item_defs
        .get(item_id)
        .ok_or_else(|| format!("Unknown item: {}", item_id))?;

    let energy_value = def
        .energy_value
        .ok_or_else(|| format!("{} cannot be eaten", def.name))?;

    if !inventory.remove_item(item_id, 1) {
        return Err(format!("No {} in inventory", def.name));
    }

    let gained = energy_value as f64;
    let new_energy = (energy + gained).min(max_energy);
    let mood_gained = def.mood_value.map(|v| v as f64).unwrap_or(0.0);

    Ok((new_energy, gained, mood_gained))
}
```

- [ ] **Step 10: Update all callers of eat() to handle the new return tuple**

In `src-tauri/src/lib.rs`, in the `eat_item` IPC command, update the destructuring from `let (new_energy, gained) = ...` to `let (new_energy, gained, mood_gained) = ...` and apply mood:

```rust
    let (new_energy, gained, mood_gained) = item::energy::eat(
        &item_id,
        state.energy,
        state.max_energy,
        &mut state.inventory,
        &item_defs,
    )
    .map_err(|e| e.to_string())?;

    state.energy = new_energy;
    if mood_gained > 0.0 {
        state.mood.apply_mood_change(mood_gained);
    }
```

Update existing tests in `energy.rs` to destructure three values instead of two.

- [ ] **Step 11: Add `moodValue` to food items in items.json**

Update `assets/items.json` -- add `"moodValue"` to food items that should have it. The key items from the spec:

| Item | moodValue |
|------|-----------|
| cherry | 3 |
| grain | 2 |
| meat | 5 |
| milk | 4 |
| bread | 20 |
| cherry_pie | 30 |
| steak | 25 |
| butter | 15 |

For example, the cherry entry becomes:

```json
"cherry": {
    "name": "Cherry",
    "description": "A basic cherry, freshly picked from a Fruit Tree.",
    "category": "food",
    "stackLimit": 250,
    "icon": "cherry",
    "baseCost": 1,
    "energyValue": 1,
    "moodValue": 3
},
```

Apply `moodValue` to all food items that have `energyValue`. Crafted foods should have proportionally higher mood values (roughly `energyValue * 0.25` for raw foods, `energyValue * 0.3` for crafted foods). Items without `energyValue` do not need `moodValue`.

- [ ] **Step 12: Run all tests**

```
cd src-tauri && cargo test -- --nocapture
```

Expected: ALL PASS

- [ ] **Step 13: Commit**

```bash
git add src-tauri/src/item/types.rs src-tauri/src/item/energy.rs src-tauri/src/engine/state.rs src-tauri/src/lib.rs assets/items.json
git commit -m "feat(mood): add mood_value to items and integrate mood restoration into eat"
```

---

### Task 4: MoodHud frontend

**Files:**
- Create: `src/lib/components/MoodHud.svelte`
- Create: `src/lib/components/MoodHud.test.ts`
- Modify: `src/lib/types.ts`
- Modify: `src/App.svelte`

- [ ] **Step 1: Add mood/maxMood to TypeScript RenderFrame**

In `src/lib/types.ts`, add to the `RenderFrame` interface (after `questProgress`):

```typescript
  mood: number;
  maxMood: number;
```

- [ ] **Step 2: Write MoodHud tests**

Create `src/lib/components/MoodHud.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import MoodHud from './MoodHud.svelte';

describe('MoodHud', () => {
  it('renders bar with correct fill percentage', () => {
    render(MoodHud, { props: { mood: 50, maxMood: 100 } });
    const fill = document.querySelector('.mood-fill') as HTMLElement;
    expect(fill.style.width).toBe('50%');
  });

  it('renders numeric mood value floored', () => {
    render(MoodHud, { props: { mood: 72.8, maxMood: 100 } });
    const amount = document.querySelector('.mood-amount') as HTMLElement;
    expect(amount.textContent).toBe('72');
  });

  it('applies low class when mood below 50%', () => {
    render(MoodHud, { props: { mood: 40, maxMood: 100 } });
    const hud = document.querySelector('.mood-hud') as HTMLElement;
    expect(hud.classList.contains('low')).toBe(true);
  });

  it('does not apply low class when mood at 50%', () => {
    render(MoodHud, { props: { mood: 50, maxMood: 100 } });
    const hud = document.querySelector('.mood-hud') as HTMLElement;
    expect(hud.classList.contains('low')).toBe(false);
  });

  it('caps fill at 100%', () => {
    render(MoodHud, { props: { mood: 120, maxMood: 100 } });
    const fill = document.querySelector('.mood-fill') as HTMLElement;
    expect(fill.style.width).toBe('100%');
  });

  it('handles zero maxMood gracefully', () => {
    render(MoodHud, { props: { mood: 0, maxMood: 0 } });
    const fill = document.querySelector('.mood-fill') as HTMLElement;
    expect(fill.style.width).toBe('0%');
  });

  it('has correct aria label', () => {
    render(MoodHud, { props: { mood: 72, maxMood: 100 } });
    const hud = document.querySelector('.mood-hud') as HTMLElement;
    expect(hud.getAttribute('aria-label')).toBe('Mood: 72 of 100');
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

```
npx vitest run src/lib/components/MoodHud.test.ts
```

Expected: FAIL -- `MoodHud.svelte` does not exist

- [ ] **Step 4: Create MoodHud.svelte**

Create `src/lib/components/MoodHud.svelte`:

```svelte
<script lang="ts">
  let { mood = 0, maxMood = 100 }: { mood: number; maxMood: number } = $props();
  let percent = $derived(maxMood > 0 ? Math.min(100, (mood / maxMood) * 100) : 0);
  let isLow = $derived(mood < maxMood * 0.5);
  let displayMood = $derived(Math.floor(mood));
</script>

<div class="mood-hud" class:low={isLow} role="status" aria-label="Mood: {displayMood} of {maxMood}">
  <span class="mood-icon">😊</span>
  <div class="mood-bar">
    <div class="mood-fill" style="width: {percent}%"></div>
  </div>
  <span class="mood-amount">{displayMood}</span>
</div>

<style>
  .mood-hud {
    position: absolute;
    top: 52px;
    left: 12px;
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    background: rgba(26, 26, 46, 0.85);
    border-radius: 20px;
    color: #e0c0e8;
    font-size: 14px;
    font-family: 'Lato', sans-serif;
    pointer-events: none;
    user-select: none;
    min-width: 120px;
  }
  .mood-icon {
    font-size: 16px;
  }
  .mood-bar {
    flex: 1;
    height: 10px;
    background: rgba(255, 255, 255, 0.12);
    border-radius: 5px;
    overflow: hidden;
    min-width: 60px;
  }
  .mood-fill {
    height: 100%;
    background: linear-gradient(90deg, #c084fc, #e879a8);
    border-radius: 5px;
    transition: width 0.3s ease;
  }
  .mood-hud.low .mood-fill {
    background: linear-gradient(90deg, #9ca3af, #a78baf);
  }
  .mood-hud.low {
    color: #a0a0b0;
  }
  .mood-amount {
    min-width: 24px;
    text-align: right;
    font-weight: 600;
  }
</style>
```

- [ ] **Step 5: Run tests to verify they pass**

```
npx vitest run src/lib/components/MoodHud.test.ts
```

Expected: ALL PASS

- [ ] **Step 6: Wire MoodHud into App.svelte**

In `src/App.svelte`, import and add MoodHud alongside EnergyHud. Add after the EnergyHud component:

```svelte
<MoodHud mood={frame.mood} maxMood={frame.maxMood} />
```

Import at the top:

```typescript
import MoodHud from './lib/components/MoodHud.svelte';
```

- [ ] **Step 7: Run all tests**

```
npx vitest run
```

Expected: ALL PASS

- [ ] **Step 8: Commit**

```bash
git add src/lib/components/MoodHud.svelte src/lib/components/MoodHud.test.ts src/lib/types.ts src/App.svelte
git commit -m "feat(mood): add MoodHud component with purple/pink bar below EnergyHud"
```

---

### Task 5: Emote module -- types, variants, BLAKE3 seeding

**Files:**
- Create: `src-tauri/src/emote/types.rs`
- Create: `src-tauri/src/emote/mod.rs`

- [ ] **Step 1: Create `src-tauri/src/emote/types.rs` with types and tests**

Create `src-tauri/src/emote/types.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// The 11 hi variants faithful to original Glitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiVariant {
    Bats,
    Birds,
    Butterflies,
    Cubes,
    Flowers,
    Hands,
    Hearts,
    Hi,
    Pigs,
    Rocketships,
    Stars,
}

impl HiVariant {
    pub const ALL: [HiVariant; 11] = [
        HiVariant::Bats,
        HiVariant::Birds,
        HiVariant::Butterflies,
        HiVariant::Cubes,
        HiVariant::Flowers,
        HiVariant::Hands,
        HiVariant::Hearts,
        HiVariant::Hi,
        HiVariant::Pigs,
        HiVariant::Rocketships,
        HiVariant::Stars,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            HiVariant::Bats => "bats",
            HiVariant::Birds => "birds",
            HiVariant::Butterflies => "butterflies",
            HiVariant::Cubes => "cubes",
            HiVariant::Flowers => "flowers",
            HiVariant::Hands => "hands",
            HiVariant::Hearts => "hearts",
            HiVariant::Hi => "hi",
            HiVariant::Pigs => "pigs",
            HiVariant::Rocketships => "rocketships",
            HiVariant::Stars => "stars",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmoteType {
    Hi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmoteMessage {
    pub emote_type: EmoteType,
    pub variant: HiVariant,
    pub target: Option<[u8; 16]>,
}

/// Deterministic daily variant assignment using BLAKE3.
/// variant_index = BLAKE3(identity_bytes || "hi-variant" || date_str) mod 11
pub fn daily_variant(identity: &[u8; 16], date: &str) -> HiVariant {
    let mut hasher = blake3::Hasher::new();
    hasher.update(identity);
    hasher.update(b"hi-variant");
    hasher.update(date.as_bytes());
    let hash = hasher.finalize();
    let bytes = hash.as_bytes();
    // Use first 4 bytes as a u32 for modulus
    let index = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % 11;
    HiVariant::ALL[index as usize]
}

/// Emote state tracking for the current session. Ephemeral -- not persisted.
#[derive(Debug, Clone)]
pub struct EmoteState {
    /// Players we have hi'd today (prevents repeat hi to same player).
    pub hi_today: HashSet<[u8; 16]>,
    /// Players who have hi'd us today (prevents repeat mood gains).
    pub hi_received_today: HashSet<[u8; 16]>,
    /// Caught variant from receiving a hi. Overrides daily seed.
    pub caught_variant: Option<HiVariant>,
    /// Our identity for daily variant calculation.
    pub identity: [u8; 16],
    /// The current date string (YYYY-MM-DD) for daily tracking.
    pub current_date: String,
}

impl EmoteState {
    pub fn new(identity: [u8; 16], date: &str) -> Self {
        Self {
            hi_today: HashSet::new(),
            hi_received_today: HashSet::new(),
            caught_variant: None,
            identity,
            current_date: date.to_string(),
        }
    }

    /// Returns the player's active variant: caught variant if set, otherwise daily seed.
    pub fn active_variant(&self) -> HiVariant {
        self.caught_variant
            .unwrap_or_else(|| daily_variant(&self.identity, &self.current_date))
    }

    /// Clears daily state if the date has changed.
    pub fn check_date_change(&mut self, date: &str) {
        if self.current_date != date {
            self.hi_today.clear();
            self.hi_received_today.clear();
            self.caught_variant = None;
            self.current_date = date.to_string();
        }
    }

    /// Check if we can hi a specific player today.
    pub fn can_hi(&self, target: &[u8; 16]) -> bool {
        !self.hi_today.contains(target)
    }

    /// Record that we hi'd a player.
    pub fn record_hi_sent(&mut self, target: [u8; 16]) {
        self.hi_today.insert(target);
    }

    /// Handle an incoming hi emote. Returns mood delta (0, 5, or 10).
    /// - Returns 0 if sender is blocked or already received today
    /// - Returns 5 for normal hi
    /// - Returns 10 for variant match
    pub fn handle_incoming_hi(
        &mut self,
        sender: [u8; 16],
        sender_variant: HiVariant,
        blocked: &[[u8; 16]],
    ) -> f64 {
        // Check if sender is blocked
        if blocked.contains(&sender) {
            return 0.0;
        }
        // Check if already received from this sender today
        if self.hi_received_today.contains(&sender) {
            return 0.0;
        }

        // Check for match BEFORE catching the variant
        let is_match = sender_variant == self.active_variant();

        // Catch the sender's variant
        self.caught_variant = Some(sender_variant);

        // Record receipt
        self.hi_received_today.insert(sender);

        // Return mood delta
        if is_match {
            10.0
        } else {
            5.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_variant_deterministic_same_identity_same_date() {
        let identity = [1u8; 16];
        let date = "2026-04-10";
        let v1 = daily_variant(&identity, date);
        let v2 = daily_variant(&identity, date);
        assert_eq!(v1, v2);
    }

    #[test]
    fn daily_variant_differs_across_dates() {
        let identity = [1u8; 16];
        let v1 = daily_variant(&identity, "2026-04-10");
        let v2 = daily_variant(&identity, "2026-04-11");
        // Could theoretically collide, but extremely unlikely with different dates
        // We test 10 different dates and expect at least 2 distinct variants
        let mut variants = HashSet::new();
        for day in 1..=10 {
            variants.insert(daily_variant(&identity, &format!("2026-04-{:02}", day)));
        }
        assert!(variants.len() >= 2, "Expected at least 2 distinct variants across 10 days");
    }

    #[test]
    fn daily_variant_differs_across_identities() {
        let date = "2026-04-10";
        let mut variants = HashSet::new();
        for i in 0..20u8 {
            let mut identity = [0u8; 16];
            identity[0] = i;
            variants.insert(daily_variant(&identity, date));
        }
        assert!(variants.len() >= 2, "Expected at least 2 distinct variants across 20 identities");
    }

    #[test]
    fn active_variant_uses_daily_seed_by_default() {
        let identity = [42u8; 16];
        let state = EmoteState::new(identity, "2026-04-10");
        let expected = daily_variant(&identity, "2026-04-10");
        assert_eq!(state.active_variant(), expected);
    }

    #[test]
    fn active_variant_uses_caught_variant_when_set() {
        let identity = [42u8; 16];
        let mut state = EmoteState::new(identity, "2026-04-10");
        state.caught_variant = Some(HiVariant::Butterflies);
        assert_eq!(state.active_variant(), HiVariant::Butterflies);
    }

    #[test]
    fn can_hi_returns_true_for_new_target() {
        let state = EmoteState::new([1u8; 16], "2026-04-10");
        assert!(state.can_hi(&[2u8; 16]));
    }

    #[test]
    fn can_hi_returns_false_after_sending() {
        let mut state = EmoteState::new([1u8; 16], "2026-04-10");
        let target = [2u8; 16];
        state.record_hi_sent(target);
        assert!(!state.can_hi(&target));
    }

    #[test]
    fn can_hi_different_player_after_sending() {
        let mut state = EmoteState::new([1u8; 16], "2026-04-10");
        state.record_hi_sent([2u8; 16]);
        assert!(state.can_hi(&[3u8; 16]));
    }

    #[test]
    fn handle_incoming_hi_no_match_gives_5() {
        let mut state = EmoteState::new([1u8; 16], "2026-04-10");
        // Force a known active variant
        state.caught_variant = Some(HiVariant::Hearts);
        let sender = [2u8; 16];
        // Send a different variant
        let delta = state.handle_incoming_hi(sender, HiVariant::Stars, &[]);
        assert_eq!(delta, 5.0);
    }

    #[test]
    fn handle_incoming_hi_match_gives_10() {
        let mut state = EmoteState::new([1u8; 16], "2026-04-10");
        state.caught_variant = Some(HiVariant::Hearts);
        let sender = [2u8; 16];
        let delta = state.handle_incoming_hi(sender, HiVariant::Hearts, &[]);
        assert_eq!(delta, 10.0);
    }

    #[test]
    fn handle_incoming_hi_catches_variant() {
        let mut state = EmoteState::new([1u8; 16], "2026-04-10");
        state.caught_variant = Some(HiVariant::Hearts);
        let sender = [2u8; 16];
        state.handle_incoming_hi(sender, HiVariant::Butterflies, &[]);
        assert_eq!(state.caught_variant, Some(HiVariant::Butterflies));
    }

    #[test]
    fn handle_incoming_hi_blocked_sender_returns_0() {
        let mut state = EmoteState::new([1u8; 16], "2026-04-10");
        let sender = [2u8; 16];
        let blocked = vec![sender];
        let delta = state.handle_incoming_hi(sender, HiVariant::Hearts, &blocked);
        assert_eq!(delta, 0.0);
    }

    #[test]
    fn handle_incoming_hi_duplicate_returns_0() {
        let mut state = EmoteState::new([1u8; 16], "2026-04-10");
        let sender = [2u8; 16];
        state.handle_incoming_hi(sender, HiVariant::Hearts, &[]);
        let delta = state.handle_incoming_hi(sender, HiVariant::Hearts, &[]);
        assert_eq!(delta, 0.0);
    }

    #[test]
    fn date_change_clears_daily_state() {
        let mut state = EmoteState::new([1u8; 16], "2026-04-10");
        state.record_hi_sent([2u8; 16]);
        state.hi_received_today.insert([3u8; 16]);
        state.caught_variant = Some(HiVariant::Hearts);

        state.check_date_change("2026-04-11");

        assert!(state.hi_today.is_empty());
        assert!(state.hi_received_today.is_empty());
        assert!(state.caught_variant.is_none());
        assert_eq!(state.current_date, "2026-04-11");
    }

    #[test]
    fn date_change_no_op_when_same_date() {
        let mut state = EmoteState::new([1u8; 16], "2026-04-10");
        state.record_hi_sent([2u8; 16]);
        state.caught_variant = Some(HiVariant::Hearts);

        state.check_date_change("2026-04-10");

        assert!(!state.hi_today.is_empty());
        assert!(state.caught_variant.is_some());
    }

    #[test]
    fn emote_message_serialization_round_trip() {
        let msg = EmoteMessage {
            emote_type: EmoteType::Hi,
            variant: HiVariant::Butterflies,
            target: Some([42u8; 16]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: EmoteMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.variant, HiVariant::Butterflies);
        assert_eq!(restored.target, Some([42u8; 16]));
    }

    #[test]
    fn emote_message_fits_within_mtu() {
        let msg = EmoteMessage {
            emote_type: EmoteType::Hi,
            variant: HiVariant::Rocketships,
            target: Some([255u8; 16]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        // Typical UDP MTU is ~1200 bytes for QUIC; message should be well under
        assert!(json.len() < 500, "EmoteMessage serialized to {} bytes", json.len());
    }
}
```

- [ ] **Step 2: Create `src-tauri/src/emote/mod.rs`**

Create `src-tauri/src/emote/mod.rs`:

```rust
pub mod types;

pub use types::{
    daily_variant, EmoteMessage, EmoteState, EmoteType, HiVariant,
};
```

- [ ] **Step 3: Run tests to verify they pass**

```
cd src-tauri && cargo test emote:: -- --nocapture
```

Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/emote/mod.rs src-tauri/src/emote/types.rs
git commit -m "feat(emote): add HiVariant enum, BLAKE3 daily seeding, EmoteState with cooldowns"
```

---

### Task 6: Emote message and network integration

**Files:**
- Modify: `src-tauri/src/network/types.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing test for EmoteMessage in NetMessage**

In `src-tauri/src/network/types.rs`, add to the existing `#[cfg(test)] mod tests` block (or create one if none exists):

```rust
#[test]
fn net_message_emote_round_trip() {
    let msg = NetMessage::Emote(crate::emote::EmoteMessage {
        emote_type: crate::emote::EmoteType::Hi,
        variant: crate::emote::HiVariant::Hearts,
        target: Some([1u8; 16]),
    });
    let json = serde_json::to_string(&msg).unwrap();
    let restored: NetMessage = serde_json::from_str(&json).unwrap();
    match restored {
        NetMessage::Emote(e) => {
            assert_eq!(e.variant, crate::emote::HiVariant::Hearts);
            assert_eq!(e.target, Some([1u8; 16]));
        }
        _ => panic!("Expected Emote variant"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cd src-tauri && cargo test net_message_emote -- --nocapture
```

Expected: FAIL -- `NetMessage` has no `Emote` variant

- [ ] **Step 3: Add `pub mod emote;` to lib.rs**

In `src-tauri/src/lib.rs`, add to the module declarations:

```rust
pub mod emote;
```

- [ ] **Step 4: Add Emote variant to NetMessage**

In `src-tauri/src/network/types.rs`, add to the `NetMessage` enum:

```rust
    Emote(crate::emote::EmoteMessage),
```

- [ ] **Step 5: Run tests to verify they pass**

```
cd src-tauri && cargo test net_message_emote -- --nocapture
```

Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/network/types.rs src-tauri/src/lib.rs
git commit -m "feat(emote): add Emote variant to NetMessage for emote broadcasting"
```

---

### Task 7: Social module -- buddy types and logic

**Files:**
- Create: `src-tauri/src/social/types.rs`
- Create: `src-tauri/src/social/buddy.rs`
- Create: `src-tauri/src/social/mod.rs` (partial -- completed in Task 9)

- [ ] **Step 1: Create `src-tauri/src/social/types.rs`**

Create `src-tauri/src/social/types.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Messages for social interactions (buddy requests, party management).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialMessage {
    BuddyRequest { from: [u8; 16] },
    BuddyAccept { from: [u8; 16] },
    BuddyDecline { from: [u8; 16] },
    BuddyRemove { from: [u8; 16] },
    PartyInvite { leader: [u8; 16], members: Vec<[u8; 16]> },
    PartyAccept { from: [u8; 16] },
    PartyDecline { from: [u8; 16] },
    PartyLeave { from: [u8; 16] },
    PartyKick { target: [u8; 16] },
    PartyMemberJoined { member: [u8; 16], display_name: String },
    PartyMemberLeft { member: [u8; 16] },
    PartyDissolved,
    PartyLeaderChanged { new_leader: [u8; 16] },
}

/// Serialized buddy entry for SaveState persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuddySaveEntry {
    pub address_hash: String,
    pub display_name: String,
    pub added_date: String,
    pub co_presence_total: f64,
    pub last_seen_date: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn social_message_buddy_request_round_trip() {
        let msg = SocialMessage::BuddyRequest { from: [1u8; 16] };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: SocialMessage = serde_json::from_str(&json).unwrap();
        match restored {
            SocialMessage::BuddyRequest { from } => assert_eq!(from, [1u8; 16]),
            _ => panic!("Expected BuddyRequest"),
        }
    }

    #[test]
    fn social_message_party_invite_round_trip() {
        let msg = SocialMessage::PartyInvite {
            leader: [1u8; 16],
            members: vec![[2u8; 16], [3u8; 16]],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: SocialMessage = serde_json::from_str(&json).unwrap();
        match restored {
            SocialMessage::PartyInvite { leader, members } => {
                assert_eq!(leader, [1u8; 16]);
                assert_eq!(members.len(), 2);
            }
            _ => panic!("Expected PartyInvite"),
        }
    }

    #[test]
    fn buddy_save_entry_round_trip() {
        let entry = BuddySaveEntry {
            address_hash: "abcdef1234567890".to_string(),
            display_name: "TestBuddy".to_string(),
            added_date: "2026-04-10".to_string(),
            co_presence_total: 3600.0,
            last_seen_date: Some("2026-04-10".to_string()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let restored: BuddySaveEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.display_name, "TestBuddy");
        assert!((restored.co_presence_total - 3600.0).abs() < f64::EPSILON);
    }
}
```

- [ ] **Step 2: Create `src-tauri/src/social/buddy.rs`**

Create `src-tauri/src/social/buddy.rs`:

```rust
use serde::{Deserialize, Serialize};

use super::types::BuddySaveEntry;

/// A buddy entry in the local buddy list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuddyEntry {
    pub address_hash: [u8; 16],
    pub display_name: String,
    pub added_date: String,
    pub co_presence_total: f64,
    pub last_seen_date: Option<String>,
}

impl BuddyEntry {
    /// Convert to serializable save entry (hex-encoded address).
    pub fn to_save_entry(&self) -> BuddySaveEntry {
        BuddySaveEntry {
            address_hash: hex::encode(self.address_hash),
            display_name: self.display_name.clone(),
            added_date: self.added_date.clone(),
            co_presence_total: self.co_presence_total,
            last_seen_date: self.last_seen_date.clone(),
        }
    }

    /// Restore from a save entry (hex-decoded address).
    pub fn from_save_entry(save: &BuddySaveEntry) -> Option<Self> {
        let bytes = hex::decode(&save.address_hash).ok()?;
        if bytes.len() != 16 {
            return None;
        }
        let mut address_hash = [0u8; 16];
        address_hash.copy_from_slice(&bytes);
        Some(Self {
            address_hash,
            display_name: save.display_name.clone(),
            added_date: save.added_date.clone(),
            co_presence_total: save.co_presence_total,
            last_seen_date: save.last_seen_date.clone(),
        })
    }
}

/// Pending buddy request with timeout tracking.
#[derive(Debug, Clone)]
pub struct PendingBuddyRequest {
    pub from: [u8; 16],
    pub from_name: String,
    pub received_at: f64,
}

/// Buddy system state.
#[derive(Debug, Clone)]
pub struct BuddyState {
    pub buddies: Vec<BuddyEntry>,
    pub blocked: Vec<[u8; 16]>,
    pub pending_requests: Vec<PendingBuddyRequest>,
}

/// Timeout for pending buddy requests (90 seconds).
const BUDDY_REQUEST_TIMEOUT: f64 = 90.0;

impl Default for BuddyState {
    fn default() -> Self {
        Self {
            buddies: Vec::new(),
            blocked: Vec::new(),
            pending_requests: Vec::new(),
        }
    }
}

impl BuddyState {
    /// Check if a player is a buddy.
    pub fn is_buddy(&self, address: &[u8; 16]) -> bool {
        self.buddies.iter().any(|b| &b.address_hash == address)
    }

    /// Check if a player is blocked.
    pub fn is_blocked(&self, address: &[u8; 16]) -> bool {
        self.blocked.contains(address)
    }

    /// Add a buddy. Returns Err if already a buddy or if the player is blocked.
    pub fn add_buddy(
        &mut self,
        address_hash: [u8; 16],
        display_name: &str,
        date: &str,
    ) -> Result<(), String> {
        if self.is_buddy(&address_hash) {
            return Err("Already a buddy".to_string());
        }
        if self.is_blocked(&address_hash) {
            return Err("Player is blocked".to_string());
        }
        self.buddies.push(BuddyEntry {
            address_hash,
            display_name: display_name.to_string(),
            added_date: date.to_string(),
            co_presence_total: 0.0,
            last_seen_date: Some(date.to_string()),
        });
        // Remove any pending request from this player
        self.pending_requests.retain(|r| r.from != address_hash);
        Ok(())
    }

    /// Remove a buddy. Returns true if the buddy was found and removed.
    pub fn remove_buddy(&mut self, address_hash: &[u8; 16]) -> bool {
        let before = self.buddies.len();
        self.buddies.retain(|b| &b.address_hash != address_hash);
        self.buddies.len() < before
    }

    /// Block a player. If they are a buddy, the buddy entry is removed first.
    pub fn block_player(&mut self, address_hash: [u8; 16]) {
        // Remove buddy entry if present
        self.remove_buddy(&address_hash);
        // Remove any pending request from this player
        self.pending_requests.retain(|r| r.from != address_hash);
        // Add to blocked list (avoid duplicates)
        if !self.is_blocked(&address_hash) {
            self.blocked.push(address_hash);
        }
    }

    /// Unblock a player.
    pub fn unblock_player(&mut self, address_hash: &[u8; 16]) {
        self.blocked.retain(|b| b != address_hash);
    }

    /// Add a pending buddy request. Returns Err if the sender is blocked or already a buddy.
    pub fn add_pending_request(
        &mut self,
        from: [u8; 16],
        from_name: &str,
        game_time: f64,
    ) -> Result<(), String> {
        if self.is_blocked(&from) {
            return Err("Sender is blocked".to_string());
        }
        if self.is_buddy(&from) {
            return Err("Already a buddy".to_string());
        }
        // Replace existing pending request from same player
        self.pending_requests.retain(|r| r.from != from);
        self.pending_requests.push(PendingBuddyRequest {
            from,
            from_name: from_name.to_string(),
            received_at: game_time,
        });
        Ok(())
    }

    /// Get a pending request from a specific player, if it exists and hasn't expired.
    pub fn get_pending_request(&self, from: &[u8; 16], game_time: f64) -> Option<&PendingBuddyRequest> {
        self.pending_requests
            .iter()
            .find(|r| &r.from == from && (game_time - r.received_at) < BUDDY_REQUEST_TIMEOUT)
    }

    /// Remove expired pending requests.
    pub fn expire_requests(&mut self, game_time: f64) {
        self.pending_requests
            .retain(|r| (game_time - r.received_at) < BUDDY_REQUEST_TIMEOUT);
    }

    /// Update display name for a buddy when encountered on a street.
    pub fn update_buddy_name(&mut self, address_hash: &[u8; 16], display_name: &str, date: &str) {
        if let Some(buddy) = self.buddies.iter_mut().find(|b| &b.address_hash == address_hash) {
            buddy.display_name = display_name.to_string();
            buddy.last_seen_date = Some(date.to_string());
        }
    }

    /// Accumulate co-presence time with a buddy.
    pub fn record_copresence(&mut self, address_hash: &[u8; 16], dt: f64) {
        if let Some(buddy) = self.buddies.iter_mut().find(|b| &b.address_hash == address_hash) {
            buddy.co_presence_total += dt;
        }
    }

    /// Convert all buddies to save entries.
    pub fn to_save_entries(&self) -> Vec<BuddySaveEntry> {
        self.buddies.iter().map(|b| b.to_save_entry()).collect()
    }

    /// Convert blocked list to hex strings for saving.
    pub fn blocked_to_hex(&self) -> Vec<String> {
        self.blocked.iter().map(hex::encode).collect()
    }

    /// Restore buddies from save entries.
    pub fn restore_from_save(
        buddies_save: &[BuddySaveEntry],
        blocked_hex: &[String],
    ) -> Self {
        let buddies: Vec<BuddyEntry> = buddies_save
            .iter()
            .filter_map(BuddyEntry::from_save_entry)
            .collect();
        let blocked: Vec<[u8; 16]> = blocked_hex
            .iter()
            .filter_map(|h| {
                let bytes = hex::decode(h).ok()?;
                if bytes.len() != 16 {
                    return None;
                }
                let mut arr = [0u8; 16];
                arr.copy_from_slice(&bytes);
                Some(arr)
            })
            .collect();
        Self {
            buddies,
            blocked,
            pending_requests: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_buddy_success() {
        let mut state = BuddyState::default();
        let result = state.add_buddy([1u8; 16], "Alice", "2026-04-10");
        assert!(result.is_ok());
        assert!(state.is_buddy(&[1u8; 16]));
    }

    #[test]
    fn add_buddy_duplicate_rejected() {
        let mut state = BuddyState::default();
        state.add_buddy([1u8; 16], "Alice", "2026-04-10").unwrap();
        let result = state.add_buddy([1u8; 16], "Alice", "2026-04-10");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Already a buddy");
    }

    #[test]
    fn add_buddy_blocked_rejected() {
        let mut state = BuddyState::default();
        state.block_player([1u8; 16]);
        let result = state.add_buddy([1u8; 16], "Alice", "2026-04-10");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Player is blocked");
    }

    #[test]
    fn remove_buddy_success() {
        let mut state = BuddyState::default();
        state.add_buddy([1u8; 16], "Alice", "2026-04-10").unwrap();
        assert!(state.remove_buddy(&[1u8; 16]));
        assert!(!state.is_buddy(&[1u8; 16]));
    }

    #[test]
    fn remove_buddy_not_found() {
        let mut state = BuddyState::default();
        assert!(!state.remove_buddy(&[1u8; 16]));
    }

    #[test]
    fn block_removes_buddy_entry() {
        let mut state = BuddyState::default();
        state.add_buddy([1u8; 16], "Alice", "2026-04-10").unwrap();
        state.block_player([1u8; 16]);
        assert!(!state.is_buddy(&[1u8; 16]));
        assert!(state.is_blocked(&[1u8; 16]));
    }

    #[test]
    fn block_removes_pending_request() {
        let mut state = BuddyState::default();
        state.add_pending_request([1u8; 16], "Alice", 0.0).unwrap();
        state.block_player([1u8; 16]);
        assert!(state.get_pending_request(&[1u8; 16], 0.0).is_none());
    }

    #[test]
    fn block_idempotent() {
        let mut state = BuddyState::default();
        state.block_player([1u8; 16]);
        state.block_player([1u8; 16]);
        assert_eq!(state.blocked.len(), 1);
    }

    #[test]
    fn unblock_player() {
        let mut state = BuddyState::default();
        state.block_player([1u8; 16]);
        state.unblock_player(&[1u8; 16]);
        assert!(!state.is_blocked(&[1u8; 16]));
    }

    #[test]
    fn pending_request_from_blocked_rejected() {
        let mut state = BuddyState::default();
        state.block_player([1u8; 16]);
        let result = state.add_pending_request([1u8; 16], "Alice", 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn pending_request_from_existing_buddy_rejected() {
        let mut state = BuddyState::default();
        state.add_buddy([1u8; 16], "Alice", "2026-04-10").unwrap();
        let result = state.add_pending_request([1u8; 16], "Alice", 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn pending_request_expires_after_90_seconds() {
        let mut state = BuddyState::default();
        state.add_pending_request([1u8; 16], "Alice", 0.0).unwrap();
        assert!(state.get_pending_request(&[1u8; 16], 89.0).is_some());
        assert!(state.get_pending_request(&[1u8; 16], 90.0).is_none());
    }

    #[test]
    fn expire_requests_removes_old() {
        let mut state = BuddyState::default();
        state.add_pending_request([1u8; 16], "Alice", 0.0).unwrap();
        state.add_pending_request([2u8; 16], "Bob", 50.0).unwrap();
        state.expire_requests(91.0);
        assert_eq!(state.pending_requests.len(), 1);
        assert_eq!(state.pending_requests[0].from, [2u8; 16]);
    }

    #[test]
    fn update_buddy_name() {
        let mut state = BuddyState::default();
        state.add_buddy([1u8; 16], "Alice", "2026-04-10").unwrap();
        state.update_buddy_name(&[1u8; 16], "Alice2", "2026-04-11");
        assert_eq!(state.buddies[0].display_name, "Alice2");
        assert_eq!(state.buddies[0].last_seen_date, Some("2026-04-11".to_string()));
    }

    #[test]
    fn record_copresence_accumulates() {
        let mut state = BuddyState::default();
        state.add_buddy([1u8; 16], "Alice", "2026-04-10").unwrap();
        state.record_copresence(&[1u8; 16], 10.0);
        state.record_copresence(&[1u8; 16], 5.0);
        assert!((state.buddies[0].co_presence_total - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn save_restore_round_trip() {
        let mut state = BuddyState::default();
        state.add_buddy([1u8; 16], "Alice", "2026-04-10").unwrap();
        state.add_buddy([2u8; 16], "Bob", "2026-04-10").unwrap();
        state.block_player([3u8; 16]);

        let save_entries = state.to_save_entries();
        let blocked_hex = state.blocked_to_hex();

        let restored = BuddyState::restore_from_save(&save_entries, &blocked_hex);
        assert_eq!(restored.buddies.len(), 2);
        assert!(restored.is_buddy(&[1u8; 16]));
        assert!(restored.is_buddy(&[2u8; 16]));
        assert!(restored.is_blocked(&[3u8; 16]));
        assert!(restored.pending_requests.is_empty()); // ephemeral, not restored
    }

    #[test]
    fn buddy_entry_to_from_save_entry() {
        let entry = BuddyEntry {
            address_hash: [0xAB; 16],
            display_name: "Test".to_string(),
            added_date: "2026-04-10".to_string(),
            co_presence_total: 100.0,
            last_seen_date: Some("2026-04-10".to_string()),
        };
        let save = entry.to_save_entry();
        let restored = BuddyEntry::from_save_entry(&save).unwrap();
        assert_eq!(restored.address_hash, [0xAB; 16]);
        assert_eq!(restored.display_name, "Test");
    }
}
```

- [ ] **Step 3: Create initial `src-tauri/src/social/mod.rs`**

Create `src-tauri/src/social/mod.rs` (partial -- party and aggregator added in later tasks):

```rust
pub mod buddy;
pub mod types;

pub use buddy::BuddyState;
pub use types::{BuddySaveEntry, SocialMessage};
```

- [ ] **Step 4: Add `hex` dependency to Cargo.toml**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
hex = "0.4"
```

- [ ] **Step 5: Run tests to verify they pass**

```
cd src-tauri && cargo test social:: -- --nocapture
```

Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/social/mod.rs src-tauri/src/social/types.rs src-tauri/src/social/buddy.rs src-tauri/Cargo.toml
git commit -m "feat(social): add BuddyState with mutual-witness add, block list, and persistence"
```

---

### Task 8: Social module -- party types and logic

**Files:**
- Create: `src-tauri/src/social/party.rs`
- Modify: `src-tauri/src/social/mod.rs`

- [ ] **Step 1: Create `src-tauri/src/social/party.rs` with types and tests**

Create `src-tauri/src/social/party.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Maximum number of members in a party.
pub const MAX_PARTY_SIZE: usize = 5;

/// Timeout for pending party invites (90 seconds).
const PARTY_INVITE_TIMEOUT: f64 = 90.0;

/// Grace period for street transitions before party dissolves (30 seconds).
pub const PARTY_GRACE_PERIOD: f64 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyRole {
    Leader,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyMember {
    pub address_hash: [u8; 16],
    pub display_name: String,
    pub joined_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveParty {
    pub leader: [u8; 16],
    pub members: Vec<PartyMember>,
    pub created_at: f64,
}

#[derive(Debug, Clone)]
pub struct PendingPartyInvite {
    pub leader: [u8; 16],
    pub leader_name: String,
    pub members: Vec<[u8; 16]>,
    pub received_at: f64,
}

/// Party system state. Ephemeral -- not persisted.
#[derive(Debug, Clone, Default)]
pub struct PartyState {
    pub party: Option<ActiveParty>,
    pub pending_invite: Option<PendingPartyInvite>,
}

impl ActiveParty {
    /// Create a new party with the given leader.
    pub fn new(leader: [u8; 16], leader_name: &str, game_time: f64) -> Self {
        Self {
            leader,
            members: vec![PartyMember {
                address_hash: leader,
                display_name: leader_name.to_string(),
                joined_at: game_time,
            }],
            created_at: game_time,
        }
    }

    /// Check if a player is in this party.
    pub fn is_member(&self, address: &[u8; 16]) -> bool {
        self.members.iter().any(|m| &m.address_hash == address)
    }

    /// Check if a player is the leader.
    pub fn is_leader(&self, address: &[u8; 16]) -> bool {
        &self.leader == address
    }

    /// Get the role of a player in this party.
    pub fn role_of(&self, address: &[u8; 16]) -> Option<PartyRole> {
        if !self.is_member(address) {
            return None;
        }
        if self.is_leader(address) {
            Some(PartyRole::Leader)
        } else {
            Some(PartyRole::Member)
        }
    }

    /// Add a member. Returns Err if party is full or member already exists.
    pub fn add_member(
        &mut self,
        address_hash: [u8; 16],
        display_name: &str,
        game_time: f64,
    ) -> Result<(), String> {
        if self.members.len() >= MAX_PARTY_SIZE {
            return Err("Party is full".to_string());
        }
        if self.is_member(&address_hash) {
            return Err("Already in party".to_string());
        }
        self.members.push(PartyMember {
            address_hash,
            display_name: display_name.to_string(),
            joined_at: game_time,
        });
        Ok(())
    }

    /// Remove a member. Returns the remaining member count.
    /// If the leader leaves, transfers to the longest-tenured remaining member.
    /// Returns (remaining_count, leader_changed_to)
    pub fn remove_member(&mut self, address: &[u8; 16]) -> (usize, Option<[u8; 16]>) {
        self.members.retain(|m| &m.address_hash != address);
        let mut new_leader = None;

        // If the removed member was the leader, transfer leadership
        if &self.leader == address && !self.members.is_empty() {
            // Find longest-tenured (smallest joined_at)
            let oldest = self
                .members
                .iter()
                .min_by(|a, b| a.joined_at.partial_cmp(&b.joined_at).unwrap())
                .unwrap();
            self.leader = oldest.address_hash;
            new_leader = Some(self.leader);
        }

        (self.members.len(), new_leader)
    }

    /// Kick a member (leader-only action). Returns Err if not leader or target not found.
    pub fn kick_member(
        &mut self,
        kicker: &[u8; 16],
        target: &[u8; 16],
    ) -> Result<(), String> {
        if !self.is_leader(kicker) {
            return Err("Only the leader can kick".to_string());
        }
        if !self.is_member(target) {
            return Err("Player not in party".to_string());
        }
        if kicker == target {
            return Err("Cannot kick yourself".to_string());
        }
        self.members.retain(|m| &m.address_hash != target);
        Ok(())
    }

    /// Get all member address hashes.
    pub fn member_hashes(&self) -> Vec<[u8; 16]> {
        self.members.iter().map(|m| m.address_hash).collect()
    }
}

impl PartyState {
    /// Check if we're currently in a party.
    pub fn in_party(&self) -> bool {
        self.party.is_some()
    }

    /// Check if we're in a party with 2+ members (for mood bonus).
    pub fn has_party_bonus(&self) -> bool {
        self.party
            .as_ref()
            .map(|p| p.members.len() >= 2)
            .unwrap_or(false)
    }

    /// Create a party with us as leader (for inviting when not in a party).
    pub fn create_party(
        &mut self,
        our_address: [u8; 16],
        our_name: &str,
        game_time: f64,
    ) -> Result<(), String> {
        if self.party.is_some() {
            return Err("Already in a party".to_string());
        }
        self.party = Some(ActiveParty::new(our_address, our_name, game_time));
        Ok(())
    }

    /// Accept a pending invite and join the party.
    pub fn accept_invite(
        &mut self,
        our_address: [u8; 16],
        our_name: &str,
        game_time: f64,
    ) -> Result<[u8; 16], String> {
        let invite = self
            .pending_invite
            .take()
            .ok_or("No pending invite")?;
        if (game_time - invite.received_at) >= PARTY_INVITE_TIMEOUT {
            return Err("Invite has expired".to_string());
        }
        // Create party from the invite info
        let mut party = ActiveParty {
            leader: invite.leader,
            members: Vec::new(),
            created_at: invite.received_at,
        };
        // Add leader placeholder (will be updated by leader's broadcasts)
        party.members.push(PartyMember {
            address_hash: invite.leader,
            display_name: invite.leader_name.clone(),
            joined_at: invite.received_at,
        });
        // Add existing members from invite
        for member_hash in &invite.members {
            if member_hash != &invite.leader {
                party.members.push(PartyMember {
                    address_hash: *member_hash,
                    display_name: String::new(), // will be updated
                    joined_at: invite.received_at,
                });
            }
        }
        // Add ourselves
        party.members.push(PartyMember {
            address_hash: our_address,
            display_name: our_name.to_string(),
            joined_at: game_time,
        });
        let leader = invite.leader;
        self.party = Some(party);
        Ok(leader)
    }

    /// Decline a pending invite.
    pub fn decline_invite(&mut self) -> Result<[u8; 16], String> {
        let invite = self
            .pending_invite
            .take()
            .ok_or("No pending invite")?;
        Ok(invite.leader)
    }

    /// Leave the current party. Returns member hashes to notify, plus whether party dissolved.
    pub fn leave_party(
        &mut self,
        our_address: &[u8; 16],
    ) -> Result<(Vec<[u8; 16]>, bool, Option<[u8; 16]>), String> {
        let party = self.party.as_mut().ok_or("Not in a party")?;
        let members_to_notify: Vec<[u8; 16]> = party
            .members
            .iter()
            .filter(|m| &m.address_hash != our_address)
            .map(|m| m.address_hash)
            .collect();

        let (remaining, new_leader) = party.remove_member(our_address);

        let dissolved = remaining <= 1;
        if dissolved || remaining == 0 {
            self.party = None;
        }

        Ok((members_to_notify, dissolved, new_leader))
    }

    /// Set a pending invite. Returns Err if already in a party.
    pub fn set_pending_invite(
        &mut self,
        leader: [u8; 16],
        leader_name: &str,
        members: Vec<[u8; 16]>,
        game_time: f64,
    ) -> Result<(), String> {
        if self.party.is_some() {
            return Err("Already in a party".to_string());
        }
        self.pending_invite = Some(PendingPartyInvite {
            leader,
            leader_name: leader_name.to_string(),
            members,
            received_at: game_time,
        });
        Ok(())
    }

    /// Check if the pending invite has expired and clear it if so.
    pub fn expire_invite(&mut self, game_time: f64) {
        if let Some(invite) = &self.pending_invite {
            if (game_time - invite.received_at) >= PARTY_INVITE_TIMEOUT {
                self.pending_invite = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_party_makes_leader() {
        let mut state = PartyState::default();
        state.create_party([1u8; 16], "Alice", 0.0).unwrap();
        assert!(state.in_party());
        let party = state.party.as_ref().unwrap();
        assert!(party.is_leader(&[1u8; 16]));
        assert_eq!(party.members.len(), 1);
    }

    #[test]
    fn create_party_when_already_in_party_rejected() {
        let mut state = PartyState::default();
        state.create_party([1u8; 16], "Alice", 0.0).unwrap();
        let result = state.create_party([1u8; 16], "Alice", 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn add_member_to_party() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        party.add_member([2u8; 16], "Bob", 1.0).unwrap();
        assert_eq!(party.members.len(), 2);
        assert!(party.is_member(&[2u8; 16]));
    }

    #[test]
    fn add_member_max_size_enforced() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        for i in 2..=5u8 {
            let mut addr = [0u8; 16];
            addr[0] = i;
            party.add_member(addr, &format!("Player{}", i), i as f64).unwrap();
        }
        assert_eq!(party.members.len(), 5);
        let result = party.add_member([6u8; 16], "Player6", 6.0);
        assert_eq!(result.unwrap_err(), "Party is full");
    }

    #[test]
    fn add_duplicate_member_rejected() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        party.add_member([2u8; 16], "Bob", 1.0).unwrap();
        let result = party.add_member([2u8; 16], "Bob", 2.0);
        assert_eq!(result.unwrap_err(), "Already in party");
    }

    #[test]
    fn remove_member_returns_count() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        party.add_member([2u8; 16], "Bob", 1.0).unwrap();
        let (remaining, new_leader) = party.remove_member(&[2u8; 16]);
        assert_eq!(remaining, 1);
        assert!(new_leader.is_none());
    }

    #[test]
    fn leader_leave_transfers_leadership() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        party.add_member([2u8; 16], "Bob", 1.0).unwrap();
        party.add_member([3u8; 16], "Charlie", 2.0).unwrap();
        let (remaining, new_leader) = party.remove_member(&[1u8; 16]);
        assert_eq!(remaining, 2);
        // Bob joined first (lowest joined_at), so Bob becomes leader
        assert_eq!(new_leader, Some([2u8; 16]));
        assert!(party.is_leader(&[2u8; 16]));
    }

    #[test]
    fn kick_by_leader_success() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        party.add_member([2u8; 16], "Bob", 1.0).unwrap();
        party.kick_member(&[1u8; 16], &[2u8; 16]).unwrap();
        assert!(!party.is_member(&[2u8; 16]));
    }

    #[test]
    fn kick_by_non_leader_rejected() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        party.add_member([2u8; 16], "Bob", 1.0).unwrap();
        let result = party.kick_member(&[2u8; 16], &[1u8; 16]);
        assert_eq!(result.unwrap_err(), "Only the leader can kick");
    }

    #[test]
    fn kick_self_rejected() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        let result = party.kick_member(&[1u8; 16], &[1u8; 16]);
        assert_eq!(result.unwrap_err(), "Cannot kick yourself");
    }

    #[test]
    fn kick_nonexistent_rejected() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        let result = party.kick_member(&[1u8; 16], &[99u8; 16]);
        assert_eq!(result.unwrap_err(), "Player not in party");
    }

    #[test]
    fn leave_party_dissolves_when_one_remains() {
        let mut state = PartyState::default();
        state.create_party([1u8; 16], "Alice", 0.0).unwrap();
        state.party.as_mut().unwrap().add_member([2u8; 16], "Bob", 1.0).unwrap();
        let (notified, dissolved, _) = state.leave_party(&[1u8; 16]).unwrap();
        assert!(dissolved);
        assert!(state.party.is_none());
        assert_eq!(notified, vec![[2u8; 16]]);
    }

    #[test]
    fn leave_party_not_in_party_error() {
        let mut state = PartyState::default();
        let result = state.leave_party(&[1u8; 16]);
        assert!(result.is_err());
    }

    #[test]
    fn has_party_bonus_with_two_members() {
        let mut state = PartyState::default();
        state.create_party([1u8; 16], "Alice", 0.0).unwrap();
        assert!(!state.has_party_bonus()); // only 1 member
        state.party.as_mut().unwrap().add_member([2u8; 16], "Bob", 1.0).unwrap();
        assert!(state.has_party_bonus()); // 2 members
    }

    #[test]
    fn has_party_bonus_no_party() {
        let state = PartyState::default();
        assert!(!state.has_party_bonus());
    }

    #[test]
    fn role_of_leader() {
        let party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        assert_eq!(party.role_of(&[1u8; 16]), Some(PartyRole::Leader));
    }

    #[test]
    fn role_of_member() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        party.add_member([2u8; 16], "Bob", 1.0).unwrap();
        assert_eq!(party.role_of(&[2u8; 16]), Some(PartyRole::Member));
    }

    #[test]
    fn role_of_non_member() {
        let party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        assert_eq!(party.role_of(&[99u8; 16]), None);
    }

    #[test]
    fn pending_invite_set_and_accept() {
        let mut state = PartyState::default();
        state.set_pending_invite([1u8; 16], "Alice", vec![[1u8; 16]], 0.0).unwrap();
        assert!(state.pending_invite.is_some());
        let leader = state.accept_invite([2u8; 16], "Bob", 1.0).unwrap();
        assert_eq!(leader, [1u8; 16]);
        assert!(state.in_party());
    }

    #[test]
    fn pending_invite_set_while_in_party_rejected() {
        let mut state = PartyState::default();
        state.create_party([1u8; 16], "Alice", 0.0).unwrap();
        let result = state.set_pending_invite([2u8; 16], "Bob", vec![[2u8; 16]], 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn pending_invite_expires_after_90_seconds() {
        let mut state = PartyState::default();
        state.set_pending_invite([1u8; 16], "Alice", vec![[1u8; 16]], 0.0).unwrap();
        state.expire_invite(90.0);
        assert!(state.pending_invite.is_none());
    }

    #[test]
    fn pending_invite_not_expired_before_90_seconds() {
        let mut state = PartyState::default();
        state.set_pending_invite([1u8; 16], "Alice", vec![[1u8; 16]], 0.0).unwrap();
        state.expire_invite(89.0);
        assert!(state.pending_invite.is_some());
    }

    #[test]
    fn accept_expired_invite_rejected() {
        let mut state = PartyState::default();
        state.set_pending_invite([1u8; 16], "Alice", vec![[1u8; 16]], 0.0).unwrap();
        let result = state.accept_invite([2u8; 16], "Bob", 91.0);
        assert_eq!(result.unwrap_err(), "Invite has expired");
    }

    #[test]
    fn decline_invite_returns_leader() {
        let mut state = PartyState::default();
        state.set_pending_invite([1u8; 16], "Alice", vec![[1u8; 16]], 0.0).unwrap();
        let leader = state.decline_invite().unwrap();
        assert_eq!(leader, [1u8; 16]);
        assert!(state.pending_invite.is_none());
    }

    #[test]
    fn decline_no_invite_error() {
        let mut state = PartyState::default();
        let result = state.decline_invite();
        assert!(result.is_err());
    }

    #[test]
    fn member_hashes_returns_all() {
        let mut party = ActiveParty::new([1u8; 16], "Alice", 0.0);
        party.add_member([2u8; 16], "Bob", 1.0).unwrap();
        let hashes = party.member_hashes();
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains(&[1u8; 16]));
        assert!(hashes.contains(&[2u8; 16]));
    }

    #[test]
    fn party_mood_bonus_lost_when_solo() {
        let mut state = PartyState::default();
        state.create_party([1u8; 16], "Alice", 0.0).unwrap();
        state.party.as_mut().unwrap().add_member([2u8; 16], "Bob", 1.0).unwrap();
        assert!(state.has_party_bonus());
        // Bob leaves, only Alice remains
        state.party.as_mut().unwrap().remove_member(&[2u8; 16]);
        assert!(!state.has_party_bonus()); // solo party, no bonus
    }
}
```

- [ ] **Step 2: Update `src-tauri/src/social/mod.rs` to include party**

Update `src-tauri/src/social/mod.rs`:

```rust
pub mod buddy;
pub mod party;
pub mod types;

pub use buddy::BuddyState;
pub use party::PartyState;
pub use types::{BuddySaveEntry, SocialMessage};
```

- [ ] **Step 3: Run tests to verify they pass**

```
cd src-tauri && cargo test party:: -- --nocapture
```

Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/social/party.rs src-tauri/src/social/mod.rs
git commit -m "feat(social): add PartyState with leader-authority lifecycle, kick, and invite timeout"
```

---

### Task 9: SocialState aggregator and GameState integration

**Files:**
- Modify: `src-tauri/src/social/mod.rs`
- Modify: `src-tauri/src/engine/state.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing test for SocialState on SaveState**

In `src-tauri/src/engine/state.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn save_state_buddies_default_empty() {
    let json = r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50}"#;
    let save: SaveState = serde_json::from_str(json).unwrap();
    assert!(save.buddies.is_empty());
    assert!(save.blocked.is_empty());
}

#[test]
fn save_state_buddies_round_trip() {
    let json = r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50,"buddies":[{"addressHash":"01010101010101010101010101010101","displayName":"Alice","addedDate":"2026-04-10","coPresenceTotal":100.0,"lastSeenDate":"2026-04-10"}],"blocked":["02020202020202020202020202020202"]}"#;
    let save: SaveState = serde_json::from_str(json).unwrap();
    assert_eq!(save.buddies.len(), 1);
    assert_eq!(save.buddies[0].display_name, "Alice");
    assert_eq!(save.blocked.len(), 1);
    let reserialized = serde_json::to_string(&save).unwrap();
    let restored: SaveState = serde_json::from_str(&reserialized).unwrap();
    assert_eq!(restored.buddies.len(), 1);
    assert_eq!(restored.blocked.len(), 1);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cd src-tauri && cargo test save_state_buddies -- --nocapture
```

Expected: FAIL -- `SaveState` has no field `buddies`

- [ ] **Step 3: Add SocialState aggregator to `src-tauri/src/social/mod.rs`**

Update `src-tauri/src/social/mod.rs` to add the aggregator:

```rust
pub mod buddy;
pub mod party;
pub mod types;

pub use buddy::BuddyState;
pub use party::PartyState;
pub use types::{BuddySaveEntry, SocialMessage};

use crate::emote::EmoteState;
use crate::mood::MoodState;

/// Aggregated social state on GameState.
#[derive(Debug, Clone)]
pub struct SocialState {
    pub mood: MoodState,
    pub emotes: EmoteState,
    pub buddies: BuddyState,
    pub party: PartyState,
}

/// Read-only context for social tick.
pub struct SocialTickContext<'a> {
    pub current_date: &'a str,
    pub in_dialogue: bool,
    pub game_time: f64,
}

impl SocialState {
    pub fn new(identity: [u8; 16], date: &str) -> Self {
        Self {
            mood: MoodState::default(),
            emotes: EmoteState::new(identity, date),
            buddies: BuddyState::default(),
            party: PartyState::default(),
        }
    }

    pub fn tick(&mut self, dt: f64, ctx: &SocialTickContext) {
        // Date change detection for emotes
        self.emotes.check_date_change(ctx.current_date);

        // Mood decay
        let party_bonus = self.party.has_party_bonus();
        self.mood.tick(dt, ctx.game_time, ctx.in_dialogue, party_bonus);

        // Expire pending requests/invites
        self.buddies.expire_requests(ctx.game_time);
        self.party.expire_invite(ctx.game_time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn social_state_tick_decays_mood() {
        let mut state = SocialState::new([1u8; 16], "2026-04-10");
        state.mood.mood = 90.0;
        state.mood.mood_grace_until = 0.0;
        let before = state.mood.mood;
        let ctx = SocialTickContext {
            current_date: "2026-04-10",
            in_dialogue: false,
            game_time: 1.0,
        };
        state.tick(1.0, &ctx);
        assert!(state.mood.mood < before);
    }

    #[test]
    fn social_state_tick_with_party_bonus() {
        let mut state = SocialState::new([1u8; 16], "2026-04-10");
        state.mood.mood = 90.0;
        state.mood.mood_grace_until = 0.0;

        // Create party with 2 members for bonus
        state.party.create_party([1u8; 16], "Alice", 0.0).unwrap();
        state.party.party.as_mut().unwrap().add_member([2u8; 16], "Bob", 0.0).unwrap();

        let mut no_party = SocialState::new([1u8; 16], "2026-04-10");
        no_party.mood.mood = 90.0;
        no_party.mood.mood_grace_until = 0.0;

        let ctx = SocialTickContext {
            current_date: "2026-04-10",
            in_dialogue: false,
            game_time: 1.0,
        };

        state.tick(1.0, &ctx);
        no_party.tick(1.0, &ctx);

        // Party state should have less decay
        assert!(state.mood.mood > no_party.mood.mood);
    }

    #[test]
    fn social_state_tick_clears_expired_requests() {
        let mut state = SocialState::new([1u8; 16], "2026-04-10");
        state.buddies.add_pending_request([2u8; 16], "Bob", 0.0).unwrap();
        let ctx = SocialTickContext {
            current_date: "2026-04-10",
            in_dialogue: false,
            game_time: 91.0,
        };
        state.tick(0.016, &ctx);
        assert!(state.buddies.pending_requests.is_empty());
    }

    #[test]
    fn social_state_tick_date_change_clears_emote_state() {
        let mut state = SocialState::new([1u8; 16], "2026-04-10");
        state.emotes.record_hi_sent([2u8; 16]);
        assert!(!state.emotes.can_hi(&[2u8; 16]));

        let ctx = SocialTickContext {
            current_date: "2026-04-11",
            in_dialogue: false,
            game_time: 1.0,
        };
        state.tick(0.016, &ctx);
        assert!(state.emotes.can_hi(&[2u8; 16]));
    }
}
```

- [ ] **Step 4: Add `pub mod social;` to lib.rs**

In `src-tauri/src/lib.rs`, add to the module declarations:

```rust
pub mod social;
```

- [ ] **Step 5: Add buddies/blocked/last_hi_date to SaveState**

In `src-tauri/src/engine/state.rs`, add to `SaveState` struct (after `max_mood`):

```rust
    #[serde(default)]
    pub buddies: Vec<crate::social::BuddySaveEntry>,
    #[serde(default)]
    pub blocked: Vec<String>,
    #[serde(default)]
    pub last_hi_date: Option<String>,
```

- [ ] **Step 6: Replace mood field on GameState with SocialState**

In `src-tauri/src/engine/state.rs`, replace the `mood: crate::mood::MoodState` field on `GameState` with:

```rust
    pub social: crate::social::SocialState,
```

- [ ] **Step 7: Update GameState::new() to initialize SocialState**

In `src-tauri/src/engine/state.rs`, in `GameState::new()`, replace `mood: crate::mood::MoodState::default(),` with:

```rust
    social: crate::social::SocialState::new([0u8; 16], ""),
```

Note: The real identity will be set when the network initializes. The empty date will be updated on first tick.

- [ ] **Step 8: Update GameState::tick() to use SocialState**

In `src-tauri/src/engine/state.rs`, in the `tick()` method, replace the mood tick lines:

```rust
    // Mood decay
    let in_dialogue = self.active_dialogue.is_some();
    self.mood.tick(dt, self.game_time, in_dialogue, false);
```

with:

```rust
    // Social tick (mood decay, request expiry, date change)
    {
        let ctx = crate::social::SocialTickContext {
            current_date: "", // TODO: wire real date from system clock
            in_dialogue: self.active_dialogue.is_some(),
            game_time: self.game_time,
        };
        self.social.tick(dt, &ctx);
    }
```

- [ ] **Step 9: Update RenderFrame construction to use SocialState**

In `src-tauri/src/engine/state.rs`, in the `tick()` method where `RenderFrame` is constructed, replace the mood fields:

```rust
    mood: self.mood.mood,
    max_mood: self.mood.max_mood,
```

with:

```rust
    mood: self.social.mood.mood,
    max_mood: self.social.mood.max_mood,
```

- [ ] **Step 10: Update save_state() to include buddies/blocked**

In `src-tauri/src/engine/state.rs`, in `save_state()`, replace the mood fields:

```rust
    mood: self.mood.mood,
    max_mood: self.mood.max_mood,
```

with:

```rust
    mood: self.social.mood.mood,
    max_mood: self.social.mood.max_mood,
    buddies: self.social.buddies.to_save_entries(),
    blocked: self.social.buddies.blocked_to_hex(),
    last_hi_date: Some(self.social.emotes.current_date.clone()),
```

- [ ] **Step 11: Update restore_save() to restore social state**

In `src-tauri/src/engine/state.rs`, in `restore_save()`, replace the mood restoration line:

```rust
    self.mood = crate::mood::MoodState::new_with_grace(save.mood, save.max_mood, self.game_time);
```

with:

```rust
    self.social.mood = crate::mood::MoodState::new_with_grace(save.mood, save.max_mood, self.game_time);
    self.social.buddies = crate::social::BuddyState::restore_from_save(&save.buddies, &save.blocked);
    if let Some(ref date) = save.last_hi_date {
        self.social.emotes.check_date_change(date);
    }
```

- [ ] **Step 12: Update eat_item IPC to use social.mood**

In `src-tauri/src/lib.rs`, in the `eat_item` IPC command, replace:

```rust
    if mood_gained > 0.0 {
        state.mood.apply_mood_change(mood_gained);
    }
```

with:

```rust
    if mood_gained > 0.0 {
        state.social.mood.apply_mood_change(mood_gained);
    }
```

- [ ] **Step 13: Run all tests**

```
cd src-tauri && cargo test -- --nocapture
```

Expected: ALL PASS

- [ ] **Step 14: Commit**

```bash
git add src-tauri/src/social/mod.rs src-tauri/src/engine/state.rs src-tauri/src/lib.rs
git commit -m "feat(social): add SocialState aggregator, wire into GameState/SaveState/RenderFrame"
```

---

### Task 10: Proximity extension for remote players

**Files:**
- Modify: `src-tauri/src/item/interaction.rs`

- [ ] **Step 1: Write failing test for RemotePlayer in NearestInteractable**

In `src-tauri/src/item/interaction.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn proximity_scan_finds_nearest_remote_player() {
    let entities = vec![];
    let entity_defs = std::collections::HashMap::new();
    let world_items = vec![];
    let remote_players = vec![
        RemotePlayerPosition { address_hash: [1u8; 16], x: 100.0, y: 0.0 },
        RemotePlayerPosition { address_hash: [2u8; 16], x: 500.0, y: 0.0 },
    ];
    let result = proximity_scan(0.0, 0.0, &entities, &entity_defs, &world_items, &remote_players);
    match result {
        Some(NearestInteractable::RemotePlayer { address_hash, distance }) => {
            assert_eq!(address_hash, [1u8; 16]);
            assert!((distance - 100.0).abs() < 1.0);
        }
        _ => panic!("Expected RemotePlayer"),
    }
}

#[test]
fn proximity_scan_remote_player_outside_400px_ignored() {
    let entities = vec![];
    let entity_defs = std::collections::HashMap::new();
    let world_items = vec![];
    let remote_players = vec![
        RemotePlayerPosition { address_hash: [1u8; 16], x: 500.0, y: 0.0 },
    ];
    let result = proximity_scan(0.0, 0.0, &entities, &entity_defs, &world_items, &remote_players);
    // 500px > 400px social radius, so no remote player
    assert!(!matches!(result, Some(NearestInteractable::RemotePlayer { .. })));
}

#[test]
fn proximity_scan_entity_preferred_over_remote_player_when_closer() {
    // Entity at 50px, remote player at 100px — entity wins
    let entities = vec![WorldEntity {
        entity_type: "test_tree".to_string(),
        x: 50.0,
        y: 0.0,
        state: crate::street::types::EntityState::default(),
    }];
    let mut entity_defs = std::collections::HashMap::new();
    entity_defs.insert("test_tree".to_string(), crate::street::types::EntityDef {
        name: "Test Tree".to_string(),
        interactions: vec![],
        variants: 1,
    });
    let world_items = vec![];
    let remote_players = vec![
        RemotePlayerPosition { address_hash: [1u8; 16], x: 100.0, y: 0.0 },
    ];
    let result = proximity_scan(0.0, 0.0, &entities, &entity_defs, &world_items, &remote_players);
    match result {
        Some(NearestInteractable::Entity { .. }) => {} // correct
        _ => panic!("Expected Entity to be preferred when closer"),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cd src-tauri && cargo test proximity_scan_finds_nearest_remote -- --nocapture
```

Expected: FAIL -- `NearestInteractable` has no `RemotePlayer` variant, `proximity_scan` doesn't accept remote players

- [ ] **Step 3: Add RemotePlayer variant to NearestInteractable**

In `src-tauri/src/item/interaction.rs`, update the enum:

```rust
pub enum NearestInteractable {
    Entity { index: usize, distance: f64 },
    GroundItem { index: usize, distance: f64 },
    RemotePlayer { address_hash: [u8; 16], distance: f64 },
}
```

- [ ] **Step 4: Add RemotePlayerPosition struct**

In `src-tauri/src/item/interaction.rs`, add above `proximity_scan`:

```rust
/// Lightweight position data for remote player proximity checks.
#[derive(Debug, Clone)]
pub struct RemotePlayerPosition {
    pub address_hash: [u8; 16],
    pub x: f64,
    pub y: f64,
}
```

- [ ] **Step 5: Extend proximity_scan to accept remote players**

Update the `proximity_scan` signature and implementation:

```rust
/// Social interaction radius for remote players (400px).
const SOCIAL_INTERACTION_RADIUS: f64 = 400.0;

pub fn proximity_scan(
    player_x: f64,
    player_y: f64,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
    remote_players: &[RemotePlayerPosition],
) -> Option<NearestInteractable> {
    let mut nearest: Option<NearestInteractable> = None;
    let mut nearest_dist = f64::MAX;

    // Check entities (existing logic)
    for (i, entity) in entities.iter().enumerate() {
        if entity_defs.contains_key(&entity.entity_type) {
            let dx = player_x - entity.x;
            let dy = player_y - entity.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < nearest_dist {
                nearest_dist = dist;
                nearest = Some(NearestInteractable::Entity {
                    index: i,
                    distance: dist,
                });
            }
        }
    }

    // Check ground items (existing logic)
    for (i, item) in world_items.iter().enumerate() {
        let dx = player_x - item.x;
        let dy = player_y - item.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < nearest_dist {
            nearest_dist = dist;
            nearest = Some(NearestInteractable::GroundItem {
                index: i,
                distance: dist,
            });
        }
    }

    // Check remote players (new, 400px radius)
    for rp in remote_players {
        let dx = player_x - rp.x;
        let dy = player_y - rp.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist <= SOCIAL_INTERACTION_RADIUS && dist < nearest_dist {
            nearest_dist = dist;
            nearest = Some(NearestInteractable::RemotePlayer {
                address_hash: rp.address_hash,
                distance: dist,
            });
        }
    }

    nearest
}
```

Note: The existing entity/ground-item interaction radius logic may differ (they may not have a max distance check). Preserve the existing behavior for entities and ground items -- only remote players get the 400px cutoff. Adapt the code above to match the existing proximity_scan structure if it differs from this approximation.

- [ ] **Step 6: Update all callers of proximity_scan to pass empty remote_players**

Search for all call sites of `proximity_scan` in the codebase and add `&[]` as the last argument for remote players. In `src-tauri/src/engine/state.rs`, the main call site:

```rust
    let nearest = proximity_scan(
        self.player.x,
        self.player.y,
        &self.world_entities,
        &self.entity_defs,
        &self.world_items,
        &[], // remote players populated from NetworkState in game_loop
    );
```

- [ ] **Step 7: Run all tests**

```
cd src-tauri && cargo test -- --nocapture
```

Expected: ALL PASS

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/item/interaction.rs src-tauri/src/engine/state.rs
git commit -m "feat(social): extend proximity_scan with RemotePlayer variant at 400px radius"
```

---

### Task 11: RemotePlayerFrame social annotations

**Files:**
- Modify: `src-tauri/src/engine/state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/types.ts`

- [ ] **Step 1: Add EmoteAnimationFrame struct**

In `src-tauri/src/engine/state.rs`, add near the other frame structs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmoteAnimationFrame {
    pub variant: String,
    pub target_hash: Option<String>,
    pub started_at: f64,
}
```

- [ ] **Step 2: Add social fields to RemotePlayerFrame**

In `src-tauri/src/engine/state.rs`, add to `RemotePlayerFrame` after `avatar`:

```rust
    #[serde(default)]
    pub epoch: String,
    #[serde(default)]
    pub is_buddy: bool,
    #[serde(default)]
    pub party_role: Option<String>,
    #[serde(default)]
    pub emote_animation: Option<EmoteAnimationFrame>,
```

- [ ] **Step 3: Update step-7 augmentation in game_loop**

In `src-tauri/src/lib.rs`, in the game_loop where remote frames are augmented (step 7), after `frame.remote_players = net_state.remote_frames();`, add social annotation:

```rust
    // Annotate remote players with social state
    {
        let game = app.state::<GameWrapper>();
        let game_state = game.0.lock().unwrap_or_else(|e| e.into_inner());
        for rp in &mut frame.remote_players {
            // Parse address_hash from hex string
            if let Ok(bytes) = hex::decode(&rp.address_hash) {
                if bytes.len() == 16 {
                    let mut addr = [0u8; 16];
                    addr.copy_from_slice(&bytes);
                    rp.is_buddy = game_state.social.buddies.is_buddy(&addr);
                    if let Some(ref party) = game_state.social.party.party {
                        rp.party_role = party.role_of(&addr).map(|r| match r {
                            crate::social::party::PartyRole::Leader => "Leader".to_string(),
                            crate::social::party::PartyRole::Member => "Member".to_string(),
                        });
                    }
                }
            }
            // epoch is populated from trust store (already available in NetworkState)
        }
    }
```

- [ ] **Step 4: Update TypeScript RemotePlayerFrame**

In `src/lib/types.ts`, add to the `RemotePlayerFrame` interface:

```typescript
  epoch: string;
  isBuddy: boolean;
  partyRole: string | null;
  emoteAnimation: EmoteAnimationFrame | null;
```

Add the new interface:

```typescript
export interface EmoteAnimationFrame {
  variant: string;
  targetHash: string | null;
  startedAt: number;
}
```

- [ ] **Step 5: Run all tests**

```
cd src-tauri && cargo test -- --nocapture
npx vitest run
```

Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine/state.rs src-tauri/src/lib.rs src/lib/types.ts
git commit -m "feat(social): add epoch, is_buddy, party_role, emote_animation to RemotePlayerFrame"
```

---

### Task 12: IPC commands -- mood and emotes

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add get_mood IPC command**

In `src-tauri/src/lib.rs`, add the IPC command:

```rust
#[tauri::command]
fn get_mood(
    game: tauri::State<'_, GameWrapper>,
) -> Result<serde_json::Value, String> {
    let state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    Ok(serde_json::json!({
        "mood": state.social.mood.mood,
        "maxMood": state.social.mood.max_mood,
        "multiplier": state.social.mood.multiplier(),
    }))
}
```

Register in the `tauri::Builder` invoke_handler.

- [ ] **Step 2: Add emote_hi IPC command**

In `src-tauri/src/lib.rs`, add the IPC command:

```rust
#[tauri::command]
fn emote_hi(
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<serde_json::Value, String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());

    // Check epoch gate: must be Initiate or above
    let our_epoch = net_state.our_epoch();
    if !crate::trust::epoch::can_chat(&our_epoch) {
        return Err("Need more time in world".to_string());
    }

    // Get our active variant
    let our_variant = state.social.emotes.active_variant();

    // Find nearest remote player via proximity
    let remote_positions: Vec<crate::item::interaction::RemotePlayerPosition> = net_state
        .remote_frames()
        .iter()
        .filter_map(|rf| {
            let bytes = hex::decode(&rf.address_hash).ok()?;
            if bytes.len() != 16 { return None; }
            let mut addr = [0u8; 16];
            addr.copy_from_slice(&bytes);
            Some(crate::item::interaction::RemotePlayerPosition {
                address_hash: addr,
                x: rf.x,
                y: rf.y,
            })
        })
        .collect();

    // Find nearest player within 400px
    let nearest_player = remote_positions.iter().min_by(|a, b| {
        let da = ((state.player.x - a.x).powi(2) + (state.player.y - a.y).powi(2)).sqrt();
        let db = ((state.player.x - b.x).powi(2) + (state.player.y - b.y).powi(2)).sqrt();
        da.partial_cmp(&db).unwrap()
    });

    let target = nearest_player.and_then(|rp| {
        let dist = ((state.player.x - rp.x).powi(2) + (state.player.y - rp.y).powi(2)).sqrt();
        if dist <= 400.0 { Some(rp.address_hash) } else { None }
    });

    // Check cooldown if targeted
    if let Some(target_hash) = target {
        if !state.social.emotes.can_hi(&target_hash) {
            return Err("Already greeted today".to_string());
        }
        // Check if target is blocked
        if state.social.buddies.is_blocked(&target_hash) {
            return Err("Player is blocked".to_string());
        }
        state.social.emotes.record_hi_sent(target_hash);
    }

    // Build and send emote message
    let msg = crate::emote::EmoteMessage {
        emote_type: crate::emote::EmoteType::Hi,
        variant: our_variant,
        target,
    };

    // Send via network (broadcast to all peers on street)
    drop(net_state);
    let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());
    // net_state.broadcast(NetMessage::Emote(msg.clone())); // wire to actual network send

    let mood_delta = if let Some(target_hash) = target {
        // Initiator only gets mood on match (checked when response comes back)
        0.0
    } else {
        0.0 // untargeted, no mood
    };

    Ok(serde_json::json!({
        "variant": our_variant.as_str(),
        "targeted": target.is_some(),
        "moodDelta": mood_delta,
    }))
}
```

Register in the `tauri::Builder` invoke_handler.

- [ ] **Step 3: Add TypeScript IPC functions**

In `src/lib/ipc.ts`, add:

```typescript
export interface MoodResult {
  mood: number;
  maxMood: number;
  multiplier: number;
}

export interface EmoteHiResult {
  variant: string;
  targeted: boolean;
  moodDelta: number;
}

export async function getMood(): Promise<MoodResult> {
  return invoke<MoodResult>('get_mood');
}

export async function emoteHi(): Promise<EmoteHiResult> {
  return invoke<EmoteHiResult>('emote_hi');
}
```

- [ ] **Step 4: Run all tests**

```
cd src-tauri && cargo test -- --nocapture
npx vitest run
```

Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src/lib/ipc.ts
git commit -m "feat(social): add get_mood and emote_hi IPC commands"
```

---

### Task 13: IPC commands -- buddies and parties

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/network/types.rs`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add Social variant to NetMessage**

In `src-tauri/src/network/types.rs`, add to the `NetMessage` enum:

```rust
    Social(crate::social::SocialMessage),
```

- [ ] **Step 2: Write test for NetMessage::Social round-trip**

In `src-tauri/src/network/types.rs`, add to tests:

```rust
#[test]
fn net_message_social_round_trip() {
    let msg = NetMessage::Social(crate::social::SocialMessage::BuddyRequest { from: [1u8; 16] });
    let json = serde_json::to_string(&msg).unwrap();
    let restored: NetMessage = serde_json::from_str(&json).unwrap();
    match restored {
        NetMessage::Social(crate::social::SocialMessage::BuddyRequest { from }) => {
            assert_eq!(from, [1u8; 16]);
        }
        _ => panic!("Expected Social BuddyRequest"),
    }
}
```

- [ ] **Step 3: Add buddy IPC commands**

In `src-tauri/src/lib.rs`, add the IPC commands:

```rust
#[tauri::command]
fn buddy_request(
    peer_hash: String,
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = hex::decode(&peer_hash).map_err(|_| "Invalid peer hash".to_string())?;
    if bytes.len() != 16 {
        return Err("Invalid peer hash length".to_string());
    }
    let mut addr = [0u8; 16];
    addr.copy_from_slice(&bytes);

    if state.social.buddies.is_buddy(&addr) {
        return Err("Already a buddy".to_string());
    }
    if state.social.buddies.is_blocked(&addr) {
        return Err("Player is blocked".to_string());
    }

    // Send buddy request via network
    // net.broadcast(NetMessage::Social(SocialMessage::BuddyRequest { from: our_address }));

    Ok(())
}

#[tauri::command]
fn buddy_accept(
    peer_hash: String,
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = hex::decode(&peer_hash).map_err(|_| "Invalid peer hash".to_string())?;
    if bytes.len() != 16 {
        return Err("Invalid peer hash length".to_string());
    }
    let mut addr = [0u8; 16];
    addr.copy_from_slice(&bytes);

    // Check if there's a pending request from this player
    let game_time = state.game_time;
    if state.social.buddies.get_pending_request(&addr, game_time).is_none() {
        return Err("No pending request from this player".to_string());
    }

    // Get the display name from the pending request
    let display_name = state.social.buddies.pending_requests
        .iter()
        .find(|r| r.from == addr)
        .map(|r| r.from_name.clone())
        .unwrap_or_default();

    // Add buddy with today's date
    let today = state.social.emotes.current_date.clone();
    state.social.buddies.add_buddy(addr, &display_name, &today)?;

    // Send accept via network
    // net.broadcast(NetMessage::Social(SocialMessage::BuddyAccept { from: our_address }));

    // Trust boost on buddy add: opinion shifts positive by 0.2
    // trust_store.record_buddy_add(&addr, 0.2);

    Ok(())
}

#[tauri::command]
fn buddy_decline(
    peer_hash: String,
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = hex::decode(&peer_hash).map_err(|_| "Invalid peer hash".to_string())?;
    if bytes.len() != 16 {
        return Err("Invalid peer hash length".to_string());
    }
    let mut addr = [0u8; 16];
    addr.copy_from_slice(&bytes);

    // Remove pending request
    state.social.buddies.pending_requests.retain(|r| r.from != addr);

    // Send decline via network
    // net.broadcast(NetMessage::Social(SocialMessage::BuddyDecline { from: our_address }));

    Ok(())
}

#[tauri::command]
fn buddy_remove(
    peer_hash: String,
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = hex::decode(&peer_hash).map_err(|_| "Invalid peer hash".to_string())?;
    if bytes.len() != 16 {
        return Err("Invalid peer hash length".to_string());
    }
    let mut addr = [0u8; 16];
    addr.copy_from_slice(&bytes);

    if !state.social.buddies.remove_buddy(&addr) {
        return Err("Not a buddy".to_string());
    }

    // Send remove via network (advisory)
    // net.broadcast(NetMessage::Social(SocialMessage::BuddyRemove { from: our_address }));

    Ok(())
}

#[tauri::command]
fn block_player(
    peer_hash: String,
    game: tauri::State<'_, GameWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = hex::decode(&peer_hash).map_err(|_| "Invalid peer hash".to_string())?;
    if bytes.len() != 16 {
        return Err("Invalid peer hash length".to_string());
    }
    let mut addr = [0u8; 16];
    addr.copy_from_slice(&bytes);

    state.social.buddies.block_player(addr);
    Ok(())
}

#[tauri::command]
fn unblock_player(
    peer_hash: String,
    game: tauri::State<'_, GameWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = hex::decode(&peer_hash).map_err(|_| "Invalid peer hash".to_string())?;
    if bytes.len() != 16 {
        return Err("Invalid peer hash length".to_string());
    }
    let mut addr = [0u8; 16];
    addr.copy_from_slice(&bytes);

    state.social.buddies.unblock_player(&addr);
    Ok(())
}

#[tauri::command]
fn get_buddy_list(
    game: tauri::State<'_, GameWrapper>,
) -> Result<serde_json::Value, String> {
    let state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let entries: Vec<serde_json::Value> = state.social.buddies.buddies.iter().map(|b| {
        serde_json::json!({
            "addressHash": hex::encode(b.address_hash),
            "displayName": b.display_name,
            "addedDate": b.added_date,
            "coPresenceTotal": b.co_presence_total,
            "lastSeenDate": b.last_seen_date,
        })
    }).collect();
    Ok(serde_json::json!({ "buddies": entries }))
}

#[tauri::command]
fn get_blocked_list(
    game: tauri::State<'_, GameWrapper>,
) -> Result<serde_json::Value, String> {
    let state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let blocked: Vec<String> = state.social.buddies.blocked.iter().map(hex::encode).collect();
    Ok(serde_json::json!({ "blocked": blocked }))
}
```

- [ ] **Step 4: Add party IPC commands**

In `src-tauri/src/lib.rs`, add the IPC commands:

```rust
#[tauri::command]
fn party_invite(
    peer_hash: String,
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());

    let bytes = hex::decode(&peer_hash).map_err(|_| "Invalid peer hash".to_string())?;
    if bytes.len() != 16 {
        return Err("Invalid peer hash length".to_string());
    }
    let mut addr = [0u8; 16];
    addr.copy_from_slice(&bytes);

    // Epoch gate
    let our_epoch = net_state.our_epoch();
    if !crate::trust::epoch::can_chat(&our_epoch) {
        return Err("Need more time in world".to_string());
    }

    // Block check
    if state.social.buddies.is_blocked(&addr) {
        return Err("Player is blocked".to_string());
    }

    // Create party if not in one
    if !state.social.party.in_party() {
        let our_address = net_state.our_address();
        let our_name = net_state.our_name();
        state.social.party.create_party(our_address, &our_name, state.game_time)?;
    }

    // Check party size
    let party = state.social.party.party.as_ref().ok_or("No party")?;
    if party.members.len() >= crate::social::party::MAX_PARTY_SIZE {
        return Err("Party is full".to_string());
    }

    // Send invite via network
    let members = party.member_hashes();
    let leader = party.leader;
    // net.broadcast(NetMessage::Social(SocialMessage::PartyInvite { leader, members }));

    Ok(())
}

#[tauri::command]
fn party_accept(
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());

    let our_address = net_state.our_address();
    let our_name = net_state.our_name();
    let game_time = state.game_time;

    let leader = state.social.party.accept_invite(our_address, &our_name, game_time)?;

    // Send accept via network
    // net.broadcast(NetMessage::Social(SocialMessage::PartyAccept { from: our_address }));

    Ok(())
}

#[tauri::command]
fn party_decline(
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());

    let leader = state.social.party.decline_invite()?;

    // Send decline via network
    // net.broadcast(NetMessage::Social(SocialMessage::PartyDecline { from: our_address }));

    Ok(())
}

#[tauri::command]
fn party_leave(
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());

    let our_address = net_state.our_address();
    let (members_to_notify, dissolved, new_leader) = state.social.party.leave_party(&our_address)?;

    // Send leave via network
    // net.broadcast(NetMessage::Social(SocialMessage::PartyLeave { from: our_address }));

    Ok(())
}

#[tauri::command]
fn party_kick(
    peer_hash: String,
    game: tauri::State<'_, GameWrapper>,
    net: tauri::State<'_, NetworkWrapper>,
) -> Result<(), String> {
    let mut state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());

    let bytes = hex::decode(&peer_hash).map_err(|_| "Invalid peer hash".to_string())?;
    if bytes.len() != 16 {
        return Err("Invalid peer hash length".to_string());
    }
    let mut addr = [0u8; 16];
    addr.copy_from_slice(&bytes);

    let our_address = net_state.our_address();
    let party = state.social.party.party.as_mut().ok_or("Not in a party")?;
    party.kick_member(&our_address, &addr)?;

    // Send kick via network
    // net.broadcast(NetMessage::Social(SocialMessage::PartyKick { target: addr }));

    // Check if party should dissolve (only 1 member left)
    if party.members.len() <= 1 {
        state.social.party.party = None;
    }

    Ok(())
}

#[tauri::command]
fn get_party_state(
    game: tauri::State<'_, GameWrapper>,
) -> Result<serde_json::Value, String> {
    let state = game.0.lock().unwrap_or_else(|e| e.into_inner());
    match &state.social.party.party {
        Some(party) => {
            let members: Vec<serde_json::Value> = party.members.iter().map(|m| {
                serde_json::json!({
                    "addressHash": hex::encode(m.address_hash),
                    "displayName": m.display_name,
                    "isLeader": party.leader == m.address_hash,
                })
            }).collect();
            Ok(serde_json::json!({
                "inParty": true,
                "leader": hex::encode(party.leader),
                "members": members,
            }))
        }
        None => Ok(serde_json::json!({
            "inParty": false,
            "leader": null,
            "members": [],
        })),
    }
}
```

Register all new commands in the `tauri::Builder` invoke_handler.

- [ ] **Step 5: Add TypeScript IPC functions for buddies and parties**

In `src/lib/ipc.ts`, add:

```typescript
export interface BuddyEntry {
  addressHash: string;
  displayName: string;
  addedDate: string;
  coPresenceTotal: number;
  lastSeenDate: string | null;
}

export interface BuddyListResult {
  buddies: BuddyEntry[];
}

export interface BlockedListResult {
  blocked: string[];
}

export interface PartyMemberInfo {
  addressHash: string;
  displayName: string;
  isLeader: boolean;
}

export interface PartyStateResult {
  inParty: boolean;
  leader: string | null;
  members: PartyMemberInfo[];
}

export async function buddyRequest(peerHash: string): Promise<void> {
  return invoke<void>('buddy_request', { peerHash });
}

export async function buddyAccept(peerHash: string): Promise<void> {
  return invoke<void>('buddy_accept', { peerHash });
}

export async function buddyDecline(peerHash: string): Promise<void> {
  return invoke<void>('buddy_decline', { peerHash });
}

export async function buddyRemove(peerHash: string): Promise<void> {
  return invoke<void>('buddy_remove', { peerHash });
}

export async function blockPlayer(peerHash: string): Promise<void> {
  return invoke<void>('block_player', { peerHash });
}

export async function unblockPlayer(peerHash: string): Promise<void> {
  return invoke<void>('unblock_player', { peerHash });
}

export async function getBuddyList(): Promise<BuddyListResult> {
  return invoke<BuddyListResult>('get_buddy_list');
}

export async function getBlockedList(): Promise<BlockedListResult> {
  return invoke<BlockedListResult>('get_blocked_list');
}

export async function partyInvite(peerHash: string): Promise<void> {
  return invoke<void>('party_invite', { peerHash });
}

export async function partyAccept(): Promise<void> {
  return invoke<void>('party_accept');
}

export async function partyDecline(): Promise<void> {
  return invoke<void>('party_decline');
}

export async function partyLeave(): Promise<void> {
  return invoke<void>('party_leave');
}

export async function partyKick(peerHash: string): Promise<void> {
  return invoke<void>('party_kick', { peerHash });
}

export async function getPartyState(): Promise<PartyStateResult> {
  return invoke<PartyStateResult>('get_party_state');
}
```

- [ ] **Step 6: Run all tests**

```
cd src-tauri && cargo test -- --nocapture
npx vitest run
```

Expected: ALL PASS

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/network/types.rs src/lib/ipc.ts
git commit -m "feat(social): add buddy and party IPC commands with Social NetMessage variant"
```

---

### Task 14: Party chat (ChatChannel)

**Files:**
- Modify: `src-tauri/src/network/types.rs`

- [ ] **Step 1: Write failing test for ChatChannel on ChatMessage**

In `src-tauri/src/network/types.rs`, add to tests:

```rust
#[test]
fn chat_message_defaults_to_street_channel() {
    let json = r#"{"text":"hello","sender":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"senderName":"Alice"}"#;
    let msg: ChatMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.channel, ChatChannel::Street);
}

#[test]
fn chat_message_party_channel_round_trip() {
    let msg = ChatMessage {
        text: "hello team".to_string(),
        sender: [1u8; 16],
        sender_name: "Alice".to_string(),
        channel: ChatChannel::Party,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.channel, ChatChannel::Party);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```
cd src-tauri && cargo test chat_message_defaults_to_street chat_message_party_channel -- --nocapture
```

Expected: FAIL -- `ChatMessage` has no field `channel`

- [ ] **Step 3: Add ChatChannel enum and channel field to ChatMessage**

In `src-tauri/src/network/types.rs`, add the enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatChannel {
    #[default]
    Street,
    Party,
}
```

Add to `ChatMessage` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub text: String,
    pub sender: [u8; 16],
    pub sender_name: String,
    #[serde(default)]
    pub channel: ChatChannel,
}
```

- [ ] **Step 4: Run tests to verify they pass**

```
cd src-tauri && cargo test chat_message -- --nocapture
```

Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/network/types.rs
git commit -m "feat(social): add ChatChannel enum (Street/Party) to ChatMessage with backward compat"
```

---

### Task 15: Frontend -- emote animation, buddy indicators, party panel

**Files:**
- Create: `src/lib/components/EmoteAnimation.svelte`
- Create: `src/lib/components/PartyPanel.svelte`
- Create: `src/lib/components/PartyPanel.test.ts`
- Create: `src/lib/components/BuddyListPanel.svelte`
- Create: `src/lib/components/BuddyListPanel.test.ts`

- [ ] **Step 1: Create EmoteAnimation.svelte**

Create `src/lib/components/EmoteAnimation.svelte`:

```svelte
<script lang="ts">
  import type { EmoteAnimationFrame } from '$lib/types';

  let { animation, x, y }: {
    animation: EmoteAnimationFrame;
    x: number;
    y: number;
  } = $props();

  const VARIANT_EMOJIS: Record<string, string> = {
    bats: '🦇',
    birds: '🐦',
    butterflies: '🦋',
    cubes: '🧊',
    flowers: '🌸',
    hands: '👋',
    hearts: '❤️',
    hi: '👋',
    pigs: '🐷',
    rocketships: '🚀',
    stars: '⭐',
  };

  let emoji = $derived(VARIANT_EMOJIS[animation.variant] ?? '👋');
  let elapsed = $derived(Date.now() / 1000 - animation.startedAt);
  let visible = $derived(elapsed < 2.0);
</script>

{#if visible}
  <div
    class="emote-animation"
    style="left: {x}px; top: {y - 60}px;"
    aria-label="Emote: {animation.variant}"
  >
    <span class="emote-sprite">{emoji}</span>
  </div>
{/if}

<style>
  .emote-animation {
    position: absolute;
    pointer-events: none;
    z-index: 60;
    animation: emote-float 2s ease-out forwards;
  }
  .emote-sprite {
    font-size: 28px;
    filter: drop-shadow(0 0 4px rgba(255, 255, 255, 0.5));
  }
  @keyframes emote-float {
    0% {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
    100% {
      opacity: 0;
      transform: translateY(-80px) scale(1.3);
    }
  }
</style>
```

- [ ] **Step 2: Write PartyPanel tests**

Create `src/lib/components/PartyPanel.test.ts`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import PartyPanel from './PartyPanel.svelte';

describe('PartyPanel', () => {
  const mockMembers = [
    { addressHash: 'aabb', displayName: 'Alice', isLeader: true },
    { addressHash: 'ccdd', displayName: 'Bob', isLeader: false },
  ];

  it('renders member list', () => {
    render(PartyPanel, { props: {
      inParty: true,
      members: mockMembers,
      isLeader: true,
      onLeave: () => {},
      onKick: () => {},
    }});
    expect(document.querySelector('.party-panel')).toBeTruthy();
    expect(document.querySelectorAll('.party-member').length).toBe(2);
  });

  it('shows leader badge', () => {
    render(PartyPanel, { props: {
      inParty: true,
      members: mockMembers,
      isLeader: true,
      onLeave: () => {},
      onKick: () => {},
    }});
    const leaderMember = document.querySelector('.party-member.leader');
    expect(leaderMember).toBeTruthy();
  });

  it('shows kick button only for leader', () => {
    render(PartyPanel, { props: {
      inParty: true,
      members: mockMembers,
      isLeader: true,
      onLeave: () => {},
      onKick: () => {},
    }});
    const kickButtons = document.querySelectorAll('.kick-btn');
    // Should show kick for Bob but not Alice (self)
    expect(kickButtons.length).toBe(1);
  });

  it('hides kick button for non-leader', () => {
    render(PartyPanel, { props: {
      inParty: true,
      members: mockMembers,
      isLeader: false,
      onLeave: () => {},
      onKick: () => {},
    }});
    const kickButtons = document.querySelectorAll('.kick-btn');
    expect(kickButtons.length).toBe(0);
  });

  it('does not render when not in party', () => {
    render(PartyPanel, { props: {
      inParty: false,
      members: [],
      isLeader: false,
      onLeave: () => {},
      onKick: () => {},
    }});
    expect(document.querySelector('.party-panel')).toBeNull();
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

```
npx vitest run src/lib/components/PartyPanel.test.ts
```

Expected: FAIL -- `PartyPanel.svelte` does not exist

- [ ] **Step 4: Create PartyPanel.svelte**

Create `src/lib/components/PartyPanel.svelte`:

```svelte
<script lang="ts">
  import type { PartyMemberInfo } from '$lib/ipc';

  let {
    inParty = false,
    members = [] as PartyMemberInfo[],
    isLeader = false,
    onLeave = () => {},
    onKick = (_hash: string) => {},
  }: {
    inParty: boolean;
    members: PartyMemberInfo[];
    isLeader: boolean;
    onLeave: () => void;
    onKick: (hash: string) => void;
  } = $props();

  let collapsed = $state(false);
</script>

{#if inParty}
  <div class="party-panel" role="region" aria-label="Party">
    <button
      class="party-header"
      onclick={() => collapsed = !collapsed}
      aria-expanded={!collapsed}
    >
      <span class="party-icon">👥</span>
      <span>Party ({members.length})</span>
    </button>
    {#if !collapsed}
      <div class="party-members">
        {#each members as member}
          <div class="party-member" class:leader={member.isLeader}>
            <span class="member-name">
              {#if member.isLeader}<span class="leader-badge">★</span>{/if}
              {member.displayName}
            </span>
            {#if isLeader && !member.isLeader}
              <button
                class="kick-btn"
                onclick={() => onKick(member.addressHash)}
                aria-label="Kick {member.displayName}"
              >✕</button>
            {/if}
          </div>
        {/each}
      </div>
      <button class="leave-btn" onclick={onLeave}>Leave Party</button>
    {/if}
  </div>
{/if}

<style>
  .party-panel {
    position: absolute;
    top: 100px;
    right: 12px;
    z-index: 50;
    background: rgba(26, 26, 46, 0.9);
    border-radius: 12px;
    color: #e0e0f0;
    font-family: 'Lato', sans-serif;
    font-size: 13px;
    min-width: 160px;
    overflow: hidden;
  }
  .party-header {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 8px 12px;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font-size: 13px;
    font-weight: 600;
  }
  .party-header:hover {
    background: rgba(255, 255, 255, 0.05);
  }
  .party-icon {
    font-size: 16px;
  }
  .party-members {
    padding: 0 8px;
  }
  .party-member {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 6px;
    border-radius: 4px;
  }
  .party-member.leader .member-name {
    color: #fbbf24;
  }
  .leader-badge {
    margin-right: 4px;
  }
  .kick-btn {
    background: none;
    border: none;
    color: #f87171;
    cursor: pointer;
    font-size: 14px;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .kick-btn:hover {
    background: rgba(248, 113, 113, 0.2);
  }
  .leave-btn {
    display: block;
    width: calc(100% - 16px);
    margin: 8px;
    padding: 6px;
    background: rgba(239, 68, 68, 0.2);
    border: 1px solid rgba(239, 68, 68, 0.3);
    border-radius: 6px;
    color: #fca5a5;
    cursor: pointer;
    font-size: 12px;
  }
  .leave-btn:hover {
    background: rgba(239, 68, 68, 0.3);
  }
</style>
```

- [ ] **Step 5: Run PartyPanel tests to verify they pass**

```
npx vitest run src/lib/components/PartyPanel.test.ts
```

Expected: ALL PASS

- [ ] **Step 6: Write BuddyListPanel tests**

Create `src/lib/components/BuddyListPanel.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import BuddyListPanel from './BuddyListPanel.svelte';

describe('BuddyListPanel', () => {
  const mockBuddies = [
    { addressHash: 'aabb', displayName: 'Alice', addedDate: '2026-04-10', coPresenceTotal: 3600, lastSeenDate: '2026-04-10' },
    { addressHash: 'ccdd', displayName: 'Bob', addedDate: '2026-04-09', coPresenceTotal: 7200, lastSeenDate: '2026-04-09' },
  ];

  it('renders buddy list', () => {
    render(BuddyListPanel, { props: {
      buddies: mockBuddies,
      visible: true,
      onRemove: () => {},
      onBlock: () => {},
    }});
    expect(document.querySelectorAll('.buddy-entry').length).toBe(2);
  });

  it('shows buddy display names', () => {
    render(BuddyListPanel, { props: {
      buddies: mockBuddies,
      visible: true,
      onRemove: () => {},
      onBlock: () => {},
    }});
    const names = document.querySelectorAll('.buddy-name');
    expect(names[0]?.textContent).toContain('Alice');
    expect(names[1]?.textContent).toContain('Bob');
  });

  it('does not render when not visible', () => {
    render(BuddyListPanel, { props: {
      buddies: mockBuddies,
      visible: false,
      onRemove: () => {},
      onBlock: () => {},
    }});
    expect(document.querySelector('.buddy-panel')).toBeNull();
  });

  it('shows empty state when no buddies', () => {
    render(BuddyListPanel, { props: {
      buddies: [],
      visible: true,
      onRemove: () => {},
      onBlock: () => {},
    }});
    const emptyMsg = document.querySelector('.buddy-empty');
    expect(emptyMsg).toBeTruthy();
  });

  it('formats co-presence time', () => {
    render(BuddyListPanel, { props: {
      buddies: mockBuddies,
      visible: true,
      onRemove: () => {},
      onBlock: () => {},
    }});
    const timeElements = document.querySelectorAll('.buddy-copresence');
    // 3600 seconds = "1h"
    expect(timeElements[0]?.textContent).toContain('1h');
  });
});
```

- [ ] **Step 7: Run tests to verify they fail**

```
npx vitest run src/lib/components/BuddyListPanel.test.ts
```

Expected: FAIL -- `BuddyListPanel.svelte` does not exist

- [ ] **Step 8: Create BuddyListPanel.svelte**

Create `src/lib/components/BuddyListPanel.svelte`:

```svelte
<script lang="ts">
  import type { BuddyEntry } from '$lib/ipc';

  let {
    buddies = [] as BuddyEntry[],
    visible = false,
    onRemove = (_hash: string) => {},
    onBlock = (_hash: string) => {},
  }: {
    buddies: BuddyEntry[];
    visible: boolean;
    onRemove: (hash: string) => void;
    onBlock: (hash: string) => void;
  } = $props();

  function formatCopresence(seconds: number): string {
    if (seconds < 60) return `${Math.floor(seconds)}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    return `${Math.floor(seconds / 3600)}h`;
  }
</script>

{#if visible}
  <div class="buddy-panel" role="region" aria-label="Buddy List">
    <div class="buddy-header">
      <span class="buddy-icon">⭐</span>
      <span>Buddies ({buddies.length})</span>
    </div>
    {#if buddies.length === 0}
      <div class="buddy-empty">No buddies yet</div>
    {:else}
      <div class="buddy-list">
        {#each buddies as buddy}
          <div class="buddy-entry">
            <div class="buddy-info">
              <span class="buddy-name">{buddy.displayName}</span>
              <span class="buddy-copresence">{formatCopresence(buddy.coPresenceTotal)}</span>
            </div>
            <div class="buddy-actions">
              <button
                class="buddy-action-btn remove"
                onclick={() => onRemove(buddy.addressHash)}
                aria-label="Remove {buddy.displayName}"
              >Remove</button>
              <button
                class="buddy-action-btn block"
                onclick={() => onBlock(buddy.addressHash)}
                aria-label="Block {buddy.displayName}"
              >Block</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .buddy-panel {
    position: absolute;
    top: 100px;
    left: 12px;
    z-index: 50;
    background: rgba(26, 26, 46, 0.9);
    border-radius: 12px;
    color: #e0e0f0;
    font-family: 'Lato', sans-serif;
    font-size: 13px;
    min-width: 200px;
    max-height: 300px;
    overflow-y: auto;
  }
  .buddy-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    font-weight: 600;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }
  .buddy-icon {
    font-size: 16px;
  }
  .buddy-empty {
    padding: 12px 14px;
    color: #888;
    font-style: italic;
  }
  .buddy-list {
    padding: 4px 0;
  }
  .buddy-entry {
    padding: 6px 14px;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .buddy-entry:hover {
    background: rgba(255, 255, 255, 0.05);
  }
  .buddy-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .buddy-name {
    color: #fbbf24;
    font-weight: 500;
  }
  .buddy-copresence {
    font-size: 11px;
    color: #888;
  }
  .buddy-actions {
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .buddy-entry:hover .buddy-actions {
    opacity: 1;
  }
  .buddy-action-btn {
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    color: #ccc;
    cursor: pointer;
    font-size: 11px;
    padding: 2px 6px;
  }
  .buddy-action-btn.remove:hover {
    border-color: #f59e0b;
    color: #f59e0b;
  }
  .buddy-action-btn.block:hover {
    border-color: #ef4444;
    color: #ef4444;
  }
</style>
```

- [ ] **Step 9: Run BuddyListPanel tests to verify they pass**

```
npx vitest run src/lib/components/BuddyListPanel.test.ts
```

Expected: ALL PASS

- [ ] **Step 10: Run all frontend tests**

```
npx vitest run
```

Expected: ALL PASS

- [ ] **Step 11: Commit**

```bash
git add src/lib/components/EmoteAnimation.svelte src/lib/components/PartyPanel.svelte src/lib/components/PartyPanel.test.ts src/lib/components/BuddyListPanel.svelte src/lib/components/BuddyListPanel.test.ts
git commit -m "feat(social): add EmoteAnimation, PartyPanel, BuddyListPanel frontend components"
```

---

### Task 16: Frontend -- social interaction prompt

**Files:**
- Create: `src/lib/components/SocialPrompt.svelte`
- Create: `src/lib/components/SocialPrompt.test.ts`
- Modify: `src/App.svelte`

- [ ] **Step 1: Write SocialPrompt tests**

Create `src/lib/components/SocialPrompt.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import SocialPrompt from './SocialPrompt.svelte';

describe('SocialPrompt', () => {
  it('renders action buttons when visible', () => {
    render(SocialPrompt, { props: {
      visible: true,
      targetName: 'Alice',
      canHi: true,
      canTrade: true,
      canInvite: true,
      canBuddy: true,
      onHi: () => {},
      onTrade: () => {},
      onInvite: () => {},
      onBuddy: () => {},
    }});
    expect(document.querySelector('.social-prompt')).toBeTruthy();
    expect(document.querySelectorAll('.social-action').length).toBe(4);
  });

  it('hides when not visible', () => {
    render(SocialPrompt, { props: {
      visible: false,
      targetName: 'Alice',
      canHi: true,
      canTrade: false,
      canInvite: false,
      canBuddy: false,
      onHi: () => {},
      onTrade: () => {},
      onInvite: () => {},
      onBuddy: () => {},
    }});
    expect(document.querySelector('.social-prompt')).toBeNull();
  });

  it('filters actions by availability', () => {
    render(SocialPrompt, { props: {
      visible: true,
      targetName: 'Alice',
      canHi: true,
      canTrade: false,
      canInvite: false,
      canBuddy: false,
      onHi: () => {},
      onTrade: () => {},
      onInvite: () => {},
      onBuddy: () => {},
    }});
    expect(document.querySelectorAll('.social-action').length).toBe(1);
  });

  it('shows target name', () => {
    render(SocialPrompt, { props: {
      visible: true,
      targetName: 'Alice',
      canHi: true,
      canTrade: false,
      canInvite: false,
      canBuddy: false,
      onHi: () => {},
      onTrade: () => {},
      onInvite: () => {},
      onBuddy: () => {},
    }});
    expect(document.querySelector('.social-prompt-name')?.textContent).toContain('Alice');
  });

  it('has correct aria labels on action buttons', () => {
    render(SocialPrompt, { props: {
      visible: true,
      targetName: 'Alice',
      canHi: true,
      canTrade: true,
      canInvite: false,
      canBuddy: false,
      onHi: () => {},
      onTrade: () => {},
      onInvite: () => {},
      onBuddy: () => {},
    }});
    const hiBtn = document.querySelector('[aria-label="Hi Alice"]');
    expect(hiBtn).toBeTruthy();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```
npx vitest run src/lib/components/SocialPrompt.test.ts
```

Expected: FAIL -- `SocialPrompt.svelte` does not exist

- [ ] **Step 3: Create SocialPrompt.svelte**

Create `src/lib/components/SocialPrompt.svelte`:

```svelte
<script lang="ts">
  let {
    visible = false,
    targetName = '',
    canHi = false,
    canTrade = false,
    canInvite = false,
    canBuddy = false,
    onHi = () => {},
    onTrade = () => {},
    onInvite = () => {},
    onBuddy = () => {},
  }: {
    visible: boolean;
    targetName: string;
    canHi: boolean;
    canTrade: boolean;
    canInvite: boolean;
    canBuddy: boolean;
    onHi: () => void;
    onTrade: () => void;
    onInvite: () => void;
    onBuddy: () => void;
  } = $props();

  interface Action {
    label: string;
    key: string;
    enabled: boolean;
    handler: () => void;
  }

  let actions = $derived.by((): Action[] => {
    const all: Action[] = [
      { label: 'Hi', key: 'H', enabled: canHi, handler: onHi },
      { label: 'Trade', key: 'T', enabled: canTrade, handler: onTrade },
      { label: 'Invite', key: 'I', enabled: canInvite, handler: onInvite },
      { label: 'Add Buddy', key: 'B', enabled: canBuddy, handler: onBuddy },
    ];
    return all.filter(a => a.enabled);
  });
</script>

{#if visible && actions.length > 0}
  <div class="social-prompt" role="menu" aria-label="Social actions for {targetName}">
    <span class="social-prompt-name">{targetName}</span>
    <div class="social-actions">
      {#each actions as action}
        <button
          class="social-action"
          onclick={action.handler}
          aria-label="{action.label} {targetName}"
        >
          <span class="action-key">{action.key}</span>
          <span class="action-label">{action.label}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .social-prompt {
    position: absolute;
    bottom: 120px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 55;
    background: rgba(26, 26, 46, 0.92);
    border-radius: 12px;
    padding: 10px 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    pointer-events: auto;
    font-family: 'Lato', sans-serif;
  }
  .social-prompt-name {
    color: #fbbf24;
    font-weight: 600;
    font-size: 14px;
  }
  .social-actions {
    display: flex;
    gap: 6px;
  }
  .social-action {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 12px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    color: #e0e0f0;
    cursor: pointer;
    font-size: 13px;
    transition: background 0.15s;
  }
  .social-action:hover {
    background: rgba(255, 255, 255, 0.15);
  }
  .social-action:focus-visible {
    outline: 2px solid #c084fc;
    outline-offset: 2px;
  }
  .action-key {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    background: rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    font-size: 11px;
    font-weight: 700;
    color: #c084fc;
  }
  .action-label {
    font-size: 13px;
  }
</style>
```

- [ ] **Step 4: Run SocialPrompt tests to verify they pass**

```
npx vitest run src/lib/components/SocialPrompt.test.ts
```

Expected: ALL PASS

- [ ] **Step 5: Wire social components into App.svelte**

In `src/App.svelte`, add imports:

```typescript
import MoodHud from './lib/components/MoodHud.svelte';
import PartyPanel from './lib/components/PartyPanel.svelte';
import BuddyListPanel from './lib/components/BuddyListPanel.svelte';
import SocialPrompt from './lib/components/SocialPrompt.svelte';
import EmoteAnimation from './lib/components/EmoteAnimation.svelte';
import { emoteHi, partyLeave, partyKick, buddyRemove, blockPlayer, getPartyState, getBuddyList } from './lib/ipc';
```

Add state variables:

```typescript
let partyState = $state({ inParty: false, leader: null as string | null, members: [] as any[] });
let buddyList = $state([] as any[]);
let showBuddyPanel = $state(false);
```

Add keyboard handler for H key (emote):

```typescript
function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'h' || e.key === 'H') {
    if (!e.repeat && !e.ctrlKey && !e.altKey && !e.metaKey) {
      emoteHi().catch(() => {});
    }
  }
}
```

Add components to the template (after existing HUD components):

```svelte
<MoodHud mood={frame.mood} maxMood={frame.maxMood} />

<PartyPanel
  inParty={partyState.inParty}
  members={partyState.members}
  isLeader={partyState.leader === ourAddressHash}
  onLeave={() => partyLeave()}
  onKick={(hash) => partyKick(hash)}
/>

<BuddyListPanel
  buddies={buddyList}
  visible={showBuddyPanel}
  onRemove={(hash) => buddyRemove(hash)}
  onBlock={(hash) => blockPlayer(hash)}
/>

{#each frame.remotePlayers as rp}
  {#if rp.emoteAnimation}
    <EmoteAnimation animation={rp.emoteAnimation} x={rp.x} y={rp.y} />
  {/if}
{/each}
```

The SocialPrompt integration depends on proximity detection results being available in the frame. Wire it based on the nearest remote player interaction:

```svelte
<SocialPrompt
  visible={nearestRemotePlayer !== null}
  targetName={nearestRemotePlayer?.displayName ?? ''}
  canHi={true}
  canTrade={nearestRemotePlayer?.epoch !== 'Sandbox'}
  canInvite={!partyState.inParty || partyState.leader === ourAddressHash}
  canBuddy={nearestRemotePlayer !== null && !nearestRemotePlayer?.isBuddy}
  onHi={() => emoteHi()}
  onTrade={() => {}}
  onInvite={() => {}}
  onBuddy={() => {}}
/>
```

- [ ] **Step 6: Run all tests**

```
cd src-tauri && cargo test -- --nocapture
npx vitest run
```

Expected: ALL PASS

- [ ] **Step 7: Commit**

```bash
git add src/lib/components/SocialPrompt.svelte src/lib/components/SocialPrompt.test.ts src/App.svelte
git commit -m "feat(social): add SocialPrompt with contextual actions, wire all social components into App"
```
