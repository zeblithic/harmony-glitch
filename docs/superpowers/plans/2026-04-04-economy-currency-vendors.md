# Economy: Currency & Vendors — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add currants currency, vendor NPC entities with buy/sell mechanics, a shop panel UI, and a currant balance HUD.

**Architecture:** Rust owns all economy logic (currency state, buy/sell validation, transactions). Svelte renders the shop panel and HUD. New `stores.json` and `base_cost` on items drive vendor inventories and pricing. Vendors are street entities detected via `store.is_some()`, following the same pattern as jukeboxes (`playlist.is_some()`).

**Tech Stack:** Rust (Tauri v2), Svelte 5 (runes), TypeScript, serde_json

**Spec:** `docs/superpowers/specs/2026-04-04-economy-currency-vendors-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `src-tauri/src/item/types.rs` | Modify | Add `base_cost` to `ItemDef`, `store` to `EntityDef`, `StoreDef`/`StoreCatalog` types, `CurrencyFeedback` |
| `src-tauri/src/item/loader.rs` | Modify | Add `parse_store_catalog()` function |
| `src-tauri/src/item/interaction.rs` | Modify | Add `Vendor` variant to `InteractionType`, vendor detection in `build_prompt`/`execute_interaction` |
| `src-tauri/src/item/vendor.rs` | Create | Buy/sell transaction logic, `validate_vendor_inventory` |
| `src-tauri/src/item/mod.rs` | Modify | Add `pub mod vendor;` |
| `src-tauri/src/engine/state.rs` | Modify | Add `currants`/`store_catalog` to `GameState`/`SaveState`/`RenderFrame`, vendor interaction handling in `tick()` |
| `src-tauri/src/lib.rs` | Modify | Rename `validate_jukebox_proximity` → `validate_entity_proximity`, add vendor IPC commands, load stores.json |
| `assets/items.json` | Modify | Add `baseCost` to all items |
| `assets/entities.json` | Modify | Add `vendor_grocery` and `vendor_hardware` entity defs |
| `assets/stores.json` | Create | Store catalog with grocery/hardware vendors |
| `assets/streets/demo_meadow_entities.json` | Modify | Place `vendor_grocery` |
| `assets/streets/demo_heights_entities.json` | Modify | Place `vendor_hardware` (convert to object format) |
| `src/lib/types.ts` | Modify | Add `StoreState`/`StoreItem`/`SellableItem` interfaces, `currants` to `RenderFrame`/`SavedState` |
| `src/lib/ipc.ts` | Modify | Add vendor IPC functions |
| `src/lib/components/ShopPanel.svelte` | Create | Tabbed Buy/Sell shop panel |
| `src/lib/components/ShopPanel.test.ts` | Create | ShopPanel tests |
| `src/lib/components/CurrantHud.svelte` | Create | Always-visible currant balance display |
| `src/App.svelte` | Modify | Wire up vendor interaction, ShopPanel, CurrantHud |

---

### Task 1: Item Prices & Store Catalog Types (Rust)

**Files:**
- Modify: `src-tauri/src/item/types.rs`
- Modify: `src-tauri/src/item/loader.rs`
- Create: `assets/stores.json`
- Modify: `assets/items.json`

- [ ] **Step 1: Write tests for base_cost on ItemDef and store catalog parsing**

In `src-tauri/src/item/types.rs`, add to the existing `mod tests` block:

```rust
#[test]
fn item_def_with_base_cost() {
    let json = r#"{
        "cherry": {
            "name": "Cherry",
            "description": "A cherry.",
            "category": "food",
            "stackLimit": 50,
            "icon": "cherry",
            "baseCost": 3
        }
    }"#;
    let defs: ItemDefs = serde_json::from_str(json).unwrap();
    assert_eq!(defs["cherry"].base_cost, Some(3));
}

#[test]
fn item_def_without_base_cost() {
    let json = r#"{
        "quest_item": {
            "name": "Quest Item",
            "description": "Cannot be sold.",
            "category": "quest",
            "stackLimit": 1,
            "icon": "quest"
        }
    }"#;
    let defs: ItemDefs = serde_json::from_str(json).unwrap();
    assert_eq!(defs["quest_item"].base_cost, None);
}
```

In `src-tauri/src/item/loader.rs`, add to the existing `mod tests` block:

```rust
#[test]
fn parse_store_catalog_from_json() {
    let json = r#"{
        "grocery": {
            "name": "Grocery Vendor",
            "buyMultiplier": 0.66,
            "inventory": ["cherry", "grain"]
        }
    }"#;
    let catalog = parse_store_catalog(json).unwrap();
    assert_eq!(catalog.stores.len(), 1);
    let grocery = &catalog.stores["grocery"];
    assert_eq!(grocery.name, "Grocery Vendor");
    assert!((grocery.buy_multiplier - 0.66).abs() < 0.001);
    assert_eq!(grocery.inventory, vec!["cherry", "grain"]);
}

#[test]
fn parse_store_catalog_empty() {
    let catalog = parse_store_catalog("{}").unwrap();
    assert!(catalog.stores.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test`
Expected: Compilation errors — `base_cost` field doesn't exist on `ItemDef`, `parse_store_catalog` doesn't exist.

- [ ] **Step 3: Add base_cost to ItemDef**

In `src-tauri/src/item/types.rs`, add to the `ItemDef` struct (after the `icon` field, around line 15):

```rust
#[serde(default)]
pub base_cost: Option<u32>,
```

- [ ] **Step 4: Add StoreDef and StoreCatalog types**

In `src-tauri/src/item/types.rs`, add after the `RecipeDefs` type alias (around line 137):

```rust
/// A vendor store definition loaded from stores.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreDef {
    pub name: String,
    pub buy_multiplier: f64,
    pub inventory: Vec<String>,
}

/// All store definitions, keyed by store ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreCatalog {
    #[serde(flatten)]
    pub stores: HashMap<String, StoreDef>,
}
```

- [ ] **Step 5: Add parse_store_catalog to loader.rs**

In `src-tauri/src/item/loader.rs`, add after `parse_recipe_defs` (around line 78):

```rust
use crate::item::types::StoreCatalog;

/// Parse store catalog from JSON string.
pub fn parse_store_catalog(json: &str) -> Result<StoreCatalog, String> {
    serde_json::from_str(json).map_err(|e| format!("Failed to parse stores.json: {e}"))
}
```

Note: The `use` statement should go at the top of the file with the other imports.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd src-tauri && cargo test`
Expected: PASS — both new tests pass, existing tests still pass.

- [ ] **Step 7: Add baseCost to all items in items.json**

Replace `assets/items.json` with the same content but with `baseCost` added to each item:

```json
{
  "cherry": {
    "name": "Cherry",
    "description": "A plump, juicy cherry from a Fruit Tree.",
    "category": "food",
    "stackLimit": 50,
    "icon": "cherry",
    "baseCost": 3
  },
  "grain": {
    "name": "Grain",
    "description": "Squeezed from a chicken. Don't ask.",
    "category": "food",
    "stackLimit": 50,
    "icon": "grain",
    "baseCost": 3
  },
  "meat": {
    "name": "Meat",
    "description": "Nibbled off a friendly pig. It didn't seem to mind.",
    "category": "food",
    "stackLimit": 50,
    "icon": "meat",
    "baseCost": 5
  },
  "milk": {
    "name": "Milk",
    "description": "Milked from a butterfly. Surprisingly creamy.",
    "category": "food",
    "stackLimit": 50,
    "icon": "milk",
    "baseCost": 4
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
    "baseCost": 20
  },
  "bread": {
    "name": "Bread",
    "description": "Simple baked bread. Hearty and filling.",
    "category": "food",
    "stackLimit": 20,
    "icon": "bread",
    "baseCost": 16
  },
  "steak": {
    "name": "Steak",
    "description": "Grilled meat on a wood fire. Savory.",
    "category": "food",
    "stackLimit": 10,
    "icon": "steak",
    "baseCost": 22
  },
  "butter": {
    "name": "Butter",
    "description": "Churned from fresh butterfly milk.",
    "category": "food",
    "stackLimit": 20,
    "icon": "butter",
    "baseCost": 15
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

- [ ] **Step 8: Create stores.json**

Create `assets/stores.json`:

```json
{
  "grocery": {
    "name": "Grocery Vendor",
    "buyMultiplier": 0.66,
    "inventory": ["cherry", "grain", "meat", "milk"]
  },
  "hardware": {
    "name": "Hardware Vendor",
    "buyMultiplier": 0.50,
    "inventory": ["wood", "plank", "pot", "bubble_wand"]
  }
}
```

- [ ] **Step 9: Add bundled-file parse tests**

In `src-tauri/src/item/loader.rs` tests, add:

```rust
#[test]
fn parse_bundled_items_have_base_cost() {
    let json = include_str!("../../../assets/items.json");
    let defs = parse_item_defs(json).unwrap();
    // All current items should have base_cost
    for (id, def) in &defs {
        assert!(def.base_cost.is_some(), "Item '{id}' missing baseCost");
    }
}

#[test]
fn parse_bundled_stores_json() {
    let json = include_str!("../../../assets/stores.json");
    let catalog = parse_store_catalog(json).unwrap();
    assert_eq!(catalog.stores.len(), 2);
    assert!(catalog.stores.contains_key("grocery"));
    assert!(catalog.stores.contains_key("hardware"));
}
```

- [ ] **Step 10: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: PASS

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/item/types.rs src-tauri/src/item/loader.rs assets/items.json assets/stores.json
git commit -m "feat(economy): add item base costs, store catalog types, and stores.json"
```

---

### Task 2: Vendor Entity Definition & Interaction Routing

**Files:**
- Modify: `src-tauri/src/item/types.rs`
- Modify: `src-tauri/src/item/interaction.rs`
- Modify: `assets/entities.json`
- Modify: `assets/streets/demo_meadow_entities.json`
- Modify: `assets/streets/demo_heights_entities.json`

- [ ] **Step 1: Write tests for vendor entity detection**

In `src-tauri/src/item/types.rs` tests, add:

```rust
#[test]
fn entity_def_with_store() {
    let json = r#"{
        "vendor_grocery": {
            "name": "Grocery Vendor",
            "verb": "Shop",
            "yields": [],
            "cooldownSecs": 0,
            "maxHarvests": 0,
            "respawnSecs": 0,
            "spriteClass": "vendor",
            "interactRadius": 100,
            "store": "grocery"
        }
    }"#;
    let defs: EntityDefs = serde_json::from_str(json).unwrap();
    assert_eq!(defs["vendor_grocery"].store, Some("grocery".to_string()));
}

#[test]
fn entity_def_without_store() {
    let json = r#"{
        "fruit_tree": {
            "name": "Fruit Tree",
            "verb": "Harvest",
            "yields": [{ "item": "cherry", "min": 1, "max": 3 }],
            "cooldownSecs": 5.0,
            "maxHarvests": 3,
            "respawnSecs": 30.0,
            "spriteClass": "tree_fruit",
            "interactRadius": 80
        }
    }"#;
    let defs: EntityDefs = serde_json::from_str(json).unwrap();
    assert!(defs["fruit_tree"].store.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test`
Expected: Compilation error — `store` field doesn't exist on `EntityDef`.

- [ ] **Step 3: Add store field to EntityDef**

In `src-tauri/src/item/types.rs`, add to the `EntityDef` struct (after the `audio_radius` field, around line 46):

```rust
#[serde(default)]
pub store: Option<String>,
```

- [ ] **Step 4: Add Vendor variant to InteractionType**

In `src-tauri/src/item/interaction.rs`, add to the `InteractionType` enum (after the `Jukebox` variant, around line 163):

```rust
Vendor { entity_id: String },
```

- [ ] **Step 5: Add vendor detection to build_prompt**

In `src-tauri/src/item/interaction.rs`, in the `build_prompt` function, add vendor detection after the jukebox detection block (after line 94):

```rust
// Vendor entities use "Shop" prompt, always actionable
if let Some(d) = def {
    if d.store.is_some() {
        return InteractionPrompt {
            verb: d.verb.clone(),
            target_name: d.name.clone(),
            target_x: entity.x,
            target_y: entity.y,
            actionable: true,
            entity_id: Some(entity.id.clone()),
        };
    }
}
```

- [ ] **Step 6: Add vendor detection to execute_interaction**

In `src-tauri/src/item/interaction.rs`, in `execute_interaction`, add vendor detection after the jukebox detection block (after line 213):

```rust
// Vendor entities don't harvest — return a Vendor interaction type
if def.store.is_some() {
    result.interaction_type = Some(InteractionType::Vendor {
        entity_id: entity.id.clone(),
    });
    return result;
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd src-tauri && cargo test`
Expected: PASS

- [ ] **Step 8: Add vendor entities to entities.json**

In `assets/entities.json`, add before the closing `}`:

```json
,
"vendor_grocery": {
    "name": "Grocery Vendor",
    "verb": "Shop",
    "yields": [],
    "cooldownSecs": 0,
    "maxHarvests": 0,
    "respawnSecs": 0,
    "spriteClass": "vendor",
    "interactRadius": 100,
    "store": "grocery"
},
"vendor_hardware": {
    "name": "Hardware Vendor",
    "verb": "Shop",
    "yields": [],
    "cooldownSecs": 0,
    "maxHarvests": 0,
    "respawnSecs": 0,
    "spriteClass": "vendor",
    "interactRadius": 100,
    "store": "hardware"
}
```

- [ ] **Step 9: Place vendors on streets**

In `assets/streets/demo_meadow_entities.json`, add to the entities array:

```json
{ "id": "vendor_grocery_1", "type": "vendor_grocery", "x": -500, "y": -2 }
```

Convert `assets/streets/demo_heights_entities.json` from array format to object format and add the vendor:

```json
{
  "entities": [
    { "id": "btree_1", "type": "bubble_tree", "x": -500, "y": -2 },
    { "id": "chicken_2", "type": "chicken", "x": 400, "y": -2 },
    { "id": "butterfly_2", "type": "butterfly", "x": -100, "y": -60 },
    { "id": "wood_tree_2", "type": "wood_tree", "x": 800, "y": -2 },
    { "id": "vendor_hardware_1", "type": "vendor_hardware", "x": 200, "y": -2 }
  ],
  "groundItems": []
}
```

- [ ] **Step 10: Update bundled entity parse test count**

In `src-tauri/src/item/loader.rs` tests, update `parse_bundled_entities_json`:

```rust
assert_eq!(defs.len(), 9); // was 7, added vendor_grocery + vendor_hardware
```

And add:

```rust
assert!(defs.contains_key("vendor_grocery"));
assert!(defs.contains_key("vendor_hardware"));
```

- [ ] **Step 11: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: PASS

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/item/types.rs src-tauri/src/item/interaction.rs assets/entities.json assets/streets/demo_meadow_entities.json assets/streets/demo_heights_entities.json src-tauri/src/item/loader.rs
git commit -m "feat(economy): vendor entity definition and interaction routing"
```

---

### Task 3: Currency State, Persistence & Vendor Transaction Logic

**Files:**
- Modify: `src-tauri/src/engine/state.rs`
- Create: `src-tauri/src/item/vendor.rs`
- Modify: `src-tauri/src/item/mod.rs`

- [ ] **Step 1: Write vendor transaction tests**

Create `src-tauri/src/item/vendor.rs`:

```rust
use crate::item::inventory::Inventory;
use crate::item::types::{ItemDefs, StoreCatalog, StoreDef};

/// Calculate the sell price for an item at a store.
/// Returns None if the item has no base_cost.
pub fn sell_price(item_id: &str, item_defs: &ItemDefs, store: &StoreDef) -> Option<u32> {
    let def = item_defs.get(item_id)?;
    let base = def.base_cost? as f64;
    let price = (base * store.buy_multiplier).floor() as u32;
    Some(price.max(1)) // Minimum 1 currant
}

/// Attempt to buy items from a vendor. Returns updated currant balance.
pub fn buy(
    item_id: &str,
    count: u32,
    currants: &mut u64,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
    store: &StoreDef,
) -> Result<u64, String> {
    // Validate item is in vendor inventory
    if !store.inventory.contains(&item_id.to_string()) {
        return Err(format!("Item '{item_id}' not sold by this vendor"));
    }
    let def = item_defs.get(item_id)
        .ok_or_else(|| format!("Unknown item: {item_id}"))?;
    let base_cost = def.base_cost
        .ok_or_else(|| format!("Item '{item_id}' has no price"))?;
    let total_cost = base_cost as u64 * count as u64;
    if *currants < total_cost {
        return Err("Not enough currants".to_string());
    }
    if !inventory.has_room_for_count(item_id, count, item_defs) {
        return Err("Inventory full".to_string());
    }
    *currants -= total_cost;
    inventory.add(item_id, count, item_defs);
    Ok(*currants)
}

/// Attempt to sell items to a vendor. Returns updated currant balance.
pub fn sell(
    item_id: &str,
    count: u32,
    currants: &mut u64,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
    store: &StoreDef,
) -> Result<u64, String> {
    let price = sell_price(item_id, item_defs, store)
        .ok_or_else(|| format!("Item '{item_id}' cannot be sold"))?;
    let available = inventory.count_item(item_id);
    if available < count {
        return Err(format!("Not enough {item_id} (have {available}, need {count})"));
    }
    inventory.remove_item(item_id, count);
    let total = price as u64 * count as u64;
    *currants += total;
    Ok(*currants)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::item::types::{ItemDef, StoreDef};

    fn test_item_defs() -> ItemDefs {
        let mut defs = HashMap::new();
        defs.insert("cherry".to_string(), ItemDef {
            id: "cherry".to_string(),
            name: "Cherry".to_string(),
            description: "A cherry.".to_string(),
            category: "food".to_string(),
            stack_limit: 50,
            icon: "cherry".to_string(),
            base_cost: Some(3),
        });
        defs.insert("quest_item".to_string(), ItemDef {
            id: "quest_item".to_string(),
            name: "Quest Item".to_string(),
            description: "Cannot be sold.".to_string(),
            category: "quest".to_string(),
            stack_limit: 1,
            icon: "quest".to_string(),
            base_cost: None,
        });
        defs
    }

    fn test_store() -> StoreDef {
        StoreDef {
            name: "Grocery".to_string(),
            buy_multiplier: 0.66,
            inventory: vec!["cherry".to_string()],
        }
    }

    #[test]
    fn sell_price_calculation() {
        let defs = test_item_defs();
        let store = test_store();
        // floor(3 * 0.66) = floor(1.98) = 1
        assert_eq!(sell_price("cherry", &defs, &store), Some(1));
    }

    #[test]
    fn sell_price_minimum_one() {
        let mut defs = test_item_defs();
        defs.get_mut("cherry").unwrap().base_cost = Some(1);
        let store = StoreDef {
            name: "Test".to_string(),
            buy_multiplier: 0.01,
            inventory: vec![],
        };
        // floor(1 * 0.01) = 0, but minimum is 1
        assert_eq!(sell_price("cherry", &defs, &store), Some(1));
    }

    #[test]
    fn sell_price_no_base_cost() {
        let defs = test_item_defs();
        let store = test_store();
        assert_eq!(sell_price("quest_item", &defs, &store), None);
    }

    #[test]
    fn buy_success() {
        let defs = test_item_defs();
        let store = test_store();
        let mut currants: u64 = 50;
        let mut inventory = Inventory::new(16);
        let result = buy("cherry", 5, &mut currants, &mut inventory, &defs, &store);
        assert_eq!(result, Ok(35)); // 50 - (3*5) = 35
        assert_eq!(inventory.count_item("cherry"), 5);
    }

    #[test]
    fn buy_insufficient_currants() {
        let defs = test_item_defs();
        let store = test_store();
        let mut currants: u64 = 2; // Not enough for 3c cherry
        let mut inventory = Inventory::new(16);
        let result = buy("cherry", 1, &mut currants, &mut inventory, &defs, &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Not enough currants"));
        assert_eq!(currants, 2); // Unchanged
    }

    #[test]
    fn buy_inventory_full() {
        let defs = test_item_defs();
        let store = test_store();
        let mut currants: u64 = 500;
        let mut inventory = Inventory::new(1);
        // Fill the single slot with a different item
        inventory.add("quest_item", 1, &defs);
        let result = buy("cherry", 1, &mut currants, &mut inventory, &defs, &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Inventory full"));
    }

    #[test]
    fn buy_item_not_in_vendor_inventory() {
        let defs = test_item_defs();
        let store = test_store(); // Only sells cherry
        let mut currants: u64 = 50;
        let mut inventory = Inventory::new(16);
        let result = buy("quest_item", 1, &mut currants, &mut inventory, &defs, &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not sold by this vendor"));
    }

    #[test]
    fn sell_success() {
        let defs = test_item_defs();
        let store = test_store();
        let mut currants: u64 = 10;
        let mut inventory = Inventory::new(16);
        inventory.add("cherry", 5, &defs);
        let result = sell("cherry", 3, &mut currants, &mut inventory, &defs, &store);
        // sell_price("cherry") = max(1, floor(3 * 0.66)) = 1
        assert_eq!(result, Ok(13)); // 10 + (1*3) = 13
        assert_eq!(inventory.count_item("cherry"), 2);
    }

    #[test]
    fn sell_insufficient_items() {
        let defs = test_item_defs();
        let store = test_store();
        let mut currants: u64 = 10;
        let mut inventory = Inventory::new(16);
        inventory.add("cherry", 2, &defs);
        let result = sell("cherry", 5, &mut currants, &mut inventory, &defs, &store);
        assert!(result.is_err());
        assert_eq!(inventory.count_item("cherry"), 2); // Unchanged
    }

    #[test]
    fn sell_no_base_cost_rejected() {
        let defs = test_item_defs();
        let store = test_store();
        let mut currants: u64 = 10;
        let mut inventory = Inventory::new(16);
        inventory.add("quest_item", 1, &defs);
        let result = sell("quest_item", 1, &mut currants, &mut inventory, &defs, &store);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cannot be sold"));
    }
}
```

- [ ] **Step 2: Add module declaration**

In `src-tauri/src/item/mod.rs`, add:

```rust
pub mod vendor;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd src-tauri && cargo test item::vendor`
Expected: PASS — all 8 vendor tests pass.

- [ ] **Step 4: Add currants to GameState and SaveState**

In `src-tauri/src/engine/state.rs`, add `currants: u64` to `GameState` struct (after `avatar`, around line 59):

```rust
pub currants: u64,
pub store_catalog: StoreCatalog,
```

Add the import at the top of the file:

```rust
use crate::item::types::StoreCatalog;
```

In `SaveState` (around line 23), add:

```rust
#[serde(default = "default_currants")]
pub currants: u64,
```

And add the default function outside the struct:

```rust
fn default_currants() -> u64 { 50 }
```

- [ ] **Step 5: Update GameState::new() to accept store_catalog**

In `src-tauri/src/engine/state.rs`, update `GameState::new()` signature to add `store_catalog: StoreCatalog` parameter (after `track_catalog`), and add to the struct initialization:

```rust
currants: 50,
store_catalog,
```

- [ ] **Step 6: Add currants to RenderFrame**

In `src-tauri/src/engine/state.rs`, add to the `RenderFrame` struct (after `audio_events`, around line 88):

```rust
pub currants: u64,
```

In the `tick()` method where `RenderFrame` is constructed (around line 628), add:

```rust
currants: self.currants,
```

- [ ] **Step 7: Update save_state() and restore_save()**

In `save_state()` (around line 682), add to the `SaveState` construction:

```rust
currants: self.currants,
```

In `restore_save()` (around line 700), add after `self.avatar = save.avatar.clone();`:

```rust
self.currants = save.currants;
```

- [ ] **Step 8: Write SaveState round-trip test**

In `src-tauri/src/engine/state.rs` tests (if there are any, otherwise add a test module), add:

```rust
#[test]
fn save_state_currants_default() {
    let json = r#"{
        "streetId": "demo_meadow",
        "x": 0.0,
        "y": 0.0,
        "facing": "right",
        "inventory": []
    }"#;
    let save: SaveState = serde_json::from_str(json).unwrap();
    assert_eq!(save.currants, 50); // Default for missing field
}

#[test]
fn save_state_currants_round_trip() {
    let save = SaveState {
        street_id: "demo_meadow".to_string(),
        x: 100.0,
        y: -2.0,
        facing: Direction::Right,
        inventory: vec![],
        avatar: AvatarAppearance::default(),
        currants: 999,
    };
    let json = serde_json::to_string(&save).unwrap();
    let restored: SaveState = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.currants, 999);
}
```

- [ ] **Step 9: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: May need to fix compilation errors in `GameState::new()` calls throughout lib.rs — the signature changed to accept `store_catalog`. Fix the single call site in `lib.rs` (around line 891) to pass the loaded store catalog. See Task 4 for the full wiring.

Note: This step may require temporarily passing a dummy `StoreCatalog { stores: HashMap::new() }` to get tests passing before Task 4 wires up real loading. The implementer should use their judgment — if it compiles and tests pass, commit. If `GameState::new` calls need the catalog, add the loading in this step.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/item/vendor.rs src-tauri/src/item/mod.rs src-tauri/src/engine/state.rs
git commit -m "feat(economy): currants on game state, vendor buy/sell logic"
```

---

### Task 4: Vendor Interaction in Tick Loop & IPC Commands

**Files:**
- Modify: `src-tauri/src/engine/state.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add vendor interaction handling to tick()**

In `src-tauri/src/engine/state.rs`, in the `tick()` method's interaction match block (around line 574), add a new arm for the `Vendor` interaction type:

```rust
Some(interaction::InteractionType::Vendor { .. }) => {
    audio_events.push(AudioEvent::EntityInteract {
        entity_type: "vendor".to_string(),
    });
}
```

- [ ] **Step 2: Rename validate_jukebox_proximity to validate_entity_proximity**

In `src-tauri/src/lib.rs`, rename the function (around line 511):

```rust
fn validate_entity_proximity(state: &engine::state::GameState, entity_id: &str) -> Result<(), String> {
    let entity = state.world_entities.iter().find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type);
    let radius = def.map(|d| d.interact_radius).unwrap_or(60.0);
    let dx = state.player.x - entity.x;
    let dy = state.player.y - entity.y;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > radius { return Err("Too far".to_string()); }
    Ok(())
}
```

Update all 3 call sites in jukebox commands (`jukebox_play`, `jukebox_pause`, `jukebox_select_track`) to use `validate_entity_proximity`.

- [ ] **Step 3: Load store catalog at startup**

In `src-tauri/src/lib.rs`, in the `.manage` block where `GameStateWrapper` is created (around line 885), add after the track_catalog loading:

```rust
let store_catalog = item::loader::parse_store_catalog(
    include_str!("../../assets/stores.json")
).unwrap_or_else(|e| {
    eprintln!("[economy] Failed to load stores.json: {e}");
    item::types::StoreCatalog { stores: std::collections::HashMap::new() }
});
```

Pass `store_catalog` to `GameState::new()`.

- [ ] **Step 4: Add vendor IPC commands**

In `src-tauri/src/lib.rs`, add the new IPC commands:

```rust
#[tauri::command]
fn get_store_state(entity_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let entity = state.world_entities.iter().find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type)
        .ok_or_else(|| "Unknown entity type".to_string())?;
    let store_id = def.store.as_ref()
        .ok_or_else(|| "Entity is not a vendor".to_string())?;
    let store = state.store_catalog.stores.get(store_id)
        .ok_or_else(|| format!("Unknown store: {store_id}"))?;

    let vendor_inventory: Vec<serde_json::Value> = store.inventory.iter()
        .filter_map(|item_id| {
            let item_def = state.item_defs.get(item_id)?;
            let base_cost = item_def.base_cost?;
            Some(serde_json::json!({
                "itemId": item_id,
                "name": item_def.name,
                "baseCost": base_cost
            }))
        })
        .collect();

    // Build deduplicated player inventory (multiple slots may have same item)
    let mut deduped: std::collections::HashMap<String, (String, u32, u32)> = std::collections::HashMap::new();
    for slot in &state.inventory.slots {
        if let Some(stack) = slot {
            if let Some(sell_price) = item::vendor::sell_price(&stack.item_id, &state.item_defs, store) {
                let item_def = state.item_defs.get(&stack.item_id).unwrap();
                let entry = deduped.entry(stack.item_id.clone()).or_insert((item_def.name.clone(), 0, sell_price));
                entry.1 += stack.count;
            }
        }
    }
    let player_inv_json: Vec<serde_json::Value> = deduped.into_iter().map(|(item_id, (name, count, sell_price))| {
        serde_json::json!({
            "itemId": item_id,
            "name": name,
            "count": count,
            "sellPrice": sell_price
        })
    }).collect();

    Ok(serde_json::json!({
        "entityId": entity_id,
        "name": store.name,
        "vendorInventory": vendor_inventory,
        "playerInventory": player_inv_json,
        "currants": state.currants
    }))
}

#[tauri::command]
fn vendor_buy(entity_id: String, item_id: String, count: u32, app: AppHandle) -> Result<u64, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;

    let entity = state.world_entities.iter().find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type)
        .ok_or_else(|| "Unknown entity type".to_string())?;
    let store_id = def.store.as_ref()
        .ok_or_else(|| "Entity is not a vendor".to_string())?;
    let store = state.store_catalog.stores.get(store_id)
        .ok_or_else(|| format!("Unknown store: {store_id}"))?
        .clone();

    let item_defs = state.item_defs.clone();
    let base_cost = item_defs.get(&item_id)
        .and_then(|d| d.base_cost)
        .ok_or_else(|| "Item has no price".to_string())?;

    let result = item::vendor::buy(&item_id, count, &mut state.currants, &mut state.inventory, &item_defs, &store)?;

    // Currency feedback
    let total = base_cost as u64 * count as u64;
    state.pickup_feedback.push(crate::item::types::PickupFeedback {
        id: state.next_feedback_id,
        text: format!("-{} currants", total),
        success: true,
        x: state.player.x,
        y: state.player.y,
        age_secs: 0.0,
    });
    state.next_feedback_id += 1;

    Ok(result)
}

#[tauri::command]
fn vendor_sell(entity_id: String, item_id: String, count: u32, app: AppHandle) -> Result<u64, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    validate_entity_proximity(&state, &entity_id)?;

    let entity = state.world_entities.iter().find(|e| e.id == entity_id)
        .ok_or_else(|| format!("Unknown entity: {entity_id}"))?;
    let def = state.entity_defs.get(&entity.entity_type)
        .ok_or_else(|| "Unknown entity type".to_string())?;
    let store_id = def.store.as_ref()
        .ok_or_else(|| "Entity is not a vendor".to_string())?;
    let store = state.store_catalog.stores.get(store_id)
        .ok_or_else(|| format!("Unknown store: {store_id}"))?
        .clone();

    let item_defs = state.item_defs.clone();
    let sell_price = item::vendor::sell_price(&item_id, &item_defs, &store)
        .ok_or_else(|| "Item cannot be sold".to_string())?;

    let result = item::vendor::sell(&item_id, count, &mut state.currants, &mut state.inventory, &item_defs, &store)?;

    // Currency feedback
    let total = sell_price as u64 * count as u64;
    state.pickup_feedback.push(crate::item::types::PickupFeedback {
        id: state.next_feedback_id,
        text: format!("+{} currants", total),
        success: true,
        x: state.player.x,
        y: state.player.y,
        age_secs: 0.0,
    });
    state.next_feedback_id += 1;

    Ok(result)
}
```

- [ ] **Step 5: Register new IPC commands**

In `src-tauri/src/lib.rs`, add to the `invoke_handler` list (around line 946):

```rust
get_store_state,
vendor_buy,
vendor_sell,
```

- [ ] **Step 6: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: PASS

Run: `cd src-tauri && cargo clippy`
Expected: No warnings

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/engine/state.rs src-tauri/src/lib.rs
git commit -m "feat(economy): vendor IPC commands and currants in render frame"
```

---

### Task 5: Frontend Types & IPC

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add economy types to types.ts**

In `src/lib/types.ts`, add after the `JukeboxInfo` interface (around line 304):

```typescript
export interface StoreState {
  entityId: string;
  name: string;
  vendorInventory: StoreItem[];
  playerInventory: SellableItem[];
  currants: number;
}

export interface StoreItem {
  itemId: string;
  name: string;
  baseCost: number;
}

export interface SellableItem {
  itemId: string;
  name: string;
  count: number;
  sellPrice: number;
}
```

Add `currants` to `RenderFrame` (around line 162, after `audioEvents`):

```typescript
currants: number;
```

Add `currants` to `SavedState` (around line 270, after `inventory`):

```typescript
currants?: number;
```

- [ ] **Step 2: Add vendor IPC functions**

In `src/lib/ipc.ts`, add the import for `StoreState`:

```typescript
import type { StreetData, InputState, RenderFrame, NetworkStatus, PlayerIdentity, ChatEvent, RecipeDef, SavedState, SoundKitMeta, JukeboxInfo, AvatarAppearance, StoreState } from './types';
```

Add after the avatar IPC functions (around line 111):

```typescript
export async function getStoreState(entityId: string): Promise<StoreState> {
  return invoke<StoreState>('get_store_state', { entityId });
}

export async function vendorBuy(entityId: string, itemId: string, count: number): Promise<number> {
  return invoke<number>('vendor_buy', { entityId, itemId, count });
}

export async function vendorSell(entityId: string, itemId: string, count: number): Promise<number> {
  return invoke<number>('vendor_sell', { entityId, itemId, count });
}
```

- [ ] **Step 3: Run frontend tests**

Run: `npx vitest run`
Expected: PASS — existing tests still pass (types are additive).

- [ ] **Step 4: Commit**

```bash
git add src/lib/types.ts src/lib/ipc.ts
git commit -m "feat(economy): frontend types and IPC for vendors"
```

---

### Task 6: ShopPanel Component

**Files:**
- Create: `src/lib/components/ShopPanel.svelte`
- Create: `src/lib/components/ShopPanel.test.ts`

- [ ] **Step 1: Write ShopPanel tests**

Create `src/lib/components/ShopPanel.test.ts`:

```typescript
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import ShopPanel from './ShopPanel.svelte';
import type { StoreState } from '../types';

const mockStoreState: StoreState = {
  entityId: 'vendor_1',
  name: 'Grocery Vendor',
  vendorInventory: [
    { itemId: 'cherry', name: 'Cherry', baseCost: 3 },
    { itemId: 'grain', name: 'Grain', baseCost: 3 },
  ],
  playerInventory: [
    { itemId: 'cherry', name: 'Cherry', count: 12, sellPrice: 2 },
    { itemId: 'wood', name: 'Wood', count: 8, sellPrice: 2 },
  ],
  currants: 50,
};

describe('ShopPanel', () => {
  it('renders nothing when not visible', () => {
    render(ShopPanel, { props: { storeState: null, visible: false } });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('renders store name and currant balance', () => {
    render(ShopPanel, { props: { storeState: mockStoreState, visible: true } });
    expect(screen.getByText('Grocery Vendor')).toBeTruthy();
    expect(screen.getByText(/50/)).toBeTruthy();
  });

  it('renders buy tab with vendor inventory', () => {
    render(ShopPanel, { props: { storeState: mockStoreState, visible: true } });
    expect(screen.getByText('Cherry')).toBeTruthy();
    expect(screen.getByText('Grain')).toBeTruthy();
  });

  it('shows sell tab with player inventory', async () => {
    render(ShopPanel, { props: { storeState: mockStoreState, visible: true } });
    const sellTab = screen.getByRole('tab', { name: /sell/i });
    await sellTab.click();
    expect(screen.getByText(/Wood/)).toBeTruthy();
    expect(screen.getByText(/×8/)).toBeTruthy();
  });

  it('shows empty state on sell tab with no items', async () => {
    const emptyState = { ...mockStoreState, playerInventory: [] };
    render(ShopPanel, { props: { storeState: emptyState, visible: true } });
    const sellTab = screen.getByRole('tab', { name: /sell/i });
    await sellTab.click();
    expect(screen.getByText(/no items to sell/i)).toBeTruthy();
  });

  it('calls onBuy when buy button clicked', async () => {
    const onBuy = vi.fn();
    render(ShopPanel, { props: { storeState: mockStoreState, visible: true, onBuy } });
    const buyButtons = screen.getAllByRole('button', { name: /buy/i });
    await buyButtons[0].click();
    expect(onBuy).toHaveBeenCalledWith('cherry', 1);
  });

  it('calls onSell when sell button clicked', async () => {
    const onSell = vi.fn();
    render(ShopPanel, { props: { storeState: mockStoreState, visible: true, onSell } });
    const sellTab = screen.getByRole('tab', { name: /sell/i });
    await sellTab.click();
    const sellButtons = screen.getAllByRole('button', { name: /sell/i });
    await sellButtons[0].click();
    expect(onSell).toHaveBeenCalled();
  });

  it('has accessible dialog label', () => {
    render(ShopPanel, { props: { storeState: mockStoreState, visible: true } });
    const dialog = screen.getByRole('dialog');
    expect(dialog.getAttribute('aria-label')).toBe('Shop: Grocery Vendor');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/lib/components/ShopPanel.test.ts`
Expected: FAIL — ShopPanel.svelte doesn't exist yet.

- [ ] **Step 3: Create ShopPanel.svelte**

Create `src/lib/components/ShopPanel.svelte`:

```svelte
<script lang="ts">
  import type { StoreState } from '../types';

  let {
    storeState = null,
    visible = false,
    onClose = undefined,
    onBuy = undefined,
    onSell = undefined,
  }: {
    storeState: StoreState | null;
    visible: boolean;
    onClose?: () => void;
    onBuy?: (itemId: string, count: number) => void;
    onSell?: (itemId: string, count: number) => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;
  let activeTab = $state<'buy' | 'sell'>('buy');

  let dialogLabel = $derived(storeState ? `Shop: ${storeState.name}` : 'Shop');

  $effect(() => {
    if (!dialogEl) return;
    if (visible && storeState) {
      if (!dialogEl.open) {
        previousFocus = document.activeElement as HTMLElement | null;
        dialogEl.showModal();
        const first = dialogEl.querySelector('button, [tabindex]') as HTMLElement | null;
        first?.focus();
      }
    } else {
      if (dialogEl.open) {
        dialogEl.close();
        previousFocus?.focus();
        previousFocus = null;
        activeTab = 'buy';
      }
    }
  });

  function handleCancel() {
    onClose?.();
  }

  function handleBuy(itemId: string, e: MouseEvent) {
    const count = e.shiftKey ? maxBuyCount(itemId) : 1;
    if (count > 0) onBuy?.(itemId, count);
  }

  function handleSell(itemId: string, count: number, e: MouseEvent) {
    const sellCount = e.shiftKey ? count : 1;
    if (sellCount > 0) onSell?.(itemId, sellCount);
  }

  function maxBuyCount(itemId: string): number {
    if (!storeState) return 0;
    const item = storeState.vendorInventory.find(i => i.itemId === itemId);
    if (!item) return 0;
    return Math.floor(storeState.currants / item.baseCost);
  }

  function canAfford(baseCost: number): boolean {
    return (storeState?.currants ?? 0) >= baseCost;
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<dialog
  bind:this={dialogEl}
  aria-label={dialogLabel}
  oncancel|preventDefault={handleCancel}
  onkeydown={(e) => { if (e.key === 'Escape') handleCancel(); }}
>
  {#if storeState}
    <div class="shop-panel">
      <header class="shop-header">
        <h2>{storeState.name}</h2>
        <span class="currants">{storeState.currants}c</span>
        <button class="close-btn" aria-label="Close shop" onclick={handleCancel}>×</button>
      </header>

      <div role="tablist" class="tab-bar">
        <button
          role="tab"
          aria-selected={activeTab === 'buy'}
          class:active={activeTab === 'buy'}
          onclick={() => activeTab = 'buy'}
        >Buy</button>
        <button
          role="tab"
          aria-selected={activeTab === 'sell'}
          class:active={activeTab === 'sell'}
          onclick={() => activeTab = 'sell'}
        >Sell</button>
      </div>

      {#if activeTab === 'buy'}
        <ul class="item-list" role="list">
          {#each storeState.vendorInventory as item (item.itemId)}
            <li class="item-row">
              <span class="item-name">{item.name}</span>
              <span class="item-price">{item.baseCost}c</span>
              <button
                class="action-btn buy-btn"
                aria-label="Buy {item.name}"
                disabled={!canAfford(item.baseCost)}
                onclick={(e) => handleBuy(item.itemId, e)}
              >Buy</button>
            </li>
          {/each}
        </ul>
      {:else}
        {#if storeState.playerInventory.length === 0}
          <p class="empty-state">No items to sell</p>
        {:else}
          <ul class="item-list" role="list">
            {#each storeState.playerInventory as item (item.itemId)}
              <li class="item-row">
                <span class="item-name">{item.name} <span class="item-count">×{item.count}</span></span>
                <span class="item-price">{item.sellPrice}c ea</span>
                <button
                  class="action-btn sell-btn"
                  aria-label="Sell {item.name}"
                  onclick={(e) => handleSell(item.itemId, item.count, e)}
                >Sell</button>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}

      <p class="hint">Click = 1 · Shift+click = stack</p>
    </div>
  {/if}
</dialog>

<style>
  dialog {
    border: none;
    border-radius: 8px;
    padding: 0;
    background: #1a1a2e;
    color: #e0e0e0;
    min-width: 320px;
    max-width: 400px;
    font-family: inherit;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.4);
  }
  .shop-panel {
    padding: 16px;
  }
  .shop-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
  }
  .shop-header h2 {
    margin: 0;
    font-size: 16px;
    flex: 1;
  }
  .currants {
    color: #ffd700;
    font-weight: bold;
  }
  .close-btn {
    background: none;
    border: none;
    color: #888;
    font-size: 18px;
    cursor: pointer;
    padding: 0 4px;
  }
  .close-btn:hover { color: #fff; }
  .tab-bar {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
    border-bottom: 2px solid #333;
    padding-bottom: 8px;
  }
  .tab-bar button {
    background: none;
    border: none;
    color: #888;
    cursor: pointer;
    padding: 4px 8px;
    font-size: 14px;
  }
  .tab-bar button.active {
    color: #fff;
    border-bottom: 2px solid #7c6fe0;
    margin-bottom: -10px;
    padding-bottom: 10px;
  }
  .item-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .item-row {
    display: flex;
    align-items: center;
    background: #252545;
    padding: 8px 10px;
    border-radius: 4px;
    gap: 8px;
  }
  .item-name {
    flex: 1;
    color: #ccc;
  }
  .item-count {
    color: #888;
    font-size: 12px;
  }
  .item-price {
    color: #ffd700;
    min-width: 50px;
    text-align: right;
  }
  .action-btn {
    padding: 2px 10px;
    border: none;
    border-radius: 3px;
    font-size: 12px;
    cursor: pointer;
    color: #fff;
  }
  .buy-btn { background: #7c6fe0; }
  .buy-btn:disabled { background: #444; cursor: not-allowed; opacity: 0.5; }
  .sell-btn { background: #4a8c5c; }
  .empty-state {
    color: #666;
    text-align: center;
    padding: 24px 0;
  }
  .hint {
    color: #555;
    font-size: 11px;
    text-align: center;
    margin-top: 12px;
    margin-bottom: 0;
  }
</style>
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/lib/components/ShopPanel.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/ShopPanel.svelte src/lib/components/ShopPanel.test.ts
git commit -m "feat(economy): ShopPanel component with buy/sell tabs"
```

---

### Task 7: CurrantHud Component & App.svelte Integration

**Files:**
- Create: `src/lib/components/CurrantHud.svelte`
- Modify: `src/App.svelte`

- [ ] **Step 1: Create CurrantHud component**

Create `src/lib/components/CurrantHud.svelte`:

```svelte
<script lang="ts">
  let { currants = 0 }: { currants: number } = $props();
</script>

<div class="currant-hud" aria-label="Currant balance: {currants}">
  <span class="currant-icon">●</span>
  <span class="currant-amount">{currants}</span>
</div>

<style>
  .currant-hud {
    position: fixed;
    top: 12px;
    right: 12px;
    background: rgba(26, 26, 46, 0.85);
    color: #ffd700;
    padding: 6px 12px;
    border-radius: 16px;
    font-weight: bold;
    font-size: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
    z-index: 50;
    pointer-events: none;
    user-select: none;
  }
  .currant-icon {
    font-size: 10px;
  }
</style>
```

- [ ] **Step 2: Wire up vendor interaction in App.svelte**

In `src/App.svelte`, add the imports:

```typescript
import ShopPanel from './lib/components/ShopPanel.svelte';
import CurrantHud from './lib/components/CurrantHud.svelte';
import { getStoreState, vendorBuy, vendorSell } from './lib/ipc';
import type { StoreState } from './lib/types';
```

Add state variables (near the jukebox state, around line 37):

```typescript
let shopOpen = $state(false);
let storeState = $state<StoreState | null>(null);
let shopCloseFrames = 0;
```

Add vendor interaction detection in the frame handler (similar pattern to jukebox, around line 186). In the section where `audioEvents` are processed for jukebox interaction, add a similar block for vendor:

```typescript
// Vendor interaction detection
const vendorInteract = frame.audioEvents?.find(
  (e) => e.type === 'entityInteract' && e.entityType === 'vendor'
);
if (vendorInteract && frame.interactionPrompt?.entityId) {
  const eid = frame.interactionPrompt.entityId;
  try {
    storeState = await getStoreState(eid);
    shopOpen = true;
    inventoryOpen = false;
    volumeOpen = false;
    jukeboxOpen = false;
    jukeboxInfo = null;
    shopCloseFrames = 0;
  } catch (e) {
    console.error('Failed to get store state:', e);
  }
}
```

Add shop panel auto-close logic (same 2-frame debounce pattern as jukebox):

```typescript
// Shop panel range detection
if (shopOpen && storeState) {
  if (frame.interactionPrompt?.entityId === storeState.entityId) {
    shopCloseFrames = 0;
  } else {
    shopCloseFrames++;
    if (shopCloseFrames >= 2) {
      shopOpen = false;
      storeState = null;
    }
  }
}
```

Add keyboard shortcut blocking (where jukebox blocks I/P, also block for shop):

Update the condition to include `shopOpen`:
```typescript
if (e.key === 'i' || e.key === 'I') {
  if (jukeboxOpen || shopOpen) return;
  // ...
}
```

Pass `uiOpen` to GameCanvas with shop included:
```typescript
uiOpen={volumeOpen || jukeboxOpen || inventoryOpen || shopOpen}
```

Add the ShopPanel and CurrantHud to the template:

```svelte
<CurrantHud currants={latestFrame?.currants ?? 0} />

<ShopPanel
  {storeState}
  visible={shopOpen}
  onClose={() => { shopOpen = false; storeState = null; }}
  onBuy={async (itemId, count) => {
    if (!storeState) return;
    try {
      await vendorBuy(storeState.entityId, itemId, count);
      storeState = await getStoreState(storeState.entityId);
    } catch (e) {
      console.error('Buy failed:', e);
    }
  }}
  onSell={async (itemId, count) => {
    if (!storeState) return;
    try {
      await vendorSell(storeState.entityId, itemId, count);
      storeState = await getStoreState(storeState.entityId);
    } catch (e) {
      console.error('Sell failed:', e);
    }
  }}
/>
```

- [ ] **Step 3: Handle back button and modal exclusivity**

In the back button handler (wherever `jukeboxOpen = false` is set for back navigation), also add:

```typescript
shopOpen = false;
storeState = null;
```

Close shop when opening inventory/volume and vice versa — when shop opens, close inventory/volume (already done above). When inventory opens, close shop:

In the inventory toggle handler:
```typescript
if (!inventoryOpen) { shopOpen = false; storeState = null; }
```

- [ ] **Step 4: Run all tests**

Run: `npx vitest run`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/CurrantHud.svelte src/App.svelte
git commit -m "feat(economy): CurrantHud, ShopPanel integration, vendor interaction wiring"
```

---

### Task 8: Final Integration Testing & Cleanup

**Files:**
- All files from previous tasks

- [ ] **Step 1: Run full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: PASS — all tests pass including new economy tests.

- [ ] **Step 2: Run Rust linter**

Run: `cd src-tauri && cargo clippy`
Expected: No warnings.

- [ ] **Step 3: Run full frontend test suite**

Run: `npx vitest run`
Expected: PASS — all tests pass including ShopPanel tests.

- [ ] **Step 4: Fix any issues found**

If any tests fail or clippy warns, fix the issues and re-run.

- [ ] **Step 5: Final commit (if fixes were needed)**

```bash
git add -A
git commit -m "fix(economy): address test failures and clippy warnings"
```
