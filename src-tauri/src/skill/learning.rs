use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::skill::types::{LearningSlot, SkillDefs, SkillProgress};

/// Errors from skill learning operations.
#[derive(Debug)]
pub enum SkillError {
    UnknownSkill,
    AlreadyLearned,
    PrerequisiteNotMet { missing: String },
    InsufficientImagination { need: u64, have: u64 },
    AlreadyLearning,
    NotLearning,
}

impl fmt::Display for SkillError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SkillError::UnknownSkill => write!(f, "Unknown skill"),
            SkillError::AlreadyLearned => write!(f, "Skill already learned"),
            SkillError::PrerequisiteNotMet { missing } => {
                write!(f, "Prerequisite not met: {missing}")
            }
            SkillError::InsufficientImagination { need, have } => {
                write!(f, "Need {need} imagination, have {have}")
            }
            SkillError::AlreadyLearning => write!(f, "Already learning a skill"),
            SkillError::NotLearning => write!(f, "Not currently learning a skill"),
        }
    }
}

/// Check whether a skill can be learned (without mutating state).
pub fn can_learn(
    skill_id: &str,
    skill_defs: &SkillDefs,
    progress: &SkillProgress,
    imagination: u64,
) -> Result<(), SkillError> {
    let def = skill_defs.get(skill_id).ok_or(SkillError::UnknownSkill)?;

    if progress.learned.contains(&skill_id.to_string()) {
        return Err(SkillError::AlreadyLearned);
    }

    // Check this early — if a skill is already in progress, that's the
    // actionable blocker regardless of prereqs or imagination.
    if progress.learning.is_some() {
        return Err(SkillError::AlreadyLearning);
    }

    for prereq in &def.prerequisites {
        if !progress.learned.contains(prereq) {
            return Err(SkillError::PrerequisiteNotMet {
                missing: prereq.clone(),
            });
        }
    }

    if imagination < def.imagination_cost {
        return Err(SkillError::InsufficientImagination {
            need: def.imagination_cost,
            have: imagination,
        });
    }

    Ok(())
}

/// Begin learning a skill. Deducts imagination and sets the learning slot.
pub fn start_learning(
    skill_id: &str,
    skill_defs: &SkillDefs,
    progress: &mut SkillProgress,
    imagination: &mut u64,
) -> Result<(), SkillError> {
    can_learn(skill_id, skill_defs, progress, *imagination)?;

    let def = &skill_defs[skill_id];
    *imagination -= def.imagination_cost;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    progress.learning = Some(LearningSlot {
        skill_id: skill_id.to_string(),
        complete_at: now + def.learn_time_secs as i64,
    });

    Ok(())
}

/// Check if the currently learning skill has completed. Returns the skill_id
/// if complete, None if still in progress or nothing is being learned.
pub fn check_completion(progress: &SkillProgress) -> Option<String> {
    let slot = progress.learning.as_ref()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    if now >= slot.complete_at {
        Some(slot.skill_id.clone())
    } else {
        None
    }
}

/// Move the learning slot skill into the learned set and clear the slot.
/// Returns the learned skill_id.
pub fn complete_learning(progress: &mut SkillProgress) -> String {
    let slot = progress.learning.take().expect("no learning slot");
    progress.learned.push(slot.skill_id.clone());
    slot.skill_id
}

/// Cancel learning and refund imagination.
pub fn cancel_learning(
    skill_defs: &SkillDefs,
    progress: &mut SkillProgress,
    imagination: &mut u64,
) -> Result<(), SkillError> {
    let slot = progress.learning.as_ref().ok_or(SkillError::NotLearning)?;
    let skill_id = slot.skill_id.clone();

    if let Some(def) = skill_defs.get(&skill_id) {
        *imagination += def.imagination_cost;
    }

    progress.learning = None;
    Ok(())
}

/// Compute the remaining seconds and progress for a learning slot.
/// Returns (remaining_secs, progress) or None if not learning.
pub fn learning_progress(progress: &SkillProgress, skill_defs: &SkillDefs) -> Option<(f64, f64)> {
    let slot = progress.learning.as_ref()?;
    let def = skill_defs.get(&slot.skill_id)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let total = def.learn_time_secs as f64;
    let remaining = (slot.complete_at - now).max(0) as f64;
    let progress_val = if total > 0.0 {
        ((total - remaining) / total).clamp(0.0, 1.0)
    } else {
        1.0
    };

    Some((remaining, progress_val))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::types::SkillDef;
    use std::collections::HashMap;

    fn test_skill_defs() -> SkillDefs {
        let mut defs = HashMap::new();
        defs.insert(
            "cooking_1".to_string(),
            SkillDef {
                id: "cooking_1".to_string(),
                name: "Cooking I".to_string(),
                description: "Basic cooking.".to_string(),
                prerequisites: vec![],
                imagination_cost: 50,
                learn_time_secs: 120,
                unlocks_recipes: vec!["bread".to_string()],
            },
        );
        defs.insert(
            "cooking_2".to_string(),
            SkillDef {
                id: "cooking_2".to_string(),
                name: "Cooking II".to_string(),
                description: "Advanced cooking.".to_string(),
                prerequisites: vec!["cooking_1".to_string()],
                imagination_cost: 150,
                learn_time_secs: 600,
                unlocks_recipes: vec!["cherry_pie".to_string()],
            },
        );
        defs
    }

    fn empty_progress() -> SkillProgress {
        SkillProgress::default()
    }

    #[test]
    fn can_learn_valid_skill() {
        let defs = test_skill_defs();
        let progress = empty_progress();
        assert!(can_learn("cooking_1", &defs, &progress, 100).is_ok());
    }

    #[test]
    fn cannot_learn_unknown_skill() {
        let defs = test_skill_defs();
        let progress = empty_progress();
        assert!(matches!(
            can_learn("nonexistent", &defs, &progress, 100),
            Err(SkillError::UnknownSkill)
        ));
    }

    #[test]
    fn cannot_learn_already_learned() {
        let defs = test_skill_defs();
        let progress = SkillProgress {
            learned: vec!["cooking_1".to_string()],
            learning: None,
        };
        assert!(matches!(
            can_learn("cooking_1", &defs, &progress, 100),
            Err(SkillError::AlreadyLearned)
        ));
    }

    #[test]
    fn cannot_learn_missing_prerequisite() {
        let defs = test_skill_defs();
        let progress = empty_progress();
        assert!(matches!(
            can_learn("cooking_2", &defs, &progress, 500),
            Err(SkillError::PrerequisiteNotMet { .. })
        ));
    }

    #[test]
    fn cannot_learn_insufficient_imagination() {
        let defs = test_skill_defs();
        let progress = empty_progress();
        assert!(matches!(
            can_learn("cooking_1", &defs, &progress, 10),
            Err(SkillError::InsufficientImagination { need: 50, have: 10 })
        ));
    }

    #[test]
    fn cannot_learn_while_learning() {
        let defs = test_skill_defs();
        let progress = SkillProgress {
            learned: vec![],
            learning: Some(LearningSlot {
                skill_id: "cooking_1".to_string(),
                complete_at: i64::MAX,
            }),
        };
        assert!(matches!(
            can_learn("cooking_1", &defs, &progress, 100),
            Err(SkillError::AlreadyLearning)
        ));
    }

    #[test]
    fn start_learning_deducts_imagination() {
        let defs = test_skill_defs();
        let mut progress = empty_progress();
        let mut imagination = 100u64;
        start_learning("cooking_1", &defs, &mut progress, &mut imagination).unwrap();
        assert_eq!(imagination, 50); // 100 - 50
        assert!(progress.learning.is_some());
        assert_eq!(progress.learning.as_ref().unwrap().skill_id, "cooking_1");
    }

    #[test]
    fn cancel_refunds_imagination() {
        let defs = test_skill_defs();
        let mut progress = empty_progress();
        let mut imagination = 100u64;
        start_learning("cooking_1", &defs, &mut progress, &mut imagination).unwrap();
        assert_eq!(imagination, 50);

        cancel_learning(&defs, &mut progress, &mut imagination).unwrap();
        assert_eq!(imagination, 100); // refunded
        assert!(progress.learning.is_none());
    }

    #[test]
    fn cancel_when_not_learning_errors() {
        let defs = test_skill_defs();
        let mut progress = empty_progress();
        let mut imagination = 100u64;
        assert!(matches!(
            cancel_learning(&defs, &mut progress, &mut imagination),
            Err(SkillError::NotLearning)
        ));
    }

    #[test]
    fn completion_check_with_past_timestamp() {
        let progress = SkillProgress {
            learned: vec![],
            learning: Some(LearningSlot {
                skill_id: "cooking_1".to_string(),
                complete_at: 0, // epoch = way in the past
            }),
        };
        assert_eq!(check_completion(&progress), Some("cooking_1".to_string()));
    }

    #[test]
    fn completion_check_with_future_timestamp() {
        let progress = SkillProgress {
            learned: vec![],
            learning: Some(LearningSlot {
                skill_id: "cooking_1".to_string(),
                complete_at: i64::MAX, // way in the future
            }),
        };
        assert_eq!(check_completion(&progress), None);
    }

    #[test]
    fn completion_check_with_no_learning() {
        let progress = empty_progress();
        assert_eq!(check_completion(&progress), None);
    }

    #[test]
    fn complete_learning_moves_to_learned() {
        let mut progress = SkillProgress {
            learned: vec![],
            learning: Some(LearningSlot {
                skill_id: "cooking_1".to_string(),
                complete_at: 0,
            }),
        };
        let skill_id = complete_learning(&mut progress);
        assert_eq!(skill_id, "cooking_1");
        assert!(progress.learned.contains(&"cooking_1".to_string()));
        assert!(progress.learning.is_none());
    }

    #[test]
    fn prerequisite_met_after_learning() {
        let defs = test_skill_defs();
        let progress = SkillProgress {
            learned: vec!["cooking_1".to_string()],
            learning: None,
        };
        assert!(can_learn("cooking_2", &defs, &progress, 500).is_ok());
    }

    #[test]
    fn learning_progress_returns_values() {
        let defs = test_skill_defs();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let progress = SkillProgress {
            learned: vec![],
            learning: Some(LearningSlot {
                skill_id: "cooking_1".to_string(),
                complete_at: now + 60, // 60 seconds from now
            }),
        };

        let (remaining, prog) = learning_progress(&progress, &defs).unwrap();
        assert!(remaining > 50.0 && remaining <= 60.0);
        assert!(prog >= 0.0 && prog < 1.0);
    }

    #[test]
    fn learning_progress_complete() {
        let defs = test_skill_defs();
        let progress = SkillProgress {
            learned: vec![],
            learning: Some(LearningSlot {
                skill_id: "cooking_1".to_string(),
                complete_at: 0, // completed
            }),
        };

        let (remaining, prog) = learning_progress(&progress, &defs).unwrap();
        assert_eq!(remaining, 0.0);
        assert_eq!(prog, 1.0);
    }
}
