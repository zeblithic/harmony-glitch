use crate::quest::types::{DialogueDefs, QuestDefs};

/// Parse dialogue tree definitions from JSON string.
/// The JSON is a map of tree_id -> DialogueTreeDef.
pub fn parse_dialogue_defs(json: &str) -> Result<DialogueDefs, String> {
    serde_json::from_str(json).map_err(|e| format!("Failed to parse dialogues.json: {e}"))
}

/// Parse quest definitions from JSON string.
/// The JSON is a map of quest_id -> QuestDef. We set each QuestDef.id from its map key.
pub fn parse_quest_defs(json: &str) -> Result<QuestDefs, String> {
    let mut raw: QuestDefs =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse quests.json: {e}"))?;
    for (key, def) in raw.iter_mut() {
        def.id = key.clone();
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dialogue_defs_from_json() {
        let json = r#"{
            "test_npc": {
                "startNode": "greeting",
                "nodes": {
                    "greeting": {
                        "speaker": "Test NPC",
                        "text": "Hello!",
                        "options": [
                            {
                                "text": "Got any work?",
                                "next": "quest_offer",
                                "conditions": [{ "type": "questNotStarted", "questId": "test_quest" }]
                            },
                            {
                                "text": "Bye!",
                                "next": null
                            }
                        ]
                    },
                    "quest_offer": {
                        "speaker": "Test NPC",
                        "text": "Fetch me 5 cherries!",
                        "options": [
                            {
                                "text": "Sure!",
                                "next": null,
                                "effects": [{ "type": "startQuest", "questId": "test_quest" }]
                            }
                        ]
                    }
                }
            }
        }"#;
        let defs = parse_dialogue_defs(json).unwrap();
        assert_eq!(defs.len(), 1);
        let tree = &defs["test_npc"];
        assert_eq!(tree.start_node, "greeting");
        assert_eq!(tree.nodes.len(), 2);
        let greeting = &tree.nodes["greeting"];
        assert_eq!(greeting.speaker, "Test NPC");
        assert_eq!(greeting.options.len(), 2);
        assert_eq!(greeting.options[0].conditions.len(), 1);
        assert!(greeting.options[1].next.is_none());
    }

    #[test]
    fn parse_quest_defs_from_json() {
        let json = r#"{
            "test_quest": {
                "name": "Test Quest",
                "description": "A test quest.",
                "objectives": [
                    { "type": "fetch", "itemId": "cherry", "count": 5, "description": "Collect 5 cherries" }
                ],
                "rewards": {
                    "currants": 25,
                    "imagination": 20,
                    "items": []
                },
                "turnInNpc": "greeter"
            }
        }"#;
        let defs = parse_quest_defs(json).unwrap();
        assert_eq!(defs.len(), 1);
        let quest = &defs["test_quest"];
        assert_eq!(quest.id, "test_quest");
        assert_eq!(quest.name, "Test Quest");
        assert_eq!(quest.objectives.len(), 1);
        assert_eq!(quest.rewards.currants, 25);
        assert_eq!(quest.rewards.imagination, 20);
        assert_eq!(quest.turn_in_npc, "greeter");
    }

    #[test]
    fn parse_quest_with_multiple_objectives() {
        let json = r#"{
            "multi": {
                "name": "Multi",
                "description": "Multiple objectives.",
                "objectives": [
                    { "type": "learnSkill", "skillId": "cooking_1", "description": "Learn Cooking I" },
                    { "type": "craft", "recipeId": "bread", "count": 1, "description": "Craft 1 Bread" }
                ],
                "rewards": { "currants": 50, "imagination": 40, "items": [{ "itemId": "cherry", "count": 10 }] },
                "turnInNpc": "greeter"
            }
        }"#;
        let defs = parse_quest_defs(json).unwrap();
        let quest = &defs["multi"];
        assert_eq!(quest.objectives.len(), 2);
        assert_eq!(quest.rewards.items.len(), 1);
        assert_eq!(quest.rewards.items[0].item_id, "cherry");
        assert_eq!(quest.rewards.items[0].count, 10);
    }

    #[test]
    fn parse_bundled_dialogues_json() {
        let json = include_str!("../../../assets/dialogues.json");
        let defs = parse_dialogue_defs(json).unwrap();
        assert!(defs.contains_key("greeter_default"));
        assert!(defs.contains_key("handyman_default"));
        // Verify greeter has expected start node
        let greeter = &defs["greeter_default"];
        assert!(!greeter.start_node.is_empty());
        assert!(greeter.nodes.contains_key(&greeter.start_node));
    }

    #[test]
    fn parse_bundled_quests_json() {
        let json = include_str!("../../../assets/quests.json");
        let defs = parse_quest_defs(json).unwrap();
        assert_eq!(defs.len(), 3);
        assert!(defs.contains_key("cherry_picking"));
        assert!(defs.contains_key("baking_basics"));
        assert!(defs.contains_key("special_delivery"));
    }
}
