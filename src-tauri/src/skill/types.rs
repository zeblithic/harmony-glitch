use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Skill definition loaded from assets/skills.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDef {
    /// Set from the JSON map key (not present in the JSON value).
    #[serde(skip_deserializing)]
    pub id: String,
    pub name: String,
    pub description: String,
    /// Skill IDs that must be learned before this one can be queued.
    #[serde(default)]
    pub prerequisites: Vec<String>,
    /// Imagination cost to begin learning.
    pub imagination_cost: u64,
    /// Wall-clock seconds to learn (real time, not game time).
    pub learn_time_secs: u64,
    /// Recipe IDs unlocked by learning this skill.
    #[serde(default)]
    pub unlocks_recipes: Vec<String>,
}

pub type SkillDefs = HashMap<String, SkillDef>;

/// Player's skill learning progress (persisted in save file).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillProgress {
    /// Skill IDs the player has fully learned.
    #[serde(default)]
    pub learned: Vec<String>,
    /// Currently learning skill, if any.
    #[serde(default)]
    pub learning: Option<LearningSlot>,
}

/// A skill currently being learned (wall-clock countdown).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningSlot {
    pub skill_id: String,
    /// Unix timestamp (seconds since epoch) when learning completes.
    pub complete_at: i64,
}

/// Skill progress data sent to the frontend each tick via RenderFrame.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillProgressFrame {
    pub learned: Vec<String>,
    pub learning: Option<LearningFrame>,
    pub imagination: u64,
}

/// Learning-in-progress data for frontend display.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningFrame {
    pub skill_id: String,
    /// Seconds remaining until the skill is learned.
    pub remaining_secs: f64,
    /// Progress from 0.0 (just started) to 1.0 (complete).
    pub progress: f64,
}
