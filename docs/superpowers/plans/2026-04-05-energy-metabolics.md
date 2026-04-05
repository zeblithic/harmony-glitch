# Energy Metabolics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add energy as a player metabolic stat — actions cost energy, food restores it, zero energy blocks harvesting.

**Architecture:** Energy is an `f64` on `GameState` that decays per-tick and is consumed by harvest interactions. A new `eat_item` IPC command lets players consume food from inventory. Frontend gets an `EnergyHud` (top-left) and a "Use" button on food items in InventoryPanel.

**Tech Stack:** Rust (Tauri v2), Svelte 5 (runes), TypeScript, Vitest

**Spec:** `docs/superpowers/specs/2026-04-05-energy-metabolics-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `src-tauri/src/item/types.rs` | Modify | Add `energy_value` to `ItemDef` and `ItemStackFrame` |
| `src-tauri/src/item/energy.rs` | Create | `eat()` function — validate and consume food |
| `src-tauri/src/item/mod.rs` | Modify | Add `pub mod energy;` |
| `src-tauri/src/item/interaction.rs` | Modify | Add energy gate before harvest |
| `src-tauri/src/engine/state.rs` | Modify | Energy fields on `GameState`/`SaveState`/`RenderFrame`, tick decay, `build_inventory_frame` |
| `src-tauri/src/lib.rs` | Modify | `eat_item` IPC command |
| `assets/items.json` | Modify | Add `energyValue` to food items |
| `src/lib/types.ts` | Modify | Add energy fields to `RenderFrame`, `ItemStackFrame`, `SavedState` |
| `src/lib/ipc.ts` | Modify | Add `eatItem()` function |
| `src/lib/components/EnergyHud.svelte` | Create | Energy bar display (top-left) |
| `src/lib/components/EnergyHud.test.ts` | Create | Tests for energy bar |
| `src/lib/components/InventoryPanel.svelte` | Modify | Add "Use" button for food items |
| `src/lib/components/InventoryPanel.test.ts` | Modify | Add tests for "Use" button |
| `src/App.svelte` | Modify | Wire EnergyHud + eat handler |

---

### Task 1: Add `energy_value` to ItemDef and items.json

**Files:**
- Modify: `src-tauri/src/item/types.rs:8-18`
- Modify: `assets/items.json`
- Modify: `src-tauri/src/item/vendor.rs:112-138` (update test helper)

- [ ] **Step 1: Write failing tests for ItemDef energy_value parsing**

In `src-tauri/src/item/types.rs`, add to the existing `#[cfg(test)] mod tests` block at the bottom of the file:

```rust
#[test]
fn item_def_with_energy_value() {
    let json = r#"{"name":"Cherry","description":"A cherry.","category":"food","stackLimit":50,"icon":"cherry","baseCost":3,"energyValue":12}"#;
    let def: ItemDef = serde_json::from_str(json).unwrap();
    assert_eq!(def.energy_value, Some(12));
}

#[test]
fn item_def_without_energy_value() {
    let json = r#"{"name":"Wood","description":"Wood.","category":"material","stackLimit":50,"icon":"wood","baseCost":4}"#;
    let def: ItemDef = serde_json::from_str(json).unwrap();
    assert_eq!(def.energy_value, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test item_def_with_energy_value item_def_without_energy_value -- --nocapture`
Expected: FAIL — `ItemDef` has no field `energy_value`

- [ ] **Step 3: Add `energy_value` field to `ItemDef`**

In `src-tauri/src/item/types.rs`, add after `base_cost` (line 17):

```rust
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
}
```

- [ ] **Step 4: Update test helpers in vendor.rs to include `energy_value`**

In `src-tauri/src/item/vendor.rs`, update the `test_item_defs()` function (around line 112). Every `ItemDef` in test helpers needs the new field:

```rust
fn test_item_defs() -> ItemDefs {
    let mut defs = HashMap::new();
    defs.insert(
        "cherry".to_string(),
        ItemDef {
            id: "cherry".to_string(),
            name: "Cherry".to_string(),
            description: "A tasty cherry.".to_string(),
            category: "food".to_string(),
            stack_limit: 50,
            icon: "cherry".to_string(),
            base_cost: Some(3),
            energy_value: Some(12),
        },
    );
    defs.insert(
        "quest_item".to_string(),
        ItemDef {
            id: "quest_item".to_string(),
            name: "Quest Item".to_string(),
            description: "Cannot be bought or sold.".to_string(),
            category: "quest".to_string(),
            stack_limit: 1,
            icon: "quest_item".to_string(),
            base_cost: None,
            energy_value: None,
        },
    );
    defs
}
```

Also update the `sell_price_minimum_one` test's inline ItemDef (around line 161):

```rust
defs.insert(
    "cheap_item".to_string(),
    ItemDef {
        id: "cheap_item".to_string(),
        name: "Cheap Item".to_string(),
        description: "Very cheap.".to_string(),
        category: "misc".to_string(),
        stack_limit: 10,
        icon: "cheap_item".to_string(),
        base_cost: Some(1),
        energy_value: None,
    },
);
```

And the `buy_item_not_in_vendor_inventory` test's inline ItemDef (around line 225):

```rust
defs.insert(
    "rare_gem".to_string(),
    ItemDef {
        id: "rare_gem".to_string(),
        name: "Rare Gem".to_string(),
        description: "Not for sale here.".to_string(),
        category: "gem".to_string(),
        stack_limit: 5,
        icon: "gem".to_string(),
        base_cost: Some(100),
        energy_value: None,
    },
);
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -- --nocapture`
Expected: ALL PASS (including new energy_value tests and existing vendor tests)

- [ ] **Step 6: Add `energyValue` to food items in items.json**

Update `assets/items.json` — add `"energyValue"` to each food item. Non-food items don't get it (field is optional, defaults to `None`/absent):

```json
{
  "cherry": {
    "name": "Cherry",
    "description": "A plump, juicy cherry from a Fruit Tree.",
    "category": "food",
    "stackLimit": 50,
    "icon": "cherry",
    "baseCost": 3,
    "energyValue": 12
  },
  "grain": {
    "name": "Grain",
    "description": "Squeezed from a chicken. Don't ask.",
    "category": "food",
    "stackLimit": 50,
    "icon": "grain",
    "baseCost": 3,
    "energyValue": 10
  },
  "meat": {
    "name": "Meat",
    "description": "Nibbled off a friendly pig. It didn't seem to mind.",
    "category": "food",
    "stackLimit": 50,
    "icon": "meat",
    "baseCost": 5,
    "energyValue": 20
  },
  "milk": {
    "name": "Milk",
    "description": "Milked from a butterfly. Surprisingly creamy.",
    "category": "food",
    "stackLimit": 50,
    "icon": "milk",
    "baseCost": 4,
    "energyValue": 15
  },
  "bubble": {
    "name": "Bubble",
    "description": "A shimmering bubble harvested from a Bubble Tree.",
    "category": "material",
    "stackLimit": 50,
    "icon": "bubble",
    "baseCost": 2
  },
  "wood": {
    "name": "Wood",
    "description": "A sturdy plank from a Wood Tree.",
    "category": "material",
    "stackLimit": 50,
    "icon": "wood",
    "baseCost": 4
  },
  "cherry_pie": {
    "name": "Cherry Pie",
    "description": "A delicious pie baked with fresh cherries.",
    "category": "food",
    "stackLimit": 10,
    "icon": "cherry_pie",
    "baseCost": 20,
    "energyValue": 100
  },
  "bread": {
    "name": "Bread",
    "description": "Simple baked bread. Hearty and filling.",
    "category": "food",
    "stackLimit": 20,
    "icon": "bread",
    "baseCost": 16,
    "energyValue": 80
  },
  "steak": {
    "name": "Steak",
    "description": "Grilled meat on a wood fire. Savory.",
    "category": "food",
    "stackLimit": 10,
    "icon": "steak",
    "baseCost": 22,
    "energyValue": 90
  },
  "butter": {
    "name": "Butter",
    "description": "Churned from fresh butterfly milk.",
    "category": "food",
    "stackLimit": 20,
    "icon": "butter",
    "baseCost": 15,
    "energyValue": 60
  },
  "bubble_wand": {
    "name": "Bubble Wand",
    "description": "A wand for blowing iridescent bubbles.",
    "category": "tool",
    "stackLimit": 1,
    "icon": "bubble_wand",
    "baseCost": 18
  },
  "plank": {
    "name": "Plank",
    "description": "Processed lumber. Useful for building.",
    "category": "material",
    "stackLimit": 50,
    "icon": "plank",
    "baseCost": 12
  },
  "pot": {
    "name": "Pot",
    "description": "A sturdy cooking pot. Required for most recipes.",
    "category": "tool",
    "stackLimit": 1,
    "icon": "pot",
    "baseCost": 25
  }
}
```

- [ ] **Step 7: Run all tests**

Run: `cd src-tauri && cargo test -- --nocapture`
Expected: ALL PASS

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/item/types.rs src-tauri/src/item/vendor.rs assets/items.json
git commit -m "feat(energy): add energy_value to ItemDef and items.json"
```

---

### Task 2: Energy state on GameState, SaveState, RenderFrame

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

- [ ] **Step 1: Write failing tests for energy on SaveState**

In `src-tauri/src/engine/state.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn save_state_energy_default() {
    // Missing "energy" field should default to 600.0
    let json = r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50}"#;
    let save: SaveState = serde_json::from_str(json).unwrap();
    assert_eq!(save.energy, 600.0);
}

#[test]
fn save_state_energy_round_trip() {
    let save = SaveState {
        street_id: "demo".to_string(),
        x: 0.0,
        y: 0.0,
        facing: Direction::Right,
        inventory: vec![],
        avatar: AvatarAppearance::default(),
        currants: 50,
        energy: 123.4,
    };
    let json = serde_json::to_string(&save).unwrap();
    let restored: SaveState = serde_json::from_str(&json).unwrap();
    assert!((restored.energy - 123.4).abs() < f64::EPSILON);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test save_state_energy -- --nocapture`
Expected: FAIL — `SaveState` has no field `energy`

- [ ] **Step 3: Add energy to SaveState, GameState, and RenderFrame**

In `src-tauri/src/engine/state.rs`:

Add the default function near the existing `default_currants` (around line 20):

```rust
fn default_energy() -> f64 {
    600.0
}
```

Add `energy` to `SaveState` (after line 36, the `currants` field):

```rust
#[serde(default = "default_energy")]
pub energy: f64,
```

Add `energy` and `max_energy` to `GameState` (after line 67, the `store_catalog` field):

```rust
pub energy: f64,
pub max_energy: f64,
```

Add `energy` and `max_energy` to `RenderFrame` (after line 97, the `currants` field):

```rust
pub energy: f64,
pub max_energy: f64,
```

- [ ] **Step 4: Update `GameState::new()` to initialize energy**

In `src-tauri/src/engine/state.rs`, update `GameState::new()` (around line 132). Add the `energy` and `max_energy` fields at the end of the `Self { ... }` block, after `store_catalog`:

```rust
energy: 600.0,
max_energy: 600.0,
```

- [ ] **Step 5: Update `save_state()` to include energy**

In `src-tauri/src/engine/state.rs`, update `save_state()` (around line 700). Add after the `currants` field in the `SaveState { ... }` constructor:

```rust
energy: self.energy,
```

- [ ] **Step 6: Update `restore_save()` to restore energy**

In `src-tauri/src/engine/state.rs`, update `restore_save()` (around line 719). Add after `self.currants = save.currants;` (line 743):

```rust
self.energy = save.energy;
```

- [ ] **Step 7: Add `energy_value` to `ItemStackFrame`**

In `src-tauri/src/item/types.rs`, add `energy_value` to the `ItemStackFrame` struct (after `stack_limit` at line 271):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStackFrame {
    pub item_id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub count: u32,
    pub stack_limit: u32,
    pub energy_value: Option<u32>,
}
```

- [ ] **Step 8: Update `build_inventory_frame()` to include energy_value**

In `src-tauri/src/engine/state.rs`, update `build_inventory_frame()` (around line 815). Add `energy_value` to the `ItemStackFrame` constructor:

```rust
fn build_inventory_frame(&self) -> InventoryFrame {
    InventoryFrame {
        slots: self
            .inventory
            .slots
            .iter()
            .map(|slot| {
                slot.as_ref().map(|stack| {
                    let def = self.item_defs.get(&stack.item_id);
                    ItemStackFrame {
                        item_id: stack.item_id.clone(),
                        name: def.map(|d| d.name.clone()).unwrap_or_default(),
                        description: def.map(|d| d.description.clone()).unwrap_or_default(),
                        icon: def.map(|d| d.icon.clone()).unwrap_or_default(),
                        count: stack.count,
                        stack_limit: def.map(|d| d.stack_limit).unwrap_or(1),
                        energy_value: def.and_then(|d| d.energy_value),
                    }
                })
            })
            .collect(),
        capacity: self.inventory.capacity,
    }
}
```

- [ ] **Step 9: Add energy and max_energy to RenderFrame construction in `tick()`**

In `src-tauri/src/engine/state.rs`, in the `tick()` function where the `RenderFrame` is constructed (around line 645). Add after `currants: self.currants,` (line 695):

```rust
energy: self.energy,
max_energy: self.max_energy,
```

- [ ] **Step 10: Run all tests**

Run: `cd src-tauri && cargo test -- --nocapture`
Expected: ALL PASS

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/engine/state.rs src-tauri/src/item/types.rs
git commit -m "feat(energy): add energy state to GameState, SaveState, RenderFrame"
```

---

### Task 3: Passive energy decay in tick

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

- [ ] **Step 1: Write failing tests for energy decay**

In `src-tauri/src/engine/state.rs` `#[cfg(test)] mod tests`, add tests. First check what test helpers exist for creating a GameState in tests. The tests need to construct a minimal GameState. Use the existing `GameState::new()` constructor with empty defs:

```rust
#[test]
fn energy_decays_per_tick() {
    use crate::item::loader::{parse_items, parse_entities, parse_recipes, parse_store_catalog};
    use crate::engine::jukebox::TrackCatalog;
    use crate::physics::movement::InputState;

    let item_defs = parse_items("{}").unwrap();
    let entity_defs = parse_entities("{}").unwrap();
    let recipe_defs = parse_recipes("{}").unwrap();
    let store_catalog = parse_store_catalog("{}").unwrap();
    let track_catalog = TrackCatalog::default();

    let mut state = GameState::new(800.0, 600.0, item_defs, entity_defs, recipe_defs, track_catalog, store_catalog);
    // Load a minimal street so tick() doesn't return None
    let street = crate::street::types::StreetData {
        tsid: "TEST".to_string(),
        label: "Test".to_string(),
        left: -500.0,
        right: 500.0,
        top: -500.0,
        bottom: 0.0,
        ground_y: -2.0,
        layers: vec![],
        platforms: vec![],
        walls: vec![],
        ladders: vec![],
        signposts: vec![],
    };
    state.load_street(street, vec![], vec![]);

    let initial_energy = state.energy;
    let input = InputState { left: false, right: false, jump: false, interact: false };
    let mut rng = rand::rngs::mock::StepRng::new(0, 1);

    // Tick for 1 second at 60fps
    for _ in 0..60 {
        state.tick(1.0 / 60.0, &input, &mut rng);
    }

    // After 1s at 0.1/sec decay: should lose ~0.1 energy
    let lost = initial_energy - state.energy;
    assert!(lost > 0.09 && lost < 0.11, "Expected ~0.1 energy loss, got {lost}");
}

#[test]
fn energy_does_not_decay_below_zero() {
    use crate::item::loader::{parse_items, parse_entities, parse_recipes, parse_store_catalog};
    use crate::engine::jukebox::TrackCatalog;
    use crate::physics::movement::InputState;

    let item_defs = parse_items("{}").unwrap();
    let entity_defs = parse_entities("{}").unwrap();
    let recipe_defs = parse_recipes("{}").unwrap();
    let store_catalog = parse_store_catalog("{}").unwrap();
    let track_catalog = TrackCatalog::default();

    let mut state = GameState::new(800.0, 600.0, item_defs, entity_defs, recipe_defs, track_catalog, store_catalog);
    let street = crate::street::types::StreetData {
        tsid: "TEST".to_string(),
        label: "Test".to_string(),
        left: -500.0,
        right: 500.0,
        top: -500.0,
        bottom: 0.0,
        ground_y: -2.0,
        layers: vec![],
        platforms: vec![],
        walls: vec![],
        ladders: vec![],
        signposts: vec![],
    };
    state.load_street(street, vec![], vec![]);
    state.energy = 0.01; // Almost empty

    let input = InputState { left: false, right: false, jump: false, interact: false };
    let mut rng = rand::rngs::mock::StepRng::new(0, 1);

    // Tick for 10 seconds — should clamp at 0, not go negative
    for _ in 0..600 {
        state.tick(1.0 / 60.0, &input, &mut rng);
    }

    assert_eq!(state.energy, 0.0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test energy_decays_per_tick energy_does_not_decay_below_zero -- --nocapture`
Expected: FAIL — no decay logic yet, energy unchanged

- [ ] **Step 3: Add energy decay constant and tick logic**

In `src-tauri/src/engine/state.rs`, add the constant near the top of the file (after the imports, before the structs):

```rust
/// Energy lost per second from passive decay.
const PASSIVE_ENERGY_DECAY_RATE: f64 = 0.1;
```

In the `tick()` function, add energy decay right after `self.game_time += dt;` (line 262):

```rust
self.game_time += dt;

// Passive energy decay
self.energy = (self.energy - PASSIVE_ENERGY_DECAY_RATE * dt).max(0.0);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -- --nocapture`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat(energy): passive energy decay in tick loop"
```

---

### Task 4: Harvest energy gate

**Files:**
- Modify: `src-tauri/src/item/interaction.rs`

- [ ] **Step 1: Write failing tests for harvest energy gate**

In `src-tauri/src/item/interaction.rs`, add or extend the `#[cfg(test)] mod tests` block. The `execute_interaction` function needs energy passed in. We'll add `energy: &mut f64` as a parameter.

First, write the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::inventory::Inventory;
    use crate::item::types::{EntityDef, ItemDef, WorldEntity, YieldEntry};

    fn test_item_defs() -> ItemDefs {
        let mut defs = HashMap::new();
        defs.insert(
            "cherry".to_string(),
            ItemDef {
                id: "cherry".to_string(),
                name: "Cherry".to_string(),
                description: "A cherry.".to_string(),
                category: "food".to_string(),
                stack_limit: 50,
                icon: "cherry".to_string(),
                base_cost: Some(3),
                energy_value: Some(12),
            },
        );
        defs
    }

    fn test_entity_defs() -> EntityDefs {
        let mut defs = HashMap::new();
        defs.insert(
            "fruit_tree".to_string(),
            EntityDef {
                id: "fruit_tree".to_string(),
                name: "Fruit Tree".to_string(),
                verb: "Harvest".to_string(),
                yields: vec![YieldEntry {
                    item: "cherry".to_string(),
                    min: 1,
                    max: 1,
                }],
                cooldown_secs: 5.0,
                max_harvests: 3,
                respawn_secs: 30.0,
                sprite_class: "tree".to_string(),
                interact_radius: 80.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
            },
        );
        defs
    }

    #[test]
    fn harvest_with_sufficient_energy() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inventory = Inventory::new(16);
        let entities = vec![WorldEntity {
            id: "tree_1".to_string(),
            entity_type: "fruit_tree".to_string(),
            x: 0.0,
            y: 0.0,
        }];
        let world_items: Vec<crate::item::types::WorldItem> = vec![];
        let mut entity_states = HashMap::new();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let mut energy = 100.0;

        let nearest = NearestInteractable::Entity { index: 0, distance: 10.0 };
        let result = execute_interaction(
            &nearest,
            &mut inventory,
            &entities,
            &entity_defs,
            &world_items,
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
            &mut energy,
        );

        // Should succeed: items yielded, energy deducted
        assert!(matches!(result.interaction_type, Some(InteractionType::Entity { .. })));
        assert!(inventory.count_item("cherry") > 0);
        assert!(energy < 100.0, "Energy should be deducted");
    }

    #[test]
    fn harvest_with_zero_energy_rejected() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inventory = Inventory::new(16);
        let entities = vec![WorldEntity {
            id: "tree_1".to_string(),
            entity_type: "fruit_tree".to_string(),
            x: 0.0,
            y: 0.0,
        }];
        let world_items: Vec<crate::item::types::WorldItem> = vec![];
        let mut entity_states = HashMap::new();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let mut energy = 0.0;

        let nearest = NearestInteractable::Entity { index: 0, distance: 10.0 };
        let result = execute_interaction(
            &nearest,
            &mut inventory,
            &entities,
            &entity_defs,
            &world_items,
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
            &mut energy,
        );

        // Should be rejected: no items yielded, energy unchanged
        assert!(matches!(result.interaction_type, Some(InteractionType::Rejected)));
        assert_eq!(inventory.count_item("cherry"), 0);
        assert_eq!(energy, 0.0);
        // Check for "Too tired" feedback
        assert!(result.feedback.iter().any(|f| f.text.contains("Too tired")));
    }

    #[test]
    fn harvest_energy_unchanged_on_reject() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inventory = Inventory::new(16);
        let entities = vec![WorldEntity {
            id: "tree_1".to_string(),
            entity_type: "fruit_tree".to_string(),
            x: 0.0,
            y: 0.0,
        }];
        let world_items: Vec<crate::item::types::WorldItem> = vec![];
        let mut entity_states = HashMap::new();
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);
        let mut energy = 3.0; // Below HARVEST_ENERGY_COST of 5.0

        let nearest = NearestInteractable::Entity { index: 0, distance: 10.0 };
        let result = execute_interaction(
            &nearest,
            &mut inventory,
            &entities,
            &entity_defs,
            &world_items,
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
            &mut energy,
        );

        assert!(matches!(result.interaction_type, Some(InteractionType::Rejected)));
        assert_eq!(energy, 3.0, "Energy should not change on rejection");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test harvest_with_sufficient_energy harvest_with_zero_energy_rejected harvest_energy_unchanged_on_reject -- --nocapture`
Expected: FAIL — `execute_interaction` doesn't accept `energy` parameter yet

- [ ] **Step 3: Add energy parameter to `execute_interaction` and implement the gate**

In `src-tauri/src/item/interaction.rs`, add the energy cost constant near the top (after line 12):

```rust
/// Energy cost per harvest action.
const HARVEST_ENERGY_COST: f64 = 5.0;
```

Update the `execute_interaction` function signature (line 196) to accept `energy`:

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
    energy: &mut f64,
) -> InteractionResult {
```

Add the energy gate check in the entity harvest branch, AFTER the vendor/jukebox early returns and AFTER the depletion/cooldown checks (after line 272, before the harvest yields loop at line 274). Insert this block:

```rust
            // 3. Energy check — harvest requires energy
            if *energy < HARVEST_ENERGY_COST {
                result.feedback.push(PickupFeedback {
                    id: 0,
                    text: "Too tired".to_string(),
                    success: false,
                    x: entity.x,
                    y: entity.y,
                    age_secs: 0.0,
                });
                result.interaction_type = Some(InteractionType::Rejected);
                return result;
            }

            // Deduct energy for harvest
            *energy = (*energy - HARVEST_ENERGY_COST).max(0.0);
```

- [ ] **Step 4: Update the call site in `state.rs tick()`**

In `src-tauri/src/engine/state.rs`, update the `execute_interaction` call (around line 522) to pass `&mut self.energy`:

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
                        &mut self.energy,
                    );
```

- [ ] **Step 5: Run all tests**

Run: `cd src-tauri && cargo test -- --nocapture`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/item/interaction.rs src-tauri/src/engine/state.rs
git commit -m "feat(energy): harvest energy gate — blocks at zero, deducts on harvest"
```

---

### Task 5: Eat item — energy module and IPC command

**Files:**
- Create: `src-tauri/src/item/energy.rs`
- Modify: `src-tauri/src/item/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create energy.rs with eat function and tests**

Create `src-tauri/src/item/energy.rs`:

```rust
use crate::item::inventory::Inventory;
use crate::item::types::ItemDefs;

/// Consume one unit of a food item from inventory, restoring energy.
///
/// Validates:
/// - Player has the item in inventory
/// - Item has `energy_value` defined
/// - Player energy < max_energy (can't eat at full)
///
/// On success, removes 1 item and adds energy (capped at max).
/// Returns `(new_energy, max_energy)`.
pub fn eat(
    item_id: &str,
    energy: f64,
    max_energy: f64,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
) -> Result<(f64, f64), String> {
    // Item must have energy_value
    let def = item_defs
        .get(item_id)
        .ok_or_else(|| format!("Unknown item '{item_id}'"))?;
    let energy_value = def
        .energy_value
        .ok_or_else(|| format!("Item '{item_id}' is not edible"))?;

    // Can't eat at full energy
    if energy >= max_energy {
        return Err("Already full".to_string());
    }

    // Player must have the item
    let have = inventory.count_item(item_id);
    if have == 0 {
        return Err(format!("No '{item_id}' in inventory"));
    }

    // Consume 1 item, restore energy capped at max
    inventory.remove_item(item_id, 1);
    let new_energy = (energy + energy_value as f64).min(max_energy);
    Ok((new_energy, max_energy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;
    use std::collections::HashMap;

    fn test_item_defs() -> ItemDefs {
        let mut defs = HashMap::new();
        defs.insert(
            "cherry".to_string(),
            ItemDef {
                id: "cherry".to_string(),
                name: "Cherry".to_string(),
                description: "A cherry.".to_string(),
                category: "food".to_string(),
                stack_limit: 50,
                icon: "cherry".to_string(),
                base_cost: Some(3),
                energy_value: Some(12),
            },
        );
        defs.insert(
            "wood".to_string(),
            ItemDef {
                id: "wood".to_string(),
                name: "Wood".to_string(),
                description: "A piece of wood.".to_string(),
                category: "material".to_string(),
                stack_limit: 50,
                icon: "wood".to_string(),
                base_cost: Some(4),
                energy_value: None,
            },
        );
        defs
    }

    #[test]
    fn eat_restores_energy() {
        let defs = test_item_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 5, &defs);

        let result = eat("cherry", 100.0, 600.0, &mut inv, &defs);
        assert_eq!(result, Ok((112.0, 600.0)));
        assert_eq!(inv.count_item("cherry"), 4);
    }

    #[test]
    fn eat_capped_at_max_energy() {
        let defs = test_item_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 5, &defs);

        // 595 + 12 = 607, capped to 600
        let result = eat("cherry", 595.0, 600.0, &mut inv, &defs);
        assert_eq!(result, Ok((600.0, 600.0)));
        assert_eq!(inv.count_item("cherry"), 4);
    }

    #[test]
    fn eat_rejected_when_already_full() {
        let defs = test_item_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 5, &defs);

        let result = eat("cherry", 600.0, 600.0, &mut inv, &defs);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Already full"));
        // Inventory unchanged
        assert_eq!(inv.count_item("cherry"), 5);
    }

    #[test]
    fn eat_rejected_when_item_not_edible() {
        let defs = test_item_defs();
        let mut inv = Inventory::new(16);
        inv.add("wood", 5, &defs);

        let result = eat("wood", 100.0, 600.0, &mut inv, &defs);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not edible"));
        // Inventory unchanged
        assert_eq!(inv.count_item("wood"), 5);
    }

    #[test]
    fn eat_rejected_when_item_not_in_inventory() {
        let defs = test_item_defs();
        let mut inv = Inventory::new(16);
        // Inventory is empty

        let result = eat("cherry", 100.0, 600.0, &mut inv, &defs);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No 'cherry'"));
    }
}
```

- [ ] **Step 2: Add module declaration**

In `src-tauri/src/item/mod.rs`, add:

```rust
pub mod energy;
```

- [ ] **Step 3: Run energy module tests**

Run: `cd src-tauri && cargo test item::energy -- --nocapture`
Expected: ALL PASS (5 tests)

- [ ] **Step 4: Add `eat_item` IPC command in lib.rs**

In `src-tauri/src/lib.rs`, add the `eat_item` command. Place it after `vendor_sell` (around line 713):

```rust
#[tauri::command]
fn eat_item(item_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let item_defs = state.item_defs.clone();
    let energy = state.energy;
    let max_energy = state.max_energy;

    let (new_energy, new_max) = item::energy::eat(&item_id, energy, max_energy, &mut state.inventory, &item_defs)?;
    state.energy = new_energy;

    let gained = new_energy - energy;
    let px = state.player.x;
    let py = state.player.y;
    let fb_id = state.next_feedback_id;
    state.next_feedback_id += 1;
    state.pickup_feedback.push(item::types::PickupFeedback {
        id: fb_id,
        text: format!("+{} energy", gained as u32),
        success: true,
        x: px,
        y: py,
        age_secs: 0.0,
    });

    Ok(serde_json::json!({
        "energy": new_energy,
        "maxEnergy": new_max,
    }))
}
```

- [ ] **Step 5: Register `eat_item` in invoke_handler**

In `src-tauri/src/lib.rs`, add `eat_item` to the `invoke_handler` list (around line 1117, after `vendor_sell`):

```rust
            vendor_sell,
            eat_item,
```

- [ ] **Step 6: Run all Rust tests**

Run: `cd src-tauri && cargo test -- --nocapture`
Expected: ALL PASS

- [ ] **Step 7: Run clippy**

Run: `cd src-tauri && cargo clippy`
Expected: No warnings

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/item/energy.rs src-tauri/src/item/mod.rs src-tauri/src/lib.rs
git commit -m "feat(energy): eat_item IPC command — consume food to restore energy"
```

---

### Task 6: Frontend types and IPC

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add energy fields to TypeScript types**

In `src/lib/types.ts`, add `energy` and `maxEnergy` to `RenderFrame` (after `currants: number;` around line 163):

```typescript
export interface RenderFrame {
  player: PlayerFrame;
  remotePlayers: RemotePlayerFrame[];
  camera: CameraFrame;
  streetId: string;
  transition?: TransitionInfo | null;
  inventory: InventoryFrame;
  worldEntities: WorldEntityFrame[];
  worldItems: WorldItemFrame[];
  interactionPrompt: InteractionPrompt | null;
  pickupFeedback: PickupFeedback[];
  audioEvents: AudioEvent[];
  currants: number;
  energy: number;
  maxEnergy: number;
}
```

Add `energyValue` to `ItemStackFrame` (after `stackLimit: number;` around line 200):

```typescript
export interface ItemStackFrame {
  itemId: string;
  name: string;
  description: string;
  icon: string;
  count: number;
  stackLimit: number;
  energyValue: number | null;
}
```

Add `energy` to `SavedState` (after `currants?: number;` around line 271):

```typescript
export interface SavedState {
  streetId: string;
  x: number;
  y: number;
  facing: string;
  inventory: (SaveItemStack | null)[];
  currants?: number;
  energy?: number;
}
```

Add the `EatResult` interface near the other interfaces:

```typescript
export interface EatResult {
  energy: number;
  maxEnergy: number;
}
```

- [ ] **Step 2: Add `eatItem` IPC function**

In `src/lib/ipc.ts`, add the import for `EatResult` and the function. Update the import line (line 3):

```typescript
import type { StreetData, InputState, RenderFrame, NetworkStatus, PlayerIdentity, ChatEvent, RecipeDef, SavedState, SoundKitMeta, JukeboxInfo, AvatarAppearance, StoreState, EatResult } from './types';
```

Add the function at the end of the file (after `vendorSell`):

```typescript
export async function eatItem(itemId: string): Promise<EatResult> {
  return invoke<EatResult>('eat_item', { itemId });
}
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/types.ts src/lib/ipc.ts
git commit -m "feat(energy): frontend types and eatItem IPC function"
```

---

### Task 7: EnergyHud component

**Files:**
- Create: `src/lib/components/EnergyHud.svelte`
- Create: `src/lib/components/EnergyHud.test.ts`

- [ ] **Step 1: Write failing tests for EnergyHud**

Create `src/lib/components/EnergyHud.test.ts`:

```typescript
// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import EnergyHud from './EnergyHud.svelte';

describe('EnergyHud', () => {
  it('renders bar with correct fill percentage', () => {
    render(EnergyHud, { props: { energy: 300, maxEnergy: 600 } });
    const fill = document.querySelector('.energy-fill') as HTMLElement;
    expect(fill).toBeDefined();
    expect(fill.style.width).toBe('50%');
  });

  it('shows numeric energy value', () => {
    render(EnergyHud, { props: { energy: 432, maxEnergy: 600 } });
    const amount = document.querySelector('.energy-amount');
    expect(amount?.textContent).toBe('432');
  });

  it('has low-energy class when below 150', () => {
    render(EnergyHud, { props: { energy: 100, maxEnergy: 600 } });
    const hud = document.querySelector('.energy-hud');
    expect(hud?.classList.contains('low')).toBe(true);
  });

  it('does not have low-energy class when above 150', () => {
    render(EnergyHud, { props: { energy: 300, maxEnergy: 600 } });
    const hud = document.querySelector('.energy-hud');
    expect(hud?.classList.contains('low')).toBe(false);
  });

  it('has accessible role="status"', () => {
    render(EnergyHud, { props: { energy: 432, maxEnergy: 600 } });
    const hud = document.querySelector('[role="status"]');
    expect(hud).toBeDefined();
    expect(hud?.getAttribute('aria-label')).toContain('432');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/lib/components/EnergyHud.test.ts`
Expected: FAIL — component doesn't exist yet

- [ ] **Step 3: Create EnergyHud component**

Create `src/lib/components/EnergyHud.svelte`:

```svelte
<script lang="ts">
  let { energy = 0, maxEnergy = 600 }: { energy: number; maxEnergy: number } = $props();

  let percent = $derived(maxEnergy > 0 ? Math.min(100, (energy / maxEnergy) * 100) : 0);
  let isLow = $derived(energy < 150);
  let displayEnergy = $derived(Math.floor(energy));
</script>

<div class="energy-hud" class:low={isLow} role="status" aria-label="Energy: {displayEnergy} of {maxEnergy}">
  <span class="energy-icon">⚡</span>
  <div class="energy-bar">
    <div class="energy-fill" style="width: {percent}%"></div>
  </div>
  <span class="energy-amount">{displayEnergy}</span>
</div>

<style>
  .energy-hud {
    position: fixed;
    top: 12px;
    left: 12px;
    background: rgba(26, 26, 46, 0.85);
    padding: 4px 10px;
    border-radius: 16px;
    display: flex;
    align-items: center;
    gap: 6px;
    z-index: 50;
    pointer-events: none;
    user-select: none;
  }

  .energy-icon {
    font-size: 10px;
    color: #4ade80;
  }

  .energy-bar {
    width: 60px;
    height: 8px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    overflow: hidden;
  }

  .energy-fill {
    height: 100%;
    background: linear-gradient(90deg, #22c55e, #4ade80);
    border-radius: 4px;
    transition: width 0.3s ease;
  }

  .energy-amount {
    font-size: 11px;
    font-weight: bold;
    color: #4ade80;
    min-width: 24px;
    text-align: right;
  }

  /* Low energy warning — bar and text shift to amber/red */
  .energy-hud.low .energy-fill {
    background: linear-gradient(90deg, #ef4444, #f59e0b);
  }

  .energy-hud.low .energy-icon,
  .energy-hud.low .energy-amount {
    color: #f59e0b;
  }
</style>
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/lib/components/EnergyHud.test.ts`
Expected: ALL PASS (5 tests)

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/EnergyHud.svelte src/lib/components/EnergyHud.test.ts
git commit -m "feat(energy): EnergyHud component with bar, value, and low-energy warning"
```

---

### Task 8: Inventory "Use" button for food items

**Files:**
- Modify: `src/lib/components/InventoryPanel.svelte`
- Modify: `src/lib/components/InventoryPanel.test.ts`

- [ ] **Step 1: Write failing tests for "Use" button**

In `src/lib/components/InventoryPanel.test.ts`, update the mock to include `eatItem` and update the `makeInventory` helper to support `energyValue`. Add tests at the end of the describe block.

Update the mock at the top (around line 8):

```typescript
vi.mock('../ipc', () => ({
  dropItem: vi.fn().mockResolvedValue(undefined),
  craftRecipe: vi.fn().mockResolvedValue(undefined),
  eatItem: vi.fn().mockResolvedValue({ energy: 112, maxEnergy: 600 }),
}));
```

Update `makeInventory` to accept `energyValue` (replace the existing function around line 21):

```typescript
function makeInventory(items: { itemId: string; name: string; count: number; energyValue?: number | null }[]): InventoryFrame {
  const slots: (null | { itemId: string; name: string; description: string; icon: string; count: number; stackLimit: number; energyValue: number | null })[] =
    items.map(i => ({
      itemId: i.itemId,
      name: i.name,
      description: '',
      icon: i.itemId,
      count: i.count,
      stackLimit: 50,
      energyValue: i.energyValue ?? null,
    }));
  while (slots.length < 16) slots.push(null);
  return { slots, capacity: 16 };
}
```

Add tests:

```typescript
  it('shows Use button for food items when selected', async () => {
    const inv = makeInventory([{ itemId: 'cherry', name: 'Cherry', count: 5, energyValue: 12 }]);
    render(InventoryPanel, {
      props: { inventory: inv, visible: true, energy: 100, maxEnergy: 600 },
    });

    // Click the first filled slot to select it
    const slots = screen.getAllByRole('gridcell');
    const firstSlot = slots[0].querySelector('button');
    await fireEvent.click(firstSlot!);

    const useBtn = screen.getByRole('button', { name: /use/i });
    expect(useBtn).toBeDefined();
  });

  it('hides Use button for non-food items', async () => {
    const inv = makeInventory([{ itemId: 'wood', name: 'Wood', count: 3, energyValue: null }]);
    render(InventoryPanel, {
      props: { inventory: inv, visible: true, energy: 100, maxEnergy: 600 },
    });

    const slots = screen.getAllByRole('gridcell');
    const firstSlot = slots[0].querySelector('button');
    await fireEvent.click(firstSlot!);

    const useBtn = screen.queryByRole('button', { name: /use/i });
    expect(useBtn).toBeNull();
  });

  it('Use button triggers eatItem IPC', async () => {
    const { eatItem } = await import('../ipc');
    const inv = makeInventory([{ itemId: 'cherry', name: 'Cherry', count: 5, energyValue: 12 }]);
    render(InventoryPanel, {
      props: { inventory: inv, visible: true, energy: 100, maxEnergy: 600 },
    });

    const slots = screen.getAllByRole('gridcell');
    const firstSlot = slots[0].querySelector('button');
    await fireEvent.click(firstSlot!);

    const useBtn = screen.getByRole('button', { name: /use/i });
    await fireEvent.click(useBtn);

    expect(eatItem).toHaveBeenCalledWith('cherry');
  });

  it('Use button disabled when energy is full', async () => {
    const inv = makeInventory([{ itemId: 'cherry', name: 'Cherry', count: 5, energyValue: 12 }]);
    render(InventoryPanel, {
      props: { inventory: inv, visible: true, energy: 600, maxEnergy: 600 },
    });

    const slots = screen.getAllByRole('gridcell');
    const firstSlot = slots[0].querySelector('button');
    await fireEvent.click(firstSlot!);

    const useBtn = screen.getByRole('button', { name: /use/i });
    expect((useBtn as HTMLButtonElement).disabled).toBe(true);
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/lib/components/InventoryPanel.test.ts`
Expected: FAIL — InventoryPanel doesn't accept `energy`/`maxEnergy` props or have a "Use" button

- [ ] **Step 3: Add energy props and "Use" button to InventoryPanel**

In `src/lib/components/InventoryPanel.svelte`, update the props to accept energy state (line 5):

```typescript
let { inventory, recipes = [], visible = false, onClose, energy = 0, maxEnergy = 600, onEat }: {
    inventory: InventoryFrame | null;
    recipes?: RecipeDef[];
    visible?: boolean;
    onClose?: () => void;
    energy?: number;
    maxEnergy?: number;
    onEat?: (itemId: string) => void;
} = $props();
```

Add a derived value for whether the selected item is edible (after the existing `selectedItem` derived, around line 23):

```typescript
let isSelectedEdible = $derived(selectedItem?.energyValue != null && selectedItem.energyValue > 0);
let isEnergyFull = $derived(energy >= maxEnergy);
```

Add the `handleEat` function (after `handleDrop`, around line 115):

```typescript
function handleEat() {
    if (!selectedItem || !isSelectedEdible) return;
    onEat?.(selectedItem.itemId);
}
```

In the template, add the "Use" button next to the "Drop" button in the item-details section. Replace the existing item-details block (around line 277-286) with:

```svelte
        {#if selectedItem}
          <div class="item-details">
            <div class="item-name">{selectedItem.name}</div>
            <div class="item-desc">{selectedItem.description}</div>
            <div class="item-count">{selectedItem.count} / {selectedItem.stackLimit}</div>
            <div class="item-actions">
              {#if isSelectedEdible}
                <button
                  type="button"
                  class="use-btn"
                  disabled={isEnergyFull}
                  onclick={handleEat}
                  aria-label="Use {selectedItem.name}"
                >
                  Use
                </button>
              {/if}
              <button type="button" class="drop-btn" onclick={handleDrop}>
                Drop
              </button>
            </div>
          </div>
        {/if}
```

Add styles for the new button and actions row. In the `<style>` block, add:

```css
  .item-actions {
    display: flex;
    gap: 4px;
  }

  .use-btn {
    background: rgba(40, 80, 60, 0.8);
    color: #8cd48c;
    border: 1px solid #4a7a4a;
    border-radius: 3px;
    padding: 4px 12px;
    cursor: pointer;
    font-size: 0.75rem;
  }

  .use-btn:hover:not(:disabled) { background: rgba(50, 100, 70, 0.9); }
  .use-btn:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }
  .use-btn:disabled { opacity: 0.4; cursor: not-allowed; }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/lib/components/InventoryPanel.test.ts`
Expected: ALL PASS

- [ ] **Step 5: Run all frontend tests**

Run: `npx vitest run`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/InventoryPanel.svelte src/lib/components/InventoryPanel.test.ts
git commit -m "feat(energy): Use button on food items in inventory panel"
```

---

### Task 9: Wire EnergyHud and eat handler into App.svelte

**Files:**
- Modify: `src/App.svelte`

- [ ] **Step 1: Add EnergyHud import**

In `src/App.svelte`, add the import alongside the existing component imports (near the top of the `<script>` tag):

```typescript
import EnergyHud from './lib/components/EnergyHud.svelte';
```

Also add `eatItem` to the ipc imports:

```typescript
import { ..., eatItem } from './lib/ipc';
```

- [ ] **Step 2: Add EnergyHud to the template**

In `src/App.svelte`, add the `EnergyHud` component near the existing `CurrantHud` (around line 440). Place it right before or after `CurrantHud`:

```svelte
    <CurrantHud currants={latestFrame?.currants ?? 0} />
    <EnergyHud energy={latestFrame?.energy ?? 600} maxEnergy={latestFrame?.maxEnergy ?? 600} />
```

- [ ] **Step 3: Wire eat handler into InventoryPanel**

Update the `InventoryPanel` in the template (around line 409) to pass energy props and the eat handler:

```svelte
    <InventoryPanel
      inventory={latestFrame?.inventory ?? null}
      {recipes}
      visible={inventoryOpen}
      onClose={() => { inventoryOpen = false; }}
      energy={latestFrame?.energy ?? 600}
      maxEnergy={latestFrame?.maxEnergy ?? 600}
      onEat={async (itemId) => {
        try {
          await eatItem(itemId);
        } catch (e) {
          console.error('Eat failed:', e);
        }
      }}
    />
```

- [ ] **Step 4: Run all frontend tests**

Run: `npx vitest run`
Expected: ALL PASS

- [ ] **Step 5: Run all Rust tests**

Run: `cd src-tauri && cargo test -- --nocapture`
Expected: ALL PASS

- [ ] **Step 6: Run clippy**

Run: `cd src-tauri && cargo clippy`
Expected: No warnings

- [ ] **Step 7: Commit**

```bash
git add src/App.svelte
git commit -m "feat(energy): wire EnergyHud and eat handler into App"
```
