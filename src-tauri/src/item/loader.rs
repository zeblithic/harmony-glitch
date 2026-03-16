use crate::item::types::{EntityDefs, ItemDefs, WorldEntity};

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
    fn parse_entity_defs_rejects_min_greater_than_max() {
        let json = r#"{
            "bad": {
                "name": "Bad",
                "verb": "Use",
                "yields": [{ "item": "x", "min": 5, "max": 2 }],
                "cooldownSecs": 0,
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
        assert_eq!(defs.len(), 6);
        assert!(defs.contains_key("fruit_tree"));
        assert!(defs.contains_key("chicken"));
        assert!(defs.contains_key("pig"));
        assert!(defs.contains_key("butterfly"));
        assert!(defs.contains_key("bubble_tree"));
        assert!(defs.contains_key("wood_tree"));
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
