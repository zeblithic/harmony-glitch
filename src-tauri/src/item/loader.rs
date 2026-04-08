use serde::Deserialize;

use crate::item::types::{EntityDefs, ItemDefs, RecipeDefs, StoreCatalog, WorldEntity, WorldItem};

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
        for y in &def.yields {
            if y.min > y.max {
                return Err(format!(
                    "Entity '{}' has yield for '{}' with min ({}) > max ({})",
                    key, y.item, y.min, y.max
                ));
            }
        }
    }
    Ok(raw)
}

/// Placement data parsed from a street's entity/item JSON file.
pub struct PlacementData {
    pub entities: Vec<WorldEntity>,
    pub ground_items: Vec<WorldItem>,
}

/// Parse entity and ground item placements from JSON string.
/// Supports both array format (legacy, entities only) and object format
/// (with optional groundItems field).
pub fn parse_entity_placements(json: &str) -> Result<PlacementData, String> {
    let is_object = json.trim_start().starts_with('{');

    if is_object {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PlacementJson {
            entities: Vec<WorldEntity>,
            #[serde(default)]
            ground_items: Vec<WorldItem>,
        }
        let data: PlacementJson = serde_json::from_str(json)
            .map_err(|e| format!("Failed to parse entity placements (object format): {e}"))?;
        Ok(PlacementData {
            entities: data.entities,
            ground_items: data.ground_items,
        })
    } else {
        let entities: Vec<WorldEntity> = serde_json::from_str(json)
            .map_err(|e| format!("Failed to parse entity placements (array format): {e}"))?;
        Ok(PlacementData {
            entities,
            ground_items: vec![],
        })
    }
}

/// Parse store catalog from JSON string.
pub fn parse_store_catalog(json: &str) -> Result<StoreCatalog, String> {
    serde_json::from_str(json).map_err(|e| format!("Failed to parse stores.json: {e}"))
}

/// Parse recipe definitions from JSON string.
pub fn parse_recipe_defs(json: &str) -> Result<RecipeDefs, String> {
    let mut raw: RecipeDefs =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse recipes.json: {e}"))?;
    for (key, def) in raw.iter_mut() {
        def.id = key.clone();
    }
    Ok(raw)
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
                "maxHarvests": 2,
                "respawnSecs": 45.0,
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
        let data = parse_entity_placements(json).unwrap();
        assert_eq!(data.entities.len(), 2);
        assert_eq!(data.entities[0].entity_type, "fruit_tree");
        assert_eq!(data.entities[1].entity_type, "chicken");
    }

    #[test]
    fn parse_entity_defs_rejects_min_greater_than_max() {
        let json = r#"{
            "bad": {
                "name": "Bad",
                "verb": "Use",
                "yields": [{ "item": "x", "min": 5, "max": 2 }],
                "cooldownSecs": 0,
                "maxHarvests": 0,
                "respawnSecs": 0.0,
                "spriteClass": "bad",
                "interactRadius": 60
            }
        }"#;
        let result = parse_entity_defs(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("min (5) > max (2)"));
    }

    #[test]
    fn parse_bundled_items_json() {
        let json = include_str!("../../../assets/items.json");
        let defs = parse_item_defs(json).unwrap();
        assert!(defs.len() > 1200);
        // Real Glitch items
        assert!(defs.contains_key("apple"));
        assert!(defs.contains_key("cherry"));
        assert!(defs.contains_key("grain"));
        assert!(defs.contains_key("meat"));
        assert!(defs.contains_key("plank"));
        assert!(defs.contains_key("knife_and_board"));
        // Demo-only items preserved
        assert!(defs.contains_key("milk"));
        assert!(defs.contains_key("bubble"));
        assert!(defs.contains_key("wood"));
        assert!(defs.contains_key("pot"));
    }

    #[test]
    fn parse_bundled_entities_json() {
        let json = include_str!("../../../assets/entities.json");
        let defs = parse_entity_defs(json).unwrap();
        assert_eq!(defs.len(), 11);
        assert!(defs.contains_key("fruit_tree"));
        assert!(defs.contains_key("chicken"));
        assert!(defs.contains_key("pig"));
        assert!(defs.contains_key("butterfly"));
        assert!(defs.contains_key("bubble_tree"));
        assert!(defs.contains_key("wood_tree"));
        assert!(defs.contains_key("jukebox_tavern"));
        assert!(defs.contains_key("vendor_grocery"));
        assert!(defs.contains_key("vendor_hardware"));
    }

    #[test]
    fn parse_bundled_meadow_entities() {
        let json = include_str!("../../../assets/streets/demo_meadow_entities.json");
        let data = parse_entity_placements(json).unwrap();
        assert!(data.entities.len() >= 3);
    }

    #[test]
    fn parse_bundled_heights_entities() {
        let json = include_str!("../../../assets/streets/demo_heights_entities.json");
        let data = parse_entity_placements(json).unwrap();
        assert!(data.entities.len() >= 2);
    }

    #[test]
    fn parse_bundled_meadow_has_ground_items() {
        let json = include_str!("../../../assets/streets/demo_meadow_entities.json");
        let data = parse_entity_placements(json).unwrap();
        assert!(data.entities.len() >= 3);
        assert_eq!(data.ground_items.len(), 1);
        assert_eq!(data.ground_items[0].item_id, "pot");
    }

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
        assert!(defs.len() > 300);
        // Real Glitch recipes
        assert!(defs.contains_key("simple_slaw"));
        // Demo recipes preserved
        assert!(defs.contains_key("cherry_pie"));
        assert!(defs.contains_key("bread"));
        assert!(defs.contains_key("plank"));
        // Verify tools field parsed correctly
        assert_eq!(defs["cherry_pie"].tools.len(), 1);
        assert_eq!(defs["cherry_pie"].tools[0].item, "pot");
        // Verify no-tool recipe
        assert!(defs["plank"].tools.is_empty());
    }

    #[test]
    fn parse_store_catalog_from_json() {
        let json = r#"{
            "grocery": {
                "name": "Grocery Vendor",
                "buyMultiplier": 0.66,
                "inventory": ["cherry", "grain", "meat", "milk"]
            }
        }"#;
        let catalog = parse_store_catalog(json).unwrap();
        assert_eq!(catalog.stores.len(), 1);
        let grocery = &catalog.stores["grocery"];
        assert_eq!(grocery.name, "Grocery Vendor");
        assert!((grocery.buy_multiplier - 0.66).abs() < 0.001);
        assert_eq!(grocery.inventory, vec!["cherry", "grain", "meat", "milk"]);
    }

    #[test]
    fn parse_store_catalog_empty() {
        let catalog = parse_store_catalog("{}").unwrap();
        assert_eq!(catalog.stores.len(), 0);
    }

    #[test]
    fn parse_bundled_stores_json() {
        let json = include_str!("../../../assets/stores.json");
        let catalog = parse_store_catalog(json).unwrap();
        assert_eq!(catalog.stores.len(), 2);
        assert!(catalog.stores.contains_key("grocery"));
        assert!(catalog.stores.contains_key("hardware"));
    }

    #[test]
    fn parse_bundled_entities_has_movement_fields() {
        let json = include_str!("../../../assets/entities.json");
        let defs = parse_entity_defs(json).unwrap();

        let chicken = &defs["chicken"];
        assert!((chicken.walk_speed.unwrap() - 40.0).abs() < 0.01);
        assert!((chicken.wander_radius.unwrap() - 120.0).abs() < 0.01);
        assert!(chicken.bob_amplitude.is_none());

        let butterfly = &defs["butterfly"];
        assert!((butterfly.bob_amplitude.unwrap() - 15.0).abs() < 0.01);
        assert!((butterfly.bob_frequency.unwrap() - 1.5).abs() < 0.01);

        let fruit_tree = &defs["fruit_tree"];
        assert!(fruit_tree.walk_speed.is_none());
        assert!(fruit_tree.wander_radius.is_none());
    }
}
