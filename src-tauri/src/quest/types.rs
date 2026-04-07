use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Dialogue definitions (loaded from assets/dialogues.json)
// ---------------------------------------------------------------------------

pub type DialogueDefs = HashMap<String, DialogueTreeDef>;

/// A full dialogue tree for one NPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueTreeDef {
    pub start_node: String,
    pub nodes: HashMap<String, DialogueNodeDef>,
}

/// A single node in a dialogue tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueNodeDef {
    pub speaker: String,
    pub text: String,
    #[serde(default)]
    pub options: Vec<DialogueOptionDef>,
}

/// A selectable option within a dialogue node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueOptionDef {
    pub text: String,
    /// Next node ID, or None to end dialogue.
    pub next: Option<String>,
    /// Conditions that must ALL be true for this option to appear.
    #[serde(default)]
    pub conditions: Vec<DialogueCondition>,
    /// Effects to apply when this option is selected.
    #[serde(default)]
    pub effects: Vec<DialogueEffect>,
}

/// Conditions for dialogue option visibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DialogueCondition {
    #[serde(rename_all = "camelCase")]
    QuestNotStarted { quest_id: String },
    #[serde(rename_all = "camelCase")]
    QuestActive { quest_id: String },
    /// Quest is active AND all objectives are complete (ready for turn-in).
    #[serde(rename_all = "camelCase")]
    QuestReady { quest_id: String },
    #[serde(rename_all = "camelCase")]
    QuestComplete { quest_id: String },
    #[serde(rename_all = "camelCase")]
    HasItem { item_id: String, count: u32 },
    #[serde(rename_all = "camelCase")]
    SkillLearned { skill_id: String },
}

/// Side effects triggered by selecting a dialogue option.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DialogueEffect {
    #[serde(rename_all = "camelCase")]
    StartQuest { quest_id: String },
    #[serde(rename_all = "camelCase")]
    CompleteQuest { quest_id: String },
    #[serde(rename_all = "camelCase")]
    GiveItem { item_id: String, count: u32 },
    #[serde(rename_all = "camelCase")]
    RemoveItem { item_id: String, count: u32 },
    #[serde(rename_all = "camelCase")]
    GiveCurrants { amount: u64 },
    #[serde(rename_all = "camelCase")]
    GiveImagination { amount: u64 },
}

// ---------------------------------------------------------------------------
// Quest definitions (loaded from assets/quests.json)
// ---------------------------------------------------------------------------

pub type QuestDefs = HashMap<String, QuestDef>;

/// Quest definition loaded from assets/quests.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestDef {
    /// Set from the JSON map key.
    #[serde(skip_deserializing)]
    pub id: String,
    pub name: String,
    pub description: String,
    pub objectives: Vec<QuestObjective>,
    pub rewards: QuestRewards,
    pub turn_in_npc: String,
}

/// An objective the player must complete.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum QuestObjective {
    #[serde(rename_all = "camelCase")]
    Fetch {
        item_id: String,
        count: u32,
        description: String,
    },
    #[serde(rename_all = "camelCase")]
    Craft {
        recipe_id: String,
        count: u32,
        description: String,
    },
    #[serde(rename_all = "camelCase")]
    Deliver {
        item_id: String,
        count: u32,
        npc_id: String,
        description: String,
    },
    #[serde(rename_all = "camelCase")]
    Visit {
        street_id: String,
        description: String,
    },
    #[serde(rename_all = "camelCase")]
    LearnSkill {
        skill_id: String,
        description: String,
    },
}

impl QuestObjective {
    /// The target count for this objective.
    pub fn target_count(&self) -> u32 {
        match self {
            QuestObjective::Fetch { count, .. } => *count,
            QuestObjective::Craft { count, .. } => *count,
            QuestObjective::Deliver { count, .. } => *count,
            QuestObjective::Visit { .. } => 1,
            QuestObjective::LearnSkill { .. } => 1,
        }
    }

    /// Human-readable description of this objective.
    pub fn description(&self) -> &str {
        match self {
            QuestObjective::Fetch { description, .. } => description,
            QuestObjective::Craft { description, .. } => description,
            QuestObjective::Deliver { description, .. } => description,
            QuestObjective::Visit { description, .. } => description,
            QuestObjective::LearnSkill { description, .. } => description,
        }
    }
}

/// Rewards granted on quest completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRewards {
    #[serde(default)]
    pub currants: u64,
    #[serde(default)]
    pub imagination: u64,
    #[serde(default)]
    pub items: Vec<QuestRewardItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestRewardItem {
    pub item_id: String,
    pub count: u32,
}

// ---------------------------------------------------------------------------
// Quest progress (persisted in SaveState)
// ---------------------------------------------------------------------------

/// Player's quest progress.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QuestProgress {
    /// Quest IDs the player has fully completed.
    #[serde(default)]
    pub completed: Vec<String>,
    /// Currently active quests with per-objective progress.
    #[serde(default)]
    pub active: HashMap<String, ActiveQuest>,
}

/// Runtime tracking for one active quest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveQuest {
    pub quest_id: String,
    /// Per-objective progress count, indexed parallel to QuestDef.objectives.
    pub objective_progress: Vec<u32>,
}

// ---------------------------------------------------------------------------
// Runtime dialogue session (NOT persisted)
// ---------------------------------------------------------------------------

/// Tracks an in-progress dialogue session.
#[derive(Debug, Clone)]
pub struct ActiveDialogue {
    pub tree_id: String,
    pub entity_id: String,
    pub current_node: String,
}

// ---------------------------------------------------------------------------
// Frames sent to the frontend
// ---------------------------------------------------------------------------

/// A dialogue node evaluated for display (conditions already filtered).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueFrame {
    pub speaker: String,
    pub text: String,
    pub options: Vec<DialogueOptionFrame>,
    pub entity_id: String,
}

/// A visible dialogue option (index into the original options list).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueOptionFrame {
    pub text: String,
    pub index: usize,
}

/// Result of choosing a dialogue option.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DialogueChoiceResult {
    /// Dialogue continues to the next node.
    #[serde(rename_all = "camelCase")]
    Continue {
        frame: DialogueFrame,
        feedback: Vec<String>,
        /// The node ID we navigated to (used by the IPC command to update
        /// active_dialogue.current_node — not sent to the frontend).
        #[serde(skip_serializing)]
        #[serde(default)]
        next_node_id: String,
    },
    /// Dialogue ended.
    #[serde(rename_all = "camelCase")]
    End { feedback: Vec<String> },
}

/// Full quest log fetched on demand via IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestLogFrame {
    pub active: Vec<QuestEntry>,
    pub completed: Vec<QuestCompletedEntry>,
}

/// An active quest in the quest log.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestEntry {
    pub quest_id: String,
    pub name: String,
    pub description: String,
    pub objectives: Vec<ObjectiveEntry>,
}

/// A single objective's progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveEntry {
    pub description: String,
    pub current: u32,
    pub target: u32,
    pub complete: bool,
}

/// A completed quest entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestCompletedEntry {
    pub quest_id: String,
    pub name: String,
}

/// Lightweight quest summary sent in RenderFrame every tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestProgressFrame {
    pub active_count: usize,
}
