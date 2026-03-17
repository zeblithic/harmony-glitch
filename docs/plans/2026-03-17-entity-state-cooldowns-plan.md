# Entity State & Cooldowns — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enforce per-entity-instance cooldowns and depletion cycles, with visual/textual feedback, so harvesting entities feels meaningful.

**Architecture:** Timestamp-based state stored in a HashMap on GameState, keyed by entity instance ID. All checks are O(1) comparisons against a session-global `game_time` clock — no per-tick scanning. Entity definitions drive tuning values from JSON.

**Tech Stack:** Rust (Tauri v2), Svelte 5, TypeScript, PixiJS v8

**Spec:** `docs/plans/2026-03-17-entity-state-cooldowns-design.md`

---

## File Structure

| File | Responsibility | Change Type |
|------|---------------|-------------|
| `src-tauri/src/item/types.rs` | Type definitions for items, entities, frames, prompts | Modify: add `EntityInstanceState`, extend `EntityDef`, `InteractionPrompt`, `WorldEntityFrame` |
| `src-tauri/src/item/interaction.rs` | Proximity scan, prompt building, interaction execution | Modify: add cooldown/depletion logic, state-aware prompts |
| `src-tauri/src/engine/state.rs` | GameState, tick loop, frame building | Modify: add `game_time`, `entity_states`, thread through tick, extend frame builder |
| `assets/entities.json` | Entity type definitions (data-driven tuning) | Modify: add `maxHarvests`, `respawnSecs` to all 6 entries |
| `src/lib/types.ts` | TypeScript mirrors of Rust DTOs | Modify: extend `WorldEntityFrame`, `InteractionPrompt` |
| `src/lib/engine/renderer.ts` | PixiJS rendering | Modify: opacity fade, conditional `[E]` prefix |

---

## Chunk 1: Data Model & Storage

### Task 1: Data Model Types & JSON

Add all new type definitions and fields. Update every construction site so the codebase compiles and all existing tests pass. No behavioral changes yet.

**Files:**
- Modify: `src-tauri/src/item/types.rs:25-37` (EntityDef), `:70-80` (WorldEntityFrame), `:114-122` (InteractionPrompt)
- Modify: `src-tauri/src/item/interaction.rs:242-261` (test helper)
- Modify: `src-tauri/src/engine/state.rs:636-644` (test EntityDef)
- Modify: `assets/entities.json`
- Test: `src-tauri/src/item/types.rs` (new tests in existing module)

- [ ] **Step 1: Write test for EntityInstanceState**

In `src-tauri/src/item/types.rs`, add to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn entity_instance_state_creation() {
    let state = EntityInstanceState::new(3);
    assert_eq!(state.harvests_remaining, 3);
    assert_eq!(state.cooldown_until, 0.0);
    assert_eq!(state.depleted_until, 0.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test entity_instance_state_creation`
Expected: FAIL — `EntityInstanceState` not found

- [ ] **Step 3: Add EntityInstanceState struct**

In `src-tauri/src/item/types.rs`, after the `WorldEntity` struct (after line 54), add:

```rust
/// Per-instance runtime state for a world entity.
/// Stored in GameState::entity_states, keyed by entity instance ID.
#[derive(Debug, Clone)]
pub struct EntityInstanceState {
    /// Harvests remaining before depletion. Initialized from EntityDef::max_harvests.
    pub harvests_remaining: u32,
    /// Game-time timestamp when cooldown expires. 0.0 = not on cooldown.
    pub cooldown_until: f64,
    /// Game-time timestamp when respawn completes. 0.0 = not depleted.
    pub depleted_until: f64,
}

impl EntityInstanceState {
    pub fn new(max_harvests: u32) -> Self {
        Self {
            harvests_remaining: max_harvests,
            cooldown_until: 0.0,
            depleted_until: 0.0,
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test entity_instance_state_creation`
Expected: PASS

- [ ] **Step 5: Add max_harvests and respawn_secs to EntityDef**

In `src-tauri/src/item/types.rs`, add two fields to `EntityDef` (after `cooldown_secs`):

```rust
pub struct EntityDef {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub verb: String,
    pub yields: Vec<YieldEntry>,
    pub cooldown_secs: f64,
    pub max_harvests: u32,
    pub respawn_secs: f64,
    pub sprite_class: String,
    pub interact_radius: f64,
}
```

- [ ] **Step 6: Update EntityDef construction in interaction.rs test helper**

In `src-tauri/src/item/interaction.rs`, update `test_entity_defs()` (around line 246):

```rust
EntityDef {
    id: "fruit_tree".into(),
    name: "Fruit Tree".into(),
    verb: "Harvest".into(),
    yields: vec![YieldEntry {
        item: "cherry".into(),
        min: 2,
        max: 2,
    }],
    cooldown_secs: 5.0,
    max_harvests: 3,
    respawn_secs: 30.0,
    sprite_class: "tree_fruit".into(),
    interact_radius: 80.0,
},
```

Note: `cooldown_secs` changes from `0.0` to `5.0`. This is safe because existing tests only do one harvest and don't check cooldown behavior yet.

- [ ] **Step 7: Update EntityDef construction in state.rs test**

In `src-tauri/src/engine/state.rs`, update the `interaction_adds_to_inventory` test (around line 636):

```rust
entity_defs.insert("fruit_tree".into(), EntityDef {
    id: "fruit_tree".into(),
    name: "Fruit Tree".into(),
    verb: "Harvest".into(),
    yields: vec![YieldEntry { item: "cherry".into(), min: 1, max: 1 }],
    cooldown_secs: 0.0,
    max_harvests: 0,
    respawn_secs: 0.0,
    sprite_class: "tree_fruit".into(),
    interact_radius: 80.0,
});
```

`max_harvests: 0` preserves infinite-harvest behavior (no depletion). `cooldown_secs: 0.0` preserves no-cooldown behavior.

- [ ] **Step 8: Add actionable to InteractionPrompt**

In `src-tauri/src/item/types.rs`, add field to `InteractionPrompt`:

```rust
pub struct InteractionPrompt {
    pub verb: String,
    pub target_name: String,
    pub target_x: f64,
    pub target_y: f64,
    pub actionable: bool,
}
```

Then update both construction sites in `src-tauri/src/item/interaction.rs` `build_prompt()`:

In the `Entity` arm (around line 77):
```rust
InteractionPrompt {
    verb: def.map(|d| d.verb.clone()).unwrap_or_else(|| "Use".into()),
    target_name: def
        .map(|d| d.name.clone())
        .unwrap_or_else(|| "Unknown".into()),
    target_x: entity.x,
    target_y: entity.y,
    actionable: true,
}
```

In the `GroundItem` arm (around line 97):
```rust
InteractionPrompt {
    verb: "Pick up".into(),
    target_name,
    target_x: item.x,
    target_y: item.y,
    actionable: true,
}
```

- [ ] **Step 9: Add cooldown_remaining and depleted to WorldEntityFrame**

In `src-tauri/src/item/types.rs`, add fields to `WorldEntityFrame`:

```rust
pub struct WorldEntityFrame {
    pub id: String,
    pub entity_type: String,
    pub name: String,
    pub sprite_class: String,
    pub x: f64,
    pub y: f64,
    pub cooldown_remaining: Option<f64>,
    pub depleted: bool,
}
```

Then update the construction site in `src-tauri/src/engine/state.rs` `build_entity_frames()` (around line 435):

```rust
WorldEntityFrame {
    id: e.id.clone(),
    entity_type: e.entity_type.clone(),
    name: def.map(|d| d.name.clone()).unwrap_or_default(),
    sprite_class: def.map(|d| d.sprite_class.clone()).unwrap_or_default(),
    x: e.x,
    y: e.y,
    cooldown_remaining: None,
    depleted: false,
}
```

- [ ] **Step 10: Update entities.json**

Add `maxHarvests` and `respawnSecs` to all 6 entity definitions in `assets/entities.json`:

```json
{
  "fruit_tree": {
    "name": "Fruit Tree",
    "verb": "Harvest",
    "yields": [{ "item": "cherry", "min": 1, "max": 3 }],
    "cooldownSecs": 5.0,
    "maxHarvests": 3,
    "respawnSecs": 30.0,
    "spriteClass": "tree_fruit",
    "interactRadius": 80
  },
  "chicken": {
    "name": "Chicken",
    "verb": "Squeeze",
    "yields": [{ "item": "grain", "min": 1, "max": 2 }],
    "cooldownSecs": 8.0,
    "maxHarvests": 2,
    "respawnSecs": 45.0,
    "spriteClass": "npc_chicken",
    "interactRadius": 60
  },
  "pig": {
    "name": "Pig",
    "verb": "Nibble",
    "yields": [{ "item": "meat", "min": 1, "max": 2 }],
    "cooldownSecs": 8.0,
    "maxHarvests": 2,
    "respawnSecs": 45.0,
    "spriteClass": "npc_pig",
    "interactRadius": 60
  },
  "butterfly": {
    "name": "Butterfly",
    "verb": "Milk",
    "yields": [{ "item": "milk", "min": 1, "max": 1 }],
    "cooldownSecs": 0.0,
    "maxHarvests": 1,
    "respawnSecs": 20.0,
    "spriteClass": "npc_butterfly",
    "interactRadius": 90
  },
  "bubble_tree": {
    "name": "Bubble Tree",
    "verb": "Harvest",
    "yields": [{ "item": "bubble", "min": 1, "max": 4 }],
    "cooldownSecs": 3.0,
    "maxHarvests": 4,
    "respawnSecs": 25.0,
    "spriteClass": "tree_bubble",
    "interactRadius": 80
  },
  "wood_tree": {
    "name": "Wood Tree",
    "verb": "Chop",
    "yields": [{ "item": "wood", "min": 1, "max": 2 }],
    "cooldownSecs": 6.0,
    "maxHarvests": 3,
    "respawnSecs": 35.0,
    "spriteClass": "tree_wood",
    "interactRadius": 80
  }
}
```

- [ ] **Step 11: Run all tests to verify nothing is broken**

Run: `cd src-tauri && cargo test`
Expected: ALL tests pass (existing behavior preserved, new fields have inert defaults)

- [ ] **Step 12: Commit**

```bash
cd src-tauri && cargo test && cd ..
git add src-tauri/src/item/types.rs src-tauri/src/item/interaction.rs src-tauri/src/engine/state.rs assets/entities.json
git commit -m "feat: add entity state data model types and JSON tuning values"
```

---

### Task 2: Game Time & Entity States on GameState

Add `game_time` and `entity_states` to `GameState`. Accumulate time in tick, clear states on street load.

**Files:**
- Modify: `src-tauri/src/engine/state.rs:17-36` (GameState struct), `:98-126` (new), `:128-145` (load_street), `:148` (tick)
- Test: `src-tauri/src/engine/state.rs` (existing test module)

- [ ] **Step 1: Write test for game_time accumulation**

In `src-tauri/src/engine/state.rs` test module, add:

```rust
#[test]
fn game_time_accumulates() {
    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
    state.load_street(test_street(), vec![]);
    let input = InputState::default();

    state.tick(0.5, &input, &mut rand::thread_rng());
    assert!((state.game_time - 0.5).abs() < 0.001);

    state.tick(0.25, &input, &mut rand::thread_rng());
    assert!((state.game_time - 0.75).abs() < 0.001);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test game_time_accumulates`
Expected: FAIL — `game_time` field not found on GameState

- [ ] **Step 3: Write test for entity_states clearing on load_street**

```rust
#[test]
fn entity_states_cleared_on_load_street() {
    use crate::item::types::EntityInstanceState;

    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
    state.entity_states.insert("tree_1".into(), EntityInstanceState::new(3));
    assert_eq!(state.entity_states.len(), 1);

    state.load_street(test_street(), vec![]);
    assert!(state.entity_states.is_empty());
}
```

- [ ] **Step 4: Add game_time and entity_states to GameState**

In `src-tauri/src/engine/state.rs`, add import for `EntityInstanceState`:

```rust
use crate::item::types::{
    EntityDefs, EntityInstanceState, InteractionPrompt, InventoryFrame, ItemDefs, ItemStackFrame,
    PickupFeedback, WorldEntity, WorldEntityFrame, WorldItem, WorldItemFrame,
};
```

Add fields to `GameState` struct (after `tsid_to_name`):

```rust
pub entity_states: std::collections::HashMap<String, EntityInstanceState>,
pub game_time: f64,
```

Initialize in `GameState::new()` (after `tsid_to_name` initialization):

```rust
entity_states: std::collections::HashMap::new(),
game_time: 0.0,
```

- [ ] **Step 5: Clear entity_states in load_street**

In `load_street()`, add after `self.pickup_feedback.clear();`:

```rust
self.entity_states.clear();
```

- [ ] **Step 6: Accumulate game_time in tick**

At the top of `tick()`, right after `let street = self.street.as_ref()?;`:

```rust
self.game_time += dt;
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd src-tauri && cargo test game_time_accumulates entity_states_cleared`
Expected: PASS

- [ ] **Step 8: Run full test suite**

Run: `cd src-tauri && cargo test`
Expected: ALL tests pass

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: add game_time clock and entity_states storage to GameState"
```

---

## Chunk 2: Interaction Logic & Prompts

### Task 3: Interaction Cooldown & Depletion Logic

Update `execute_interaction` to enforce cooldowns and depletion. This is the core behavioral change.

**Files:**
- Modify: `src-tauri/src/item/interaction.rs:1-6` (imports), `:121-129` (signature), `:137-177` (Entity arm)
- Modify: `src-tauri/src/engine/state.rs:281-289` (call site in tick)
- Test: `src-tauri/src/item/interaction.rs` (new + updated tests)

- [ ] **Step 1: Write failing tests for cooldown and depletion**

In `src-tauri/src/item/interaction.rs` test module, add `use std::collections::HashMap;` and `use crate::item::types::EntityInstanceState;` to the existing test imports. Then add these tests:

```rust
#[test]
fn harvest_decrements_harvests_remaining() {
    let item_defs = test_item_defs();
    let entity_defs = test_entity_defs();
    let mut inv = Inventory::new(16);
    let mut rng = StdRng::seed_from_u64(42);
    let mut entity_states = HashMap::new();

    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 0.0 };

    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 0.0,
    );

    let state = entity_states.get("t1").unwrap();
    assert_eq!(state.harvests_remaining, 2);
}

#[test]
fn interaction_rejected_during_cooldown() {
    let item_defs = test_item_defs();
    let entity_defs = test_entity_defs();
    let mut inv = Inventory::new(16);
    let mut rng = StdRng::seed_from_u64(42);
    let mut entity_states = HashMap::new();

    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 0.0 };

    // First harvest at t=0
    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 0.0,
    );
    let count_after_first = inv.slots[0].as_ref().unwrap().count;

    // Try again at t=2.0 (within 5s cooldown)
    let result = execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 2.0,
    );

    assert!(result.feedback.iter().any(|f| !f.success && f.text.contains("Available")));
    assert_eq!(inv.slots[0].as_ref().unwrap().count, count_after_first);
}

#[test]
fn cooldown_expires_after_cooldown_secs() {
    let item_defs = test_item_defs();
    let entity_defs = test_entity_defs();
    let mut inv = Inventory::new(16);
    let mut rng = StdRng::seed_from_u64(42);
    let mut entity_states = HashMap::new();

    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 0.0 };

    // Harvest at t=0
    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 0.0,
    );
    let count_after_first = inv.slots[0].as_ref().unwrap().count;

    // Harvest at t=5.0 (cooldown expired)
    let result = execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 5.0,
    );

    assert!(result.feedback.iter().any(|f| f.success));
    assert!(inv.slots[0].as_ref().unwrap().count > count_after_first);
}

#[test]
fn depletion_triggers_after_last_harvest() {
    let item_defs = test_item_defs();
    let entity_defs = test_entity_defs(); // max_harvests=3, cooldown_secs=5
    let mut inv = Inventory::new(16);
    let mut rng = StdRng::seed_from_u64(42);
    let mut entity_states = HashMap::new();

    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 0.0 };

    // 3 harvests at t=0, 5, 10 (each after cooldown expires)
    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 0.0,
    );
    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 5.0,
    );
    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 10.0,
    );

    // Entity should now be depleted
    let state = entity_states.get("t1").unwrap();
    assert!(state.depleted_until > 10.0);
    assert_eq!(state.harvests_remaining, 3); // pre-set for respawn

    // Try at t=15 (within 30s respawn)
    let result = execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 15.0,
    );
    assert!(result.feedback.iter().any(|f| !f.success && f.text.contains("Regrowing")));
}

#[test]
fn full_cycle_harvest_deplete_respawn() {
    let item_defs = test_item_defs();
    let entity_defs = test_entity_defs(); // max_harvests=3, cooldown=5, respawn=30
    let mut inv = Inventory::new(16);
    let mut rng = StdRng::seed_from_u64(42);
    let mut entity_states = HashMap::new();

    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 0.0 };

    // Exhaust all 3 harvests
    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 0.0,
    );
    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 5.0,
    );
    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 10.0,
    );

    // Depleted at t=10, respawn at t=40 (10 + 30)
    // Try at t=40 — should work again
    let result = execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 40.0,
    );
    assert!(result.feedback.iter().any(|f| f.success));

    let state = entity_states.get("t1").unwrap();
    assert_eq!(state.harvests_remaining, 2); // was 3, decremented to 2
}

#[test]
fn lazy_init_creates_state_on_first_interaction() {
    let item_defs = test_item_defs();
    let entity_defs = test_entity_defs();
    let mut inv = Inventory::new(16);
    let mut rng = StdRng::seed_from_u64(42);
    let mut entity_states = HashMap::new();

    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 0.0 };

    assert!(!entity_states.contains_key("t1"));

    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 0.0,
    );

    assert!(entity_states.contains_key("t1"));
}

#[test]
fn max_harvests_zero_means_no_depletion() {
    let item_defs = test_item_defs();
    let mut entity_defs = EntityDefs::new();
    entity_defs.insert("infinite".into(), EntityDef {
        id: "infinite".into(),
        name: "Infinite".into(),
        verb: "Use".into(),
        yields: vec![YieldEntry { item: "cherry".into(), min: 1, max: 1 }],
        cooldown_secs: 0.0,
        max_harvests: 0,
        respawn_secs: 0.0,
        sprite_class: "test".into(),
        interact_radius: 80.0,
    });
    let mut inv = Inventory::new(16);
    let mut rng = StdRng::seed_from_u64(42);
    let mut entity_states = HashMap::new();

    let entities = vec![WorldEntity {
        id: "inf1".into(),
        entity_type: "infinite".into(),
        x: 0.0,
        y: 0.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 0.0 };

    // Harvest 10 times — should never deplete
    for i in 0..10 {
        let result = execute_interaction(
            &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
            &mut rng, &mut entity_states, i as f64,
        );
        assert!(result.feedback.iter().any(|f| f.success),
            "Harvest {} should succeed", i);
    }

    let state = entity_states.get("inf1").unwrap();
    assert_eq!(state.depleted_until, 0.0);
}

#[test]
fn instant_respawn_when_respawn_secs_zero() {
    let item_defs = test_item_defs();
    let mut entity_defs = EntityDefs::new();
    entity_defs.insert("fast".into(), EntityDef {
        id: "fast".into(),
        name: "Fast".into(),
        verb: "Use".into(),
        yields: vec![YieldEntry { item: "cherry".into(), min: 1, max: 1 }],
        cooldown_secs: 0.0,
        max_harvests: 1,
        respawn_secs: 0.0,
        sprite_class: "test".into(),
        interact_radius: 80.0,
    });
    let mut inv = Inventory::new(16);
    let mut rng = StdRng::seed_from_u64(42);
    let mut entity_states = HashMap::new();

    let entities = vec![WorldEntity {
        id: "f1".into(),
        entity_type: "fast".into(),
        x: 0.0,
        y: 0.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 0.0 };

    // First harvest depletes (max_harvests=1)
    execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 0.0,
    );

    // Immediately available (respawn_secs=0, so depleted_until = 0.0)
    let result = execute_interaction(
        &nearest, &mut inv, &entities, &entity_defs, &[], &item_defs,
        &mut rng, &mut entity_states, 0.0,
    );
    assert!(result.feedback.iter().any(|f| f.success));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test harvest_decrements`
Expected: FAIL — signature mismatch (execute_interaction doesn't take entity_states/game_time yet)

- [ ] **Step 3: Update execute_interaction signature and add cooldown/depletion logic**

In `src-tauri/src/item/interaction.rs`, add to imports:

```rust
use std::collections::HashMap;
use crate::item::types::{
    EntityDefs, EntityInstanceState, InteractionPrompt, ItemDefs, PickupFeedback, WorldEntity, WorldItem,
};
```

Update `execute_interaction` signature and add cooldown/depletion logic in the Entity arm:

```rust
pub fn execute_interaction(
    nearest: &NearestInteractable,
    inventory: &mut Inventory,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
    item_defs: &ItemDefs,
    rng: &mut impl Rng,
    entity_states: &mut HashMap<String, EntityInstanceState>,
    game_time: f64,
) -> InteractionResult {
    let mut result = InteractionResult {
        feedback: vec![],
        spawned_items: vec![],
        remove_ground_item: None,
        update_ground_item: None,
    };

    match nearest {
        NearestInteractable::Entity { index, .. } => {
            let entity = &entities[*index];
            let Some(def) = entity_defs.get(&entity.entity_type) else {
                return result;
            };

            // Lazy-init entity state on first interaction
            let state = entity_states
                .entry(entity.id.clone())
                .or_insert_with(|| EntityInstanceState::new(def.max_harvests));

            // 1. Depletion check
            if state.depleted_until > game_time {
                let remaining = (state.depleted_until - game_time).ceil() as u32;
                result.feedback.push(PickupFeedback {
                    id: 0,
                    text: format!("Regrowing... ({}s)", remaining),
                    success: false,
                    x: entity.x,
                    y: entity.y,
                    age_secs: 0.0,
                });
                return result;
            }

            // 2. Cooldown check
            if state.cooldown_until > game_time {
                let remaining = (state.cooldown_until - game_time).ceil() as u32;
                result.feedback.push(PickupFeedback {
                    id: 0,
                    text: format!("Available in {}s", remaining),
                    success: false,
                    x: entity.x,
                    y: entity.y,
                    age_secs: 0.0,
                });
                return result;
            }

            // Harvest yields
            for yield_entry in &def.yields {
                let count = rng.gen_range(yield_entry.min..=yield_entry.max);
                let overflow = inventory.add(&yield_entry.item, count, item_defs);
                let added = count - overflow;

                if added > 0 {
                    let name = item_defs
                        .get(&yield_entry.item)
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| yield_entry.item.clone());
                    result.feedback.push(PickupFeedback {
                        id: 0,
                        text: format!("+{} x{}", name, added),
                        success: true,
                        x: entity.x,
                        y: entity.y,
                        age_secs: 0.0,
                    });
                }

                if overflow > 0 {
                    result
                        .spawned_items
                        .push((yield_entry.item.clone(), overflow, entity.x, entity.y));
                    result.feedback.push(PickupFeedback {
                        id: 0,
                        text: "Inventory full!".into(),
                        success: false,
                        x: entity.x,
                        y: entity.y,
                        age_secs: 0.0,
                    });
                }
            }

            // 3. Post-harvest state update
            if def.max_harvests > 0 {
                state.harvests_remaining -= 1;
                if state.harvests_remaining == 0 {
                    state.depleted_until = game_time + def.respawn_secs;
                    state.harvests_remaining = def.max_harvests;
                } else {
                    state.cooldown_until = game_time + def.cooldown_secs;
                }
            } else {
                // max_harvests == 0: infinite mode, cooldown only
                state.cooldown_until = game_time + def.cooldown_secs;
            }
        }
        NearestInteractable::GroundItem { index, .. } => {
            // ... (unchanged ground item logic)
```

The `GroundItem` arm stays exactly as-is.

- [ ] **Step 4: Update existing test call sites in interaction.rs**

Update ALL existing `execute_interaction` calls in the interaction.rs test module to pass the new parameters. Each call gets `&mut entity_states, 0.0` appended:

For `execute_entity_interaction_adds_to_inventory`, `execute_entity_interaction_overflows_to_ground`: add `let mut entity_states = HashMap::new();` and append `&mut entity_states, 0.0` to each call.

For `execute_ground_item_pickup`, `execute_ground_item_partial_pickup`: same treatment (ground item path ignores these params).

- [ ] **Step 5: Update call site in state.rs tick()**

In `src-tauri/src/engine/state.rs`, update the `execute_interaction` call in `tick()` (around line 281):

```rust
let result = interaction::execute_interaction(
    nearest,
    &mut self.inventory,
    &self.world_entities,
    &self.entity_defs,
    &self.world_items,
    &self.item_defs,
    rng,
    &mut self.entity_states,
    self.game_time,
);
```

- [ ] **Step 6: Remove the deferred-phase comment**

In `src-tauri/src/item/interaction.rs`, remove lines 119-120:
```rust
/// Note: `cooldownSecs` on EntityDef is parsed but not enforced yet — cooldowns
/// and entity state/depletion are deferred to a future phase.
```

- [ ] **Step 7: Run new tests**

Run: `cd src-tauri && cargo test harvest_decrements interaction_rejected_during_cooldown cooldown_expires depletion_triggers full_cycle lazy_init max_harvests_zero instant_respawn`
Expected: ALL pass

- [ ] **Step 8: Run full test suite**

Run: `cd src-tauri && cargo test`
Expected: ALL tests pass

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/item/interaction.rs src-tauri/src/engine/state.rs
git commit -m "feat: enforce entity cooldowns and depletion in execute_interaction"
```

---

### Task 4: State-Aware Interaction Prompt

Update `build_prompt` to show cooldown/depletion status text and set `actionable: false` when the entity is unavailable.

**Files:**
- Modify: `src-tauri/src/item/interaction.rs:65-105` (build_prompt)
- Modify: `src-tauri/src/engine/state.rs:263-271` (call site in tick)
- Test: `src-tauri/src/item/interaction.rs`

- [ ] **Step 1: Write failing tests for state-aware prompt**

In `src-tauri/src/item/interaction.rs` test module, add:

```rust
#[test]
fn build_prompt_shows_cooldown_status() {
    let entity_defs = test_entity_defs();
    let item_defs = test_item_defs();
    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 100.0,
        y: -2.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 10.0 };

    // Entity on cooldown until t=5.0
    let mut entity_states = HashMap::new();
    entity_states.insert("t1".into(), EntityInstanceState {
        harvests_remaining: 2,
        cooldown_until: 5.0,
        depleted_until: 0.0,
    });

    let prompt = build_prompt(
        &nearest, &entities, &entity_defs, &[], &item_defs,
        &entity_states, 2.0,
    );
    assert!(!prompt.actionable);
    assert!(prompt.verb.contains("Available"));
    assert!(prompt.verb.contains("3")); // ceil(5.0 - 2.0) = 3
    assert!(prompt.target_name.is_empty());
}

#[test]
fn build_prompt_shows_depleted_status() {
    let entity_defs = test_entity_defs();
    let item_defs = test_item_defs();
    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 100.0,
        y: -2.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 10.0 };

    let mut entity_states = HashMap::new();
    entity_states.insert("t1".into(), EntityInstanceState {
        harvests_remaining: 3,
        cooldown_until: 0.0,
        depleted_until: 40.0,
    });

    let prompt = build_prompt(
        &nearest, &entities, &entity_defs, &[], &item_defs,
        &entity_states, 12.0,
    );
    assert!(!prompt.actionable);
    assert!(prompt.verb.contains("Regrowing"));
    assert!(prompt.verb.contains("28")); // ceil(40.0 - 12.0) = 28
}

#[test]
fn build_prompt_ready_entity_is_actionable() {
    let entity_defs = test_entity_defs();
    let item_defs = test_item_defs();
    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 100.0,
        y: -2.0,
    }];
    let nearest = NearestInteractable::Entity { index: 0, distance: 10.0 };
    let entity_states = HashMap::new(); // no state = ready

    let prompt = build_prompt(
        &nearest, &entities, &entity_defs, &[], &item_defs,
        &entity_states, 0.0,
    );
    assert!(prompt.actionable);
    assert_eq!(prompt.verb, "Harvest");
    assert_eq!(prompt.target_name, "Fruit Tree");
}

#[test]
fn build_prompt_ground_item_always_actionable() {
    let entity_defs = test_entity_defs();
    let item_defs = test_item_defs();
    let items = vec![WorldItem {
        id: "i1".into(),
        item_id: "cherry".into(),
        count: 1,
        x: 50.0,
        y: 0.0,
    }];
    let nearest = NearestInteractable::GroundItem { index: 0, distance: 5.0 };
    let entity_states = HashMap::new();

    let prompt = build_prompt(
        &nearest, &[], &entity_defs, &items, &item_defs,
        &entity_states, 0.0,
    );
    assert!(prompt.actionable);
    assert_eq!(prompt.verb, "Pick up");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test build_prompt_shows_cooldown`
Expected: FAIL — signature mismatch

- [ ] **Step 3: Update build_prompt signature and implementation**

```rust
pub fn build_prompt(
    nearest: &NearestInteractable,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
    item_defs: &ItemDefs,
    entity_states: &HashMap<String, EntityInstanceState>,
    game_time: f64,
) -> InteractionPrompt {
    match nearest {
        NearestInteractable::Entity { index, .. } => {
            let entity = &entities[*index];
            let def = entity_defs.get(&entity.entity_type);

            // Check entity state for cooldown/depletion
            if let Some(state) = entity_states.get(&entity.id) {
                if state.depleted_until > game_time {
                    let remaining = (state.depleted_until - game_time).ceil() as u32;
                    return InteractionPrompt {
                        verb: format!("Regrowing... ({}s)", remaining),
                        target_name: String::new(),
                        target_x: entity.x,
                        target_y: entity.y,
                        actionable: false,
                    };
                }
                if state.cooldown_until > game_time {
                    let remaining = (state.cooldown_until - game_time).ceil() as u32;
                    return InteractionPrompt {
                        verb: format!("Available in {}s", remaining),
                        target_name: String::new(),
                        target_x: entity.x,
                        target_y: entity.y,
                        actionable: false,
                    };
                }
            }

            InteractionPrompt {
                verb: def.map(|d| d.verb.clone()).unwrap_or_else(|| "Use".into()),
                target_name: def
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| "Unknown".into()),
                target_x: entity.x,
                target_y: entity.y,
                actionable: true,
            }
        }
        NearestInteractable::GroundItem { index, .. } => {
            let item = &world_items[*index];
            let name = item_defs
                .get(&item.item_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "Item".into());
            let target_name = if item.count > 1 {
                format!("{} x{}", name, item.count)
            } else {
                name
            };
            InteractionPrompt {
                verb: "Pick up".into(),
                target_name,
                target_x: item.x,
                target_y: item.y,
                actionable: true,
            }
        }
    }
}
```

- [ ] **Step 4: Update existing build_prompt test call sites**

Update `build_prompt_for_entity` and `build_prompt_for_ground_item` tests to pass the new parameters:

```rust
let entity_states = HashMap::new();
let prompt = build_prompt(&nearest, &entities, &entity_defs, &[], &item_defs, &entity_states, 0.0);
```

- [ ] **Step 5: Update build_prompt call site in state.rs tick()**

In `src-tauri/src/engine/state.rs`, update the `build_prompt` call (around line 263):

```rust
let interaction_prompt = nearest.as_ref().map(|n| {
    interaction::build_prompt(
        n,
        &self.world_entities,
        &self.entity_defs,
        &self.world_items,
        &self.item_defs,
        &self.entity_states,
        self.game_time,
    )
});
```

- [ ] **Step 6: Run all prompt tests**

Run: `cd src-tauri && cargo test build_prompt`
Expected: ALL pass (4 new + 2 existing)

- [ ] **Step 7: Run full test suite**

Run: `cd src-tauri && cargo test`
Expected: ALL tests pass

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/item/interaction.rs src-tauri/src/engine/state.rs
git commit -m "feat: state-aware interaction prompts with actionable field"
```

---

## Chunk 3: Frame Building & Frontend

### Task 5: Entity Frame Building with Cooldown State

Update `build_entity_frames` to compute `cooldown_remaining` and `depleted` from entity state.

**Files:**
- Modify: `src-tauri/src/engine/state.rs:430-445` (build_entity_frames)
- Test: `src-tauri/src/engine/state.rs`

- [ ] **Step 1: Write failing test for entity frame cooldown fields**

In `src-tauri/src/engine/state.rs` test module, add:

```rust
#[test]
fn entity_frame_includes_cooldown_remaining() {
    use crate::item::types::{EntityDef, EntityInstanceState, ItemDef, YieldEntry, WorldEntity};
    use rand::SeedableRng;

    let mut item_defs = ItemDefs::new();
    item_defs.insert("cherry".into(), ItemDef {
        id: "cherry".into(),
        name: "Cherry".into(),
        description: "".into(),
        category: "food".into(),
        stack_limit: 50,
        icon: "cherry".into(),
    });
    let mut entity_defs = EntityDefs::new();
    entity_defs.insert("fruit_tree".into(), EntityDef {
        id: "fruit_tree".into(),
        name: "Fruit Tree".into(),
        verb: "Harvest".into(),
        yields: vec![YieldEntry { item: "cherry".into(), min: 1, max: 1 }],
        cooldown_secs: 5.0,
        max_harvests: 3,
        respawn_secs: 30.0,
        sprite_class: "tree_fruit".into(),
        interact_radius: 80.0,
    });

    let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs);
    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    state.load_street(test_street(), entities);
    state.player.x = 0.0;
    state.player.on_ground = true;

    // Harvest the entity
    let input = InputState { interact: true, ..Default::default() };
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();

    // After harvest, entity should have cooldown remaining
    let entity_frame = &frame.world_entities[0];
    assert!(entity_frame.cooldown_remaining.is_some());
    assert!(!entity_frame.depleted);

    // Advance past cooldown (tick with no interact)
    let input = InputState::default();
    let mut last_frame = None;
    for _ in 0..400 {
        last_frame = state.tick(1.0 / 60.0, &input, &mut rng);
    }
    let frame = last_frame.unwrap();
    let entity_frame = &frame.world_entities[0];
    assert!(entity_frame.cooldown_remaining.is_none());
    assert!(!entity_frame.depleted);
}

#[test]
fn entity_frame_shows_depleted() {
    use crate::item::types::{EntityDef, EntityInstanceState, ItemDef, YieldEntry, WorldEntity};

    let mut item_defs = ItemDefs::new();
    item_defs.insert("cherry".into(), ItemDef {
        id: "cherry".into(),
        name: "Cherry".into(),
        description: "".into(),
        category: "food".into(),
        stack_limit: 50,
        icon: "cherry".into(),
    });
    let mut entity_defs = EntityDefs::new();
    entity_defs.insert("fruit_tree".into(), EntityDef {
        id: "fruit_tree".into(),
        name: "Fruit Tree".into(),
        verb: "Harvest".into(),
        yields: vec![YieldEntry { item: "cherry".into(), min: 1, max: 1 }],
        cooldown_secs: 0.0,
        max_harvests: 1,
        respawn_secs: 30.0,
        sprite_class: "tree_fruit".into(),
        interact_radius: 80.0,
    });

    let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs);
    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    state.load_street(test_street(), entities);
    state.player.x = 0.0;
    state.player.on_ground = true;

    // Single harvest depletes (max_harvests=1)
    let input = InputState { interact: true, ..Default::default() };
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    state.tick(1.0 / 60.0, &input, &mut rng);

    // Next frame should show depleted
    let input = InputState::default();
    let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();
    let entity_frame = &frame.world_entities[0];
    assert!(entity_frame.cooldown_remaining.is_some());
    assert!(entity_frame.depleted);
}
```

Also add an integration test for prompt text flowing through `tick()` during cooldown:

```rust
#[test]
fn prompt_shows_cooldown_text_through_tick() {
    use crate::item::types::{EntityDef, EntityInstanceState, ItemDef, YieldEntry, WorldEntity};
    use rand::SeedableRng;

    let mut item_defs = ItemDefs::new();
    item_defs.insert("cherry".into(), ItemDef {
        id: "cherry".into(),
        name: "Cherry".into(),
        description: "".into(),
        category: "food".into(),
        stack_limit: 50,
        icon: "cherry".into(),
    });
    let mut entity_defs = EntityDefs::new();
    entity_defs.insert("fruit_tree".into(), EntityDef {
        id: "fruit_tree".into(),
        name: "Fruit Tree".into(),
        verb: "Harvest".into(),
        yields: vec![YieldEntry { item: "cherry".into(), min: 1, max: 1 }],
        cooldown_secs: 5.0,
        max_harvests: 3,
        respawn_secs: 30.0,
        sprite_class: "tree_fruit".into(),
        interact_radius: 80.0,
    });

    let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs);
    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    state.load_street(test_street(), entities);
    state.player.x = 0.0;
    state.player.on_ground = true;

    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    // Harvest on first tick
    let input = InputState { interact: true, ..Default::default() };
    state.tick(1.0 / 60.0, &input, &mut rng);

    // Next tick: still near entity, prompt should show cooldown text
    let input = InputState::default();
    let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();
    let prompt = frame.interaction_prompt.unwrap();
    assert!(!prompt.actionable);
    assert!(prompt.verb.contains("Available"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test entity_frame_includes_cooldown prompt_shows_cooldown_text`
Expected: FAIL — `cooldown_remaining` is always `None` (hardcoded in Task 1), prompt doesn't reflect state yet

- [ ] **Step 3: Update build_entity_frames to compute cooldown state**

In `src-tauri/src/engine/state.rs`, replace the `build_entity_frames` method:

```rust
fn build_entity_frames(&self) -> Vec<WorldEntityFrame> {
    self.world_entities
        .iter()
        .map(|e| {
            let def = self.entity_defs.get(&e.entity_type);

            let (cooldown_remaining, depleted) = if let Some(state) = self.entity_states.get(&e.id) {
                let remaining = (state.cooldown_until.max(state.depleted_until)) - self.game_time;
                if remaining > 0.0 {
                    (Some(remaining), state.depleted_until > self.game_time)
                } else {
                    (None, false)
                }
            } else {
                (None, false)
            };

            WorldEntityFrame {
                id: e.id.clone(),
                entity_type: e.entity_type.clone(),
                name: def.map(|d| d.name.clone()).unwrap_or_default(),
                sprite_class: def.map(|d| d.sprite_class.clone()).unwrap_or_default(),
                x: e.x,
                y: e.y,
                cooldown_remaining,
                depleted,
            }
        })
        .collect()
}
```

- [ ] **Step 4: Run frame building and prompt integration tests**

Run: `cd src-tauri && cargo test entity_frame_includes entity_frame_shows prompt_shows_cooldown_text`
Expected: PASS

- [ ] **Step 5: Run full test suite**

Run: `cd src-tauri && cargo test`
Expected: ALL tests pass

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: entity frames include cooldown_remaining and depleted fields"
```

---

### Task 6: Frontend Types & Rendering

Update TypeScript interfaces and PixiJS renderer for opacity fade and conditional `[E]` prefix.

**Files:**
- Modify: `src/lib/types.ts:182-189` (WorldEntityFrame), `:201-206` (InteractionPrompt)
- Modify: `src/lib/engine/renderer.ts:284-310` (entity rendering), `:362-371` (prompt display)

- [ ] **Step 1: Update TypeScript interfaces**

In `src/lib/types.ts`, update `WorldEntityFrame`:

```typescript
export interface WorldEntityFrame {
  id: string;
  entityType: string;
  name: string;
  spriteClass: string;
  x: number;
  y: number;
  cooldownRemaining: number | null;
  depleted: boolean;
}
```

Update `InteractionPrompt`:

```typescript
export interface InteractionPrompt {
  verb: string;
  targetName: string;
  targetX: number;
  targetY: number;
  actionable: boolean;
}
```

- [ ] **Step 2: Update entity sprite opacity in renderer.ts**

In `src/lib/engine/renderer.ts`, in the entity rendering loop (around line 308, after sprite position is set), add opacity logic:

```typescript
sprite.x = entity.x - this.street.left;
sprite.y = entity.y - this.street.top;

// Opacity based on entity state
if (entity.cooldownRemaining != null) {
  sprite.alpha = entity.depleted ? 0.25 : 0.5;
} else {
  sprite.alpha = 1.0;
}
```

Note: Remove the hardcoded `alpha: 0.8` from the `body.fill()` call (around line 294) and replace it with `alpha: 1.0` so the container-level alpha controls opacity instead.

- [ ] **Step 3: Update prompt display for actionable field**

In `src/lib/engine/renderer.ts`, update the prompt text rendering (around line 364):

```typescript
if (frame.interactionPrompt && this.promptText) {
  const p = frame.interactionPrompt;
  this.promptText.text = p.actionable
    ? `[E] ${p.verb} ${p.targetName}`
    : p.verb;
  const screenX = p.targetX - this.street.left + this.worldContainer.x;
  const screenY = p.targetY - this.street.top + this.worldContainer.y - 90;
  this.promptText.x = screenX;
  this.promptText.y = screenY;
  this.promptText.visible = true;
} else if (this.promptText) {
  this.promptText.visible = false;
}
```

- [ ] **Step 4: Verify frontend builds**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: Build succeeds with no TypeScript errors

- [ ] **Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/engine/renderer.ts
git commit -m "feat: frontend entity opacity fade and conditional [E] prompt prefix"
```

---

## Final Verification

After all tasks are complete:

- [ ] **Run full Rust test suite**: `cd src-tauri && cargo test`
- [ ] **Run Rust linter**: `cd src-tauri && cargo clippy`
- [ ] **Run frontend build**: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
- [ ] **Run frontend tests** (if any): `npx vitest run`
- [ ] **Manual smoke test**: `npm run tauri dev` — harvest an entity 3 times, observe cooldown timer in prompt, observe depletion message and opacity fade, wait for respawn, harvest again
