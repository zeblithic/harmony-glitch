use crate::item::inventory::Inventory;
use crate::quest::types::{ActiveQuest, QuestDefs, QuestObjective, QuestProgress};
use crate::skill::types::SkillProgress;

/// Update objective progress for all active quests based on current game state.
/// Called each tick — lightweight scan of a handful of active quests.
pub fn tick_quest_progress(
    quest_progress: &mut QuestProgress,
    quest_defs: &QuestDefs,
    inventory: &Inventory,
    skill_progress: &SkillProgress,
    street_id: &str,
) {
    for (quest_id, active) in quest_progress.active.iter_mut() {
        let Some(def) = quest_defs.get(quest_id) else {
            continue;
        };
        // Track how many of each item have been allocated to earlier objectives
        // so cumulative display matches is_quest_ready's cumulative check.
        let mut allocated: std::collections::HashMap<&str, u32> =
            std::collections::HashMap::new();
        for (i, objective) in def.objectives.iter().enumerate() {
            let Some(slot) = active.objective_progress.get_mut(i) else {
                continue;
            };
            match objective {
                QuestObjective::Fetch {
                    item_id, count, ..
                }
                | QuestObjective::Deliver {
                    item_id, count, ..
                } => {
                    let total = inventory.count_item(item_id);
                    let used = allocated.get(item_id.as_str()).copied().unwrap_or(0);
                    let available = total.saturating_sub(used);
                    *slot = available.min(*count);
                    *allocated.entry(item_id.as_str()).or_default() += *slot;
                }
                QuestObjective::Visit {
                    street_id: target, ..
                } => {
                    if street_id == target && *slot == 0 {
                        *slot = 1;
                    }
                }
                QuestObjective::LearnSkill { skill_id, .. } => {
                    if skill_progress.learned.contains(skill_id) {
                        *slot = 1;
                    }
                }
                // Craft objectives tracked by record_craft(), not tick.
                QuestObjective::Craft { .. } => {}
            }
        }
    }
}

/// Record a completed craft for quest objective tracking.
/// Called from tick() when a craft completes (CraftSuccess).
pub fn record_craft(quest_progress: &mut QuestProgress, quest_defs: &QuestDefs, recipe_id: &str) {
    for (quest_id, active) in quest_progress.active.iter_mut() {
        let Some(def) = quest_defs.get(quest_id) else {
            continue;
        };
        for (i, objective) in def.objectives.iter().enumerate() {
            if let QuestObjective::Craft {
                recipe_id: target,
                count,
                ..
            } = objective
            {
                if recipe_id == target && i < active.objective_progress.len() {
                    let current = active.objective_progress[i];
                    active.objective_progress[i] = (current + 1).min(*count);
                }
            }
        }
    }
}

/// Start a quest, initializing its active tracking.
pub fn start_quest(
    quest_id: &str,
    quest_defs: &QuestDefs,
    quest_progress: &mut QuestProgress,
) -> Result<(), String> {
    let def = quest_defs
        .get(quest_id)
        .ok_or_else(|| format!("Unknown quest: {quest_id}"))?;

    if quest_progress.active.contains_key(quest_id) {
        return Err(format!("Quest already active: {quest_id}"));
    }
    if quest_progress.completed.contains(&quest_id.to_string()) {
        return Err(format!("Quest already completed: {quest_id}"));
    }

    let objective_count = def.objectives.len();
    quest_progress.active.insert(
        quest_id.to_string(),
        ActiveQuest {
            quest_id: quest_id.to_string(),
            objective_progress: vec![0; objective_count],
        },
    );
    Ok(())
}

/// Check if all objectives of a quest are met.
pub fn is_quest_ready(
    quest_id: &str,
    quest_progress: &QuestProgress,
    quest_defs: &QuestDefs,
    inventory: &Inventory,
    skill_progress: &SkillProgress,
) -> bool {
    let Some(active) = quest_progress.active.get(quest_id) else {
        return false;
    };
    let Some(def) = quest_defs.get(quest_id) else {
        return false;
    };

    // Accumulate total item requirements across all Deliver/Fetch objectives
    // so overlapping item types are checked against cumulative need.
    let mut item_needs: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for objective in &def.objectives {
        match objective {
            QuestObjective::Fetch { item_id, count, .. }
            | QuestObjective::Deliver { item_id, count, .. } => {
                *item_needs.entry(item_id.as_str()).or_default() += count;
            }
            _ => {}
        }
    }
    // Check cumulative item requirements
    for (item_id, need) in &item_needs {
        if inventory.count_item(item_id) < *need {
            return false;
        }
    }
    // Check non-item objectives
    for (i, objective) in def.objectives.iter().enumerate() {
        let progress = active.objective_progress.get(i).copied().unwrap_or(0);
        let met = match objective {
            QuestObjective::Fetch { .. } | QuestObjective::Deliver { .. } => true, // already checked
            QuestObjective::Craft { count, .. } => progress >= *count,
            QuestObjective::Visit { .. } => progress >= 1,
            QuestObjective::LearnSkill { skill_id, .. } => {
                skill_progress.learned.contains(skill_id)
            }
        };
        if !met {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;
    use crate::quest::types::{QuestDef, QuestRewards};
    use std::collections::HashMap;

    fn test_item_defs() -> HashMap<String, ItemDef> {
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
        defs.insert(
            "bread".to_string(),
            ItemDef {
                id: "bread".to_string(),
                name: "Bread".to_string(),
                description: "A loaf of bread.".to_string(),
                category: "food".to_string(),
                stack_limit: 10,
                icon: "bread".to_string(),
                base_cost: Some(8),
                energy_value: Some(50),
            },
        );
        defs
    }

    fn test_quest_defs() -> QuestDefs {
        let mut defs = HashMap::new();
        defs.insert(
            "fetch_quest".to_string(),
            QuestDef {
                id: "fetch_quest".to_string(),
                name: "Fetch Quest".to_string(),
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
            "craft_quest".to_string(),
            QuestDef {
                id: "craft_quest".to_string(),
                name: "Craft Quest".to_string(),
                description: "Craft 2 bread.".to_string(),
                objectives: vec![QuestObjective::Craft {
                    recipe_id: "bread".to_string(),
                    count: 2,
                    description: "Craft 2 Bread".to_string(),
                }],
                rewards: QuestRewards {
                    currants: 50,
                    imagination: 0,
                    items: vec![],
                },
                turn_in_npc: "greeter".to_string(),
            },
        );
        defs.insert(
            "visit_quest".to_string(),
            QuestDef {
                id: "visit_quest".to_string(),
                name: "Visit Quest".to_string(),
                description: "Visit the heights.".to_string(),
                objectives: vec![QuestObjective::Visit {
                    street_id: "LADEMO002".to_string(),
                    description: "Visit Demo Heights".to_string(),
                }],
                rewards: QuestRewards {
                    currants: 10,
                    imagination: 0,
                    items: vec![],
                },
                turn_in_npc: "greeter".to_string(),
            },
        );
        defs.insert(
            "skill_quest".to_string(),
            QuestDef {
                id: "skill_quest".to_string(),
                name: "Skill Quest".to_string(),
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

    #[test]
    fn start_quest_creates_active() {
        let quest_defs = test_quest_defs();
        let mut progress = QuestProgress::default();
        start_quest("fetch_quest", &quest_defs, &mut progress).unwrap();
        assert!(progress.active.contains_key("fetch_quest"));
        assert_eq!(progress.active["fetch_quest"].objective_progress, vec![0]);
    }

    #[test]
    fn start_quest_unknown_errors() {
        let quest_defs = test_quest_defs();
        let mut progress = QuestProgress::default();
        assert!(start_quest("nonexistent", &quest_defs, &mut progress).is_err());
    }

    #[test]
    fn start_quest_already_active_errors() {
        let quest_defs = test_quest_defs();
        let mut progress = QuestProgress::default();
        start_quest("fetch_quest", &quest_defs, &mut progress).unwrap();
        assert!(start_quest("fetch_quest", &quest_defs, &mut progress).is_err());
    }

    #[test]
    fn start_quest_already_completed_errors() {
        let quest_defs = test_quest_defs();
        let mut progress = QuestProgress::default();
        progress.completed.push("fetch_quest".to_string());
        assert!(start_quest("fetch_quest", &quest_defs, &mut progress).is_err());
    }

    #[test]
    fn fetch_progress_from_inventory() {
        let quest_defs = test_quest_defs();
        let item_defs = test_item_defs();
        let skill_progress = SkillProgress::default();
        let mut progress = QuestProgress::default();
        start_quest("fetch_quest", &quest_defs, &mut progress).unwrap();

        let mut inventory = Inventory::new(16);
        inventory.add("cherry", 2, &item_defs);

        tick_quest_progress(&mut progress, &quest_defs, &inventory, &skill_progress, "LADEMO001");
        assert_eq!(progress.active["fetch_quest"].objective_progress[0], 2);

        // Add more → capped at target count
        inventory.add("cherry", 5, &item_defs);
        tick_quest_progress(&mut progress, &quest_defs, &inventory, &skill_progress, "LADEMO001");
        assert_eq!(progress.active["fetch_quest"].objective_progress[0], 3);
    }

    #[test]
    fn craft_progress_increments() {
        let quest_defs = test_quest_defs();
        let mut progress = QuestProgress::default();
        start_quest("craft_quest", &quest_defs, &mut progress).unwrap();

        record_craft(&mut progress, &quest_defs, "bread");
        assert_eq!(progress.active["craft_quest"].objective_progress[0], 1);

        record_craft(&mut progress, &quest_defs, "bread");
        assert_eq!(progress.active["craft_quest"].objective_progress[0], 2);

        // Capped at target
        record_craft(&mut progress, &quest_defs, "bread");
        assert_eq!(progress.active["craft_quest"].objective_progress[0], 2);
    }

    #[test]
    fn craft_wrong_recipe_no_change() {
        let quest_defs = test_quest_defs();
        let mut progress = QuestProgress::default();
        start_quest("craft_quest", &quest_defs, &mut progress).unwrap();

        record_craft(&mut progress, &quest_defs, "cherry_pie");
        assert_eq!(progress.active["craft_quest"].objective_progress[0], 0);
    }

    #[test]
    fn visit_objective_completes() {
        let quest_defs = test_quest_defs();
        let inventory = Inventory::new(16);
        let skill_progress = SkillProgress::default();
        let mut progress = QuestProgress::default();
        start_quest("visit_quest", &quest_defs, &mut progress).unwrap();

        // Wrong street
        tick_quest_progress(&mut progress, &quest_defs, &inventory, &skill_progress, "LADEMO001");
        assert_eq!(progress.active["visit_quest"].objective_progress[0], 0);

        // Correct street
        tick_quest_progress(&mut progress, &quest_defs, &inventory, &skill_progress, "LADEMO002");
        assert_eq!(progress.active["visit_quest"].objective_progress[0], 1);

        // Stays completed even after leaving
        tick_quest_progress(&mut progress, &quest_defs, &inventory, &skill_progress, "LADEMO001");
        assert_eq!(progress.active["visit_quest"].objective_progress[0], 1);
    }

    #[test]
    fn learn_skill_objective_completes() {
        let quest_defs = test_quest_defs();
        let inventory = Inventory::new(16);
        let mut progress = QuestProgress::default();
        start_quest("skill_quest", &quest_defs, &mut progress).unwrap();

        // Skill not learned
        let skill_progress = SkillProgress::default();
        tick_quest_progress(&mut progress, &quest_defs, &inventory, &skill_progress, "LADEMO001");
        assert_eq!(progress.active["skill_quest"].objective_progress[0], 0);

        // Skill learned
        let skill_progress = SkillProgress {
            learned: vec!["cooking_1".to_string()],
            learning: None,
        };
        tick_quest_progress(&mut progress, &quest_defs, &inventory, &skill_progress, "LADEMO001");
        assert_eq!(progress.active["skill_quest"].objective_progress[0], 1);
    }

    #[test]
    fn is_quest_ready_all_objectives() {
        let quest_defs = test_quest_defs();
        let item_defs = test_item_defs();
        let skill_progress = SkillProgress::default();
        let mut progress = QuestProgress::default();
        start_quest("fetch_quest", &quest_defs, &mut progress).unwrap();

        let mut inventory = Inventory::new(16);
        inventory.add("cherry", 2, &item_defs);
        assert!(!is_quest_ready("fetch_quest", &progress, &quest_defs, &inventory, &skill_progress));

        inventory.add("cherry", 1, &item_defs);
        assert!(is_quest_ready("fetch_quest", &progress, &quest_defs, &inventory, &skill_progress));
    }

    #[test]
    fn is_quest_ready_not_active() {
        let quest_defs = test_quest_defs();
        let inventory = Inventory::new(16);
        let skill_progress = SkillProgress::default();
        let progress = QuestProgress::default();
        assert!(!is_quest_ready("fetch_quest", &progress, &quest_defs, &inventory, &skill_progress));
    }
}
