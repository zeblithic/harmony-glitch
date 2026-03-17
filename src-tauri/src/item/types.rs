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
