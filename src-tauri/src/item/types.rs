use crate::avatar::types::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Item type definition (loaded from JSON at startup).
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
    pub max_harvests: u32,
    pub respawn_secs: f64,
    pub sprite_class: String,
    pub interact_radius: f64,
    pub walk_speed: Option<f64>,
    pub wander_radius: Option<f64>,
    pub bob_amplitude: Option<f64>,
    pub bob_frequency: Option<f64>,
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
    /// Current X position in world space (may differ from spawn X for wandering entities).
    pub current_x: f64,
    /// Current horizontal velocity in pixels per second.
    pub velocity_x: f64,
    /// Direction the entity is currently facing.
    pub facing: Direction,
    /// X position the entity wanders around (set to spawn X on load).
    pub wander_origin: f64,
    /// Game-time timestamp until which the entity idles before moving again.
    pub idle_until: f64,
}

impl EntityInstanceState {
    pub fn new(max_harvests: u32) -> Self {
        Self {
            harvests_remaining: max_harvests,
            cooldown_until: 0.0,
            depleted_until: 0.0,
            current_x: 0.0,
            velocity_x: 0.0,
            facing: Direction::Right,
            wander_origin: 0.0,
            idle_until: 0.0,
        }
    }
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
    pub cooldown_remaining: Option<f64>,
    pub depleted: bool,
    pub facing: Direction,
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
    pub actionable: bool,
}

/// Floating feedback text after pickup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickupFeedback {
    pub id: u64,
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
    fn entity_instance_state_creation() {
        let state = EntityInstanceState::new(3);
        assert_eq!(state.harvests_remaining, 3);
        assert_eq!(state.cooldown_until, 0.0);
        assert_eq!(state.depleted_until, 0.0);
    }

    #[test]
    fn entity_def_deserializes_movement_fields() {
        let json = r#"{
            "name": "Chicken",
            "verb": "Squeeze",
            "yields": [{ "item": "grain", "min": 1, "max": 2 }],
            "cooldownSecs": 8.0,
            "maxHarvests": 2,
            "respawnSecs": 45.0,
            "spriteClass": "npc_chicken",
            "interactRadius": 60,
            "walkSpeed": 40.0,
            "wanderRadius": 120.0
        }"#;
        let def: EntityDef = serde_json::from_str(json).unwrap();
        assert!((def.walk_speed.unwrap() - 40.0).abs() < 0.01);
        assert!((def.wander_radius.unwrap() - 120.0).abs() < 0.01);
        assert!(def.bob_amplitude.is_none());
        assert!(def.bob_frequency.is_none());
    }

    #[test]
    fn entity_def_deserializes_bob_fields() {
        let json = r#"{
            "name": "Butterfly",
            "verb": "Milk",
            "yields": [{ "item": "milk", "min": 1, "max": 1 }],
            "cooldownSecs": 0.0,
            "maxHarvests": 1,
            "respawnSecs": 20.0,
            "spriteClass": "npc_butterfly",
            "interactRadius": 90,
            "walkSpeed": 25.0,
            "wanderRadius": 150.0,
            "bobAmplitude": 15.0,
            "bobFrequency": 1.5
        }"#;
        let def: EntityDef = serde_json::from_str(json).unwrap();
        assert!((def.bob_amplitude.unwrap() - 15.0).abs() < 0.01);
        assert!((def.bob_frequency.unwrap() - 1.5).abs() < 0.01);
    }

    #[test]
    fn entity_def_omits_movement_fields_for_static() {
        let json = r#"{
            "name": "Fruit Tree",
            "verb": "Harvest",
            "yields": [{ "item": "cherry", "min": 1, "max": 3 }],
            "cooldownSecs": 5.0,
            "maxHarvests": 3,
            "respawnSecs": 30.0,
            "spriteClass": "tree_fruit",
            "interactRadius": 80
        }"#;
        let def: EntityDef = serde_json::from_str(json).unwrap();
        assert!(def.walk_speed.is_none());
        assert!(def.wander_radius.is_none());
        assert!(def.bob_amplitude.is_none());
        assert!(def.bob_frequency.is_none());
    }

    #[test]
    fn entity_instance_state_has_movement_fields() {
        use crate::avatar::types::Direction;

        let state = EntityInstanceState::new(3);
        assert_eq!(state.current_x, 0.0);
        assert_eq!(state.velocity_x, 0.0);
        assert_eq!(state.wander_origin, 0.0);
        assert_eq!(state.idle_until, 0.0);
        assert!(matches!(state.facing, Direction::Right));
    }

    #[test]
    fn world_entity_frame_serializes_facing() {
        use crate::avatar::types::Direction;

        let frame = WorldEntityFrame {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            name: "Fruit Tree".into(),
            sprite_class: "tree_fruit".into(),
            x: 100.0,
            y: -2.0,
            cooldown_remaining: None,
            depleted: false,
            facing: Direction::Left,
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains(r#""facing":"left""#));
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
