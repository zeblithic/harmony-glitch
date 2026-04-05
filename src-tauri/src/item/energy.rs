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
        assert_eq!(inv.count_item("wood"), 5);
    }

    #[test]
    fn eat_rejected_when_item_not_in_inventory() {
        let defs = test_item_defs();
        let mut inv = Inventory::new(16);
        let result = eat("cherry", 100.0, 600.0, &mut inv, &defs);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No 'cherry'"));
    }
}
