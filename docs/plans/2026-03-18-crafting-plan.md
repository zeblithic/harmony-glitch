# Crafting System Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add recipe-based crafting so players can combine inventory items to produce new items, with tool requirements (e.g. pot for cooking).

**Architecture:** Crafting is a pure function in Rust (`craft()`) taking a recipe, inventory, and item_defs. Recipe data loads from JSON at startup (same pattern as items/entities). Frontend adds a Recipes tab to the existing InventoryPanel with local availability checking. Two new IPC commands bridge the halves: `get_recipes` (load once) and `craft_recipe` (execute).

**Tech Stack:** Rust (Tauri v2), Svelte 5 (runes), TypeScript, vitest

**Spec:** `docs/plans/2026-03-18-crafting-design.md`

---

## File Structure

### New files
| File | Responsibility |
|------|---------------|
| `src-tauri/src/item/crafting.rs` | `craft()` pure function, `check_recipe_availability()` helper |
| `assets/recipes.json` | 6 recipe definitions (cherry_pie, bread, steak, butter, bubble_wand, plank) |

### Modified files
| File | Changes |
|------|---------|
| `src-tauri/src/item/types.rs` | Add RecipeDef, RecipeItem, CraftError, CraftOutput, RecipeAvailability, IngredientStatus, RecipeDefs type alias |
| `src-tauri/src/item/inventory.rs` | Add `count_item()`, `has_room_for_count()`, and `remove_item()` methods |
| `src-tauri/src/item/loader.rs` | Add `parse_recipe_defs()` function |
| `src-tauri/src/item/mod.rs` | Add `pub mod crafting;` |
| `src-tauri/src/engine/state.rs` | Add `recipe_defs` field to GameState, `craft_recipe()` method, ground items in `load_street()` |
| `src-tauri/src/lib.rs` | Add `get_recipes` and `craft_recipe` IPC commands, load recipes at startup, load ground items in `load_street` |
| `assets/items.json` | Add 7 new items (cherry_pie, bread, steak, butter, bubble_wand, plank, pot) |
| `assets/streets/demo_meadow_entities.json` | Add `groundItems` array with pot placement (requires changing from array to object format) |
| `src/lib/types.ts` | Add RecipeDef, RecipeItem interfaces |
| `src/lib/ipc.ts` | Add `getRecipes()`, `craftRecipe()` wrappers |
| `src/lib/components/InventoryPanel.svelte` | Add tabbed interface (Items \| Recipes), recipe list, craft button |
| `src/App.svelte` | Load recipes on init, pass to InventoryPanel |

---

## Chunk 1: Data Model and Inventory Methods

### Task 1: Add crafting types to types.rs

**Files:**
- Modify: `src-tauri/src/item/types.rs`

- [ ] **Step 1: Write tests for RecipeDef serialization**

Add these tests to the existing `#[cfg(test)] mod tests` block at the bottom of `types.rs`:

```rust
#[test]
fn recipe_def_deserializes_from_json() {
    let json = r#"{
        "name": "Cherry Pie",
        "description": "A delicious pie.",
        "inputs": [{ "item": "cherry", "count": 5 }],
        "tools": [{ "item": "pot", "count": 1 }],
        "outputs": [{ "item": "cherry_pie", "count": 1 }],
        "durationSecs": 10.0,
        "category": "food"
    }"#;
    let def: RecipeDef = serde_json::from_str(json).unwrap();
    assert_eq!(def.name, "Cherry Pie");
    assert_eq!(def.inputs.len(), 1);
    assert_eq!(def.inputs[0].item, "cherry");
    assert_eq!(def.inputs[0].count, 5);
    assert_eq!(def.tools.len(), 1);
    assert_eq!(def.tools[0].item, "pot");
    assert_eq!(def.outputs.len(), 1);
    assert_eq!(def.outputs[0].item, "cherry_pie");
    assert_eq!(def.id, ""); // id is skip_deserializing
}

#[test]
fn recipe_def_serializes_id() {
    let def = RecipeDef {
        id: "cherry_pie".into(),
        name: "Cherry Pie".into(),
        description: "A delicious pie.".into(),
        inputs: vec![RecipeItem { item: "cherry".into(), count: 5 }],
        tools: vec![],
        outputs: vec![RecipeItem { item: "cherry_pie".into(), count: 1 }],
        duration_secs: 10.0,
        category: "food".into(),
    };
    let json = serde_json::to_string(&def).unwrap();
    // id IS included in serialization (skip_deserializing, not skip)
    assert!(json.contains(r#""id":"cherry_pie""#));
    // camelCase field names
    assert!(json.contains("durationSecs"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch-lib --lib item::types::tests::recipe_def 2>&1 | tail -5`
Expected: FAIL — `RecipeDef` not defined

- [ ] **Step 3: Add the type definitions**

Add after the `pub type EntityDefs = HashMap<String, EntityDef>;` line (line 112 of types.rs):

```rust
/// Recipe definition (loaded from JSON at startup).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeDef {
    #[serde(skip_deserializing)]
    pub id: String,
    pub name: String,
    pub description: String,
    pub inputs: Vec<RecipeItem>,
    pub tools: Vec<RecipeItem>,
    pub outputs: Vec<RecipeItem>,
    pub duration_secs: f64,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeItem {
    pub item: String,
    pub count: u32,
}

pub type RecipeDefs = HashMap<String, RecipeDef>;

/// Error from a crafting attempt.
#[derive(Debug, Clone)]
pub enum CraftError {
    MissingInput { item: String, need: u32, have: u32 },
    MissingTool { item: String },
    NoRoom,
    UnknownRecipe,
}

impl std::fmt::Display for CraftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CraftError::MissingInput { item, need, have } => {
                write!(f, "Need {} {} but only have {}", need, item, have)
            }
            CraftError::MissingTool { item } => write!(f, "Missing tool: {}", item),
            CraftError::NoRoom => write!(f, "Inventory full"),
            CraftError::UnknownRecipe => write!(f, "Unknown recipe"),
        }
    }
}

/// Result of a successful craft — one entry per output item.
#[derive(Debug, Clone)]
pub struct CraftOutput {
    pub item_id: String,
    pub name: String,
    pub count: u32,
}

/// Per-ingredient availability status (for frontend display).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientStatus {
    pub item: String,
    pub need: u32,
    pub have: u32,
}

/// Whether a recipe can be crafted given current inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeAvailability {
    pub craftable: bool,
    pub inputs: Vec<IngredientStatus>,
    pub tools: Vec<IngredientStatus>,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch-lib --lib item::types::tests::recipe_def`
Expected: PASS (both tests)

- [ ] **Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/item/types.rs
git commit -m "feat(crafting): add RecipeDef, CraftError, CraftOutput types"
```

---

### Task 2: Add count_item() and remove_item() to Inventory

**Files:**
- Modify: `src-tauri/src/item/inventory.rs`

- [ ] **Step 1: Write tests for count_item and remove_item**

Add these tests to the existing `#[cfg(test)] mod tests` block in `inventory.rs`:

```rust
#[test]
fn count_item_empty_inventory() {
    let inv = Inventory::new(4);
    assert_eq!(inv.count_item("cherry"), 0);
}

#[test]
fn count_item_single_slot() {
    let defs = test_defs();
    let mut inv = Inventory::new(4);
    inv.add("cherry", 3, &defs);
    assert_eq!(inv.count_item("cherry"), 3);
    assert_eq!(inv.count_item("grain"), 0);
}

#[test]
fn count_item_across_multiple_slots() {
    let defs = test_defs();
    let mut inv = Inventory::new(4);
    inv.add("cherry", 5, &defs); // fills slot 0 (stack_limit=5)
    inv.add("cherry", 3, &defs); // goes to slot 1
    assert_eq!(inv.count_item("cherry"), 8);
}

#[test]
fn remove_item_by_id_single_slot() {
    let defs = test_defs();
    let mut inv = Inventory::new(4);
    inv.add("cherry", 5, &defs);
    inv.remove_item("cherry", 3);
    assert_eq!(inv.count_item("cherry"), 2);
}

#[test]
fn remove_item_by_id_across_slots() {
    let defs = test_defs();
    let mut inv = Inventory::new(4);
    inv.add("cherry", 5, &defs); // slot 0: 5
    inv.add("cherry", 3, &defs); // slot 1: 3
    inv.remove_item("cherry", 7); // removes 5 from slot 0, 2 from slot 1
    assert_eq!(inv.count_item("cherry"), 1);
    assert!(inv.slots[0].is_none()); // slot 0 fully consumed
    assert_eq!(inv.slots[1].as_ref().unwrap().count, 1);
}

#[test]
fn remove_item_clears_empty_slots() {
    let defs = test_defs();
    let mut inv = Inventory::new(4);
    inv.add("cherry", 3, &defs);
    inv.remove_item("cherry", 3);
    assert!(inv.slots[0].is_none());
    assert_eq!(inv.count_item("cherry"), 0);
}

#[test]
fn has_room_for_count_empty_inventory() {
    let defs = test_defs();
    let inv = Inventory::new(4);
    assert!(inv.has_room_for_count("cherry", 10, &defs)); // 4 slots * 5 limit = 20 room
    assert!(!inv.has_room_for_count("cherry", 21, &defs));
}

#[test]
fn has_room_for_count_partial_stack() {
    let defs = test_defs();
    let mut inv = Inventory::new(2);
    inv.add("cherry", 4, &defs); // slot 0: 4/5
    // Room: 1 in slot 0 + 5 in slot 1 = 6
    assert!(inv.has_room_for_count("cherry", 6, &defs));
    assert!(!inv.has_room_for_count("cherry", 7, &defs));
}

#[test]
fn has_room_for_count_full_inventory() {
    let defs = test_defs();
    let mut inv = Inventory::new(1);
    inv.add("cherry", 5, &defs); // slot 0: full
    assert!(!inv.has_room_for_count("cherry", 1, &defs));
}

#[test]
fn remove_item_does_not_touch_other_items() {
    let defs = test_defs();
    let mut inv = Inventory::new(4);
    inv.add("cherry", 3, &defs);
    inv.add("grain", 5, &defs);
    inv.remove_item("cherry", 3);
    assert_eq!(inv.count_item("grain"), 5);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch-lib --lib item::inventory::tests::count_item 2>&1 | tail -5`
Expected: FAIL — `count_item` not defined

- [ ] **Step 3: Implement count_item and remove_item**

Add these methods to the `impl Inventory` block, after the `has_room_for` method (before the closing `}`):

```rust
    /// Count total quantity of an item across all inventory slots.
    pub fn count_item(&self, item_id: &str) -> u32 {
        self.slots
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| stack.count)
            .sum()
    }

    /// Check if `count` items of this type can fit in the inventory.
    /// More precise than `has_room_for` (which only checks for 1 item).
    /// Counts available room across existing stacks + empty slots.
    pub fn has_room_for_count(&self, item_id: &str, count: u32, defs: &ItemDefs) -> bool {
        let stack_limit = defs.get(item_id).map(|d| d.stack_limit).unwrap_or(1);
        let mut room: u32 = 0;
        for slot in &self.slots {
            match slot {
                Some(stack) if stack.item_id == item_id => {
                    room += stack_limit - stack.count;
                }
                None => {
                    room += stack_limit;
                }
                _ => {}
            }
            if room >= count {
                return true;
            }
        }
        room >= count
    }

    /// Remove `count` items of the given item_id across inventory slots.
    /// Caller must verify sufficient quantity exists first via `count_item`.
    pub fn remove_item(&mut self, item_id: &str, mut count: u32) {
        for slot in 0..self.capacity {
            if count == 0 {
                break;
            }
            let matches = self.slots[slot]
                .as_ref()
                .is_some_and(|s| s.item_id == item_id);
            if matches {
                let available = self.slots[slot].as_ref().unwrap().count;
                let to_remove = count.min(available);
                self.remove(slot, to_remove);
                count -= to_remove;
            }
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch-lib --lib item::inventory::tests`
Expected: PASS (all inventory tests, including new ones)

- [ ] **Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/item/inventory.rs
git commit -m "feat(crafting): add count_item() and remove_item() to Inventory"
```

---

### Task 3: Add recipe data files

**Files:**
- Create: `assets/recipes.json`
- Modify: `assets/items.json`

- [ ] **Step 1: Add 7 new items to items.json**

Add these entries to the existing object in `assets/items.json`, after the `"wood"` entry:

```json
  "cherry_pie": {
    "name": "Cherry Pie",
    "description": "A delicious pie baked with fresh cherries.",
    "category": "food",
    "stackLimit": 10,
    "icon": "cherry_pie"
  },
  "bread": {
    "name": "Bread",
    "description": "Simple baked bread. Hearty and filling.",
    "category": "food",
    "stackLimit": 20,
    "icon": "bread"
  },
  "steak": {
    "name": "Steak",
    "description": "Grilled meat on a wood fire. Savory.",
    "category": "food",
    "stackLimit": 10,
    "icon": "steak"
  },
  "butter": {
    "name": "Butter",
    "description": "Churned from fresh butterfly milk.",
    "category": "food",
    "stackLimit": 20,
    "icon": "butter"
  },
  "bubble_wand": {
    "name": "Bubble Wand",
    "description": "A wand for blowing iridescent bubbles.",
    "category": "tool",
    "stackLimit": 1,
    "icon": "bubble_wand"
  },
  "plank": {
    "name": "Plank",
    "description": "Processed lumber. Useful for building.",
    "category": "material",
    "stackLimit": 50,
    "icon": "plank"
  },
  "pot": {
    "name": "Pot",
    "description": "A sturdy cooking pot. Required for most recipes.",
    "category": "tool",
    "stackLimit": 1,
    "icon": "pot"
  }
```

- [ ] **Step 2: Create recipes.json**

Create `assets/recipes.json` with exactly the content from the spec (lines 94-150 of the design doc). This is the complete JSON object with 6 recipes: cherry_pie, bread, steak, butter, bubble_wand, plank.

```json
{
  "cherry_pie": {
    "name": "Cherry Pie",
    "description": "A delicious pie.",
    "inputs": [{ "item": "cherry", "count": 5 }, { "item": "grain", "count": 2 }],
    "tools": [{ "item": "pot", "count": 1 }],
    "outputs": [{ "item": "cherry_pie", "count": 1 }],
    "durationSecs": 10.0,
    "category": "food"
  },
  "bread": {
    "name": "Bread",
    "description": "Simple baked bread.",
    "inputs": [{ "item": "grain", "count": 4 }],
    "tools": [{ "item": "pot", "count": 1 }],
    "outputs": [{ "item": "bread", "count": 1 }],
    "durationSecs": 8.0,
    "category": "food"
  },
  "steak": {
    "name": "Steak",
    "description": "Grilled meat on a wood fire.",
    "inputs": [{ "item": "meat", "count": 2 }, { "item": "wood", "count": 1 }],
    "tools": [{ "item": "pot", "count": 1 }],
    "outputs": [{ "item": "steak", "count": 1 }],
    "durationSecs": 12.0,
    "category": "food"
  },
  "butter": {
    "name": "Butter",
    "description": "Churned from fresh milk.",
    "inputs": [{ "item": "milk", "count": 3 }],
    "tools": [{ "item": "pot", "count": 1 }],
    "outputs": [{ "item": "butter", "count": 1 }],
    "durationSecs": 6.0,
    "category": "food"
  },
  "bubble_wand": {
    "name": "Bubble Wand",
    "description": "A wand for blowing bubbles.",
    "inputs": [{ "item": "bubble", "count": 3 }, { "item": "wood", "count": 2 }],
    "tools": [],
    "outputs": [{ "item": "bubble_wand", "count": 1 }],
    "durationSecs": 5.0,
    "category": "tool"
  },
  "plank": {
    "name": "Plank",
    "description": "Processed lumber.",
    "inputs": [{ "item": "wood", "count": 3 }],
    "tools": [],
    "outputs": [{ "item": "plank", "count": 2 }],
    "durationSecs": 4.0,
    "category": "material"
  }
}
```

- [ ] **Step 3: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add assets/items.json assets/recipes.json
git commit -m "feat(crafting): add 7 new items and 6 recipes data files"
```

---

### Task 4: Add recipe loader and update existing tests

**Files:**
- Modify: `src-tauri/src/item/loader.rs`

- [ ] **Step 1: Write tests for parse_recipe_defs**

Add these tests to the existing `#[cfg(test)] mod tests` block in `loader.rs`. You will also need to add `RecipeDefs` to the existing `use crate::item::types::{...}` import at the top of the file.

```rust
#[test]
fn parse_recipe_defs_from_json() {
    let json = r#"{
        "bread": {
            "name": "Bread",
            "description": "Simple bread.",
            "inputs": [{ "item": "grain", "count": 4 }],
            "tools": [{ "item": "pot", "count": 1 }],
            "outputs": [{ "item": "bread", "count": 1 }],
            "durationSecs": 8.0,
            "category": "food"
        }
    }"#;
    let defs = parse_recipe_defs(json).unwrap();
    assert_eq!(defs.len(), 1);
    let bread = &defs["bread"];
    assert_eq!(bread.id, "bread");
    assert_eq!(bread.name, "Bread");
    assert_eq!(bread.inputs.len(), 1);
    assert_eq!(bread.inputs[0].item, "grain");
    assert_eq!(bread.tools.len(), 1);
    assert_eq!(bread.tools[0].item, "pot");
}

#[test]
fn parse_bundled_recipes_json() {
    let json = include_str!("../../../assets/recipes.json");
    let defs = parse_recipe_defs(json).unwrap();
    assert_eq!(defs.len(), 6);
    assert!(defs.contains_key("cherry_pie"));
    assert!(defs.contains_key("bread"));
    assert!(defs.contains_key("plank"));
    // Verify tools field parsed correctly
    assert_eq!(defs["cherry_pie"].tools.len(), 1);
    assert_eq!(defs["cherry_pie"].tools[0].item, "pot");
    // Verify no-tool recipe
    assert!(defs["plank"].tools.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch-lib --lib item::loader::tests::parse_recipe 2>&1 | tail -5`
Expected: FAIL — `parse_recipe_defs` not defined

- [ ] **Step 3: Implement parse_recipe_defs**

Add to `loader.rs`, after the `parse_entity_placements` function. Also add `RecipeDefs` to the imports at the top of the file.

Top of file — update the import:
```rust
use crate::item::types::{EntityDefs, ItemDefs, RecipeDefs, WorldEntity};
```

New function:
```rust
/// Parse recipe definitions from JSON string.
pub fn parse_recipe_defs(json: &str) -> Result<RecipeDefs, String> {
    let mut raw: RecipeDefs =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse recipes.json: {e}"))?;
    for (key, def) in raw.iter_mut() {
        def.id = key.clone();
    }
    Ok(raw)
}
```

- [ ] **Step 4: Update existing item count test**

The `parse_bundled_items_json` test currently asserts `defs.len() == 6`. Update it to `13` to account for the 7 new items:

```rust
// In parse_bundled_items_json test:
assert_eq!(defs.len(), 13);
```

Also add assertions for the new items:
```rust
assert!(defs.contains_key("cherry_pie"));
assert!(defs.contains_key("pot"));
assert!(defs.contains_key("plank"));
```

- [ ] **Step 5: Run all loader tests to verify they pass**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch-lib --lib item::loader::tests`
Expected: PASS (all loader tests)

- [ ] **Step 6: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/item/loader.rs
git commit -m "feat(crafting): add parse_recipe_defs and update item count test"
```

---

## Chunk 2: Crafting Logic

### Task 5: Implement craft() and check_recipe_availability()

**Files:**
- Create: `src-tauri/src/item/crafting.rs`
- Modify: `src-tauri/src/item/mod.rs`

This is the core of the feature. The `craft()` function is a pure function: recipe in, mutated inventory out, or typed error. No I/O, no side effects beyond inventory mutation.

**Important context for implementer:** The `Inventory` struct lives in `inventory.rs`. Key methods you'll use:
- `inventory.count_item(item_id) -> u32` — count of an item across all slots (added in Task 2)
- `inventory.remove_item(item_id, count)` — remove N items by ID across slots (added in Task 2)
- `inventory.add(item_id, count, item_defs) -> u32` — returns overflow count (0 = all fit)
- `inventory.has_room_for(item_id, item_defs) -> bool` — check if at least 1 item can fit

The `ItemDefs` type is `HashMap<String, ItemDef>`. Use `item_defs.get(item_id)` to look up an item's name for `CraftOutput`.

- [ ] **Step 1: Register the module**

Add `pub mod crafting;` to `src-tauri/src/item/mod.rs`:

```rust
pub mod crafting;
pub mod interaction;
pub mod inventory;
pub mod loader;
pub mod types;
```

- [ ] **Step 2: Write tests for craft()**

Create `src-tauri/src/item/crafting.rs` with tests first, then the minimal module structure:

```rust
use crate::item::inventory::Inventory;
use crate::item::types::{CraftError, CraftOutput, ItemDefs, RecipeDef, RecipeItem};

/// Execute a crafting recipe against the player's inventory.
///
/// Pure function: validates tools, inputs, and output room, then atomically
/// consumes inputs and produces outputs. Tools are required but not consumed.
pub fn craft(
    recipe: &RecipeDef,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
) -> Result<Vec<CraftOutput>, CraftError> {
    todo!()
}

/// Check whether a recipe can be crafted with the current inventory.
/// Used as a reference implementation — the frontend mirrors this in TypeScript.
pub fn check_recipe_availability(
    recipe: &RecipeDef,
    inventory: &Inventory,
) -> crate::item::types::RecipeAvailability {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;

    fn test_defs() -> ItemDefs {
        let mut defs = ItemDefs::new();
        for (id, name, stack_limit) in [
            ("cherry", "Cherry", 50),
            ("grain", "Grain", 50),
            ("cherry_pie", "Cherry Pie", 10),
            ("pot", "Pot", 1),
            ("wood", "Wood", 50),
            ("plank", "Plank", 50),
        ] {
            defs.insert(
                id.into(),
                ItemDef {
                    id: id.into(),
                    name: name.into(),
                    description: "".into(),
                    category: "".into(),
                    stack_limit,
                    icon: id.into(),
                },
            );
        }
        defs
    }

    fn cherry_pie_recipe() -> RecipeDef {
        RecipeDef {
            id: "cherry_pie".into(),
            name: "Cherry Pie".into(),
            description: "".into(),
            inputs: vec![
                RecipeItem { item: "cherry".into(), count: 5 },
                RecipeItem { item: "grain".into(), count: 2 },
            ],
            tools: vec![RecipeItem { item: "pot".into(), count: 1 }],
            outputs: vec![RecipeItem { item: "cherry_pie".into(), count: 1 }],
            duration_secs: 10.0,
            category: "food".into(),
        }
    }

    fn plank_recipe() -> RecipeDef {
        RecipeDef {
            id: "plank".into(),
            name: "Plank".into(),
            description: "".into(),
            inputs: vec![RecipeItem { item: "wood".into(), count: 3 }],
            tools: vec![],
            outputs: vec![RecipeItem { item: "plank".into(), count: 2 }],
            duration_secs: 4.0,
            category: "material".into(),
        }
    }

    #[test]
    fn craft_success_consumes_inputs_keeps_tools() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 10, &defs);
        inv.add("grain", 5, &defs);
        inv.add("pot", 1, &defs);

        let result = craft(&cherry_pie_recipe(), &mut inv, &defs).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].item_id, "cherry_pie");
        assert_eq!(result[0].count, 1);
        assert_eq!(inv.count_item("cherry"), 5); // 10 - 5
        assert_eq!(inv.count_item("grain"), 3);  // 5 - 2
        assert_eq!(inv.count_item("pot"), 1);    // tool not consumed
        assert_eq!(inv.count_item("cherry_pie"), 1);
    }

    #[test]
    fn craft_missing_input_returns_error() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 2, &defs); // need 5
        inv.add("grain", 5, &defs);
        inv.add("pot", 1, &defs);

        let err = craft(&cherry_pie_recipe(), &mut inv, &defs).unwrap_err();
        match err {
            CraftError::MissingInput { item, need, have } => {
                assert_eq!(item, "cherry");
                assert_eq!(need, 5);
                assert_eq!(have, 2);
            }
            _ => panic!("Expected MissingInput, got {:?}", err),
        }
        // Inventory unchanged on failure
        assert_eq!(inv.count_item("cherry"), 2);
        assert_eq!(inv.count_item("grain"), 5);
    }

    #[test]
    fn craft_missing_tool_returns_error() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 10, &defs);
        inv.add("grain", 5, &defs);
        // No pot!

        let err = craft(&cherry_pie_recipe(), &mut inv, &defs).unwrap_err();
        match err {
            CraftError::MissingTool { item } => assert_eq!(item, "pot"),
            _ => panic!("Expected MissingTool, got {:?}", err),
        }
    }

    #[test]
    fn craft_no_room_returns_error_nothing_consumed() {
        let defs = test_defs();
        let mut inv = Inventory::new(3); // tiny inventory
        inv.add("cherry", 5, &defs);  // slot 0
        inv.add("grain", 2, &defs);   // slot 1
        inv.add("pot", 1, &defs);     // slot 2 — all slots full

        let err = craft(&cherry_pie_recipe(), &mut inv, &defs).unwrap_err();
        match err {
            CraftError::NoRoom => {}
            _ => panic!("Expected NoRoom, got {:?}", err),
        }
        // Atomic: nothing consumed
        assert_eq!(inv.count_item("cherry"), 5);
        assert_eq!(inv.count_item("grain"), 2);
    }

    #[test]
    fn craft_multiple_outputs() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("wood", 3, &defs);

        let result = craft(&plank_recipe(), &mut inv, &defs).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].item_id, "plank");
        assert_eq!(result[0].count, 2);
        assert_eq!(inv.count_item("wood"), 0);
        assert_eq!(inv.count_item("plank"), 2);
    }

    #[test]
    fn craft_no_tool_recipe_succeeds_without_tools() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("wood", 3, &defs);

        let result = craft(&plank_recipe(), &mut inv, &defs);
        assert!(result.is_ok());
    }

    #[test]
    fn check_availability_craftable() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 10, &defs);
        inv.add("grain", 5, &defs);
        inv.add("pot", 1, &defs);

        let avail = check_recipe_availability(&cherry_pie_recipe(), &inv);
        assert!(avail.craftable);
        assert_eq!(avail.inputs.len(), 2);
        assert!(avail.inputs.iter().all(|i| i.have >= i.need));
        assert_eq!(avail.tools.len(), 1);
        assert!(avail.tools[0].have >= avail.tools[0].need);
    }

    #[test]
    fn check_availability_not_craftable() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 2, &defs); // need 5

        let avail = check_recipe_availability(&cherry_pie_recipe(), &inv);
        assert!(!avail.craftable);
        let cherry_status = avail.inputs.iter().find(|i| i.item == "cherry").unwrap();
        assert_eq!(cherry_status.have, 2);
        assert_eq!(cherry_status.need, 5);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch-lib --lib item::crafting::tests 2>&1 | tail -10`
Expected: FAIL — `todo!()` panics

- [ ] **Step 4: Implement craft()**

Replace the `todo!()` in `craft()` with:

```rust
pub fn craft(
    recipe: &RecipeDef,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
) -> Result<Vec<CraftOutput>, CraftError> {
    // 1. Validate tools
    for tool in &recipe.tools {
        let have = inventory.count_item(&tool.item);
        if have < tool.count {
            return Err(CraftError::MissingTool {
                item: tool.item.clone(),
            });
        }
    }

    // 2. Validate inputs
    for input in &recipe.inputs {
        let have = inventory.count_item(&input.item);
        if have < input.count {
            return Err(CraftError::MissingInput {
                item: input.item.clone(),
                need: input.count,
                have,
            });
        }
    }

    // 3. Validate output room (count-aware to prevent item loss)
    for output in &recipe.outputs {
        if !inventory.has_room_for_count(&output.item, output.count, item_defs) {
            return Err(CraftError::NoRoom);
        }
    }

    // 4. Consume inputs (tools NOT consumed)
    for input in &recipe.inputs {
        inventory.remove_item(&input.item, input.count);
    }

    // 5. Add outputs and build result
    let mut outputs = Vec::new();
    for output in &recipe.outputs {
        inventory.add(&output.item, output.count, item_defs);
        let name = item_defs
            .get(&output.item)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| output.item.clone());
        outputs.push(CraftOutput {
            item_id: output.item.clone(),
            name,
            count: output.count,
        });
    }

    Ok(outputs)
}
```

- [ ] **Step 5: Implement check_recipe_availability()**

Replace the `todo!()` in `check_recipe_availability()` with:

```rust
pub fn check_recipe_availability(
    recipe: &RecipeDef,
    inventory: &Inventory,
) -> crate::item::types::RecipeAvailability {
    use crate::item::types::{IngredientStatus, RecipeAvailability};

    let inputs: Vec<IngredientStatus> = recipe
        .inputs
        .iter()
        .map(|input| IngredientStatus {
            item: input.item.clone(),
            need: input.count,
            have: inventory.count_item(&input.item),
        })
        .collect();

    let tools: Vec<IngredientStatus> = recipe
        .tools
        .iter()
        .map(|tool| IngredientStatus {
            item: tool.item.clone(),
            need: tool.count,
            have: inventory.count_item(&tool.item),
        })
        .collect();

    let craftable = inputs.iter().all(|i| i.have >= i.need)
        && tools.iter().all(|t| t.have >= t.need);

    RecipeAvailability {
        craftable,
        inputs,
        tools,
    }
}
```

- [ ] **Step 6: Run all crafting tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch-lib --lib item::crafting::tests`
Expected: PASS (all 8 tests)

- [ ] **Step 7: Run full workspace test suite**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test --workspace`
Expected: PASS (all tests including existing ones)

- [ ] **Step 8: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/item/crafting.rs src-tauri/src/item/mod.rs
git commit -m "feat(crafting): implement craft() and check_recipe_availability()"
```

---

## Chunk 3: Backend Integration (GameState + IPC)

### Task 6: Add recipe_defs to GameState and craft_recipe method

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

**Important context for implementer:**
- `GameState` struct is at line 18 of `state.rs`. Add `recipe_defs` field.
- `GameState::new()` starts at line 100. It currently takes `item_defs` and `entity_defs` — add `recipe_defs` parameter.
- The `craft_recipe` method should call `craft()` from `crafting.rs`, then generate pickup feedback (same pattern as entity harvesting at lines 324-329).
- `next_feedback_id` is incremented by `self.next_feedback_id += 1` after assigning to each feedback entry.

- [ ] **Step 1: Write test for craft_recipe method**

Add this test to the existing `#[cfg(test)] mod tests` block at the bottom of `state.rs`. You'll need to add `RecipeDefs` to the imports.

First, add to the imports at the top of `state.rs`:
```rust
use crate::item::types::{
    // ...existing imports...,
    RecipeDefs,
};
```

Test (locate the test module and add inside it):

```rust
#[test]
fn craft_recipe_success_creates_feedback() {
    let item_defs =
        crate::item::loader::parse_item_defs(include_str!("../../../assets/items.json"))
            .unwrap();
    let entity_defs =
        crate::item::loader::parse_entity_defs(include_str!("../../../assets/entities.json"))
            .unwrap();
    let recipe_defs =
        crate::item::loader::parse_recipe_defs(include_str!("../../../assets/recipes.json"))
            .unwrap();

    let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, recipe_defs);
    // Stock inventory with cherry pie ingredients + pot
    state.inventory.add("cherry", 10, &state.item_defs);
    state.inventory.add("grain", 5, &state.item_defs);
    state.inventory.add("pot", 1, &state.item_defs);

    let result = state.craft_recipe("cherry_pie");
    assert!(result.is_ok());

    // Check feedback was generated
    assert_eq!(state.pickup_feedback.len(), 1);
    assert!(state.pickup_feedback[0].text.contains("Cherry Pie"));
    assert!(state.pickup_feedback[0].success);

    // Check inventory
    assert_eq!(state.inventory.count_item("cherry_pie"), 1);
    assert_eq!(state.inventory.count_item("cherry"), 5);
    assert_eq!(state.inventory.count_item("pot"), 1); // tool kept
}

#[test]
fn craft_recipe_unknown_returns_error() {
    let item_defs =
        crate::item::loader::parse_item_defs(include_str!("../../../assets/items.json"))
            .unwrap();
    let entity_defs =
        crate::item::loader::parse_entity_defs(include_str!("../../../assets/entities.json"))
            .unwrap();
    let recipe_defs =
        crate::item::loader::parse_recipe_defs(include_str!("../../../assets/recipes.json"))
            .unwrap();

    let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, recipe_defs);

    let result = state.craft_recipe("nonexistent");
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch-lib --lib engine::state::tests::craft_recipe 2>&1 | tail -10`
Expected: FAIL — `recipe_defs` field doesn't exist, or `craft_recipe` method doesn't exist

- [ ] **Step 3: Add recipe_defs field and craft_recipe method**

In the `GameState` struct definition (line 18), add:
```rust
pub recipe_defs: RecipeDefs,
```

In the import block at the top of `state.rs`, add `RecipeDefs` to the `use crate::item::types` import.

In `GameState::new()` (line 100), add `recipe_defs: RecipeDefs` to the parameter list and to the struct initialization:
```rust
pub fn new(
    viewport_width: f64,
    viewport_height: f64,
    item_defs: ItemDefs,
    entity_defs: EntityDefs,
    recipe_defs: RecipeDefs,
) -> Self {
    Self {
        // ... existing fields ...
        recipe_defs,
        // ... rest of fields ...
    }
}
```

Add the `craft_recipe` method to `impl GameState` (after `load_street`):

```rust
/// Execute a crafting recipe. On success, generates pickup feedback.
pub fn craft_recipe(
    &mut self,
    recipe_id: &str,
) -> Result<Vec<crate::item::types::CraftOutput>, crate::item::types::CraftError> {
    let recipe = self
        .recipe_defs
        .get(recipe_id)
        .ok_or(crate::item::types::CraftError::UnknownRecipe)?
        .clone();
    let result = crate::item::crafting::craft(&recipe, &mut self.inventory, &self.item_defs)?;
    for output in &result {
        self.pickup_feedback.push(PickupFeedback {
            id: self.next_feedback_id,
            text: format!("+{} x{}", output.name, output.count),
            success: true,
            x: self.player.x,
            y: self.player.y,
            age_secs: 0.0,
        });
        self.next_feedback_id += 1;
    }
    Ok(result)
}
```

**Note:** The `.clone()` on the recipe is needed because `craft()` takes `&RecipeDef` and `&mut self.inventory`, and the recipe comes from `self.recipe_defs` — both are behind `&mut self`. Cloning the recipe (small struct) avoids the borrow conflict.

- [ ] **Step 4: Fix all existing GameState::new() call sites**

The signature changed to include `recipe_defs`. Every call site that creates a `GameState::new()` needs updating. Search the codebase:

In `src-tauri/src/lib.rs` line 446-451 (the `GameStateWrapper` init):
```rust
GameStateWrapper(Mutex::new(GameState::new(
    1280.0,
    720.0,
    item_defs,
    entity_defs,
    recipe_defs,  // <-- add this
)))
```

You also need to load recipe_defs before this call:
```rust
let recipe_defs =
    item::loader::parse_recipe_defs(include_str!("../../assets/recipes.json"))
        .expect("Failed to parse recipes.json");
```

In test helper functions within `state.rs`: any test that calls `GameState::new()` with 4 arguments needs a 5th. Search for `GameState::new(` in `state.rs` tests. Use `RecipeDefs::new()` (empty HashMap) for tests that don't need recipes:

```rust
// For tests that don't need recipes:
use std::collections::HashMap;
GameState::new(1280.0, 720.0, item_defs, entity_defs, HashMap::new())
```

- [ ] **Step 5: Run all tests to verify**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test --workspace`
Expected: PASS (all tests)

- [ ] **Step 6: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/engine/state.rs src-tauri/src/lib.rs
git commit -m "feat(crafting): add recipe_defs to GameState and craft_recipe method"
```

---

### Task 7: Add IPC commands and ground item loading

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `assets/streets/demo_meadow_entities.json`

**Important context for implementer:**
- Existing IPC commands follow the pattern: `#[tauri::command] fn name(args, app: AppHandle) -> Result<T, String>`.
- `GameStateWrapper` wraps `Mutex<GameState>`, accessed via `app.state::<GameStateWrapper>()`.
- The `load_street` command at line 61 calls `load_entity_placement` to get entity JSON, then calls `state.load_street(street_data, entities)`.
- The entity placement file `demo_meadow_entities.json` is currently a plain JSON array. To add `groundItems`, it needs to become an object with `entities` and `groundItems` fields.

- [ ] **Step 1: Change demo_meadow_entities.json to object format with groundItems**

Replace `assets/streets/demo_meadow_entities.json` with:

```json
{
  "entities": [
    { "id": "tree_1", "type": "fruit_tree", "x": -800, "y": -2 },
    { "id": "tree_2", "type": "fruit_tree", "x": 1200, "y": -2 },
    { "id": "chicken_1", "type": "chicken", "x": 200, "y": -2 },
    { "id": "pig_1", "type": "pig", "x": -300, "y": -2 },
    { "id": "butterfly_1", "type": "butterfly", "x": 600, "y": -80 },
    { "id": "wood_tree_1", "type": "wood_tree", "x": -1400, "y": -2 }
  ],
  "groundItems": [
    { "id": "pot_1", "itemId": "pot", "count": 1, "x": 0, "y": -2 }
  ]
}
```

- [ ] **Step 2: Update parse_entity_placements to handle both formats**

Modify `parse_entity_placements` in `loader.rs` to support both the old array format (for demo_heights) and the new object format (for demo_meadow). Add `WorldItem` to the imports.

Update the import line:
```rust
use crate::item::types::{EntityDefs, ItemDefs, RecipeDefs, WorldEntity, WorldItem};
```

Replace `parse_entity_placements`:
```rust
/// Placement data parsed from a street's entity/item JSON file.
pub struct PlacementData {
    pub entities: Vec<WorldEntity>,
    pub ground_items: Vec<WorldItem>,
}

/// Parse entity and ground item placements from JSON string.
/// Supports both array format (legacy, entities only) and object format
/// (with optional groundItems field).
pub fn parse_entity_placements(json: &str) -> Result<PlacementData, String> {
    // Try object format first
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PlacementJson {
        entities: Vec<WorldEntity>,
        #[serde(default)]
        ground_items: Vec<WorldItem>,
    }

    if let Ok(data) = serde_json::from_str::<PlacementJson>(json) {
        return Ok(PlacementData {
            entities: data.entities,
            ground_items: data.ground_items,
        });
    }

    // Fall back to legacy array format (entities only)
    let entities: Vec<WorldEntity> =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse entity placements: {e}"))?;
    Ok(PlacementData {
        entities,
        ground_items: vec![],
    })
}
```

You'll also need to add `use serde::Deserialize;` to the top of `loader.rs` if it's not already there (it's used via `crate::item::types` currently, but `PlacementJson` needs it directly).

- [ ] **Step 3: Update load_street in lib.rs to use PlacementData**

In `lib.rs`, the `load_street` command (line 61-90) currently does:
```rust
let entities = item::loader::parse_entity_placements(&entity_json)?;
state.load_street(street_data.clone(), entities);
```

Update to:
```rust
let placement = item::loader::parse_entity_placements(&entity_json)?;
state.load_street(street_data.clone(), placement.entities, placement.ground_items);
```

- [ ] **Step 4: Update GameState::load_street to accept ground items**

In `state.rs`, update the `load_street` method signature (line 132):

```rust
pub fn load_street(
    &mut self,
    street: StreetData,
    entities: Vec<WorldEntity>,
    ground_items: Vec<WorldItem>,
) {
```

And change `self.world_items.clear();` to:
```rust
self.world_items = ground_items;
```

Also update `next_item_id` so runtime-generated IDs don't collide with loaded ones:
```rust
// Set next_item_id past any loaded ground item IDs to avoid collisions
self.next_item_id = self
    .world_items
    .len()
    .max(self.next_item_id as usize) as u64;
```

- [ ] **Step 5: Add IPC commands for crafting**

Add these two commands to `lib.rs`, after the `drop_item` command (line 262):

```rust
#[tauri::command]
fn get_recipes(app: AppHandle) -> Result<Vec<item::types::RecipeDef>, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    Ok(state.recipe_defs.values().cloned().collect())
}

#[tauri::command]
fn craft_recipe(recipe_id: String, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.craft_recipe(&recipe_id).map_err(|e| e.to_string())?;
    Ok(())
}
```

Register them in the invoke_handler (line 489-501). Add `get_recipes` and `craft_recipe` to the `tauri::generate_handler!` list:

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
    drop_item,
    street_transition_ready,
    get_network_status,
    get_recipes,
    craft_recipe,
])
```

- [ ] **Step 6: Fix all callers of load_street in state.rs tests**

Any test in `state.rs` that calls `self.load_street(street_data, entities)` needs updating to `self.load_street(street_data, entities, vec![])`.

Search for `load_street(` in `state.rs` tests and add the third argument.

- [ ] **Step 7: Fix loader tests for new PlacementData return type**

The loader tests that call `parse_entity_placements` now get `PlacementData` instead of `Vec<WorldEntity>`. Update them:

```rust
// Change:
let entities = parse_entity_placements(json).unwrap();
assert_eq!(entities.len(), 2);
// To:
let data = parse_entity_placements(json).unwrap();
assert_eq!(data.entities.len(), 2);
```

Also fix the bundled test:
```rust
// Change assertions like:
let entities = parse_entity_placements(json).unwrap();
assert!(entities.len() >= 3);
// To:
let data = parse_entity_placements(json).unwrap();
assert!(data.entities.len() >= 3);
```

Add a test for ground items:
```rust
#[test]
fn parse_bundled_meadow_has_ground_items() {
    let json = include_str!("../../../assets/streets/demo_meadow_entities.json");
    let data = parse_entity_placements(json).unwrap();
    assert!(data.entities.len() >= 3);
    assert_eq!(data.ground_items.len(), 1);
    assert_eq!(data.ground_items[0].item_id, "pot");
}
```

- [ ] **Step 8: Run all tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test --workspace`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/lib.rs src-tauri/src/item/loader.rs src-tauri/src/engine/state.rs assets/streets/demo_meadow_entities.json
git commit -m "feat(crafting): add IPC commands and ground item loading"
```

---

## Chunk 4: Frontend

### Task 8: Add frontend types and IPC wrappers

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add RecipeDef and RecipeItem to types.ts**

Add at the end of `src/lib/types.ts`:

```typescript
export interface RecipeDef {
  id: string;
  name: string;
  description: string;
  inputs: RecipeItem[];
  tools: RecipeItem[];
  outputs: RecipeItem[];
  category: string;
}

export interface RecipeItem {
  item: string;
  count: number;
}
```

- [ ] **Step 2: Add getRecipes and craftRecipe to ipc.ts**

Add the import for the new types:
```typescript
import type { StreetData, InputState, RenderFrame, NetworkStatus, PlayerIdentity, ChatEvent, RecipeDef } from './types';
```

Add at the end of `ipc.ts`:
```typescript
export async function getRecipes(): Promise<RecipeDef[]> {
  return invoke<RecipeDef[]>('get_recipes');
}

export async function craftRecipe(recipeId: string): Promise<void> {
  return invoke('craft_recipe', { recipeId });
}
```

- [ ] **Step 3: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src/lib/types.ts src/lib/ipc.ts
git commit -m "feat(crafting): add frontend types and IPC wrappers"
```

---

### Task 9: Add tabbed inventory with recipes tab

**Files:**
- Modify: `src/lib/components/InventoryPanel.svelte`
- Modify: `src/App.svelte`

This is the largest frontend change. The existing InventoryPanel becomes a tabbed interface with Items and Recipes tabs.

**Important context for implementer:**
- Use Svelte 5 runes: `$state()`, `$derived()`, `$props()`, `$effect()`, `onclick={handler}`
- InventoryPanel is a `<dialog>` element with `showModal()`
- The existing grid of inventory slots becomes the "Items" tab content
- The new "Recipes" tab shows all recipes sorted by craftability
- Accessibility: `role="tablist"`, `role="tab"`, `role="tabpanel"`, arrow key navigation between tabs
- The availability check runs in TypeScript using `InventoryFrame` slots data — NO IPC call needed per-frame

- [ ] **Step 1: Update App.svelte to load and pass recipes**

In `src/App.svelte`:

Add imports:
```typescript
import { stopGame, loadStreet, getIdentity, streetTransitionReady, getRecipes } from './lib/ipc';
import type { StreetData, RenderFrame, RecipeDef } from './lib/types';
```

Add state:
```typescript
let recipes = $state<RecipeDef[]>([]);
```

Load recipes in `onMount`, after the identity check:
```typescript
onMount(async () => {
    try {
      const identity = await getIdentity();
      identityReady = identity.setupComplete;
    } catch {
      identityReady = false;
    } finally {
      checkingIdentity = false;
    }

    // Load recipes once at startup
    try {
      recipes = await getRecipes();
    } catch (e) {
      console.error('Failed to load recipes:', e);
    }
  });
```

Pass recipes to InventoryPanel:
```svelte
<InventoryPanel
  inventory={latestFrame?.inventory ?? null}
  {recipes}
  visible={inventoryOpen}
  onClose={() => { inventoryOpen = false; }}
/>
```

- [ ] **Step 2: Rewrite InventoryPanel.svelte with tabs**

Replace the entire content of `src/lib/components/InventoryPanel.svelte` with the tabbed version. The component needs:

Props: add `recipes: RecipeDef[]`
State: add `activeTab: 'items' | 'recipes'`, `selectedRecipe: string | null`
Derived: `sortedRecipes` — sorted with craftable recipes first
Helper: `countItem(itemId)` — counts items in inventory slots (mirrors Rust's `count_item`)
Helper: `isRecipeCraftable(recipe)` — checks all inputs and tools against inventory

```svelte
<script lang="ts">
  import type { InventoryFrame, RecipeDef } from '../types';
  import { dropItem, craftRecipe } from '../ipc';

  let { inventory, recipes = [], visible = false, onClose }: {
    inventory: InventoryFrame | null;
    recipes?: RecipeDef[];
    visible?: boolean;
    onClose?: () => void;
  } = $props();

  let selectedSlot = $state<number | null>(null);
  let activeTab = $state<'items' | 'recipes'>('items');
  let selectedRecipeId = $state<string | null>(null);
  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;
  let craftError = $state<string | null>(null);

  let selectedItem = $derived.by(() => {
    if (selectedSlot === null || !inventory) return null;
    return inventory.slots[selectedSlot] ?? null;
  });

  function countItem(itemId: string): number {
    if (!inventory) return 0;
    return inventory.slots.reduce((sum, slot) => {
      if (slot && slot.itemId === itemId) return sum + slot.count;
      return sum;
    }, 0);
  }

  function isRecipeCraftable(recipe: RecipeDef): boolean {
    for (const input of recipe.inputs) {
      if (countItem(input.item) < input.count) return false;
    }
    for (const tool of recipe.tools) {
      if (countItem(tool.item) < tool.count) return false;
    }
    return true;
  }

  let sortedRecipes = $derived.by(() => {
    return [...recipes].sort((a, b) => {
      const aCraftable = isRecipeCraftable(a);
      const bCraftable = isRecipeCraftable(b);
      if (aCraftable && !bCraftable) return -1;
      if (!aCraftable && bCraftable) return 1;
      return a.name.localeCompare(b.name);
    });
  });

  let selectedRecipe = $derived.by(() => {
    if (!selectedRecipeId) return null;
    return recipes.find(r => r.id === selectedRecipeId) ?? null;
  });

  $effect(() => {
    if (visible && dialogEl) {
      previousFocus = document.activeElement as HTMLElement | null;
      if (!dialogEl.open) {
        dialogEl.showModal();
      }
      dialogEl.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
      return () => {
        if (dialogEl?.open) dialogEl.close();
      };
    } else if (!visible && previousFocus) {
      previousFocus.focus();
      previousFocus = null;
    }
  });

  function handleCancel(e: Event) {
    e.preventDefault();
    onClose?.();
  }

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

  async function handleCraft() {
    if (!selectedRecipeId) return;
    craftError = null;
    try {
      await craftRecipe(selectedRecipeId);
    } catch (e) {
      craftError = String(e);
    }
  }

  function switchTab(tab: 'items' | 'recipes') {
    activeTab = tab;
    selectedSlot = null;
    selectedRecipeId = null;
    // Focus first focusable element in new tab panel after Svelte renders
    requestAnimationFrame(() => {
      const panel = dialogEl?.querySelector<HTMLElement>(`[role="tabpanel"]`);
      const firstFocusable = panel?.querySelector<HTMLElement>('button, [tabindex="0"]');
      firstFocusable?.focus();
    });
  }

  function handleTabKey(e: KeyboardEvent) {
    if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
      e.preventDefault();
      switchTab(activeTab === 'items' ? 'recipes' : 'items');
    }
  }

  function handleItemsKeyDown(e: KeyboardEvent) {
    if (e.ctrlKey || e.altKey || e.metaKey) return;

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
      if (selectedSlot !== null) handleSlotClick(selectedSlot);
    }

    if (selectedSlot !== null) {
      const buttons = (e.currentTarget as HTMLElement)
        .querySelectorAll<HTMLElement>('button.slot');
      buttons[selectedSlot]?.focus();
    }
  }

  function handleSpaceKey(e: KeyboardEvent) {
    if (e.key === ' ') {
      e.preventDefault();
      handleCraft();
    }
  }
</script>

{#if visible}
  <dialog
    class="inventory-panel"
    aria-label="Inventory"
    bind:this={dialogEl}
    oncancel={handleCancel}
  >
    <div class="tab-bar" role="tablist" aria-label="Inventory sections" onkeydown={handleTabKey}>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === 'items'}
        aria-controls="panel-items"
        id="tab-items"
        tabindex={activeTab === 'items' ? 0 : -1}
        class="tab"
        class:active={activeTab === 'items'}
        onclick={() => switchTab('items')}
      >Items</button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === 'recipes'}
        aria-controls="panel-recipes"
        id="tab-recipes"
        tabindex={activeTab === 'recipes' ? 0 : -1}
        class="tab"
        class:active={activeTab === 'recipes'}
        onclick={() => switchTab('recipes')}
      >Recipes</button>
    </div>

    {#if activeTab === 'items'}
      <div
        id="panel-items"
        role="tabpanel"
        aria-labelledby="tab-items"
        onkeydown={handleItemsKeyDown}
      >
        <div class="slots" role="grid" aria-label="Inventory slots">
          {#each { length: Math.ceil((inventory?.capacity ?? 16) / 4) } as _, row}
            <div role="row" class="slot-row">
              {#each inventory?.slots?.slice(row * 4, row * 4 + 4) ?? [] as slot, col}
                {@const i = row * 4 + col}
                <div role="gridcell">
                  <button
                    type="button"
                    class="slot"
                    class:selected={selectedSlot === i}
                    class:filled={slot !== null}
                    tabindex={selectedSlot === i || (selectedSlot === null && i === 0) ? 0 : -1}
                    aria-label={slot ? `${slot.name} x${slot.count}` : `Empty slot ${i + 1}`}
                    onclick={() => handleSlotClick(i)}
                  >
                    {#if slot}
                      <span class="slot-icon">{slot.icon.charAt(0).toUpperCase()}</span>
                      <span class="slot-count">{slot.count}</span>
                    {/if}
                  </button>
                </div>
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
    {:else}
      <div
        id="panel-recipes"
        role="tabpanel"
        aria-labelledby="tab-recipes"
      >
        <div class="recipe-list" role="listbox" aria-label="Recipes">
          {#each sortedRecipes as recipe (recipe.id)}
            {@const craftable = isRecipeCraftable(recipe)}
            <button
              type="button"
              role="option"
              aria-selected={selectedRecipeId === recipe.id}
              class="recipe-row"
              class:craftable
              class:selected={selectedRecipeId === recipe.id}
              aria-label="{recipe.name}{craftable ? '' : ' (missing ingredients)'}"
              onclick={() => { selectedRecipeId = selectedRecipeId === recipe.id ? null : recipe.id; craftError = null; }}
            >
              <span class="recipe-name">{recipe.name}</span>
              {#if !craftable}
                <span class="recipe-badge">-</span>
              {/if}
            </button>
          {/each}
        </div>

        {#if selectedRecipe}
          <div class="recipe-details">
            <div class="recipe-detail-name">{selectedRecipe.name}</div>
            <div class="recipe-desc">{selectedRecipe.description}</div>

            {#if selectedRecipe.inputs.length > 0}
              <div class="ingredient-section">
                <div class="ingredient-label">Ingredients:</div>
                {#each selectedRecipe.inputs as input}
                  {@const have = countItem(input.item)}
                  <div
                    class="ingredient"
                    class:sufficient={have >= input.count}
                    aria-label="{input.item}: have {have}, need {input.count}"
                  >
                    {input.item} {have}/{input.count}
                  </div>
                {/each}
              </div>
            {/if}

            {#if selectedRecipe.tools.length > 0}
              <div class="ingredient-section">
                <div class="ingredient-label">Tools:</div>
                {#each selectedRecipe.tools as tool}
                  {@const have = countItem(tool.item)}
                  <div
                    class="ingredient"
                    class:sufficient={have >= tool.count}
                    aria-label="{tool.item}: have {have}, need {tool.count}"
                  >
                    {tool.item} {have >= tool.count ? '✓' : '✗'}
                  </div>
                {/each}
              </div>
            {/if}

            <div class="ingredient-section">
              <div class="ingredient-label">Produces:</div>
              {#each selectedRecipe.outputs as output}
                <div class="ingredient">{output.item} x{output.count}</div>
              {/each}
            </div>

            <button
              type="button"
              class="craft-btn"
              disabled={!isRecipeCraftable(selectedRecipe)}
              onclick={handleCraft}
              onkeydown={handleSpaceKey}
            >
              Craft
            </button>
            {#if craftError}
              <div class="craft-error" role="alert">{craftError}</div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </dialog>
{/if}

<style>
  .inventory-panel {
    position: fixed;
    top: 0;
    right: 0;
    left: auto;
    width: 220px;
    height: 100%;
    max-height: 100%;
    max-width: 220px;
    margin: 0;
    background: rgba(20, 20, 40, 0.92);
    border: none;
    border-left: 1px solid #444;
    padding: 12px;
    z-index: 100;
    color: #e0e0e0;
    display: flex;
    flex-direction: column;
  }

  .inventory-panel::backdrop {
    background: transparent;
  }

  .tab-bar {
    display: flex;
    gap: 2px;
    margin-bottom: 12px;
  }

  .tab {
    flex: 1;
    padding: 6px 0;
    background: rgba(40, 40, 70, 0.6);
    border: 1px solid #3a3a5a;
    border-radius: 4px 4px 0 0;
    color: #888;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 1px;
    cursor: pointer;
  }

  .tab.active {
    background: rgba(50, 50, 90, 0.9);
    color: #e0e0e0;
    border-bottom-color: transparent;
  }

  .tab:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: -2px;
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

  .slot:hover { border-color: #6a6a9a; }
  .slot.selected { border-color: #5865f2; box-shadow: 0 0 6px rgba(88, 101, 242, 0.4); }
  .slot.filled { background: rgba(50, 50, 80, 0.9); }

  .slot-icon { font-size: 1rem; line-height: 1; }
  .slot-count { font-size: 0.6rem; color: #aaa; position: absolute; bottom: 1px; right: 3px; }

  .item-details, .recipe-details {
    margin-top: 12px;
    padding: 8px;
    background: rgba(40, 40, 70, 0.6);
    border-radius: 4px;
  }

  .item-name, .recipe-detail-name { font-weight: bold; font-size: 0.85rem; margin-bottom: 2px; }
  .item-desc, .recipe-desc { font-size: 0.7rem; color: #999; margin-bottom: 4px; font-style: italic; }
  .item-count { font-size: 0.75rem; color: #aaa; margin-bottom: 8px; }

  .drop-btn {
    background: rgba(80, 60, 40, 0.8);
    color: #e8c170;
    border: 1px solid #6a5a3a;
    border-radius: 3px;
    padding: 4px 12px;
    cursor: pointer;
    font-size: 0.75rem;
  }

  .drop-btn:hover { background: rgba(100, 80, 50, 0.9); }

  .recipe-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 300px;
    overflow-y: auto;
  }

  .recipe-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 8px;
    background: rgba(40, 40, 70, 0.6);
    border: 1px solid transparent;
    border-radius: 3px;
    cursor: pointer;
    color: #888;
    font-size: 0.75rem;
    text-align: left;
    width: 100%;
  }

  .recipe-row.craftable { color: #e0e0e0; }
  .recipe-row.selected { border-color: #5865f2; background: rgba(50, 50, 90, 0.8); }
  .recipe-row:hover { background: rgba(50, 50, 80, 0.7); }
  .recipe-name { flex: 1; }
  .recipe-badge { color: #666; font-size: 0.7rem; }

  .ingredient-section { margin: 6px 0; }
  .ingredient-label { font-size: 0.65rem; color: #888; text-transform: uppercase; margin-bottom: 2px; }
  .ingredient { font-size: 0.75rem; color: #c66; padding: 1px 0; }
  .ingredient.sufficient { color: #6c6; }

  .craft-btn {
    margin-top: 8px;
    width: 100%;
    padding: 6px;
    background: rgba(40, 80, 60, 0.8);
    color: #8cd48c;
    border: 1px solid #4a7a4a;
    border-radius: 3px;
    cursor: pointer;
    font-size: 0.8rem;
  }

  .craft-btn:hover:not(:disabled) { background: rgba(50, 100, 70, 0.9); }
  .craft-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .craft-error {
    margin-top: 4px;
    font-size: 0.7rem;
    color: #e88;
  }
</style>
```

- [ ] **Step 3: Run frontend tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npx vitest run`
Expected: PASS (existing sprite tests should still pass — they don't touch InventoryPanel)

- [ ] **Step 4: Run cargo clippy**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo clippy --workspace`
Expected: No warnings

- [ ] **Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src/lib/components/InventoryPanel.svelte src/App.svelte
git commit -m "feat(crafting): add tabbed inventory with recipes tab"
```

---

### Task 10: Frontend tests for InventoryPanel

**Files:**
- Create: `src/lib/components/InventoryPanel.test.ts`

**Important context for implementer:**
- Use vitest + @testing-library/svelte (match the project's existing test patterns in `sprites.test.ts`)
- InventoryPanel is a `<dialog>` element — jsdom doesn't support `showModal()` natively, so you'll need to mock it
- The component needs `inventory` and `recipes` props
- Tests should verify rendering and ARIA structure, not IPC calls (mock `craftRecipe` and `dropItem`)

- [ ] **Step 1: Write the test file**

Create `src/lib/components/InventoryPanel.test.ts`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import InventoryPanel from './InventoryPanel.svelte';
import type { InventoryFrame, RecipeDef } from '../types';

// Mock ipc module
vi.mock('../ipc', () => ({
  dropItem: vi.fn().mockResolvedValue(undefined),
  craftRecipe: vi.fn().mockResolvedValue(undefined),
}));

// Mock dialog showModal/close (jsdom doesn't support)
HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) {
  this.setAttribute('open', '');
});
HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) {
  this.removeAttribute('open');
});

function makeInventory(items: { itemId: string; name: string; count: number }[]): InventoryFrame {
  const slots: (null | { itemId: string; name: string; description: string; icon: string; count: number; stackLimit: number })[] =
    items.map(i => ({
      itemId: i.itemId,
      name: i.name,
      description: '',
      icon: i.itemId,
      count: i.count,
      stackLimit: 50,
    }));
  while (slots.length < 16) slots.push(null);
  return { slots, capacity: 16 };
}

function makeRecipes(): RecipeDef[] {
  return [
    {
      id: 'bread',
      name: 'Bread',
      description: 'Simple bread.',
      inputs: [{ item: 'grain', count: 4 }],
      tools: [{ item: 'pot', count: 1 }],
      outputs: [{ item: 'bread', count: 1 }],
      category: 'food',
    },
    {
      id: 'plank',
      name: 'Plank',
      description: 'Wood plank.',
      inputs: [{ item: 'wood', count: 3 }],
      tools: [],
      outputs: [{ item: 'plank', count: 2 }],
      category: 'material',
    },
  ];
}

describe('InventoryPanel', () => {
  it('renders tab bar with Items and Recipes tabs', () => {
    const inv = makeInventory([]);
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const tablist = screen.getByRole('tablist');
    expect(tablist).toBeDefined();

    const tabs = screen.getAllByRole('tab');
    expect(tabs).toHaveLength(2);
    expect(tabs[0].textContent).toBe('Items');
    expect(tabs[1].textContent).toBe('Recipes');
  });

  it('shows recipes tab when clicked', async () => {
    const inv = makeInventory([]);
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const recipesTab = screen.getByRole('tab', { name: /recipes/i });
    await fireEvent.click(recipesTab);

    const panel = screen.getByRole('tabpanel');
    expect(panel.id).toBe('panel-recipes');
  });

  it('sorts craftable recipes before uncraftable', async () => {
    // Has wood for plank but not grain+pot for bread
    const inv = makeInventory([{ itemId: 'wood', name: 'Wood', count: 5 }]);
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const recipesTab = screen.getByRole('tab', { name: /recipes/i });
    await fireEvent.click(recipesTab);

    const options = screen.getAllByRole('option');
    // Plank (craftable) should come before Bread (not craftable)
    expect(options[0].textContent).toContain('Plank');
    expect(options[1].textContent).toContain('Bread');
  });

  it('disables craft button when missing ingredients', async () => {
    const inv = makeInventory([]); // empty inventory
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const recipesTab = screen.getByRole('tab', { name: /recipes/i });
    await fireEvent.click(recipesTab);

    // Select first recipe
    const options = screen.getAllByRole('option');
    await fireEvent.click(options[0]);

    const craftBtn = screen.getByRole('button', { name: /craft/i });
    expect(craftBtn).toBeDisabled();
  });

  it('has correct ARIA tab structure', () => {
    const inv = makeInventory([]);
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const tabs = screen.getAllByRole('tab');
    // Active tab has aria-selected=true
    expect(tabs[0].getAttribute('aria-selected')).toBe('true');
    expect(tabs[1].getAttribute('aria-selected')).toBe('false');

    // Active tab controls the visible panel
    expect(tabs[0].getAttribute('aria-controls')).toBe('panel-items');
    expect(tabs[1].getAttribute('aria-controls')).toBe('panel-recipes');

    // Tab panel exists and is labeled
    const panel = screen.getByRole('tabpanel');
    expect(panel.getAttribute('aria-labelledby')).toBe('tab-items');
  });
});
```

- [ ] **Step 2: Run frontend tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npx vitest run`
Expected: PASS (all tests including new InventoryPanel tests)

- [ ] **Step 3: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src/lib/components/InventoryPanel.test.ts
git commit -m "test(crafting): add InventoryPanel frontend tests"
```

---

### Task 11: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Run full Rust test suite**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test --workspace`
Expected: PASS (all tests)

- [ ] **Step 2: Run frontend tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npx vitest run`
Expected: PASS

- [ ] **Step 3: Run clippy**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo clippy --workspace`
Expected: No warnings

- [ ] **Step 4: Run format check**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo fmt --all -- --check`
Expected: No formatting issues (or run `cargo fmt --all` to fix)
