use crate::item::inventory::Inventory;
use crate::item::types::{ItemDefs, StoreDef};

/// Calculate the sell price (player sells TO vendor) for an item.
/// Returns `max(1, floor(base_cost * buy_multiplier))`.
/// Returns `None` if the item has no `base_cost`.
pub fn sell_price(item_id: &str, item_defs: &ItemDefs, store: &StoreDef) -> Option<u32> {
    let def = item_defs.get(item_id)?;
    let base_cost = def.base_cost?;
    let raw = (base_cost as f64 * store.buy_multiplier).floor() as u32;
    Some(raw.max(1))
}

/// Player buys `count` copies of `item_id` from the vendor.
///
/// Validates:
/// - Item is in the vendor's inventory list
/// - Item has a `base_cost`
/// - Player has enough currants
/// - Player has enough inventory room
///
/// On success, deducts currants and adds items to the player's inventory.
/// Returns the updated currant balance.
pub fn buy(
    item_id: &str,
    count: u32,
    currants: u64,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
    store: &StoreDef,
) -> Result<u64, String> {
    // Item must be in vendor's inventory
    if !store.inventory.iter().any(|id| id == item_id) {
        return Err(format!("Item '{item_id}' is not sold here"));
    }

    // Item must have a base_cost
    let def = item_defs
        .get(item_id)
        .ok_or_else(|| format!("Unknown item '{item_id}'"))?;
    let base_cost = def
        .base_cost
        .ok_or_else(|| format!("Item '{item_id}' has no price"))?;

    // Total cost must fit in u64 and player must have enough
    let total_cost = (base_cost as u64)
        .checked_mul(count as u64)
        .ok_or_else(|| "Cost overflow".to_string())?;
    if currants < total_cost {
        return Err(format!(
            "Not enough currants (need {total_cost}, have {currants})"
        ));
    }

    // Player must have room for count items
    if !inventory.has_room_for_count(item_id, count, item_defs) {
        return Err("Inventory is full".to_string());
    }

    // Deduct currants and add items
    let new_currants = currants - total_cost;
    inventory.add(item_id, count, item_defs);
    Ok(new_currants)
}

/// Player sells `count` copies of `item_id` to the vendor.
///
/// Validates:
/// - Item has a `base_cost` (via `sell_price`)
/// - Player has at least `count` of the item
///
/// On success, removes items from the player's inventory and adds currants.
/// Returns the updated currant balance.
pub fn sell(
    item_id: &str,
    count: u32,
    currants: u64,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
    store: &StoreDef,
) -> Result<u64, String> {
    // Item must have a sell price (base_cost required)
    let price = sell_price(item_id, item_defs, store)
        .ok_or_else(|| format!("Item '{item_id}' cannot be sold"))?;

    // Player must have enough of the item
    let have = inventory.count_item(item_id);
    if have < count {
        return Err(format!(
            "Not enough '{item_id}' (need {count}, have {have})"
        ));
    }

    // Remove items and add currants
    inventory.remove_item(item_id, count);
    let earned = (price as u64)
        .checked_mul(count as u64)
        .ok_or_else(|| "Earnings overflow".to_string())?;
    let new_currants = currants
        .checked_add(earned)
        .ok_or_else(|| "Currant balance overflow".to_string())?;
    Ok(new_currants)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::{ItemDef, StoreDef};
    use std::collections::HashMap;

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
            },
        );
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
        // floor(3 * 0.66) = floor(1.98) = 1
        let defs = test_item_defs();
        let store = test_store();
        assert_eq!(sell_price("cherry", &defs, &store), Some(1));
    }

    #[test]
    fn sell_price_minimum_one() {
        // floor(1 * 0.01) = 0 → clamped to 1
        let mut defs = test_item_defs();
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
            },
        );
        let store = StoreDef {
            name: "Store".to_string(),
            buy_multiplier: 0.01,
            inventory: vec!["cheap_item".to_string()],
        };
        assert_eq!(sell_price("cheap_item", &defs, &store), Some(1));
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
        let mut inv = Inventory::new(10);
        // 50 currants, buy 5 cherries at 3c each = 35 remaining
        let result = buy("cherry", 5, 50, &mut inv, &defs, &store);
        assert_eq!(result, Ok(35));
        assert_eq!(inv.count_item("cherry"), 5);
    }

    #[test]
    fn buy_insufficient_currants() {
        let defs = test_item_defs();
        let store = test_store();
        let mut inv = Inventory::new(10);
        // 2 currants, cherry costs 3
        let result = buy("cherry", 1, 2, &mut inv, &defs, &store);
        assert!(result.is_err());
        assert_eq!(inv.count_item("cherry"), 0);
    }

    #[test]
    fn buy_inventory_full() {
        let defs = test_item_defs();
        let store = test_store();
        // Inventory with capacity 1, already full (50 cherries = stack_limit of 50)
        let mut inv = Inventory::new(1);
        inv.add("cherry", 50, &defs);
        let result = buy("cherry", 1, 1000, &mut inv, &defs, &store);
        assert!(result.is_err());
    }

    #[test]
    fn buy_item_not_in_vendor_inventory() {
        let mut defs = test_item_defs();
        // Add an item that the store doesn't carry
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
            },
        );
        let store = test_store(); // store only sells "cherry"
        let mut inv = Inventory::new(10);
        let result = buy("rare_gem", 1, 1000, &mut inv, &defs, &store);
        assert!(result.is_err());
    }

    #[test]
    fn sell_success() {
        let defs = test_item_defs();
        let store = test_store();
        let mut inv = Inventory::new(10);
        inv.add("cherry", 5, &defs);
        // sell 3 cherries at sell_price=1 each → gain 3
        let result = sell("cherry", 3, 10, &mut inv, &defs, &store);
        assert_eq!(result, Ok(13));
        assert_eq!(inv.count_item("cherry"), 2);
    }

    #[test]
    fn sell_insufficient_items() {
        let defs = test_item_defs();
        let store = test_store();
        let mut inv = Inventory::new(10);
        inv.add("cherry", 2, &defs);
        // have 2, try to sell 5
        let result = sell("cherry", 5, 10, &mut inv, &defs, &store);
        assert!(result.is_err());
        // inventory should be unchanged
        assert_eq!(inv.count_item("cherry"), 2);
    }

    #[test]
    fn sell_no_base_cost_rejected() {
        let defs = test_item_defs();
        let store = test_store();
        let mut inv = Inventory::new(10);
        inv.add("quest_item", 1, &defs);
        let result = sell("quest_item", 1, 10, &mut inv, &defs, &store);
        assert!(result.is_err());
        // inventory should be unchanged
        assert_eq!(inv.count_item("quest_item"), 1);
    }
}
