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
    #[serde(default)]
    pub base_cost: Option<u32>,
    #[serde(default)]
    pub energy_value: Option<u32>,
}

/// A stack of items in inventory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub playlist: Option<Vec<String>>,
    pub audio_radius: Option<f64>,
    #[serde(default)]
    pub store: Option<String>,
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
    #[serde(default)]
    pub energy_cost: f64,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeItem {
    pub item: String,
    pub count: u32,
}

pub type RecipeDefs = HashMap<String, RecipeDef>;

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

/// Error from a crafting attempt.
#[derive(Debug, Clone)]
pub enum CraftError {
    MissingInput { item: String, need: u32, have: u32 },
    MissingTool { item: String },
    NoRoom,
    UnknownRecipe,
    InsufficientEnergy,
    AlreadyCrafting,
}

/// Convert an item ID like "cherry_pie" to "Cherry Pie" for display.
fn title_case(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl std::fmt::Display for CraftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CraftError::MissingInput { item, need, have } => {
                write!(
                    f,
                    "Need {} {} but only have {}",
                    need,
                    title_case(item),
                    have
                )
            }
            CraftError::MissingTool { item } => {
                write!(f, "Missing tool: {}", title_case(item))
            }
            CraftError::NoRoom => write!(f, "Inventory full"),
            CraftError::UnknownRecipe => write!(f, "Unknown recipe"),
            CraftError::InsufficientEnergy => write!(f, "Too tired to craft"),
            CraftError::AlreadyCrafting => write!(f, "Already crafting"),
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

/// An in-progress craft tracked by the game tick loop.
#[derive(Debug, Clone)]
pub struct ActiveCraft {
    pub recipe_id: String,
    pub started_at: f64,
    pub complete_at: f64,
    pub pending_outputs: Vec<CraftOutput>,
}

/// Progress data sent to frontend for the crafting progress bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveCraftFrame {
    pub recipe_id: String,
    pub progress: f64,
    pub remaining_secs: f64,
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
    pub energy_value: Option<u32>,
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
    pub entity_id: Option<String>,
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
    /// Optional hex color override (e.g. "#c084fc"). When present,
    /// the renderer uses this instead of the success-based green/red.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
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
    fn entity_def_deserializes_jukebox_fields() {
        let json = r#"{
            "name": "Tavern Jukebox",
            "verb": "Listen",
            "yields": [],
            "cooldownSecs": 0,
            "maxHarvests": 0,
            "respawnSecs": 0.0,
            "spriteClass": "jukebox",
            "interactRadius": 100,
            "playlist": ["track-a", "track-b"],
            "audioRadius": 400
        }"#;
        let def: EntityDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.playlist.as_ref().unwrap(), &vec!["track-a".to_string(), "track-b".to_string()]);
        assert!((def.audio_radius.unwrap() - 400.0).abs() < 0.01);
    }

    #[test]
    fn entity_def_jukebox_fields_default_to_none() {
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
        assert!(def.playlist.is_none());
        assert!(def.audio_radius.is_none());
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
            inputs: vec![RecipeItem {
                item: "cherry".into(),
                count: 5,
            }],
            tools: vec![],
            outputs: vec![RecipeItem {
                item: "cherry_pie".into(),
                count: 1,
            }],
            duration_secs: 10.0,
            energy_cost: 15.0,
            category: "food".into(),
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains(r#""id":"cherry_pie""#));
        assert!(json.contains("durationSecs"));
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
            base_cost: None,
            energy_value: None,
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("stackLimit"));
        assert!(!json.contains("stack_limit"));
        // id is skip-serialized
        assert!(!json.contains(r#""id""#));
    }

    #[test]
    fn item_def_with_base_cost() {
        let json = r#"{
            "name": "Cherry",
            "description": "A cherry.",
            "category": "food",
            "stackLimit": 50,
            "icon": "cherry",
            "baseCost": 3
        }"#;
        let def: ItemDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.base_cost, Some(3));
    }

    #[test]
    fn item_def_without_base_cost() {
        let json = r#"{
            "name": "Cherry",
            "description": "A cherry.",
            "category": "food",
            "stackLimit": 50,
            "icon": "cherry"
        }"#;
        let def: ItemDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.base_cost, None);
    }

    #[test]
    fn entity_def_with_store() {
        let json = r#"{
            "name": "Grocery Vendor",
            "verb": "Shop",
            "yields": [],
            "cooldownSecs": 0,
            "maxHarvests": 0,
            "respawnSecs": 0,
            "spriteClass": "vendor",
            "interactRadius": 100,
            "store": "grocery"
        }"#;
        let def: EntityDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.store, Some("grocery".to_string()));
    }

    #[test]
    fn entity_def_without_store() {
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
        assert_eq!(def.store, None);
    }

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
}
