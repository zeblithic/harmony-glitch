# Phase C Items & Interaction — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add item pickup, world entity interaction, and inventory management to harmony-glitch — the first vertical slice of Phase C.

**Architecture:** Items and inventory live inside GameState, processed in the 60Hz tick loop alongside physics. World entities and ground items are part of RenderFrame so the frontend renders them like everything else. Item/entity definitions are JSON-driven, loaded at startup. Interaction uses rising-edge detection on a new `interact` input field. Inventory UI is a Svelte side panel toggled with 'I'.

**Tech Stack:** Rust (Tauri v2), Svelte 5 (runes), TypeScript, PixiJS v8, serde_json

**Spec:** `docs/plans/2026-03-15-phase-c-items-design.md`

---

## Chunk 1: Rust Data Layer

### Task 1: JSON Data Files

**Files:**
- Create: `assets/items.json`
- Create: `assets/entities.json`
- Create: `assets/streets/demo_meadow_entities.json`
- Create: `assets/streets/demo_heights_entities.json`

- [ ] **Step 1: Create items.json**

```json
{
  "cherry": {
    "name": "Cherry",
    "description": "A plump, juicy cherry from a Fruit Tree.",
    "category": "food",
    "stackLimit": 50,
    "icon": "cherry"
  },
  "grain": {
    "name": "Grain",
    "description": "Squeezed from a chicken. Don't ask.",
    "category": "food",
    "stackLimit": 50,
    "icon": "grain"
  },
  "meat": {
    "name": "Meat",
    "description": "Nibbled off a friendly pig. It didn't seem to mind.",
    "category": "food",
    "stackLimit": 50,
    "icon": "meat"
  },
  "milk": {
    "name": "Milk",
    "description": "Milked from a butterfly. Surprisingly creamy.",
    "category": "food",
    "stackLimit": 50,
    "icon": "milk"
  },
  "bubble": {
    "name": "Bubble",
    "description": "A shimmering bubble harvested from a Bubble Tree.",
    "category": "material",
    "stackLimit": 50,
    "icon": "bubble"
  },
  "wood": {
    "name": "Wood",
    "description": "A sturdy plank from a Wood Tree.",
    "category": "material",
    "stackLimit": 50,
    "icon": "wood"
  }
}
```

- [ ] **Step 2: Create entities.json**

```json
{
  "fruit_tree": {
    "name": "Fruit Tree",
    "verb": "Harvest",
    "yields": [{ "item": "cherry", "min": 1, "max": 3 }],
    "cooldownSecs": 0,
    "spriteClass": "tree_fruit",
    "interactRadius": 80
  },
  "chicken": {
    "name": "Chicken",
    "verb": "Squeeze",
    "yields": [{ "item": "grain", "min": 1, "max": 2 }],
    "cooldownSecs": 0,
    "spriteClass": "npc_chicken",
    "interactRadius": 60
  },
  "pig": {
    "name": "Pig",
    "verb": "Nibble",
    "yields": [{ "item": "meat", "min": 1, "max": 2 }],
    "cooldownSecs": 0,
    "spriteClass": "npc_pig",
    "interactRadius": 60
  },
  "butterfly": {
    "name": "Butterfly",
    "verb": "Milk",
    "yields": [{ "item": "milk", "min": 1, "max": 1 }],
    "cooldownSecs": 0,
    "spriteClass": "npc_butterfly",
    "interactRadius": 50
  },
  "bubble_tree": {
    "name": "Bubble Tree",
    "verb": "Harvest",
    "yields": [{ "item": "bubble", "min": 1, "max": 4 }],
    "cooldownSecs": 0,
    "spriteClass": "tree_bubble",
    "interactRadius": 80
  }
}
```

- [ ] **Step 3: Create demo_meadow_entities.json**

Place entities along the ground (y=-2, slightly above ground level so they visually sit on platforms):

```json
[
  { "id": "tree_1", "type": "fruit_tree", "x": -800, "y": -2 },
  { "id": "tree_2", "type": "fruit_tree", "x": 1200, "y": -2 },
  { "id": "chicken_1", "type": "chicken", "x": 200, "y": -2 },
  { "id": "pig_1", "type": "pig", "x": -300, "y": -2 },
  { "id": "butterfly_1", "type": "butterfly", "x": 600, "y": -80 }
]
```

- [ ] **Step 4: Create demo_heights_entities.json**

```json
[
  { "id": "btree_1", "type": "bubble_tree", "x": -500, "y": -2 },
  { "id": "chicken_2", "type": "chicken", "x": 400, "y": -2 },
  { "id": "butterfly_2", "type": "butterfly", "x": -100, "y": -60 }
]
```

- [ ] **Step 5: Commit**

```bash
git add assets/items.json assets/entities.json assets/streets/demo_meadow_entities.json assets/streets/demo_heights_entities.json
git commit -m "feat: add item and entity definition JSON files"
```

---

### Task 2: Item & Entity Types

**Files:**
- Create: `src-tauri/src/item/mod.rs`
- Create: `src-tauri/src/item/types.rs`
- Modify: `src-tauri/src/lib.rs:1` (add `pub mod item;`)

- [ ] **Step 1: Create module root**

Create `src-tauri/src/item/mod.rs`:

```rust
pub mod types;
```

- [ ] **Step 2: Write type definition tests**

Create `src-tauri/src/item/types.rs` with types and tests:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Item type definition (loaded from JSON at startup).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDef {
    /// Set programmatically from the JSON map key, not deserialized.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub stack_limit: u32,
    pub icon: String,
}

/// A stack of items in inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStack {
    pub item_id: String,
    pub count: u32,
}

/// Entity type definition (loaded from JSON at startup).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityDef {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub verb: String,
    pub yields: Vec<YieldEntry>,
    pub cooldown_secs: f64,
    pub sprite_class: String,
    pub interact_radius: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YieldEntry {
    pub item: String,
    pub min: u32,
    pub max: u32,
}

/// An entity instance placed in the world (per-street).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEntity {
    pub id: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    pub x: f64,
    pub y: f64,
}

/// An item sitting on the ground (runtime-created).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldItem {
    pub id: String,
    pub item_id: String,
    pub count: u32,
    pub x: f64,
    pub y: f64,
}

pub type ItemDefs = HashMap<String, ItemDef>;
pub type EntityDefs = HashMap<String, EntityDef>;

/// Data sent to frontend for rendering an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldEntityFrame {
    pub id: String,
    pub entity_type: String,
    pub name: String,
    pub sprite_class: String,
    pub x: f64,
    pub y: f64,
}

/// Data sent to frontend for rendering a ground item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldItemFrame {
    pub id: String,
    pub item_id: String,
    pub name: String,
    pub icon: String,
    pub count: u32,
    pub x: f64,
    pub y: f64,
}

/// Data sent to frontend for rendering inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryFrame {
    pub slots: Vec<Option<ItemStackFrame>>,
    pub capacity: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStackFrame {
    pub item_id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub count: u32,
    pub stack_limit: u32,
}

/// Prompt shown when player is near an interactable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionPrompt {
    pub verb: String,
    pub target_name: String,
    pub target_x: f64,
    pub target_y: f64,
}

/// Floating feedback text after pickup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickupFeedback {
    pub text: String,
    pub success: bool,
    pub x: f64,
    pub y: f64,
    pub age_secs: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_stack_creation() {
        let stack = ItemStack {
            item_id: "cherry".into(),
            count: 5,
        };
        assert_eq!(stack.item_id, "cherry");
        assert_eq!(stack.count, 5);
    }

    #[test]
    fn world_entity_deserialize() {
        let json = r#"{"id":"tree_1","type":"fruit_tree","x":-800,"y":-2}"#;
        let entity: WorldEntity = serde_json::from_str(json).unwrap();
        assert_eq!(entity.id, "tree_1");
        assert_eq!(entity.entity_type, "fruit_tree");
        assert!((entity.x - (-800.0)).abs() < 0.01);
    }

    #[test]
    fn item_def_serializes_camel_case() {
        let def = ItemDef {
            id: "cherry".into(),
            name: "Cherry".into(),
            description: "Yummy".into(),
            category: "food".into(),
            stack_limit: 50,
            icon: "cherry".into(),
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("stackLimit"));
        assert!(!json.contains("stack_limit"));
        // id is skip-serialized
        assert!(!json.contains(r#""id""#));
    }
}
```

- [ ] **Step 3: Add module declaration to lib.rs**

Add `pub mod item;` to `src-tauri/src/lib.rs` after the existing module declarations (line 6, after `pub mod street;`).

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test -p harmony-glitch --lib item::types`
Expected: 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/item/mod.rs src-tauri/src/item/types.rs src-tauri/src/lib.rs
git commit -m "feat: add item and entity type definitions"
```

---

### Task 3: JSON Loader

**Files:**
- Create: `src-tauri/src/item/loader.rs`
- Modify: `src-tauri/src/item/mod.rs` (add `pub mod loader;`)

- [ ] **Step 1: Write failing test for item def loading**

Create `src-tauri/src/item/loader.rs`:

```rust
use crate::item::types::{EntityDef, EntityDefs, ItemDef, ItemDefs, WorldEntity};

/// Parse item definitions from JSON string.
/// The JSON is a map of id → ItemDef. We set each ItemDef.id from its map key.
pub fn parse_item_defs(json: &str) -> Result<ItemDefs, String> {
    let mut raw: ItemDefs =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse items.json: {e}"))?;
    for (key, def) in raw.iter_mut() {
        def.id = key.clone();
    }
    Ok(raw)
}

/// Parse entity definitions from JSON string.
pub fn parse_entity_defs(json: &str) -> Result<EntityDefs, String> {
    let mut raw: EntityDefs =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse entities.json: {e}"))?;
    for (key, def) in raw.iter_mut() {
        def.id = key.clone();
    }
    Ok(raw)
}

/// Parse entity placements from JSON string.
pub fn parse_entity_placements(json: &str) -> Result<Vec<WorldEntity>, String> {
    serde_json::from_str(json).map_err(|e| format!("Failed to parse entity placements: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_item_defs_from_json() {
        let json = r#"{
            "cherry": {
                "name": "Cherry",
                "description": "A cherry.",
                "category": "food",
                "stackLimit": 50,
                "icon": "cherry"
            }
        }"#;
        let defs = parse_item_defs(json).unwrap();
        assert_eq!(defs.len(), 1);
        let cherry = &defs["cherry"];
        assert_eq!(cherry.id, "cherry");
        assert_eq!(cherry.name, "Cherry");
        assert_eq!(cherry.stack_limit, 50);
    }

    #[test]
    fn parse_entity_defs_from_json() {
        let json = r#"{
            "chicken": {
                "name": "Chicken",
                "verb": "Squeeze",
                "yields": [{ "item": "grain", "min": 1, "max": 2 }],
                "cooldownSecs": 0,
                "spriteClass": "npc_chicken",
                "interactRadius": 60
            }
        }"#;
        let defs = parse_entity_defs(json).unwrap();
        assert_eq!(defs.len(), 1);
        let chicken = &defs["chicken"];
        assert_eq!(chicken.id, "chicken");
        assert_eq!(chicken.verb, "Squeeze");
        assert_eq!(chicken.yields.len(), 1);
        assert_eq!(chicken.yields[0].item, "grain");
    }

    #[test]
    fn parse_entity_placements_from_json() {
        let json = r#"[
            { "id": "tree_1", "type": "fruit_tree", "x": -800, "y": -2 },
            { "id": "chicken_1", "type": "chicken", "x": 200, "y": -2 }
        ]"#;
        let entities = parse_entity_placements(json).unwrap();
        assert_eq!(entities.len(), 2);
        assert_eq!(entities[0].entity_type, "fruit_tree");
        assert_eq!(entities[1].entity_type, "chicken");
    }

    #[test]
    fn parse_bundled_items_json() {
        let json = include_str!("../../../assets/items.json");
        let defs = parse_item_defs(json).unwrap();
        assert_eq!(defs.len(), 6);
        assert!(defs.contains_key("cherry"));
        assert!(defs.contains_key("grain"));
        assert!(defs.contains_key("meat"));
        assert!(defs.contains_key("milk"));
        assert!(defs.contains_key("bubble"));
        assert!(defs.contains_key("wood"));
    }

    #[test]
    fn parse_bundled_entities_json() {
        let json = include_str!("../../../assets/entities.json");
        let defs = parse_entity_defs(json).unwrap();
        assert_eq!(defs.len(), 5);
        assert!(defs.contains_key("fruit_tree"));
        assert!(defs.contains_key("chicken"));
        assert!(defs.contains_key("pig"));
        assert!(defs.contains_key("butterfly"));
        assert!(defs.contains_key("bubble_tree"));
    }

    #[test]
    fn parse_bundled_meadow_entities() {
        let json = include_str!("../../../assets/streets/demo_meadow_entities.json");
        let entities = parse_entity_placements(json).unwrap();
        assert!(entities.len() >= 3);
    }

    #[test]
    fn parse_bundled_heights_entities() {
        let json = include_str!("../../../assets/streets/demo_heights_entities.json");
        let entities = parse_entity_placements(json).unwrap();
        assert!(entities.len() >= 2);
    }
}
```

- [ ] **Step 2: Add module to mod.rs**

Add `pub mod loader;` to `src-tauri/src/item/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test -p harmony-glitch --lib item::loader`
Expected: 7 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/item/loader.rs src-tauri/src/item/mod.rs
git commit -m "feat: add JSON loader for item and entity definitions"
```

---

### Task 4: Inventory Operations

**Files:**
- Create: `src-tauri/src/item/inventory.rs`
- Modify: `src-tauri/src/item/mod.rs` (add `pub mod inventory;`)

- [ ] **Step 1: Write failing tests for inventory add**

Create `src-tauri/src/item/inventory.rs`:

```rust
use crate::item::types::{ItemDefs, ItemStack};

/// Player inventory — fixed-size array of optional item stacks.
#[derive(Debug, Clone)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
    pub capacity: usize,
}

impl Inventory {
    pub fn new(capacity: usize) -> Self {
        Self {
            slots: vec![None; capacity],
            capacity,
        }
    }

    /// Try to add items. Returns the count that couldn't fit.
    /// First stacks onto existing matching slots, then fills empty slots.
    pub fn add(&mut self, item_id: &str, mut count: u32, defs: &ItemDefs) -> u32 {
        let stack_limit = defs
            .get(item_id)
            .map(|d| d.stack_limit)
            .unwrap_or(1);

        // Phase 1: stack onto existing slots with the same item
        for slot in self.slots.iter_mut() {
            if count == 0 {
                break;
            }
            if let Some(stack) = slot {
                if stack.item_id == item_id && stack.count < stack_limit {
                    let room = stack_limit - stack.count;
                    let added = count.min(room);
                    stack.count += added;
                    count -= added;
                }
            }
        }

        // Phase 2: fill empty slots
        for slot in self.slots.iter_mut() {
            if count == 0 {
                break;
            }
            if slot.is_none() {
                let added = count.min(stack_limit);
                *slot = Some(ItemStack {
                    item_id: item_id.to_string(),
                    count: added,
                });
                count -= added;
            }
        }

        count // overflow
    }

    /// Remove items from a specific slot. Returns actual count removed.
    pub fn remove(&mut self, slot: usize, count: u32) -> u32 {
        if slot >= self.capacity {
            return 0;
        }
        if let Some(stack) = &mut self.slots[slot] {
            let removed = count.min(stack.count);
            stack.count -= removed;
            if stack.count == 0 {
                self.slots[slot] = None;
            }
            removed
        } else {
            0
        }
    }

    /// Drop entire stack from slot — returns what was there.
    pub fn drop_item(&mut self, slot: usize) -> Option<ItemStack> {
        if slot >= self.capacity {
            return None;
        }
        self.slots[slot].take()
    }

    /// Check if any room exists for this item type.
    pub fn has_room_for(&self, item_id: &str, defs: &ItemDefs) -> bool {
        let stack_limit = defs
            .get(item_id)
            .map(|d| d.stack_limit)
            .unwrap_or(1);

        // Any empty slot?
        if self.slots.iter().any(|s| s.is_none()) {
            return true;
        }

        // Any existing stack with room?
        self.slots.iter().any(|s| {
            s.as_ref()
                .is_some_and(|stack| stack.item_id == item_id && stack.count < stack_limit)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;

    fn test_defs() -> ItemDefs {
        let mut defs = ItemDefs::new();
        defs.insert(
            "cherry".into(),
            ItemDef {
                id: "cherry".into(),
                name: "Cherry".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 5,
                icon: "cherry".into(),
            },
        );
        defs.insert(
            "grain".into(),
            ItemDef {
                id: "grain".into(),
                name: "Grain".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 10,
                icon: "grain".into(),
            },
        );
        defs
    }

    #[test]
    fn add_to_empty_inventory() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        let overflow = inv.add("cherry", 3, &defs);
        assert_eq!(overflow, 0);
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 3);
    }

    #[test]
    fn add_stacks_onto_existing() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        let overflow = inv.add("cherry", 2, &defs);
        assert_eq!(overflow, 0);
        // Should stack onto slot 0 (limit 5), total 5
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 5);
        assert!(inv.slots[1].is_none());
    }

    #[test]
    fn add_overflows_to_new_slot() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 4, &defs);
        let overflow = inv.add("cherry", 3, &defs);
        assert_eq!(overflow, 0);
        // Slot 0: 5 (filled), Slot 1: 2 (overflow)
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 5);
        assert_eq!(inv.slots[1].as_ref().unwrap().count, 2);
    }

    #[test]
    fn add_returns_overflow_when_full() {
        let defs = test_defs();
        let mut inv = Inventory::new(2);
        inv.add("cherry", 5, &defs); // fills slot 0
        inv.add("cherry", 5, &defs); // fills slot 1
        let overflow = inv.add("cherry", 3, &defs);
        assert_eq!(overflow, 3);
    }

    #[test]
    fn add_different_items_use_separate_slots() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 2, &defs);
        inv.add("grain", 3, &defs);
        assert_eq!(inv.slots[0].as_ref().unwrap().item_id, "cherry");
        assert_eq!(inv.slots[1].as_ref().unwrap().item_id, "grain");
    }

    #[test]
    fn remove_from_slot() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 5, &defs);
        let removed = inv.remove(0, 3);
        assert_eq!(removed, 3);
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 2);
    }

    #[test]
    fn remove_all_clears_slot() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        let removed = inv.remove(0, 3);
        assert_eq!(removed, 3);
        assert!(inv.slots[0].is_none());
    }

    #[test]
    fn remove_from_empty_slot() {
        let inv = Inventory::new(4);
        // Calling remove requires &mut, but on a new inventory
        let mut inv = inv;
        let removed = inv.remove(0, 5);
        assert_eq!(removed, 0);
    }

    #[test]
    fn remove_capped_at_available() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        let removed = inv.remove(0, 10);
        assert_eq!(removed, 3);
        assert!(inv.slots[0].is_none());
    }

    #[test]
    fn drop_item_returns_stack() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        let dropped = inv.drop_item(0).unwrap();
        assert_eq!(dropped.item_id, "cherry");
        assert_eq!(dropped.count, 3);
        assert!(inv.slots[0].is_none());
    }

    #[test]
    fn drop_item_from_empty_slot() {
        let mut inv = Inventory::new(4);
        assert!(inv.drop_item(0).is_none());
    }

    #[test]
    fn drop_item_out_of_bounds() {
        let mut inv = Inventory::new(4);
        assert!(inv.drop_item(99).is_none());
    }

    #[test]
    fn has_room_for_empty_inventory() {
        let defs = test_defs();
        let inv = Inventory::new(4);
        assert!(inv.has_room_for("cherry", &defs));
    }

    #[test]
    fn has_room_for_existing_stack_with_space() {
        let defs = test_defs();
        let mut inv = Inventory::new(1);
        inv.add("cherry", 3, &defs); // limit 5, room for 2 more
        assert!(inv.has_room_for("cherry", &defs));
    }

    #[test]
    fn no_room_when_full() {
        let defs = test_defs();
        let mut inv = Inventory::new(1);
        inv.add("cherry", 5, &defs); // slot full
        assert!(!inv.has_room_for("cherry", &defs));
        assert!(!inv.has_room_for("grain", &defs));
    }
}
```

- [ ] **Step 2: Add module to mod.rs**

Add `pub mod inventory;` to `src-tauri/src/item/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test -p harmony-glitch --lib item::inventory`
Expected: 14 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/item/inventory.rs src-tauri/src/item/mod.rs
git commit -m "feat: add inventory operations with stacking and overflow"
```

---

### Task 5: Interaction System

**Files:**
- Create: `src-tauri/src/item/interaction.rs`
- Modify: `src-tauri/src/item/mod.rs` (add `pub mod interaction;`)

- [ ] **Step 1: Write interaction module with proximity scan and execution**

Create `src-tauri/src/item/interaction.rs`:

```rust
use rand::Rng;

use crate::item::inventory::Inventory;
use crate::item::types::{
    EntityDefs, InteractionPrompt, ItemDefs, PickupFeedback, WorldEntity, WorldItem,
};

/// Fixed pickup radius for ground items (pixels).
const GROUND_ITEM_PICKUP_RADIUS: f64 = 60.0;

/// What kind of interactable is nearest.
#[derive(Debug)]
pub enum NearestInteractable {
    Entity { index: usize, distance: f64 },
    GroundItem { index: usize, distance: f64 },
}

/// Find the nearest interactable within range of the player.
/// Returns None if nothing is in range.
/// Entities take priority over ground items at equal distance.
pub fn proximity_scan(
    player_x: f64,
    player_y: f64,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
) -> Option<NearestInteractable> {
    let mut best: Option<NearestInteractable> = None;
    let mut best_dist = f64::MAX;

    for (i, entity) in entities.iter().enumerate() {
        let dx = player_x - entity.x;
        let dy = player_y - entity.y;
        let dist = (dx * dx + dy * dy).sqrt();
        let radius = entity_defs
            .get(&entity.entity_type)
            .map(|d| d.interact_radius)
            .unwrap_or(60.0);
        if dist <= radius && dist < best_dist {
            best_dist = dist;
            best = Some(NearestInteractable::Entity {
                index: i,
                distance: dist,
            });
        }
    }

    for (i, item) in world_items.iter().enumerate() {
        let dx = player_x - item.x;
        let dy = player_y - item.y;
        let dist = (dx * dx + dy * dy).sqrt();
        // Ground items only win if strictly closer (entities take priority at equal distance)
        if dist <= GROUND_ITEM_PICKUP_RADIUS && dist < best_dist {
            best_dist = dist;
            best = Some(NearestInteractable::GroundItem {
                index: i,
                distance: dist,
            });
        }
    }

    best
}

/// Build an interaction prompt for the nearest interactable.
pub fn build_prompt(
    nearest: &NearestInteractable,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
    item_defs: &ItemDefs,
) -> InteractionPrompt {
    match nearest {
        NearestInteractable::Entity { index, .. } => {
            let entity = &entities[*index];
            let def = entity_defs.get(&entity.entity_type);
            InteractionPrompt {
                verb: def.map(|d| d.verb.clone()).unwrap_or_else(|| "Use".into()),
                target_name: def
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| "Unknown".into()),
                target_x: entity.x,
                target_y: entity.y,
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
            }
        }
    }
}

/// Result of executing an interaction.
pub struct InteractionResult {
    pub feedback: Vec<PickupFeedback>,
    /// Ground items to spawn (overflow when inventory full).
    pub spawned_items: Vec<(String, u32, f64, f64)>, // (item_id, count, x, y)
    /// Index of ground item to remove (if fully picked up).
    pub remove_ground_item: Option<usize>,
    /// Updated count for ground item (if partially picked up).
    pub update_ground_item: Option<(usize, u32)>,
}

/// Execute an interaction with the nearest interactable.
pub fn execute_interaction(
    nearest: &NearestInteractable,
    inventory: &mut Inventory,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
    item_defs: &ItemDefs,
    rng: &mut impl Rng,
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
                        text: "Inventory full!".into(),
                        success: false,
                        x: entity.x,
                        y: entity.y,
                        age_secs: 0.0,
                    });
                }
            }
        }
        NearestInteractable::GroundItem { index, .. } => {
            let item = &world_items[*index];
            let overflow = inventory.add(&item.item_id, item.count, item_defs);
            let added = item.count - overflow;

            if added > 0 {
                let name = item_defs
                    .get(&item.item_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| item.item_id.clone());
                result.feedback.push(PickupFeedback {
                    text: format!("+{} x{}", name, added),
                    success: true,
                    x: item.x,
                    y: item.y,
                    age_secs: 0.0,
                });
            }

            if overflow == 0 {
                result.remove_ground_item = Some(*index);
            } else if added > 0 {
                result.update_ground_item = Some((*index, overflow));
            } else {
                result.feedback.push(PickupFeedback {
                    text: "Inventory full!".into(),
                    success: false,
                    x: item.x,
                    y: item.y,
                    age_secs: 0.0,
                });
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::{EntityDef, ItemDef, YieldEntry};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn test_item_defs() -> ItemDefs {
        let mut defs = ItemDefs::new();
        defs.insert(
            "cherry".into(),
            ItemDef {
                id: "cherry".into(),
                name: "Cherry".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 5,
                icon: "cherry".into(),
            },
        );
        defs
    }

    fn test_entity_defs() -> EntityDefs {
        let mut defs = EntityDefs::new();
        defs.insert(
            "fruit_tree".into(),
            EntityDef {
                id: "fruit_tree".into(),
                name: "Fruit Tree".into(),
                verb: "Harvest".into(),
                yields: vec![YieldEntry {
                    item: "cherry".into(),
                    min: 2,
                    max: 2, // Fixed for deterministic tests
                }],
                cooldown_secs: 0.0,
                sprite_class: "tree_fruit".into(),
                interact_radius: 80.0,
            },
        );
        defs
    }

    #[test]
    fn proximity_scan_finds_entity_in_range() {
        let entity_defs = test_entity_defs();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 50.0,
            y: 0.0,
        }];
        let result = proximity_scan(40.0, 0.0, &entities, &entity_defs, &[]);
        assert!(matches!(
            result,
            Some(NearestInteractable::Entity { index: 0, .. })
        ));
    }

    #[test]
    fn proximity_scan_ignores_out_of_range() {
        let entity_defs = test_entity_defs();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 500.0,
            y: 0.0,
        }];
        let result = proximity_scan(0.0, 0.0, &entities, &entity_defs, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn proximity_scan_prefers_entity_over_ground_item_at_equal_distance() {
        let entity_defs = test_entity_defs();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 10.0,
            y: 0.0,
        }];
        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 1,
            x: 10.0,
            y: 0.0,
        }];
        let result = proximity_scan(10.0, 0.0, &entities, &entity_defs, &items);
        assert!(matches!(
            result,
            Some(NearestInteractable::Entity { .. })
        ));
    }

    #[test]
    fn proximity_scan_finds_ground_item() {
        let entity_defs = test_entity_defs();
        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 3,
            x: 30.0,
            y: 0.0,
        }];
        let result = proximity_scan(20.0, 0.0, &[], &entity_defs, &items);
        assert!(matches!(
            result,
            Some(NearestInteractable::GroundItem { index: 0, .. })
        ));
    }

    #[test]
    fn execute_entity_interaction_adds_to_inventory() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(4);
        let mut rng = StdRng::seed_from_u64(42);

        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 0.0,
            y: 0.0,
        }];

        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 0.0,
        };
        let result =
            execute_interaction(&nearest, &mut inv, &entities, &entity_defs, &[], &item_defs, &mut rng);

        assert_eq!(inv.slots[0].as_ref().unwrap().item_id, "cherry");
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 2);
        assert!(result.feedback.iter().any(|f| f.success));
    }

    #[test]
    fn execute_entity_interaction_overflows_to_ground() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(1);
        let mut rng = StdRng::seed_from_u64(42);

        // Fill inventory
        inv.add("cherry", 5, &item_defs);

        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 0.0,
            y: 0.0,
        }];

        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 0.0,
        };
        let result =
            execute_interaction(&nearest, &mut inv, &entities, &entity_defs, &[], &item_defs, &mut rng);

        assert!(!result.spawned_items.is_empty());
        assert!(result.feedback.iter().any(|f| !f.success));
    }

    #[test]
    fn execute_ground_item_pickup() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(4);
        let mut rng = StdRng::seed_from_u64(42);

        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 3,
            x: 0.0,
            y: 0.0,
        }];

        let nearest = NearestInteractable::GroundItem {
            index: 0,
            distance: 0.0,
        };
        let result =
            execute_interaction(&nearest, &mut inv, &[], &entity_defs, &items, &item_defs, &mut rng);

        assert_eq!(inv.slots[0].as_ref().unwrap().count, 3);
        assert_eq!(result.remove_ground_item, Some(0));
    }

    #[test]
    fn execute_ground_item_partial_pickup() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(1);
        let mut rng = StdRng::seed_from_u64(42);

        // Partially fill so only 2 more fit (limit 5, have 3)
        inv.add("cherry", 3, &item_defs);

        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 5,
            x: 0.0,
            y: 0.0,
        }];

        let nearest = NearestInteractable::GroundItem {
            index: 0,
            distance: 0.0,
        };
        let result =
            execute_interaction(&nearest, &mut inv, &[], &entity_defs, &items, &item_defs, &mut rng);

        assert_eq!(inv.slots[0].as_ref().unwrap().count, 5);
        assert_eq!(result.update_ground_item, Some((0, 3))); // 3 left on ground
    }

    #[test]
    fn build_prompt_for_entity() {
        let entity_defs = test_entity_defs();
        let item_defs = test_item_defs();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 100.0,
            y: -2.0,
        }];
        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 10.0,
        };
        let prompt = build_prompt(&nearest, &entities, &entity_defs, &[], &item_defs);
        assert_eq!(prompt.verb, "Harvest");
        assert_eq!(prompt.target_name, "Fruit Tree");
    }

    #[test]
    fn build_prompt_for_ground_item() {
        let entity_defs = test_entity_defs();
        let item_defs = test_item_defs();
        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 3,
            x: 50.0,
            y: 0.0,
        }];
        let nearest = NearestInteractable::GroundItem {
            index: 0,
            distance: 5.0,
        };
        let prompt = build_prompt(&nearest, &[], &entity_defs, &items, &item_defs);
        assert_eq!(prompt.verb, "Pick up");
        assert_eq!(prompt.target_name, "Cherry x3");
    }
}
```

- [ ] **Step 2: Add module to mod.rs**

Add `pub mod interaction;` to `src-tauri/src/item/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test -p harmony-glitch --lib item::interaction`
Expected: 10 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/item/interaction.rs src-tauri/src/item/mod.rs
git commit -m "feat: add interaction system with proximity scan and execution"
```

---

## Chunk 2: Rust Integration

### Task 6: Extend InputState

**Files:**
- Modify: `src-tauri/src/physics/movement.rs:38-42` (add interact field to InputState)

- [ ] **Step 1: Add interact field to InputState**

In `src-tauri/src/physics/movement.rs`, add `interact` to the `InputState` struct. The field defaults to `false` via `Default` derive, so existing tests and usage are unaffected:

```rust
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputState {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub interact: bool,
}
```

- [ ] **Step 2: Run existing tests to verify no breakage**

Run: `cd src-tauri && cargo test -p harmony-glitch`
Expected: All existing tests PASS (interact defaults to false)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/physics/movement.rs
git commit -m "feat: add interact field to InputState"
```

---

### Task 7: Extend GameState and Tick Loop

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

This is the largest integration task. GameState gains new fields, `tick()` gains an RNG parameter, and the tick loop includes proximity scan + interaction execution.

- [ ] **Step 1: Add imports and extend GameState struct**

Add to the imports at top of `src-tauri/src/engine/state.rs`:

```rust
use rand::Rng;
use crate::item::inventory::Inventory;
use crate::item::interaction;
use crate::item::types::{
    EntityDefs, InventoryFrame, InteractionPrompt, ItemDefs, ItemStackFrame,
    PickupFeedback, WorldEntity, WorldEntityFrame, WorldItem, WorldItemFrame,
};
```

Extend the `GameState` struct with new fields:

```rust
pub struct GameState {
    pub player: PhysicsBody,
    pub facing: Direction,
    pub street: Option<StreetData>,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub inventory: Inventory,
    pub world_entities: Vec<WorldEntity>,
    pub world_items: Vec<WorldItem>,
    pub item_defs: ItemDefs,
    pub entity_defs: EntityDefs,
    pub prev_interact: bool,
    pub next_item_id: u64,
    pub pickup_feedback: Vec<PickupFeedback>,
}
```

- [ ] **Step 2: Update GameState::new() and load_street()**

Update `new()` to accept defs and initialize new fields:

```rust
pub fn new(
    viewport_width: f64,
    viewport_height: f64,
    item_defs: ItemDefs,
    entity_defs: EntityDefs,
) -> Self {
    Self {
        player: PhysicsBody::new(0.0, -100.0),
        facing: Direction::Right,
        street: None,
        viewport_width,
        viewport_height,
        inventory: Inventory::new(16),
        world_entities: vec![],
        world_items: vec![],
        item_defs,
        entity_defs,
        prev_interact: false,
        next_item_id: 0,
        pickup_feedback: vec![],
    }
}
```

Update `load_street()` to accept and store entities:

```rust
pub fn load_street(&mut self, street: StreetData, entities: Vec<WorldEntity>) {
    let center_x = (street.left + street.right) / 2.0;
    self.player = PhysicsBody::new(center_x, street.ground_y);
    self.street = Some(street);
    self.world_entities = entities;
    self.world_items.clear();
    self.pickup_feedback.clear();
}
```

- [ ] **Step 3: Extend tick() with RNG parameter and interaction logic**

Change `tick()` signature and add interaction steps:

```rust
pub fn tick(&mut self, dt: f64, input: &InputState, rng: &mut impl Rng) -> Option<RenderFrame> {
    let street = self.street.as_ref()?;

    // Update facing direction
    if input.left && !input.right {
        self.facing = Direction::Left;
    } else if input.right && !input.left {
        self.facing = Direction::Right;
    }

    // Physics tick
    self.player
        .tick(dt, input, street.platforms(), street.left, street.right);

    // --- Interaction system ---
    // Age and cull pickup feedback
    for fb in &mut self.pickup_feedback {
        fb.age_secs += dt;
    }
    self.pickup_feedback.retain(|fb| fb.age_secs < 1.5);

    // Proximity scan
    let nearest = interaction::proximity_scan(
        self.player.x,
        self.player.y,
        &self.world_entities,
        &self.entity_defs,
        &self.world_items,
    );

    // Build prompt
    let interaction_prompt = nearest.as_ref().map(|n| {
        interaction::build_prompt(
            n,
            &self.world_entities,
            &self.entity_defs,
            &self.world_items,
            &self.item_defs,
        )
    });

    // Rising edge detection for interact
    let interact_pressed = input.interact && !self.prev_interact;
    self.prev_interact = input.interact;

    // Execute interaction on rising edge
    if interact_pressed {
        if let Some(nearest) = &nearest {
            let result = interaction::execute_interaction(
                nearest,
                &mut self.inventory,
                &self.world_entities,
                &self.entity_defs,
                &self.world_items,
                &self.item_defs,
                rng,
            );

            // Apply results
            self.pickup_feedback.extend(result.feedback);

            // Spawn overflow items
            for (item_id, count, x, y) in result.spawned_items {
                self.world_items.push(WorldItem {
                    id: format!("drop_{}", self.next_item_id),
                    item_id,
                    count,
                    x,
                    y,
                });
                self.next_item_id += 1;
            }

            // Remove or update ground items
            if let Some(idx) = result.remove_ground_item {
                self.world_items.remove(idx);
            } else if let Some((idx, new_count)) = result.update_ground_item {
                self.world_items[idx].count = new_count;
            }
        }
    }

    // Determine animation state
    let animation = if !self.player.on_ground {
        if self.player.vy < 0.0 {
            AnimationState::Jumping
        } else {
            AnimationState::Falling
        }
    } else if self.player.vx.abs() > 0.1 {
        AnimationState::Walking
    } else {
        AnimationState::Idle
    };

    // Camera
    let cam_x = self.player.x - self.viewport_width / 2.0;
    let cam_y = self.player.y - self.viewport_height * 0.6;
    let cam_x_min = street.left;
    let cam_x_max = (street.right - self.viewport_width).max(cam_x_min);
    let cam_y_min = street.top;
    let cam_y_max = (street.bottom - self.viewport_height).max(cam_y_min);
    let cam_x = cam_x.clamp(cam_x_min, cam_x_max);
    let cam_y = cam_y.clamp(cam_y_min, cam_y_max);

    // Build RenderFrame
    Some(RenderFrame {
        player: PlayerFrame {
            x: self.player.x,
            y: self.player.y,
            vx: self.player.vx,
            vy: self.player.vy,
            facing: self.facing,
            animation,
            on_ground: self.player.on_ground,
        },
        camera: CameraFrame { x: cam_x, y: cam_y },
        street_id: street.tsid.clone(),
        remote_players: vec![],
        inventory: self.build_inventory_frame(),
        world_entities: self.build_entity_frames(),
        world_items: self.build_item_frames(),
        interaction_prompt,
        pickup_feedback: self.pickup_feedback.clone(),
    })
}
```

- [ ] **Step 4: Add frame builder methods**

Add these private helper methods to `GameState`:

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
                    }
                })
            })
            .collect(),
        capacity: self.inventory.capacity,
    }
}

fn build_entity_frames(&self) -> Vec<WorldEntityFrame> {
    self.world_entities
        .iter()
        .map(|e| {
            let def = self.entity_defs.get(&e.entity_type);
            WorldEntityFrame {
                id: e.id.clone(),
                entity_type: e.entity_type.clone(),
                name: def.map(|d| d.name.clone()).unwrap_or_default(),
                sprite_class: def.map(|d| d.sprite_class.clone()).unwrap_or_default(),
                x: e.x,
                y: e.y,
            }
        })
        .collect()
}

fn build_item_frames(&self) -> Vec<WorldItemFrame> {
    self.world_items
        .iter()
        .map(|i| {
            let def = self.item_defs.get(&i.item_id);
            WorldItemFrame {
                id: i.id.clone(),
                item_id: i.item_id.clone(),
                name: def.map(|d| d.name.clone()).unwrap_or_default(),
                icon: def.map(|d| d.icon.clone()).unwrap_or_default(),
                count: i.count,
                x: i.x,
                y: i.y,
            }
        })
        .collect()
}
```

- [ ] **Step 5: Extend RenderFrame struct**

Add new fields to the `RenderFrame` struct in the same file:

```rust
pub struct RenderFrame {
    pub player: PlayerFrame,
    pub camera: CameraFrame,
    pub street_id: String,
    pub remote_players: Vec<RemotePlayerFrame>,
    pub inventory: InventoryFrame,
    pub world_entities: Vec<WorldEntityFrame>,
    pub world_items: Vec<WorldItemFrame>,
    pub interaction_prompt: Option<InteractionPrompt>,
    pub pickup_feedback: Vec<PickupFeedback>,
}
```

- [ ] **Step 6: Fix existing tests**

Update test helpers to pass the new parameters. All existing `GameState::new()` calls need item/entity defs, and all `state.tick()` calls need an RNG. Update `test_street()` helper and tests:

- `GameState::new(1280.0, 720.0)` → `GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new())`
- `state.tick(dt, &input)` → `state.tick(dt, &input, &mut rand::thread_rng())`
- `state.load_street(street)` → `state.load_street(street, vec![])`

Add `use crate::item::types::{ItemDefs, EntityDefs};` to test imports.

- [ ] **Step 7: Add interaction test**

Add a new test to the existing test module:

```rust
#[test]
fn interaction_adds_to_inventory() {
    use crate::item::types::{EntityDef, ItemDef, YieldEntry, WorldEntity};
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
        cooldown_secs: 0.0,
        sprite_class: "tree_fruit".into(),
        interact_radius: 80.0,
    });

    let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs);
    let street = test_street();
    let entities = vec![WorldEntity {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        x: 0.0,
        y: 0.0,
    }];
    state.load_street(street, entities);

    // Stand next to tree and press interact
    let input = InputState { interact: true, ..Default::default() };
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();

    assert_eq!(frame.inventory.slots[0].as_ref().unwrap().item_id, "cherry");
    assert!(frame.pickup_feedback.iter().any(|f| f.success));
}
```

- [ ] **Step 8: Run all tests**

Run: `cd src-tauri && cargo test -p harmony-glitch`
Expected: All tests PASS (existing + new)

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: integrate item system into GameState and tick loop"
```

---

### Task 8: lib.rs Integration

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add entity placement loader function**

Add alongside the existing `load_street_xml()` function (near line 378):

```rust
fn load_entity_placement(name: &str) -> Result<String, String> {
    match name {
        "demo_meadow" | "LADEMO001" => {
            Ok(include_str!("../../assets/streets/demo_meadow_entities.json").to_string())
        }
        "demo_heights" | "LADEMO002" => {
            Ok(include_str!("../../assets/streets/demo_heights_entities.json").to_string())
        }
        _ => Ok("[]".to_string()), // Streets without entities get an empty list
    }
}
```

- [ ] **Step 2: Update load_street command to load entities**

In the `load_street` Tauri command, after parsing street XML, parse and store entities:

```rust
// After: let street_data = parse_street(&xml)?;
let entity_json = load_entity_placement(&name)?;
let entities = item::loader::parse_entity_placements(&entity_json)?;

// Update game state — pass entities to load_street
{
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.load_street(street_data.clone(), entities);
}
```

- [ ] **Step 3: Load item/entity defs before GameState creation**

In the `run()` function, load defs before creating GameState. Replace the `.manage(GameStateWrapper(...))` line:

```rust
// Load item and entity definitions from bundled JSON
let item_defs = item::loader::parse_item_defs(include_str!("../../assets/items.json"))
    .expect("Failed to parse items.json");
let entity_defs = item::loader::parse_entity_defs(include_str!("../../assets/entities.json"))
    .expect("Failed to parse entities.json");

// ... then in the builder chain:
.manage(GameStateWrapper(Mutex::new(GameState::new(
    1280.0, 720.0, item_defs, entity_defs,
))))
```

- [ ] **Step 4: Update game loop tick() call to pass RNG**

In `game_loop()`, the existing `state.tick(dt, &input)` call (around line 335) becomes:

```rust
state.tick(dt, &input, &mut rng)
```

The `rng` variable already exists in the game loop (`rand::rngs::ThreadRng::default()` on line 289).

- [ ] **Step 5: Add drop_item Tauri command**

```rust
#[tauri::command]
fn drop_item(slot: usize, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    if let Some(stack) = state.inventory.drop_item(slot) {
        state.world_items.push(item::types::WorldItem {
            id: format!("drop_{}", state.next_item_id),
            item_id: stack.item_id,
            count: stack.count,
            x: state.player.x,
            y: state.player.y,
        });
        state.next_item_id += 1;
    }
    Ok(())
}
```

- [ ] **Step 6: Register drop_item in invoke_handler**

Add `drop_item` to the `tauri::generate_handler!` macro (around line 434):

```rust
.invoke_handler(tauri::generate_handler![
    list_streets,
    load_street,
    send_input,
    start_game,
    stop_game,
    get_identity,
    set_display_name,
    send_chat,
    get_network_status,
    drop_item,
])
```

- [ ] **Step 7: Run tests**

Run: `cd src-tauri && cargo test -p harmony-glitch`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: wire item system into Tauri — load defs, entities, drop command"
```

---

## Chunk 3: Frontend

### Task 9: TypeScript Types

**Files:**
- Modify: `src/lib/types.ts`

- [ ] **Step 1: Add interact to InputState**

In `src/lib/types.ts`, extend `InputState` (line 155-159):

```typescript
export interface InputState {
  left: boolean;
  right: boolean;
  jump: boolean;
  interact: boolean;
}
```

- [ ] **Step 2: Add new frame types**

Add after the existing `ChatEvent` interface:

```typescript
export interface InventoryFrame {
  slots: (ItemStackFrame | null)[];
  capacity: number;
}

export interface ItemStackFrame {
  itemId: string;
  name: string;
  description: string;
  icon: string;
  count: number;
  stackLimit: number;
}

export interface WorldEntityFrame {
  id: string;
  entityType: string;
  name: string;
  spriteClass: string;
  x: number;
  y: number;
}

export interface WorldItemFrame {
  id: string;
  itemId: string;
  name: string;
  icon: string;
  count: number;
  x: number;
  y: number;
}

export interface InteractionPrompt {
  verb: string;
  targetName: string;
  targetX: number;
  targetY: number;
}

export interface PickupFeedback {
  text: string;
  success: boolean;
  x: number;
  y: number;
  ageSecs: number;
}
```

- [ ] **Step 3: Extend RenderFrame**

Add the new fields to the existing `RenderFrame` interface:

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
}
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/types.ts
git commit -m "feat: add item system TypeScript types"
```

---

### Task 10: IPC Wrapper

**Files:**
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add dropItem function**

Add to `src/lib/ipc.ts`:

```typescript
export async function dropItem(slot: number): Promise<void> {
  return invoke('drop_item', { slot });
}
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/ipc.ts
git commit -m "feat: add dropItem IPC wrapper"
```

---

### Task 11: GameCanvas Input Extension

**Files:**
- Modify: `src/lib/components/GameCanvas.svelte`

- [ ] **Step 1: Extend key state, handlers, and add inventoryOpen guard**

In `GameCanvas.svelte`, add an `inventoryOpen` prop:

```typescript
let { chatFocused, onChatFocus, inventoryOpen = false }: {
  chatFocused: boolean;
  onChatFocus: () => void;
  inventoryOpen?: boolean;
} = $props();
```

Update the `keys` initial state (line 18):

```typescript
let keys = $state<InputState>({ left: false, right: false, jump: false, interact: false });
```

Update the chat focus effect (line 25):

```typescript
keys = { left: false, right: false, jump: false, interact: false };
sendInput({ left: false, right: false, jump: false, interact: false }).catch(console.error);
```

Guard `handleKeyDown` and `handleKeyUp` to return early when inventory is open (matching the `chatFocused` pattern). Add at the top of both handlers, after the `chatFocused` guard:

```typescript
if (inventoryOpen) return;
```

Add 'e' key handling to `handleKeyDown` (after the jump handler, around line 39):

```typescript
if (e.key === 'e' || e.key === 'E') { keys.interact = true; changed = true; }
```

Add to `handleKeyUp` (around line 48):

```typescript
if (e.key === 'e' || e.key === 'E') { keys.interact = false; changed = true; }
```

- [ ] **Step 2: Update aria-label**

Update the `aria-label` on the canvas container div to mention the interact key:

```
aria-label="Harmony Glitch game — use arrow keys or WASD to move, Space to jump, E to interact, I for inventory, F3 for debug overlay"
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/GameCanvas.svelte
git commit -m "feat: add interact key (E) to game input handling"
```

---

### Task 12: PixiJS Renderer Updates

**Files:**
- Modify: `src/lib/engine/renderer.ts`

- [ ] **Step 1: Add entity and item sprite management**

Add new private fields to `GameRenderer`:

```typescript
private entitySprites: Map<string, Container> = new Map();
private groundItemSprites: Map<string, Container> = new Map();
private promptText: Text | null = null;
private feedbackTexts: { text: Text; startAge: number }[] = [];
```

- [ ] **Step 2: Add entity rendering in buildScene()**

After avatar creation in `buildScene()`, add:

```typescript
// Prompt text in UI container (screen-fixed)
this.promptText = new Text({ text: '', style: { fontSize: 14, fill: 0xffffff } });
this.promptText.anchor.set(0.5, 1);
this.promptText.visible = false;
this.uiContainer.addChild(this.promptText);
```

- [ ] **Step 3: Add entity/item rendering in updateFrame()**

Add AFTER the `dt` computation (line 260-261: `const now = performance.now(); const dt = ...`) and BEFORE `this.updateChatBubbles(dt, ...)`, in `updateFrame()`. The `dt` variable must be defined before this code runs:

```typescript
// --- World entities ---
const worldEntities = frame.worldEntities ?? [];
const seenEntities = new Set<string>();
for (const entity of worldEntities) {
  seenEntities.add(entity.id);
  let sprite = this.entitySprites.get(entity.id);
  if (!sprite) {
    sprite = new Container();
    const body = new Graphics();
    // Placeholder: colored rectangle
    const color = entity.spriteClass.startsWith('tree') ? 0x2d8a4e : 0xc4a35a;
    const w = entity.spriteClass.startsWith('tree') ? 60 : 30;
    const h = entity.spriteClass.startsWith('tree') ? 80 : 30;
    body.rect(-w / 2, -h, w, h);
    body.fill({ color, alpha: 0.8 });
    sprite.addChild(body);

    const label = new Text({
      text: entity.name,
      style: { fontSize: 10, fill: 0xffffff, align: 'center' },
    });
    label.anchor.set(0.5, 1);
    label.y = -h - 4;
    sprite.addChild(label);

    this.worldContainer.addChild(sprite);
    this.entitySprites.set(entity.id, sprite);
  }
  sprite.x = entity.x - this.street.left;
  sprite.y = entity.y - this.street.top;
}
// Remove departed entities
for (const [id, sprite] of this.entitySprites) {
  if (!seenEntities.has(id)) {
    this.worldContainer.removeChild(sprite);
    sprite.destroy();
    this.entitySprites.delete(id);
  }
}

// --- Ground items ---
const groundItems = frame.worldItems ?? [];
const seenItems = new Set<string>();
for (const item of groundItems) {
  seenItems.add(item.id);
  let sprite = this.groundItemSprites.get(item.id);
  if (!sprite) {
    sprite = new Container();
    const body = new Graphics();
    body.circle(0, -8, 8);
    body.fill({ color: 0xe8c170, alpha: 0.9 });
    sprite.addChild(body);

    const label = new Text({
      text: item.count > 1 ? `${item.name} x${item.count}` : item.name,
      style: { fontSize: 9, fill: 0xffffff, align: 'center' },
    });
    label.anchor.set(0.5, 1);
    label.y = -18;
    sprite.addChild(label);

    this.worldContainer.addChild(sprite);
    this.groundItemSprites.set(item.id, sprite);
  } else {
    // Update label if count changed
    const label = sprite.children[1] as Text;
    const expectedText = item.count > 1 ? `${item.name} x${item.count}` : item.name;
    if (label && label.text !== expectedText) {
      label.text = expectedText;
    }
  }
  sprite.x = item.x - this.street.left;
  sprite.y = item.y - this.street.top;
  // Gentle bob animation
  sprite.y += Math.sin(performance.now() / 500) * 2;
}
for (const [id, sprite] of this.groundItemSprites) {
  if (!seenItems.has(id)) {
    this.worldContainer.removeChild(sprite);
    sprite.destroy();
    this.groundItemSprites.delete(id);
  }
}

// --- Interaction prompt ---
if (frame.interactionPrompt && this.promptText) {
  const p = frame.interactionPrompt;
  this.promptText.text = `[E] ${p.verb} ${p.targetName}`;
  // Convert world coords to screen coords
  const screenX = p.targetX - this.street.left + this.worldContainer.x;
  const screenY = p.targetY - this.street.top + this.worldContainer.y - 90;
  this.promptText.x = screenX;
  this.promptText.y = screenY;
  this.promptText.visible = true;
} else if (this.promptText) {
  this.promptText.visible = false;
}

// --- Pickup feedback ---
const feedback = frame.pickupFeedback ?? [];
// Remove old feedback texts
this.feedbackTexts = this.feedbackTexts.filter((ft) => {
  if (ft.startAge >= 1.5) {
    this.uiContainer.removeChild(ft.text);
    ft.text.destroy();
    return false;
  }
  return true;
});
// Add new feedback (age_secs near 0)
for (const fb of feedback) {
  if (fb.ageSecs < dt * 2) {
    const existing = this.feedbackTexts.find(
      (ft) => ft.text.text === fb.text && ft.startAge < 0.1
    );
    if (!existing) {
      const text = new Text({
        text: fb.text,
        style: { fontSize: 14, fill: fb.success ? 0x7ae87a : 0xe87a7a },
      });
      text.anchor.set(0.5, 1);
      this.uiContainer.addChild(text);
      this.feedbackTexts.push({ text, startAge: 0 });
    }
  }
}
// Animate existing feedback
for (const ft of this.feedbackTexts) {
  ft.startAge += dt;
  const matchingFb = feedback.find((f) => f.text === ft.text.text);
  if (matchingFb) {
    const screenX = matchingFb.x - this.street.left + this.worldContainer.x;
    const screenY = matchingFb.y - this.street.top + this.worldContainer.y - 100 - ft.startAge * 30;
    ft.text.x = screenX;
    ft.text.y = screenY;
    ft.text.alpha = Math.max(0, 1 - ft.startAge / 1.5);
  }
}
```

- [ ] **Step 4: Clean up entities and items in destroy() and buildScene()**

Add cleanup to `buildScene()` (after existing cleanup of remote sprites):

```typescript
for (const [, sprite] of this.entitySprites) {
  sprite.destroy();
}
this.entitySprites.clear();
for (const [, sprite] of this.groundItemSprites) {
  sprite.destroy();
}
this.groundItemSprites.clear();
if (this.promptText) {
  this.promptText.destroy();
  this.promptText = null;
}
for (const ft of this.feedbackTexts) {
  ft.text.destroy();
}
this.feedbackTexts = [];
```

Add same cleanup to `destroy()`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/engine/renderer.ts
git commit -m "feat: render world entities, ground items, interaction prompts, and pickup feedback"
```

---

### Task 13: Inventory Panel Component

**Files:**
- Create: `src/lib/components/InventoryPanel.svelte`

- [ ] **Step 1: Create InventoryPanel component**

```svelte
<script lang="ts">
  import type { InventoryFrame, ItemStackFrame } from '../types';
  import { dropItem } from '../ipc';

  let { inventory, visible = false, onClose }: {
    inventory: InventoryFrame | null;
    visible?: boolean;
    onClose?: () => void;
  } = $props();

  let selectedSlot = $state<number | null>(null);
  let selectedItem = $derived.by(() => {
    if (selectedSlot === null || !inventory) return null;
    return inventory.slots[selectedSlot] ?? null;
  });

  function handleSlotClick(index: number) {
    selectedSlot = selectedSlot === index ? null : index;
  }

  async function handleDrop() {
    if (selectedSlot === null) return;
    try {
      await dropItem(selectedSlot);
      selectedSlot = null;
    } catch (e) {
      console.error('Drop failed:', e);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!visible) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      onClose?.();
      return;
    }
    // Note: 'I' key toggle is handled in App.svelte to avoid race conditions
    // between multiple window-level keydown listeners.

    if (e.key === 'd' || e.key === 'D') {
      handleDrop();
      return;
    }

    if (!inventory) return;
    const cols = 4;
    const total = inventory.capacity;

    if (e.key === 'ArrowRight') {
      e.preventDefault();
      selectedSlot = selectedSlot === null ? 0 : Math.min(selectedSlot + 1, total - 1);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      selectedSlot = selectedSlot === null ? 0 : Math.max(selectedSlot - 1, 0);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedSlot = selectedSlot === null ? 0 : Math.min(selectedSlot + cols, total - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedSlot = selectedSlot === null ? 0 : Math.max(selectedSlot - cols, 0);
    } else if (e.key === 'Enter') {
      if (selectedSlot !== null) {
        handleSlotClick(selectedSlot);
      }
    }
  }
</script>

{#if visible}
  <svelte:window onkeydown={handleKeyDown} />
  <div class="inventory-panel" role="dialog" aria-label="Inventory">
    <h3>Inventory</h3>
    <div class="slots" role="grid" aria-label="Inventory slots">
      {#each { length: Math.ceil((inventory?.capacity ?? 16) / 4) } as _, row}
        <div role="row" class="slot-row">
          {#each inventory?.slots?.slice(row * 4, row * 4 + 4) ?? [] as slot, col}
            {@const i = row * 4 + col}
            <button
              type="button"
              class="slot"
              class:selected={selectedSlot === i}
              class:filled={slot !== null}
              role="gridcell"
              aria-label={slot ? `${slot.name} x${slot.count}` : `Empty slot ${i + 1}`}
              onclick={() => handleSlotClick(i)}
            >
              {#if slot}
                <span class="slot-icon">{slot.icon.charAt(0).toUpperCase()}</span>
                <span class="slot-count">{slot.count}</span>
              {/if}
            </button>
          {/each}
        </div>
      {/each}
    </div>

    {#if selectedItem}
      <div class="item-details">
        <div class="item-name">{selectedItem.name}</div>
        <div class="item-desc">{selectedItem.description}</div>
        <div class="item-count">{selectedItem.count} / {selectedItem.stackLimit}</div>
        <button type="button" class="drop-btn" onclick={handleDrop}>
          Drop
        </button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .inventory-panel {
    position: fixed;
    top: 0;
    right: 0;
    width: 200px;
    height: 100%;
    background: rgba(20, 20, 40, 0.92);
    border-left: 1px solid #444;
    padding: 12px;
    z-index: 100;
    color: #e0e0e0;
    display: flex;
    flex-direction: column;
  }

  h3 {
    margin: 0 0 12px 0;
    font-size: 0.9rem;
    text-transform: uppercase;
    color: #888;
    letter-spacing: 1px;
  }

  .slots {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .slot-row {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
  }

  .slot {
    width: 40px;
    height: 40px;
    background: rgba(40, 40, 70, 0.8);
    border: 1px solid #3a3a5a;
    border-radius: 4px;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 0;
    color: #e0e0e0;
    position: relative;
    font-size: 0.7rem;
  }

  .slot:hover {
    border-color: #6a6a9a;
  }

  .slot.selected {
    border-color: #5865f2;
    box-shadow: 0 0 6px rgba(88, 101, 242, 0.4);
  }

  .slot.filled {
    background: rgba(50, 50, 80, 0.9);
  }

  .slot-icon {
    font-size: 1rem;
    line-height: 1;
  }

  .slot-count {
    font-size: 0.6rem;
    color: #aaa;
    position: absolute;
    bottom: 1px;
    right: 3px;
  }

  .item-details {
    margin-top: 12px;
    padding: 8px;
    background: rgba(40, 40, 70, 0.6);
    border-radius: 4px;
  }

  .item-name {
    font-weight: bold;
    font-size: 0.85rem;
    margin-bottom: 2px;
  }

  .item-desc {
    font-size: 0.7rem;
    color: #999;
    margin-bottom: 4px;
    font-style: italic;
  }

  .item-count {
    font-size: 0.75rem;
    color: #aaa;
    margin-bottom: 8px;
  }

  .drop-btn {
    background: rgba(80, 60, 40, 0.8);
    color: #e8c170;
    border: 1px solid #6a5a3a;
    border-radius: 3px;
    padding: 4px 12px;
    cursor: pointer;
    font-size: 0.75rem;
  }

  .drop-btn:hover {
    background: rgba(100, 80, 50, 0.9);
  }
</style>
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/components/InventoryPanel.svelte
git commit -m "feat: add inventory panel component"
```

---

### Task 14: App Integration

**Files:**
- Modify: `src/App.svelte`

- [ ] **Step 1: Add inventory toggle state and panel**

Import the panel and add toggle state:

```typescript
import InventoryPanel from './lib/components/InventoryPanel.svelte';

let inventoryOpen = $state(false);
```

Add 'I' key handler in the existing `<svelte:window>` handler:

```svelte
<svelte:window onkeydown={(e) => {
  if (e.key === 'F3') { e.preventDefault(); toggleDebug(); }
  if ((e.key === 'i' || e.key === 'I') && currentStreet && !chatFocused) {
    e.preventDefault();
    inventoryOpen = !inventoryOpen;
  }
}} />
```

Pass `inventoryOpen` to GameCanvas so it can suppress movement input when the panel is open:

```svelte
<GameCanvas ... inventoryOpen={inventoryOpen} />
```

Add the panel alongside the other game components (after `NetworkStatus`):

```svelte
<InventoryPanel
  inventory={latestFrame?.inventory ?? null}
  visible={inventoryOpen}
  onClose={() => { inventoryOpen = false; }}
/>
```

- [ ] **Step 2: Commit**

```bash
git add src/App.svelte
git commit -m "feat: wire inventory panel into app with I key toggle"
```

---

### Task 15: Smoke Test & Gitignore

**Files:**
- Modify: `.gitignore` (add `.superpowers/`)

- [ ] **Step 1: Add .superpowers to gitignore**

Add `.superpowers/` to `.gitignore` so brainstorm mockups don't get committed.

- [ ] **Step 2: Run full Rust test suite**

Run: `cd src-tauri && cargo test -p harmony-glitch`
Expected: All tests PASS (existing + ~35 new)

- [ ] **Step 3: Run cargo clippy**

Run: `cd src-tauri && cargo clippy`
Expected: No warnings

- [ ] **Step 4: Build frontend**

Run: `npm run build`
Expected: Build succeeds

- [ ] **Step 5: Commit**

```bash
git add .gitignore
git commit -m "chore: add .superpowers to gitignore"
```
