use serde::{Deserialize, Serialize};

/// Semantic audio event emitted by game logic.
/// The frontend maps these to actual sound files via a sound kit manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AudioEvent {
    #[serde(rename_all = "camelCase")]
    ItemPickup {
        item_id: String,
    },
    #[serde(rename_all = "camelCase")]
    CraftSuccess {
        recipe_id: String,
    },
    ActionFailed,
    Jump,
    Land,
    TransitionStart,
    TransitionComplete,
    #[serde(rename_all = "camelCase")]
    EntityInteract {
        entity_type: String,
    },
    #[serde(rename_all = "camelCase")]
    StreetChanged {
        street_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Footstep {
        surface: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_item_pickup() {
        let event = AudioEvent::ItemPickup {
            item_id: "cherry".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"itemPickup""#));
        assert!(json.contains(r#""itemId":"cherry""#));
    }

    #[test]
    fn serialize_action_failed() {
        let event = AudioEvent::ActionFailed;
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, r#"{"type":"actionFailed"}"#);
    }

    #[test]
    fn serialize_jump() {
        let event = AudioEvent::Jump;
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, r#"{"type":"jump"}"#);
    }

    #[test]
    fn serialize_street_changed() {
        let event = AudioEvent::StreetChanged {
            street_id: "LADEMO001".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"streetChanged""#));
        assert!(json.contains(r#""streetId":"LADEMO001""#));
    }

    #[test]
    fn serialize_entity_interact() {
        let event = AudioEvent::EntityInteract {
            entity_type: "fruit_tree".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"entityInteract""#));
        assert!(json.contains(r#""entityType":"fruit_tree""#));
    }

    #[test]
    fn serialize_craft_success() {
        let event = AudioEvent::CraftSuccess {
            recipe_id: "cherry_pie".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"craftSuccess""#));
        assert!(json.contains(r#""recipeId":"cherry_pie""#));
    }

    #[test]
    fn serialize_footstep() {
        let event = AudioEvent::Footstep {
            surface: "grass".into(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"footstep""#));
        assert!(json.contains(r#""surface":"grass""#));
    }

    #[test]
    fn roundtrip_all_variants() {
        let events = vec![
            AudioEvent::ItemPickup {
                item_id: "cherry".into(),
            },
            AudioEvent::CraftSuccess {
                recipe_id: "bread".into(),
            },
            AudioEvent::ActionFailed,
            AudioEvent::Jump,
            AudioEvent::Land,
            AudioEvent::TransitionStart,
            AudioEvent::TransitionComplete,
            AudioEvent::EntityInteract {
                entity_type: "chicken".into(),
            },
            AudioEvent::StreetChanged {
                street_id: "demo_meadow".into(),
            },
            AudioEvent::Footstep {
                surface: "stone".into(),
            },
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let deserialized: AudioEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, deserialized);
        }
    }
}
