use crate::item::inventory::Inventory;
use crate::item::types::ItemDefs;
use crate::quest::types::{
    DialogueChoiceResult, DialogueCondition, DialogueDefs, DialogueEffect,
    DialogueFrame, DialogueOptionFrame, QuestDefs, QuestObjective, QuestProgress,
};
use crate::skill::types::SkillProgress;

/// Evaluate the start node of a dialogue tree, filtering options by conditions.
/// Returns None if the tree or start node doesn't exist.
pub fn evaluate_start(
    tree_id: &str,
    defs: &DialogueDefs,
    quest_progress: &QuestProgress,
    quest_defs: &QuestDefs,
    inventory: &Inventory,
    skill_progress: &SkillProgress,
    entity_id: &str,
) -> Option<DialogueFrame> {
    let tree = defs.get(tree_id)?;
    let node = tree.nodes.get(&tree.start_node)?;
    Some(build_frame(
        node,
        quest_progress,
        quest_defs,
        inventory,
        skill_progress,
        entity_id,
    ))
}

/// Process a player's dialogue choice. Applies effects and returns the next
/// frame or an end signal with feedback messages.
#[allow(clippy::too_many_arguments)]
pub fn evaluate_choice(
    tree_id: &str,
    current_node: &str,
    option_index: usize,
    defs: &DialogueDefs,
    quest_progress: &mut QuestProgress,
    quest_defs: &QuestDefs,
    inventory: &mut Inventory,
    skill_progress: &SkillProgress,
    currants: &mut u64,
    imagination: &mut u64,
    item_defs: &ItemDefs,
    entity_id: &str,
) -> DialogueChoiceResult {
    let tree = match defs.get(tree_id) {
        Some(t) => t,
        None => {
            return DialogueChoiceResult::End {
                feedback: vec!["Dialogue tree not found.".to_string()],
            }
        }
    };
    let node = match tree.nodes.get(current_node) {
        Some(n) => n,
        None => {
            return DialogueChoiceResult::End {
                feedback: vec!["Dialogue node not found.".to_string()],
            }
        }
    };

    // option_index is the index into the ORIGINAL options list (before filtering)
    let option = match node.options.get(option_index) {
        Some(o) => o,
        None => {
            return DialogueChoiceResult::End {
                feedback: vec!["Invalid option.".to_string()],
            }
        }
    };

    // Re-validate that the selected option's conditions are still met.
    // The frontend only shows filtered options, but a direct IPC call
    // could pass a hidden option's index.
    let conditions_met = option.conditions.iter().all(|c| {
        check_condition(c, quest_progress, quest_defs, inventory, skill_progress)
    });
    if !conditions_met {
        return DialogueChoiceResult::End {
            feedback: vec!["Option not available.".to_string()],
        };
    }

    // Apply effects
    let mut feedback = Vec::new();
    for effect in &option.effects {
        let msgs = apply_effect(
            effect,
            quest_progress,
            quest_defs,
            inventory,
            skill_progress,
            currants,
            imagination,
            item_defs,
        );
        feedback.extend(msgs);
    }

    // Navigate to next node or end
    match &option.next {
        Some(next_id) => {
            if let Some(next_node) = tree.nodes.get(next_id) {
                let frame = build_frame(
                    next_node,
                    quest_progress,
                    quest_defs,
                    inventory,
                    skill_progress,
                    entity_id,
                );
                DialogueChoiceResult::Continue {
                    frame,
                    feedback,
                    next_node_id: next_id.clone(),
                }
            } else {
                DialogueChoiceResult::End { feedback }
            }
        }
        None => DialogueChoiceResult::End { feedback },
    }
}

/// Check if a dialogue condition is satisfied.
pub fn check_condition(
    condition: &DialogueCondition,
    quest_progress: &QuestProgress,
    quest_defs: &QuestDefs,
    inventory: &Inventory,
    skill_progress: &SkillProgress,
) -> bool {
    match condition {
        DialogueCondition::QuestNotStarted { quest_id } => {
            !quest_progress.active.contains_key(quest_id)
                && !quest_progress.completed.contains(quest_id)
        }
        DialogueCondition::QuestActive { quest_id } => {
            // Active but NOT ready for turn-in — avoids showing both
            // "in progress" and "turn in" options simultaneously.
            quest_progress.active.contains_key(quest_id)
                && !crate::quest::tracker::is_quest_ready(
                    quest_id,
                    quest_progress,
                    quest_defs,
                    inventory,
                    skill_progress,
                )
        }
        DialogueCondition::QuestReady { quest_id } => {
            crate::quest::tracker::is_quest_ready(
                quest_id,
                quest_progress,
                quest_defs,
                inventory,
                skill_progress,
            )
        }
        DialogueCondition::QuestComplete { quest_id } => {
            quest_progress.completed.contains(quest_id)
        }
        DialogueCondition::HasItem { item_id, count } => {
            inventory.count_item(item_id) >= *count
        }
        DialogueCondition::SkillLearned { skill_id } => {
            skill_progress.learned.contains(skill_id)
        }
    }
}

/// Apply a dialogue effect, mutating game state. Returns feedback messages.
#[allow(clippy::too_many_arguments)]
pub fn apply_effect(
    effect: &DialogueEffect,
    quest_progress: &mut QuestProgress,
    quest_defs: &QuestDefs,
    inventory: &mut Inventory,
    skill_progress: &SkillProgress,
    currants: &mut u64,
    imagination: &mut u64,
    item_defs: &ItemDefs,
) -> Vec<String> {
    let mut feedback = Vec::new();
    match effect {
        DialogueEffect::StartQuest { quest_id } => {
            if let Ok(()) =
                crate::quest::tracker::start_quest(quest_id, quest_defs, quest_progress)
            {
                let name = quest_defs
                    .get(quest_id)
                    .map(|d| d.name.as_str())
                    .unwrap_or(quest_id);
                feedback.push(format!("Quest started: {name}"));
            }
        }
        DialogueEffect::CompleteQuest { quest_id } => {
            if let Some(def) = quest_defs.get(quest_id) {
                // Guard: only complete a quest that is active with all objectives met
                if !crate::quest::tracker::is_quest_ready(
                    quest_id,
                    quest_progress,
                    quest_defs,
                    inventory,
                    skill_progress,
                ) {
                    return feedback;
                }
                // Remove items for deliver objectives
                for obj in &def.objectives {
                    if let QuestObjective::Deliver {
                        item_id, count, ..
                    } = obj
                    {
                        inventory.remove_item(item_id, *count);
                    }
                }
                // Grant rewards
                if def.rewards.currants > 0 {
                    *currants += def.rewards.currants;
                    feedback.push(format!("+{} Currants", def.rewards.currants));
                }
                if def.rewards.imagination > 0 {
                    *imagination += def.rewards.imagination;
                    feedback.push(format!("+{} Imagination", def.rewards.imagination));
                }
                for reward_item in &def.rewards.items {
                    let overflow =
                        inventory.add(&reward_item.item_id, reward_item.count, item_defs);
                    let name = item_defs
                        .get(&reward_item.item_id)
                        .map(|d| d.name.as_str())
                        .unwrap_or(&reward_item.item_id);
                    let delivered = reward_item.count - overflow;
                    if delivered > 0 {
                        feedback.push(format!("+{} {}", delivered, name));
                    }
                    if overflow > 0 {
                        feedback.push(format!("Inventory full! Lost {} {}", overflow, name));
                    }
                }
                // Move quest to completed
                quest_progress.active.remove(quest_id);
                quest_progress.completed.push(quest_id.clone());
                feedback.push(format!("Quest complete: {}", def.name));
            }
        }
        DialogueEffect::GiveItem { item_id, count } => {
            let overflow = inventory.add(item_id, *count, item_defs);
            let name = item_defs
                .get(item_id)
                .map(|d| d.name.as_str())
                .unwrap_or(item_id);
            let delivered = count - overflow;
            if delivered > 0 {
                feedback.push(format!("+{} {}", delivered, name));
            }
            if overflow > 0 {
                feedback.push(format!("Inventory full! Lost {} {}", overflow, name));
            }
        }
        DialogueEffect::RemoveItem { item_id, count } => {
            inventory.remove_item(item_id, *count);
        }
        DialogueEffect::GiveCurrants { amount } => {
            *currants += amount;
            feedback.push(format!("+{} Currants", amount));
        }
        DialogueEffect::GiveImagination { amount } => {
            *imagination += amount;
            feedback.push(format!("+{} Imagination", amount));
        }
    }
    feedback
}

// ---- Internal helpers ----

fn build_frame(
    node: &crate::quest::types::DialogueNodeDef,
    quest_progress: &QuestProgress,
    quest_defs: &QuestDefs,
    inventory: &Inventory,
    skill_progress: &SkillProgress,
    entity_id: &str,
) -> DialogueFrame {
    let options = node
        .options
        .iter()
        .enumerate()
        .filter(|(_, opt)| {
            opt.conditions.iter().all(|c| {
                check_condition(c, quest_progress, quest_defs, inventory, skill_progress)
            })
        })
        .map(|(i, opt)| DialogueOptionFrame {
            text: opt.text.clone(),
            index: i,
        })
        .collect();

    DialogueFrame {
        speaker: node.speaker.clone(),
        text: node.text.clone(),
        options,
        entity_id: entity_id.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;
    use crate::quest::types::{
        ActiveQuest, DialogueNodeDef, DialogueOptionDef, DialogueTreeDef, QuestDef,
        QuestObjective, QuestRewards,
    };
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
                energy_value: Some(12),
            },
        );
        defs
    }

    fn test_quest_defs() -> QuestDefs {
        let mut defs = HashMap::new();
        defs.insert(
            "test_fetch".to_string(),
            QuestDef {
                id: "test_fetch".to_string(),
                name: "Test Fetch".to_string(),
                description: "Fetch 3 cherries.".to_string(),
                objectives: vec![QuestObjective::Fetch {
                    item_id: "cherry".to_string(),
                    count: 3,
                    description: "Collect 3 cherries".to_string(),
                }],
                rewards: QuestRewards {
                    currants: 25,
                    imagination: 10,
                    items: vec![],
                },
                turn_in_npc: "greeter".to_string(),
            },
        );
        defs.insert(
            "test_skill".to_string(),
            QuestDef {
                id: "test_skill".to_string(),
                name: "Test Skill".to_string(),
                description: "Learn cooking.".to_string(),
                objectives: vec![QuestObjective::LearnSkill {
                    skill_id: "cooking_1".to_string(),
                    description: "Learn Cooking I".to_string(),
                }],
                rewards: QuestRewards {
                    currants: 50,
                    imagination: 0,
                    items: vec![],
                },
                turn_in_npc: "greeter".to_string(),
            },
        );
        defs
    }

    fn simple_dialogue_defs() -> DialogueDefs {
        let mut nodes = HashMap::new();
        nodes.insert(
            "start".to_string(),
            DialogueNodeDef {
                speaker: "NPC".to_string(),
                text: "Hello!".to_string(),
                options: vec![
                    DialogueOptionDef {
                        text: "Quest offer".to_string(),
                        next: Some("offer".to_string()),
                        conditions: vec![DialogueCondition::QuestNotStarted {
                            quest_id: "test_fetch".to_string(),
                        }],
                        effects: vec![],
                    },
                    DialogueOptionDef {
                        text: "Turn in".to_string(),
                        next: Some("turnin".to_string()),
                        conditions: vec![DialogueCondition::QuestReady {
                            quest_id: "test_fetch".to_string(),
                        }],
                        effects: vec![],
                    },
                    DialogueOptionDef {
                        text: "Bye".to_string(),
                        next: None,
                        conditions: vec![],
                        effects: vec![],
                    },
                ],
            },
        );
        nodes.insert(
            "offer".to_string(),
            DialogueNodeDef {
                speaker: "NPC".to_string(),
                text: "Get me 3 cherries!".to_string(),
                options: vec![DialogueOptionDef {
                    text: "Sure!".to_string(),
                    next: None,
                    conditions: vec![],
                    effects: vec![DialogueEffect::StartQuest {
                        quest_id: "test_fetch".to_string(),
                    }],
                }],
            },
        );
        nodes.insert(
            "turnin".to_string(),
            DialogueNodeDef {
                speaker: "NPC".to_string(),
                text: "Thanks for the cherries!".to_string(),
                options: vec![DialogueOptionDef {
                    text: "You're welcome!".to_string(),
                    next: None,
                    conditions: vec![],
                    effects: vec![DialogueEffect::CompleteQuest {
                        quest_id: "test_fetch".to_string(),
                    }],
                }],
            },
        );

        let mut defs = HashMap::new();
        defs.insert(
            "test_tree".to_string(),
            DialogueTreeDef {
                start_node: "start".to_string(),
                nodes,
            },
        );
        defs
    }

    #[test]
    fn filter_options_by_conditions() {
        let defs = simple_dialogue_defs();
        let quest_defs = test_quest_defs();
        let inventory = Inventory::new(16);
        let skill_progress = SkillProgress::default();
        let quest_progress = QuestProgress::default();

        let frame =
            evaluate_start("test_tree", &defs, &quest_progress, &quest_defs, &inventory, &skill_progress, "npc_1")
                .unwrap();

        // QuestNotStarted is true, QuestReady is false → 2 options visible
        assert_eq!(frame.options.len(), 2);
        assert_eq!(frame.options[0].text, "Quest offer");
        assert_eq!(frame.options[0].index, 0);
        assert_eq!(frame.options[1].text, "Bye");
        assert_eq!(frame.options[1].index, 2);
    }

    #[test]
    fn option_with_no_conditions_always_visible() {
        let defs = simple_dialogue_defs();
        let quest_defs = test_quest_defs();
        let inventory = Inventory::new(16);
        let skill_progress = SkillProgress::default();

        // Quest active — "Bye" should always be visible
        let mut quest_progress = QuestProgress::default();
        quest_progress.active.insert(
            "test_fetch".to_string(),
            ActiveQuest {
                quest_id: "test_fetch".to_string(),
                objective_progress: vec![0],
            },
        );

        let frame =
            evaluate_start("test_tree", &defs, &quest_progress, &quest_defs, &inventory, &skill_progress, "npc_1")
                .unwrap();

        // QuestNotStarted is false (active), QuestReady is false (0/3) → only "Bye"
        assert_eq!(frame.options.len(), 1);
        assert_eq!(frame.options[0].text, "Bye");
    }

    #[test]
    fn quest_not_started_condition() {
        let quest_defs = test_quest_defs();
        let inventory = Inventory::new(16);
        let skill_progress = SkillProgress::default();

        let condition = DialogueCondition::QuestNotStarted {
            quest_id: "test_fetch".to_string(),
        };

        // Not started → true
        let progress = QuestProgress::default();
        assert!(check_condition(&condition, &progress, &quest_defs, &inventory, &skill_progress));

        // Active → false
        let mut progress = QuestProgress::default();
        progress.active.insert(
            "test_fetch".to_string(),
            ActiveQuest {
                quest_id: "test_fetch".to_string(),
                objective_progress: vec![0],
            },
        );
        assert!(!check_condition(&condition, &progress, &quest_defs, &inventory, &skill_progress));

        // Completed → false
        let mut progress = QuestProgress::default();
        progress.completed.push("test_fetch".to_string());
        assert!(!check_condition(&condition, &progress, &quest_defs, &inventory, &skill_progress));
    }

    #[test]
    fn quest_ready_condition() {
        let quest_defs = test_quest_defs();
        let item_defs = test_item_defs();
        let skill_progress = SkillProgress::default();

        let condition = DialogueCondition::QuestReady {
            quest_id: "test_fetch".to_string(),
        };

        // Not active → false
        let progress = QuestProgress::default();
        let inventory = Inventory::new(16);
        assert!(!check_condition(&condition, &progress, &quest_defs, &inventory, &skill_progress));

        // Active but not enough items → false
        let mut progress = QuestProgress::default();
        progress.active.insert(
            "test_fetch".to_string(),
            ActiveQuest {
                quest_id: "test_fetch".to_string(),
                objective_progress: vec![0],
            },
        );
        let mut inventory = Inventory::new(16);
        inventory.add("cherry", 2, &item_defs);
        assert!(!check_condition(&condition, &progress, &quest_defs, &inventory, &skill_progress));

        // Active with enough items → true
        inventory.add("cherry", 1, &item_defs);
        assert!(check_condition(&condition, &progress, &quest_defs, &inventory, &skill_progress));
    }

    #[test]
    fn apply_start_quest_effect() {
        let quest_defs = test_quest_defs();
        let item_defs = test_item_defs();
        let mut quest_progress = QuestProgress::default();
        let mut inventory = Inventory::new(16);
        let mut currants = 0u64;
        let mut imagination = 0u64;

        let effect = DialogueEffect::StartQuest {
            quest_id: "test_fetch".to_string(),
        };

        let feedback = apply_effect(
            &effect,
            &mut quest_progress,
            &quest_defs,
            &mut inventory,
            &SkillProgress::default(),
            &mut currants,
            &mut imagination,
            &item_defs,
        );

        assert!(quest_progress.active.contains_key("test_fetch"));
        assert_eq!(quest_progress.active["test_fetch"].objective_progress, vec![0]);
        assert!(feedback.iter().any(|f| f.contains("Quest started")));
    }

    #[test]
    fn apply_complete_quest_effect() {
        let quest_defs = test_quest_defs();
        let item_defs = test_item_defs();
        let mut quest_progress = QuestProgress::default();
        quest_progress.active.insert(
            "test_fetch".to_string(),
            ActiveQuest {
                quest_id: "test_fetch".to_string(),
                objective_progress: vec![3],
            },
        );
        let mut inventory = Inventory::new(16);
        // Must have enough items for is_quest_ready to pass
        inventory.add("cherry", 3, &item_defs);
        let mut currants = 0u64;
        let mut imagination = 0u64;

        let effect = DialogueEffect::CompleteQuest {
            quest_id: "test_fetch".to_string(),
        };

        let feedback = apply_effect(
            &effect,
            &mut quest_progress,
            &quest_defs,
            &mut inventory,
            &SkillProgress::default(),
            &mut currants,
            &mut imagination,
            &item_defs,
        );

        assert!(!quest_progress.active.contains_key("test_fetch"));
        assert!(quest_progress.completed.contains(&"test_fetch".to_string()));
        assert_eq!(currants, 25);
        assert_eq!(imagination, 10);
        assert!(feedback.iter().any(|f| f.contains("Quest complete")));
        assert!(feedback.iter().any(|f| f.contains("+25 Currants")));
    }

    #[test]
    fn apply_give_item_effect() {
        let quest_defs = test_quest_defs();
        let item_defs = test_item_defs();
        let mut quest_progress = QuestProgress::default();
        let mut inventory = Inventory::new(16);
        let mut currants = 0u64;
        let mut imagination = 0u64;

        let effect = DialogueEffect::GiveItem {
            item_id: "cherry".to_string(),
            count: 5,
        };

        let feedback = apply_effect(
            &effect,
            &mut quest_progress,
            &quest_defs,
            &mut inventory,
            &SkillProgress::default(),
            &mut currants,
            &mut imagination,
            &item_defs,
        );

        assert_eq!(inventory.count_item("cherry"), 5);
        assert!(feedback.iter().any(|f| f.contains("+5 Cherry")));
    }

    #[test]
    fn navigate_dialogue_tree() {
        let defs = simple_dialogue_defs();
        let quest_defs = test_quest_defs();
        let item_defs = test_item_defs();
        let mut quest_progress = QuestProgress::default();
        let mut inventory = Inventory::new(16);
        let skill_progress = SkillProgress::default();
        let mut currants = 0u64;
        let mut imagination = 0u64;

        // Start → choose "Quest offer" (index 0) → navigate to "offer" node
        let result = evaluate_choice(
            "test_tree",
            "start",
            0,
            &defs,
            &mut quest_progress,
            &quest_defs,
            &mut inventory,
            &skill_progress,
            &mut currants,
            &mut imagination,
            &item_defs,
            "npc_1",
        );

        match result {
            DialogueChoiceResult::Continue { frame, .. } => {
                assert_eq!(frame.text, "Get me 3 cherries!");
                assert_eq!(frame.options.len(), 1);
                assert_eq!(frame.options[0].text, "Sure!");
            }
            DialogueChoiceResult::End { .. } => panic!("Expected Continue"),
        }
    }

    #[test]
    fn end_dialogue_on_none_next() {
        let defs = simple_dialogue_defs();
        let quest_defs = test_quest_defs();
        let item_defs = test_item_defs();
        let mut quest_progress = QuestProgress::default();
        let mut inventory = Inventory::new(16);
        let skill_progress = SkillProgress::default();
        let mut currants = 0u64;
        let mut imagination = 0u64;

        // Start → choose "Bye" (index 2, the unconditional option) → End
        let result = evaluate_choice(
            "test_tree",
            "start",
            2,
            &defs,
            &mut quest_progress,
            &quest_defs,
            &mut inventory,
            &skill_progress,
            &mut currants,
            &mut imagination,
            &item_defs,
            "npc_1",
        );

        match result {
            DialogueChoiceResult::End { feedback } => {
                assert!(feedback.is_empty()); // "Bye" has no effects
            }
            DialogueChoiceResult::Continue { .. } => panic!("Expected End"),
        }
    }

    #[test]
    fn start_quest_via_dialogue_choice() {
        let defs = simple_dialogue_defs();
        let quest_defs = test_quest_defs();
        let item_defs = test_item_defs();
        let mut quest_progress = QuestProgress::default();
        let mut inventory = Inventory::new(16);
        let skill_progress = SkillProgress::default();
        let mut currants = 0u64;
        let mut imagination = 0u64;

        // Navigate to offer node, then choose "Sure!" (index 0) which starts the quest
        let result = evaluate_choice(
            "test_tree",
            "offer",
            0,
            &defs,
            &mut quest_progress,
            &quest_defs,
            &mut inventory,
            &skill_progress,
            &mut currants,
            &mut imagination,
            &item_defs,
            "npc_1",
        );

        match result {
            DialogueChoiceResult::End { feedback } => {
                assert!(feedback.iter().any(|f| f.contains("Quest started")));
            }
            DialogueChoiceResult::Continue { .. } => panic!("Expected End"),
        }
        assert!(quest_progress.active.contains_key("test_fetch"));
    }
}
