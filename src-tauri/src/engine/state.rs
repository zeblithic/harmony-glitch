use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::avatar::types::{AnimationState, AvatarAppearance, Direction};
use crate::engine::audio::AudioEvent;
use crate::engine::jukebox::{self, JukeboxState, TrackCatalog};
use crate::engine::transition::{TransitionDirection, TransitionPhase, TransitionState};
use crate::item::imagination::PlayerUpgrades;
use crate::item::interaction;
use crate::item::inventory::Inventory;
use crate::item::types::{
    ActiveCraft, ActiveCraftFrame, EntityDefs, EntityInstanceState, InteractionPrompt,
    InventoryFrame, ItemDefs, ItemStack, ItemStackFrame, PickupFeedback, RecipeDefs, StoreCatalog,
    WorldEntity, WorldEntityFrame, WorldItem, WorldItemFrame,
};
use crate::physics::movement::{InputState, PhysicsBody};
use crate::skill::types::{LearningFrame, SkillDefs, SkillProgress, SkillProgressFrame};
use crate::street::types::StreetData;

/// Energy lost per second from passive decay.
const PASSIVE_ENERGY_DECAY_RATE: f64 = 0.1;

/// Fraction of the viewport height at which the player sprite is framed, measured
/// from the viewport's TOP edge. 0.5 = exact center, 1.0 = bottom edge.
/// 0.85 = player sits 15% up from the bottom (Glitch's typical framing),
/// leaving plenty of upward headroom to see jumps and falls to lower platforms.
const PLAYER_VERTICAL_FRAMING: f64 = 0.85;

fn default_currants() -> u64 {
    50
}

fn default_energy() -> f64 {
    600.0
}

fn default_max_energy() -> f64 {
    600.0
}

fn default_imagination() -> u64 {
    0
}

fn default_upgrades() -> PlayerUpgrades {
    PlayerUpgrades::default()
}

fn default_mood() -> f64 {
    100.0
}

/// Minimal player state for save/load.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveState {
    pub street_id: String,
    pub x: f64,
    pub y: f64,
    pub facing: Direction,
    pub inventory: Vec<Option<ItemStack>>,
    #[serde(default)]
    pub avatar: AvatarAppearance,
    #[serde(default = "default_currants")]
    pub currants: u64,
    #[serde(default = "default_energy")]
    pub energy: f64,
    #[serde(default = "default_max_energy")]
    pub max_energy: f64,
    /// ID of the last successfully completed trade (for journal recovery).
    #[serde(default)]
    pub last_trade_id: Option<u64>,
    #[serde(default = "default_imagination")]
    pub imagination: u64,
    #[serde(default = "default_upgrades")]
    pub upgrades: PlayerUpgrades,
    /// Skill learning progress (learned skills + in-progress learning).
    #[serde(default)]
    pub skill_progress: SkillProgress,
    /// Quest progress (active + completed quests).
    #[serde(default)]
    pub quest_progress: crate::quest::types::QuestProgress,
    #[serde(default = "default_mood")]
    pub mood: f64,
    #[serde(default = "default_mood")]
    pub max_mood: f64,
    #[serde(default)]
    pub buddies: Vec<crate::social::BuddySaveEntry>,
    #[serde(default)]
    pub blocked: Vec<String>,
    #[serde(default)]
    pub last_hi_date: Option<String>,
}

/// The complete game state.
pub struct GameState {
    pub player: PhysicsBody,
    pub facing: Direction,
    pub street: Option<StreetData>,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub inventory: Inventory,
    pub world_entities: Vec<WorldEntity>,
    pub world_items: Vec<WorldItem>,
    pub item_defs: ItemDefs,
    pub entity_defs: EntityDefs,
    pub recipe_defs: RecipeDefs,
    pub prev_interact: bool,
    pub next_item_id: u64,
    pub next_feedback_id: u64,
    pub pickup_feedback: Vec<PickupFeedback>,
    pub transition: TransitionState,
    pub transition_origin_tsid: Option<String>,
    /// Last valid landing location on the current street — where the safety
    /// net teleports the player when OOB is detected. Initialized from
    /// resolve_default_spawn on street load, updated on every signpost
    /// traversal via the active connection's arrival fields.
    pub last_arrival: crate::street::types::Point,
    /// Counter for consecutive ticks the player has been stuck on the
    /// floor clamp with nothing to stand on. Reset when grounded or below
    /// the clamp threshold; teleport fires at OOB_THRESHOLD_TICKS.
    pub oob_ticks: u32,
    /// Arrival SpawnPoint captured at signpost-crossing time, used by the
    /// transition-completion handler to place the player. None when not
    /// transitioning or when the active connection has no explicit arrival.
    pub pending_arrival: Option<crate::street::types::SpawnPoint>,
    pub tsid_to_name: std::collections::HashMap<String, String>,
    pub entity_states: std::collections::HashMap<String, EntityInstanceState>,
    pub game_time: f64,
    pub pending_audio_events: Vec<AudioEvent>,
    pub prev_on_ground: bool,
    pub jukebox_states: std::collections::HashMap<String, JukeboxState>,
    pub track_catalog: TrackCatalog,
    pub avatar: AvatarAppearance,
    pub currants: u64,
    pub store_catalog: StoreCatalog,
    pub energy: f64,
    pub max_energy: f64,
    /// ID of the last successfully completed trade (for journal recovery).
    pub last_trade_id: Option<u64>,
    /// In-progress timed craft, if any.
    pub active_craft: Option<ActiveCraft>,
    pub imagination: u64,
    pub upgrades: PlayerUpgrades,
    /// Skill definitions (loaded from assets/skills.json).
    pub skill_defs: SkillDefs,
    /// Player's skill learning progress.
    pub skill_progress: SkillProgress,
    /// Reverse map: recipe_id → skill_id required to craft it.
    pub recipe_skill_gate: std::collections::HashMap<String, String>,
    /// Quest definitions (loaded from assets/quests.json).
    pub quest_defs: crate::quest::types::QuestDefs,
    /// Dialogue tree definitions (loaded from assets/dialogues.json).
    pub dialogue_defs: crate::quest::types::DialogueDefs,
    /// Player's quest progress.
    pub quest_progress: crate::quest::types::QuestProgress,
    /// Active dialogue session (if any).
    pub active_dialogue: Option<crate::quest::types::ActiveDialogue>,
    pub social: crate::social::SocialState,
    /// Cached date string to avoid allocating every tick.
    cached_date: String,
}

/// Transition animation data sent to the frontend during a swoop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionFrame {
    pub progress: f64,
    pub direction: TransitionDirection,
    pub to_street: String,
    /// Generation counter — the frontend passes this back to `streetTransitionReady`
    /// so stale promises (from a timed-out swoop) don't mark a new swoop as ready.
    pub generation: u64,
}

/// The nearest remote player within social interaction range.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NearestSocialTarget {
    pub address_hash: String,
    pub display_name: String,
    pub is_buddy: bool,
    pub in_party: bool,
}

/// Data sent to the frontend each tick for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFrame {
    pub player: PlayerFrame,
    pub camera: CameraFrame,
    pub street_id: String,
    pub remote_players: Vec<RemotePlayerFrame>,
    pub inventory: InventoryFrame,
    pub world_entities: Vec<WorldEntityFrame>,
    pub world_items: Vec<WorldItemFrame>,
    pub interaction_prompt: Option<InteractionPrompt>,
    pub pickup_feedback: Vec<PickupFeedback>,
    pub transition: Option<TransitionFrame>,
    pub audio_events: Vec<AudioEvent>,
    pub currants: u64,
    pub energy: f64,
    pub max_energy: f64,
    pub active_craft: Option<ActiveCraftFrame>,
    pub imagination: u64,
    pub skill_progress: SkillProgressFrame,
    pub upgrades: PlayerUpgrades,
    pub quest_progress: crate::quest::types::QuestProgressFrame,
    pub mood: f64,
    pub max_mood: f64,
    pub nearest_social_target: Option<NearestSocialTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerFrame {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub facing: Direction,
    pub animation: AnimationState,
    pub on_ground: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraFrame {
    pub x: f64,
    pub y: f64,
}

/// An in-progress emote animation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmoteAnimationFrame {
    pub variant: String,
    pub target_hash: Option<String>,
    pub started_at: f64,
}

/// A remote player's state for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemotePlayerFrame {
    pub address_hash: String, // hex-encoded for JSON/IPC
    pub display_name: String,
    pub x: f64,
    pub y: f64,
    pub facing: String, // "left" or "right"
    pub on_ground: bool,
    pub animation: AnimationState,
    pub avatar: Option<AvatarAppearance>,
    #[serde(default)]
    pub epoch: String,
    #[serde(default)]
    pub is_buddy: bool,
    #[serde(default)]
    pub party_role: Option<String>,
    #[serde(default)]
    pub emote_animation: Option<EmoteAnimationFrame>,
}

impl GameState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        viewport_width: f64,
        viewport_height: f64,
        item_defs: ItemDefs,
        entity_defs: EntityDefs,
        recipe_defs: RecipeDefs,
        track_catalog: TrackCatalog,
        store_catalog: StoreCatalog,
        skill_defs: SkillDefs,
        quest_defs: crate::quest::types::QuestDefs,
        dialogue_defs: crate::quest::types::DialogueDefs,
    ) -> Self {
        // Build reverse map: recipe_id → skill_id from skill definitions.
        let mut recipe_skill_gate = std::collections::HashMap::new();
        for (skill_id, skill_def) in &skill_defs {
            for recipe_id in &skill_def.unlocks_recipes {
                recipe_skill_gate.insert(recipe_id.clone(), skill_id.clone());
            }
        }

        Self {
            player: PhysicsBody::new(0.0, -100.0),
            facing: Direction::Right,
            street: None,
            viewport_width,
            viewport_height,
            inventory: Inventory::new(16),
            world_entities: vec![],
            world_items: vec![],
            item_defs,
            entity_defs,
            recipe_defs,
            prev_interact: false,
            next_item_id: 0,
            next_feedback_id: 0,
            pickup_feedback: vec![],
            transition: TransitionState::new(),
            transition_origin_tsid: None,
            last_arrival: crate::street::types::Point { x: 0.0, y: 0.0 },
            oob_ticks: 0,
            pending_arrival: None,
            tsid_to_name: std::collections::HashMap::from([
                ("LADEMO001".to_string(), "demo_meadow".to_string()),
                ("LADEMO002".to_string(), "demo_heights".to_string()),
            ]),
            entity_states: std::collections::HashMap::new(),
            game_time: 0.0,
            pending_audio_events: vec![],
            prev_on_ground: true,
            jukebox_states: std::collections::HashMap::new(),
            track_catalog,
            avatar: AvatarAppearance::default(),
            currants: 50,
            store_catalog,
            energy: 600.0,
            max_energy: 600.0,
            imagination: 0,
            upgrades: PlayerUpgrades::default(),
            last_trade_id: None,
            active_craft: None,
            skill_defs,
            skill_progress: SkillProgress::default(),
            recipe_skill_gate,
            quest_defs,
            dialogue_defs,
            quest_progress: crate::quest::types::QuestProgress::default(),
            active_dialogue: None,
            social: crate::social::SocialState::new([0u8; 16], ""),
            cached_date: String::new(),
        }
    }

    pub fn load_street(
        &mut self,
        street: StreetData,
        entities: Vec<WorldEntity>,
        ground_items: Vec<WorldItem>,
    ) {
        // During an in-flight transition, skip player repositioning — the
        // Complete handler will place the player at the return signpost.
        // Only Swooping and Complete are actual in-flight phases; PreSubscribed
        // just means the player is near a signpost (no swoop yet).
        let is_transitioning = matches!(
            self.transition.phase,
            TransitionPhase::Swooping { .. } | TransitionPhase::Complete { .. }
        );
        if !is_transitioning {
            let spawn = crate::engine::arrival::resolve_default_spawn(&street);
            self.player = PhysicsBody::new(spawn.x, spawn.y);
            self.last_arrival = crate::street::types::Point { x: spawn.x, y: spawn.y };
            let facing = spawn.facing.unwrap_or_else(|| {
                crate::engine::arrival::infer_facing(spawn.x, &street)
            });
            self.facing = match facing {
                crate::street::types::Facing::Left => Direction::Left,
                crate::street::types::Facing::Right => Direction::Right,
            };
        }
        self.oob_ticks = 0;
        self.pending_arrival = None;
        self.street = Some(street);
        self.pending_audio_events.push(AudioEvent::StreetChanged {
            street_id: self.street.as_ref().unwrap().tsid.clone(),
        });
        self.world_entities = entities;
        self.world_items = ground_items;
        // Set next_item_id past any numeric IDs in loaded ground items to avoid collisions.
        // Loaded items use string IDs (e.g. "pot_1"), dynamic items use numeric IDs from
        // next_item_id (e.g. "drop_0"). Parse numeric IDs to find the max.
        let max_loaded_numeric_id = self
            .world_items
            .iter()
            .filter_map(|item| item.id.parse::<u64>().ok())
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        self.next_item_id = max_loaded_numeric_id.max(self.next_item_id);
        self.pickup_feedback.clear();
        self.entity_states.clear(); // game_time intentionally NOT reset — it's session-global
        self.jukebox_states.clear();
        for entity in &self.world_entities {
            if let Some(def) = self.entity_defs.get(&entity.entity_type) {
                if let Some(ref playlist) = def.playlist {
                    let valid =
                        jukebox::validate_playlist(playlist, &self.track_catalog, &def.name);
                    if !valid.is_empty() {
                        self.jukebox_states
                            .insert(entity.id.clone(), JukeboxState::new(valid));
                    }
                }
            }
        }
    }

    /// Start a timed crafting recipe. Consumes inputs and energy immediately;
    /// outputs are delivered when the timer completes in `tick()`.
    pub fn craft_recipe(&mut self, recipe_id: &str) -> Result<(), crate::item::types::CraftError> {
        if self.active_craft.is_some() {
            return Err(crate::item::types::CraftError::AlreadyCrafting);
        }

        // Check skill requirement
        if let Some(required_skill) = self.recipe_skill_gate.get(recipe_id) {
            if !self.skill_progress.learned.contains(required_skill) {
                return Err(crate::item::types::CraftError::SkillRequired);
            }
        }

        let recipe = self
            .recipe_defs
            .get(recipe_id)
            .ok_or(crate::item::types::CraftError::UnknownRecipe)?
            .clone();

        let pending = crate::item::crafting::start_craft(
            &recipe,
            &mut self.inventory,
            &self.item_defs,
            self.energy,
        )?;

        // Deduct energy
        self.energy = (self.energy - recipe.energy_cost).max(0.0);

        // Set active craft timer
        self.active_craft = Some(ActiveCraft {
            recipe_id: recipe.id.clone(),
            started_at: self.game_time,
            complete_at: self.game_time + recipe.duration_secs,
            pending_outputs: pending,
        });

        Ok(())
    }

    /// Begin learning a skill. Delegates to skill::learning::start_learning.
    pub fn learn_skill(
        &mut self,
        skill_id: &str,
    ) -> Result<(), crate::skill::learning::SkillError> {
        crate::skill::learning::start_learning(
            skill_id,
            &self.skill_defs,
            &mut self.skill_progress,
            &mut self.imagination,
        )
    }

    /// Cancel learning and refund imagination. Delegates to skill::learning::cancel_learning.
    pub fn cancel_skill_learning(&mut self) -> Result<(), crate::skill::learning::SkillError> {
        crate::skill::learning::cancel_learning(
            &self.skill_defs,
            &mut self.skill_progress,
            &mut self.imagination,
        )
    }

    /// Run one tick of the game loop.
    pub fn tick(&mut self, dt: f64, input: &InputState, rng: &mut impl Rng) -> Option<RenderFrame> {
        // Early return if no street loaded. Use is_none() check + unwrap pattern
        // so the immutable borrow doesn't span the entire function (tick_entities
        // needs &mut self).
        #[allow(clippy::question_mark)]
        if self.street.is_none() {
            return None;
        }
        self.game_time += dt;

        // Check if a skill has finished learning (wall-clock time)
        if crate::skill::learning::check_completion(&self.skill_progress).is_some() {
            let skill_id = crate::skill::learning::complete_learning(&mut self.skill_progress);
            let skill_name = self
                .skill_defs
                .get(&skill_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| skill_id.clone());
            self.pickup_feedback.push(PickupFeedback {
                id: self.next_feedback_id,
                text: format!("Learned: {skill_name}!"),
                success: true,
                x: self.player.x,
                y: self.player.y,
                age_secs: 0.0,
                color: Some("#c084fc".to_string()),
            });
            self.next_feedback_id += 1;
            self.pending_audio_events
                .push(AudioEvent::SkillLearned { skill_id });
        }

        // Passive energy decay
        self.energy = (self.energy - PASSIVE_ENERGY_DECAY_RATE * dt).max(0.0);

        // Passive mood decay
        {
            // Cache the date string — only recompute once per game-second
            let sec_now = self.game_time as u64;
            let sec_prev = (self.game_time - dt).max(0.0) as u64;
            if self.cached_date.is_empty() || sec_now != sec_prev {
                self.cached_date = crate::date_util::today_date_string();
            }
            let ctx = crate::social::SocialTickContext {
                current_date: &self.cached_date,
                in_dialogue: self.active_dialogue.is_some(),
                game_time: self.game_time,
            };
            self.social.tick(dt, &ctx);
        }

        // Drain pending audio events from IPC commands (craft_recipe, load_street)
        let mut audio_events: Vec<AudioEvent> = std::mem::take(&mut self.pending_audio_events);

        // Complete timed craft if timer expired
        if let Some(ref craft) = self.active_craft {
            if self.game_time >= craft.complete_at {
                let craft = self.active_craft.take().unwrap();
                for output in &craft.pending_outputs {
                    let overflow =
                        self.inventory
                            .add(&output.item_id, output.count, &self.item_defs);
                    let delivered = output.count - overflow;
                    if delivered > 0 {
                        self.pickup_feedback.push(PickupFeedback {
                            id: self.next_feedback_id,
                            text: format!("+{} x{}", output.name, delivered),
                            success: true,
                            x: self.player.x,
                            y: self.player.y,
                            age_secs: 0.0,
                            color: None,
                        });
                        self.next_feedback_id += 1;
                    }
                    if overflow > 0 {
                        // Spawn overflow as ground drop so the player can recover it
                        self.world_items.push(WorldItem {
                            id: format!("drop_{}", self.next_item_id),
                            item_id: output.item_id.clone(),
                            count: overflow,
                            x: self.player.x,
                            y: self.player.y,
                        });
                        self.next_item_id += 1;
                        self.pickup_feedback.push(PickupFeedback {
                            id: self.next_feedback_id,
                            text: format!("Inventory full — {} x{} dropped", output.name, overflow),
                            success: false,
                            x: self.player.x,
                            y: self.player.y,
                            age_secs: 0.0,
                            color: None,
                        });
                        self.next_feedback_id += 1;
                    }
                }
                // Earn iMG from crafted outputs
                let produced: Vec<(&str, u32)> = craft
                    .pending_outputs
                    .iter()
                    .map(|o| (o.item_id.as_str(), o.count))
                    .collect();
                let img_earned =
                    crate::item::imagination::earn_from_craft(&produced, &self.item_defs);
                if img_earned > 0 {
                    self.imagination = self.imagination.saturating_add(img_earned);
                    self.pickup_feedback.push(PickupFeedback {
                        id: self.next_feedback_id,
                        text: format!("+{img_earned} iMG"),
                        success: true,
                        x: self.player.x,
                        y: self.player.y,
                        age_secs: 0.0,
                        color: Some("#c084fc".to_string()),
                    });
                    self.next_feedback_id += 1;
                }
                // Record craft completion for quest objective tracking
                crate::quest::tracker::record_craft(
                    &mut self.quest_progress,
                    &self.quest_defs,
                    &craft.recipe_id,
                );
                audio_events.push(AudioEvent::CraftSuccess {
                    recipe_id: craft.recipe_id,
                });
            }
        }

        // Update quest objective progress — runs after craft completion so
        // inventory-based objectives see freshly delivered outputs this frame.
        {
            let street_id = self.street.as_ref().map(|s| s.tsid.as_str()).unwrap_or("");
            crate::quest::tracker::tick_quest_progress(
                &mut self.quest_progress,
                &self.quest_defs,
                &self.inventory,
                &self.skill_progress,
                street_id,
            );
        }

        let is_swooping = matches!(self.transition.phase, TransitionPhase::Swooping { .. });

        // Update facing direction — frozen during swoop so the player
        // sprite doesn't flip if a direction key is held.
        if !is_swooping {
            if input.left && !input.right {
                self.facing = Direction::Left;
            } else if input.right && !input.left {
                self.facing = Direction::Right;
            }
        }

        // Capture swooping state BEFORE signpost check — trigger_swoop() changes
        // the phase to Swooping, so capturing after would miss the TransitionStart edge.
        let was_swooping_before_signposts =
            matches!(self.transition.phase, TransitionPhase::Swooping { .. });

        // --- Street transition system ---
        {
            let street = self.street.as_ref().unwrap();
            self.transition.check_signposts(
                self.player.x,
                &street.signposts,
                street.left,
                street.right,
            );

            // Trigger swoop when player crosses the signpost X coordinate.
            // IMPORTANT: Copy values out of the pattern match BEFORE calling trigger_swoop,
            // because the if-let borrows self.transition immutably while
            // trigger_swoop needs &mut self.transition.
            if let TransitionPhase::PreSubscribed {
                signpost_x,
                direction,
                ..
            } = &self.transition.phase
            {
                let signpost_x = *signpost_x;
                let direction = *direction;
                let crossed = match direction {
                    TransitionDirection::Right => self.player.x >= signpost_x,
                    TransitionDirection::Left => self.player.x <= signpost_x,
                };
                if crossed {
                    // Capture the arrival SpawnPoint from the active connection (if it has
                    // explicit arrival fields) so the transition-completion handler can
                    // place the player without falling back to .find()-on-reciprocal.
                    // First connection wins — multi-connection UI is a separate ticket.
                    self.pending_arrival = street
                        .signposts
                        .iter()
                        .find(|s| (s.x - signpost_x).abs() < 0.5)
                        .and_then(|s| s.connects.first())
                        // Both-or-neither contract: arrival_x AND arrival_y must be Some, or
                        // the whole thing falls through to None and the transition-completion
                        // handler runs the legacy reciprocal-signpost fallback.
                        .and_then(|c| {
                            if let (Some(x), Some(y)) = (c.arrival_x, c.arrival_y) {
                                Some(crate::street::types::SpawnPoint {
                                    x,
                                    y,
                                    facing: c.arrival_facing,
                                })
                            } else {
                                None
                            }
                        });
                    self.transition_origin_tsid = Some(street.tsid.clone());
                    self.transition.trigger_swoop();
                }
            }
        }

        let was_swooping_before_tick =
            matches!(self.transition.phase, TransitionPhase::Swooping { .. });
        self.transition.tick(dt);

        // Handle transition completion — two-tick lifecycle:
        //   Tick 1 (origin_tsid is Some): reposition player, clear origin_tsid.
        //          TransitionFrame builder emits progress=1.0 so the renderer
        //          fully extends the viewport offset before teardown.
        //   Tick 2 (origin_tsid is None): reset phase to None.
        if let TransitionPhase::Complete { .. } = &self.transition.phase {
            if self.transition_origin_tsid.is_some() {
                let origin_tsid = self.transition_origin_tsid.take().unwrap();
                let street = self.street.as_ref().unwrap();

                // Prefer the pending_arrival captured at signpost-crossing time;
                // fall back to resolve_arrival, which walks the (reciprocal → default
                // → center) chain.
                let arrival = self.pending_arrival.take().unwrap_or_else(|| {
                    crate::engine::arrival::resolve_arrival(street, &origin_tsid, None)
                });

                let facing = arrival.facing.unwrap_or_else(|| {
                    crate::engine::arrival::infer_facing(arrival.x, street)
                });

                self.player.x = arrival.x;
                self.player.y = arrival.y;
                self.player.vx = 0.0;
                self.player.vy = 0.0;
                self.facing = match facing {
                    crate::street::types::Facing::Left => Direction::Left,
                    crate::street::types::Facing::Right => Direction::Right,
                };
                self.last_arrival = crate::street::types::Point {
                    x: arrival.x,
                    y: arrival.y,
                };
                self.oob_ticks = 0;
                // Player placed on ground — sync prev_on_ground to prevent
                // spurious Land audio event on the first post-swoop tick.
                self.prev_on_ground = true;
                // Reset footstep accumulator so the teleported player doesn't
                // immediately emit footsteps from pre-transition distance.
                self.player.distance_since_footstep = 0.0;
            } else {
                self.transition.reset();
            }
        }

        // Timeout path: Swooping → None without visiting Complete.
        // The Complete handler already clears transition_origin_tsid, so this
        // only fires when Complete was never visited (timeout cancellation).
        // Prevents a late-arriving loadStreet from seeing is_transitioning=false
        // and mis-positioning the player.
        if was_swooping_before_tick
            && self.transition.phase == TransitionPhase::None
            && self.transition_origin_tsid.is_some()
        {
            self.transition_origin_tsid = None;
        }

        // Re-check swooping state after transition system may have changed it.
        let is_swooping = matches!(self.transition.phase, TransitionPhase::Swooping { .. });

        // Transition audio events — use pre-signpost capture for start detection
        // (trigger_swoop changes phase before was_swooping_before_tick is captured)
        if !was_swooping_before_signposts && is_swooping {
            audio_events.push(AudioEvent::TransitionStart);
        }
        if was_swooping_before_tick && !is_swooping {
            audio_events.push(AudioEvent::TransitionComplete);
        }

        // Tick entity movement — runs even during swoops so NPCs keep wandering.
        // Must run BEFORE the interaction block so that lazy-init of movement
        // state happens before execute_interaction can create a partial state.
        self.tick_entities(dt, rng);

        let street = self.street.as_ref().unwrap();

        let interaction_prompt = if !is_swooping {
            self.player.tick(
                dt,
                input,
                street.platforms(),
                street.walls(),
                street.left,
                street.right,
                street.bottom,
            );

            // Jump/Land audio detection
            if self.prev_on_ground && !self.player.on_ground && self.player.vy < 0.0 {
                audio_events.push(AudioEvent::Jump);
            }
            if !self.prev_on_ground && self.player.on_ground {
                audio_events.push(AudioEvent::Land);
            }
            self.prev_on_ground = self.player.on_ground;

            // Footstep audio — emit when stride distance reached
            while self.player.on_ground
                && self.player.distance_since_footstep >= crate::physics::movement::FOOTSTEP_STRIDE
            {
                let surface = surface_at(
                    self.player.x,
                    self.player.y,
                    self.player.half_width,
                    street.platforms(),
                );
                audio_events.push(AudioEvent::Footstep {
                    surface: surface.to_string(),
                });
                self.player.distance_since_footstep -= crate::physics::movement::FOOTSTEP_STRIDE;
            }

            // --- Jukebox audio events ---
            {
                // Tick all jukebox states first so emitted events reflect post-tick state
                for jb_state in self.jukebox_states.values_mut() {
                    jb_state.tick(dt, &self.track_catalog);
                }

                // Find nearest jukebox within audio_radius using 1D horizontal
                // distance — in a side-scroller, a player on a platform directly
                // above should hear the jukebox at full volume.
                let mut nearest_jukebox: Option<(String, f64)> = None; // (entity_id, horizontal_distance)
                for entity in &self.world_entities {
                    if let Some(def) = self.entity_defs.get(&entity.entity_type) {
                        if let Some(audio_radius) = def.audio_radius {
                            if audio_radius > 0.0
                                && def.playlist.is_some()
                                && self.jukebox_states.contains_key(&entity.id)
                            {
                                let distance = (self.player.x - entity.x).abs();
                                if distance < audio_radius {
                                    let closer =
                                        nearest_jukebox.as_ref().is_none_or(|(_, d)| distance < *d);
                                    if closer {
                                        nearest_jukebox = Some((entity.id.clone(), distance));
                                    }
                                }
                            }
                        }
                    }
                }

                // Emit JukeboxUpdate for the nearest jukebox only
                if let Some((ref entity_id, distance)) = nearest_jukebox {
                    // Look up audio_radius to compute factor (distance < audio_radius guaranteed)
                    let entity = self.world_entities.iter().find(|e| e.id == *entity_id);
                    let audio_radius = entity
                        .and_then(|e| self.entity_defs.get(&e.entity_type))
                        .and_then(|d| d.audio_radius)
                        .unwrap_or(1.0);
                    let factor = 1.0 - distance / audio_radius;

                    if let Some(jb_state) = self.jukebox_states.get(entity_id) {
                        if let Some(track_id) = jb_state.current_track_id() {
                            audio_events.push(AudioEvent::JukeboxUpdate {
                                entity_id: entity_id.clone(),
                                track_id: track_id.to_string(),
                                playing: jb_state.playing,
                                distance_factor: factor,
                                elapsed_secs: jb_state.elapsed_secs,
                            });
                        }
                    }
                }
            }

            // --- Interaction system ---
            // Age and cull pickup feedback
            for fb in &mut self.pickup_feedback {
                fb.age_secs += dt;
            }
            self.pickup_feedback.retain(|fb| fb.age_secs < 1.5);

            // Proximity scan
            let nearest = interaction::proximity_scan(
                self.player.x,
                self.player.y,
                &self.world_entities,
                &self.entity_defs,
                &self.world_items,
                &[],
            );

            // Build prompt
            let interaction_prompt = nearest.as_ref().map(|n| {
                interaction::build_prompt(
                    n,
                    &self.world_entities,
                    &self.entity_defs,
                    &self.world_items,
                    &self.item_defs,
                    &self.entity_states,
                    self.game_time,
                )
            });

            // Rising edge detection for interact
            let interact_pressed = input.interact && !self.prev_interact;
            self.prev_interact = input.interact;

            // Execute interaction on rising edge
            let mut interacted = false;
            if interact_pressed {
                if let Some(nearest) = &nearest {
                    // Capture entity type before the call for audio lookup
                    let nearest_entity_type = match nearest {
                        interaction::NearestInteractable::Entity { index, .. } => {
                            Some(self.world_entities[*index].entity_type.clone())
                        }
                        _ => None,
                    };

                    let result = interaction::execute_interaction(
                        nearest,
                        &mut self.inventory,
                        &self.world_entities,
                        &self.entity_defs,
                        &self.world_items,
                        &self.item_defs,
                        rng,
                        &mut self.entity_states,
                        self.game_time,
                        &mut self.energy,
                        &mut self.imagination,
                    );

                    // Apply results — assign unique IDs to feedback
                    for mut fb in result.feedback {
                        fb.id = self.next_feedback_id;
                        self.next_feedback_id += 1;
                        self.pickup_feedback.push(fb);
                    }

                    // Remove or update ground items BEFORE appending overflow,
                    // so indices from execute_interaction remain valid.
                    if let Some(idx) = result.remove_ground_item {
                        self.world_items.remove(idx);
                    } else if let Some((idx, new_count)) = result.update_ground_item {
                        self.world_items[idx].count = new_count;
                    }

                    // Spawn overflow items (after index-based ops above)
                    for (item_id, count, x, y) in result.spawned_items {
                        self.world_items.push(WorldItem {
                            id: format!("drop_{}", self.next_item_id),
                            item_id,
                            count,
                            x,
                            y,
                        });
                        self.next_item_id += 1;
                    }

                    // Emit audio events from interaction
                    match &result.interaction_type {
                        Some(interaction::InteractionType::Entity { entity_type }) => {
                            audio_events.push(AudioEvent::EntityInteract {
                                entity_type: entity_type.clone(),
                            });
                            // Emit ItemPickup for the first yield item
                            if let Some(et) = &nearest_entity_type {
                                if let Some(def) = self.entity_defs.get(et) {
                                    if let Some(first_yield) = def.yields.first() {
                                        audio_events.push(AudioEvent::ItemPickup {
                                            item_id: first_yield.item.clone(),
                                        });
                                    }
                                }
                            }
                        }
                        Some(interaction::InteractionType::GroundItem { item_id }) => {
                            audio_events.push(AudioEvent::ItemPickup {
                                item_id: item_id.clone(),
                            });
                        }
                        Some(interaction::InteractionType::Rejected) => {
                            audio_events.push(AudioEvent::ActionFailed);
                        }
                        Some(interaction::InteractionType::Jukebox { .. }) => {
                            audio_events.push(AudioEvent::EntityInteract {
                                entity_type: "jukebox".to_string(),
                            });
                        }
                        Some(interaction::InteractionType::Vendor { .. }) => {
                            audio_events.push(AudioEvent::EntityInteract {
                                entity_type: "vendor".to_string(),
                            });
                        }
                        Some(interaction::InteractionType::Npc { .. }) => {
                            audio_events.push(AudioEvent::EntityInteract {
                                entity_type: "npc".to_string(),
                            });
                        }
                        None => {}
                    }

                    // Only blank prompt when the ground item target was removed.
                    // Entity targets persist after harvest — blanking would cause
                    // a one-frame flicker as the prompt rebuilds next tick.
                    if result.remove_ground_item.is_some() {
                        interacted = true;
                    }
                }
            }

            // Clear prompt on the frame where a ground item was picked up — the
            // target was removed, so the pre-interaction prompt is stale.
            if interacted {
                None
            } else {
                interaction_prompt
            }
        } else {
            self.prev_interact = input.interact;
            None
        };

        // Determine animation state
        let animation = if !self.player.on_ground {
            if self.player.vy < 0.0 {
                AnimationState::Jumping
            } else {
                AnimationState::Falling
            }
        } else if self.player.vx.abs() > 0.1 {
            AnimationState::Walking
        } else {
            AnimationState::Idle
        };

        // Camera: horizontal center on player, vertical frame at
        // PLAYER_VERTICAL_FRAMING. The renderer draws the player at
        // `screen_y = player.y - cam_y`, so setting `cam_y = player.y -
        // vh * 0.85` pins the player 85% down from the top (15% up from
        // the bottom). cam_y_max is offset by the same factor so the
        // framing persists when the player is grounded — the viewport
        // extends a small strip below street.bottom rather than pinning
        // the player against the bottom edge. When the street is shorter
        // than that strip, clamp min/max to avoid a panic from f64::clamp
        // (min <= max).
        let cam_x = self.player.x - self.viewport_width / 2.0;
        let cam_y = self.player.y - self.viewport_height * PLAYER_VERTICAL_FRAMING;
        let cam_x_min = street.left;
        let cam_x_max = (street.right - self.viewport_width).max(cam_x_min);
        let cam_y_min = street.top;
        let cam_y_max =
            (street.bottom - self.viewport_height * PLAYER_VERTICAL_FRAMING).max(cam_y_min);
        let cam_x = cam_x.clamp(cam_x_min, cam_x_max);
        let cam_y = cam_y.clamp(cam_y_min, cam_y_max);

        Some(RenderFrame {
            player: PlayerFrame {
                x: self.player.x,
                y: self.player.y,
                vx: self.player.vx,
                vy: self.player.vy,
                facing: self.facing,
                animation,
                on_ground: self.player.on_ground,
            },
            camera: CameraFrame { x: cam_x, y: cam_y },
            street_id: street.tsid.clone(),
            remote_players: vec![],
            inventory: self.build_inventory_frame(),
            world_entities: self.build_entity_frames(),
            world_items: self.build_item_frames(),
            interaction_prompt,
            pickup_feedback: self.pickup_feedback.clone(),
            transition: match &self.transition.phase {
                TransitionPhase::Swooping {
                    progress,
                    direction,
                    to_street,
                    ..
                } => Some(TransitionFrame {
                    progress: *progress,
                    direction: *direction,
                    to_street: self
                        .tsid_to_name
                        .get(to_street)
                        .cloned()
                        .unwrap_or_else(|| to_street.clone()),
                    generation: self.transition.generation,
                }),
                TransitionPhase::Complete {
                    new_street,
                    direction,
                } => Some(TransitionFrame {
                    progress: 1.0,
                    direction: *direction,
                    to_street: self
                        .tsid_to_name
                        .get(new_street)
                        .cloned()
                        .unwrap_or_else(|| new_street.clone()),
                    generation: self.transition.generation,
                }),
                _ => None,
            },
            audio_events,
            currants: self.currants,
            energy: self.energy,
            max_energy: self.max_energy,
            active_craft: self.active_craft.as_ref().map(|c| {
                let duration = c.complete_at - c.started_at;
                let elapsed = (self.game_time - c.started_at).min(duration);
                ActiveCraftFrame {
                    recipe_id: c.recipe_id.clone(),
                    progress: if duration > 0.0 {
                        elapsed / duration
                    } else {
                        1.0
                    },
                    remaining_secs: (c.complete_at - self.game_time).max(0.0),
                }
            }),
            imagination: self.imagination,
            skill_progress: {
                let learning = crate::skill::learning::learning_progress(
                    &self.skill_progress,
                    &self.skill_defs,
                )
                .map(|(remaining, progress)| {
                    let skill_id = self
                        .skill_progress
                        .learning
                        .as_ref()
                        .unwrap()
                        .skill_id
                        .clone();
                    LearningFrame {
                        skill_id,
                        remaining_secs: remaining,
                        progress,
                    }
                });
                SkillProgressFrame {
                    learned: self.skill_progress.learned.clone(),
                    learning,
                }
            },
            upgrades: self.upgrades.clone(),
            quest_progress: crate::quest::types::QuestProgressFrame {
                active_count: self.quest_progress.active.len(),
            },
            mood: self.social.mood.mood,
            max_mood: self.social.mood.max_mood,
            nearest_social_target: None, // Populated in lib.rs after remote players are augmented
        })
    }

    /// Complete any in-progress craft immediately, delivering outputs to
    /// inventory. Overflow is spawned as ground drops. Called before game
    /// stop or trade execution — NOT during regular saves (save_state
    /// includes pending outputs in the snapshot instead).
    pub fn flush_active_craft(&mut self) {
        if let Some(craft) = self.active_craft.take() {
            for output in &craft.pending_outputs {
                let overflow = self
                    .inventory
                    .add(&output.item_id, output.count, &self.item_defs);
                if overflow > 0 {
                    self.world_items.push(WorldItem {
                        id: format!("drop_{}", self.next_item_id),
                        item_id: output.item_id.clone(),
                        count: overflow,
                        x: self.player.x,
                        y: self.player.y,
                    });
                    self.next_item_id += 1;
                }
            }
            // Earn iMG from flushed craft (same as tick() completion path)
            let produced: Vec<(&str, u32)> = craft
                .pending_outputs
                .iter()
                .map(|o| (o.item_id.as_str(), o.count))
                .collect();
            let img_earned = crate::item::imagination::earn_from_craft(&produced, &self.item_defs);
            if img_earned > 0 {
                self.imagination = self.imagination.saturating_add(img_earned);
            }
        }
    }

    /// Extract the current save-worthy state. Returns None if no street loaded.
    /// If an active craft is in-progress, pending outputs are included in the
    /// inventory snapshot so they are not lost on restore.
    pub fn save_state(&self) -> Option<SaveState> {
        let street = self.street.as_ref()?;
        let mut slots = self.inventory.slots.clone();
        // Include pending craft outputs in the snapshot so they survive save/load.
        if let Some(ref craft) = self.active_craft {
            let mut temp_inv = self.inventory.clone();
            for output in &craft.pending_outputs {
                temp_inv.add(&output.item_id, output.count, &self.item_defs);
            }
            slots = temp_inv.slots;
        }
        Some(SaveState {
            street_id: self
                .tsid_to_name
                .get(&street.tsid)
                .cloned()
                .unwrap_or_else(|| street.tsid.clone()),
            x: self.player.x,
            y: self.player.y,
            facing: self.facing,
            inventory: slots,
            avatar: self.avatar.clone(),
            currants: self.currants,
            energy: self.energy,
            max_energy: self.max_energy,
            last_trade_id: self.last_trade_id,
            imagination: self.imagination,
            skill_progress: self.skill_progress.clone(),
            upgrades: self.upgrades.clone(),
            quest_progress: self.quest_progress.clone(),
            mood: self.social.mood.mood,
            max_mood: self.social.mood.max_mood,
            buddies: self.social.buddies.to_save_entries(),
            blocked: self.social.buddies.blocked_to_hex(),
            last_hi_date: Some(self.social.emotes.current_date.clone()),
        })
    }

    /// Restore saved state after a street has been loaded.
    /// Position is clamped to street bounds.
    pub fn restore_save(&mut self, save: &SaveState) {
        if let Some(ref street) = self.street {
            self.player.x = save.x.clamp(street.left + 1.0, street.right - 1.0);
            self.player.y = save.y.clamp(street.top + 1.0, street.bottom);
        } else {
            // Invariant: restore_save should be called after load_street.
            // Log a warning but don't panic — this path is reachable in tests
            // and must degrade gracefully in production.
            eprintln!(
                "[persistence] restore_save called with no street loaded; position not clamped"
            );
            self.player.x = save.x;
            self.player.y = save.y;
        }
        self.facing = save.facing;
        let capacity = self.inventory.capacity;
        self.inventory.slots = save.inventory.clone();
        if self.inventory.slots.len() > capacity {
            eprintln!(
                "[persistence] Inventory in save ({} slots) exceeds capacity ({}); truncating",
                self.inventory.slots.len(),
                capacity
            );
        }
        self.inventory.slots.resize(capacity, None);
        self.avatar = save.avatar.clone();
        self.currants = save.currants;
        self.energy = save.energy;
        self.max_energy = save.max_energy;
        self.last_trade_id = save.last_trade_id;
        self.imagination = save.imagination;
        self.skill_progress = save.skill_progress.clone();
        self.upgrades = save.upgrades.clone();
        self.quest_progress = save.quest_progress.clone();
        self.social.mood = crate::mood::MoodState::new_with_grace(save.mood, save.max_mood, self.game_time);
        self.social.buddies.restore_from_save(&save.buddies, &save.blocked);
        if let Some(ref date) = save.last_hi_date {
            self.social.emotes.check_date_change(date);
        }
    }

    /// Replay a journaled trade that wasn't persisted before a crash.
    pub fn recover_trade_journal(&mut self, journal: &crate::trade::journal::TradeJournal) {
        crate::trade::journal::recover(
            journal,
            &mut self.inventory,
            &mut self.currants,
            &self.item_defs,
        );
        self.last_trade_id = Some(journal.trade_id);
    }

    fn tick_entities(&mut self, dt: f64, rng: &mut impl Rng) {
        for i in 0..self.world_entities.len() {
            let entity_type = self.world_entities[i].entity_type.clone();
            let entity_id = self.world_entities[i].id.clone();
            let entity_x = self.world_entities[i].x;

            let def = match self.entity_defs.get(&entity_type) {
                Some(d) => d,
                None => continue,
            };

            let (walk_speed, wander_radius, max_harvests) =
                match (def.walk_speed, def.wander_radius) {
                    (Some(ws), Some(wr)) => (ws, wr, def.max_harvests),
                    _ => continue,
                };

            let game_time = self.game_time;
            let state = self.entity_states.entry(entity_id).or_insert_with(|| {
                let mut s = EntityInstanceState::new(max_harvests);
                s.current_x = entity_x;
                s.wander_origin = entity_x;
                s.facing = if rng.gen::<bool>() {
                    Direction::Right
                } else {
                    Direction::Left
                };
                s.idle_until = game_time + rng.gen_range(0.0..2.0);
                s
            });

            // Idle check
            if game_time < state.idle_until {
                state.velocity_x = 0.0;
                self.world_entities[i].x = state.current_x;
                continue;
            }

            // Boundary check — only when moving
            if state.velocity_x != 0.0
                && (state.current_x - state.wander_origin).abs() >= wander_radius
            {
                if state.current_x > state.wander_origin {
                    state.current_x = state.wander_origin + wander_radius;
                } else {
                    state.current_x = state.wander_origin - wander_radius;
                }
                state.facing = match state.facing {
                    Direction::Right => Direction::Left,
                    Direction::Left => Direction::Right,
                };
                state.velocity_x = 0.0;
                state.idle_until = game_time + rng.gen_range(1.0..3.0);
                self.world_entities[i].x = state.current_x;
                continue;
            }

            // Apply movement
            let direction_sign = if state.facing == Direction::Right {
                1.0
            } else {
                -1.0
            };
            state.velocity_x = walk_speed * direction_sign;
            state.current_x += state.velocity_x * dt;
            self.world_entities[i].x = state.current_x;
        }
    }

    fn build_inventory_frame(&self) -> InventoryFrame {
        InventoryFrame {
            slots: self
                .inventory
                .slots
                .iter()
                .map(|slot| {
                    slot.as_ref().map(|stack| {
                        let def = self.item_defs.get(&stack.item_id);
                        ItemStackFrame {
                            item_id: stack.item_id.clone(),
                            name: def.map(|d| d.name.clone()).unwrap_or_default(),
                            description: def.map(|d| d.description.clone()).unwrap_or_default(),
                            icon: def.map(|d| d.icon.clone()).unwrap_or_default(),
                            count: stack.count,
                            stack_limit: def.map(|d| d.stack_limit).unwrap_or(1),
                            energy_value: def.and_then(|d| d.energy_value),
                            mood_value: def.and_then(|d| d.mood_value),
                        }
                    })
                })
                .collect(),
            capacity: self.inventory.capacity,
        }
    }

    fn build_entity_frames(&self) -> Vec<WorldEntityFrame> {
        self.world_entities
            .iter()
            .map(|e| {
                let def = self.entity_defs.get(&e.entity_type);

                let (cooldown_remaining, depleted, facing) =
                    if let Some(state) = self.entity_states.get(&e.id) {
                        let remaining =
                            (state.cooldown_until.max(state.depleted_until)) - self.game_time;
                        if remaining > 0.0 {
                            (
                                Some(remaining),
                                state.depleted_until > self.game_time,
                                state.facing,
                            )
                        } else {
                            (None, false, state.facing)
                        }
                    } else {
                        (None, false, Direction::Right)
                    };

                // Apply vertical bob for entities with bob config
                let y = if let Some(d) = def {
                    match (d.bob_amplitude, d.bob_frequency) {
                        (Some(amp), Some(freq)) => {
                            e.y + (self.game_time * freq * std::f64::consts::TAU).sin() * amp
                        }
                        _ => e.y,
                    }
                } else {
                    e.y
                };

                WorldEntityFrame {
                    id: e.id.clone(),
                    entity_type: e.entity_type.clone(),
                    name: def.map(|d| d.name.clone()).unwrap_or_default(),
                    sprite_class: def.map(|d| d.sprite_class.clone()).unwrap_or_default(),
                    x: e.x,
                    y,
                    cooldown_remaining,
                    depleted,
                    facing,
                }
            })
            .collect()
    }

    fn build_item_frames(&self) -> Vec<WorldItemFrame> {
        self.world_items
            .iter()
            .map(|i| {
                let def = self.item_defs.get(&i.item_id);
                WorldItemFrame {
                    id: i.id.clone(),
                    item_id: i.item_id.clone(),
                    name: def.map(|d| d.name.clone()).unwrap_or_default(),
                    icon: def.map(|d| d.icon.clone()).unwrap_or_default(),
                    count: i.count,
                    x: i.x,
                    y: i.y,
                }
            })
            .collect()
    }
}

/// Write a save state to disk as pretty-printed JSON (atomic: temp → fsync → rename).
pub fn write_save_state(path: &std::path::Path, save: &SaveState) -> Result<(), String> {
    let json = serde_json::to_string_pretty(save).map_err(|e| e.to_string())?;
    crate::persistence::atomic_write(path, json.as_bytes(), None)
}

/// Read a save state from disk. Returns Ok(None) if the file is missing
/// or corrupted (graceful degradation — player sees street picker).
pub fn read_save_state(path: &std::path::Path) -> Result<Option<SaveState>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let json = match std::fs::read_to_string(path) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[persistence] Failed to read save file: {e}");
            return Ok(None);
        }
    };
    match serde_json::from_str::<SaveState>(&json) {
        Ok(save) => Ok(Some(save)),
        Err(e) => {
            eprintln!("[persistence] Corrupted save file: {e}");
            Ok(None)
        }
    }
}

/// Find the surface material of the platform under the player.
/// Mirrors the physics "highest platform wins" priority (most-negative Y)
/// so overlapping platforms at a junction resolve to the same one the
/// player is actually standing on.
/// Returns "default" if on ground_y or no platform matches.
fn surface_at(
    x: f64,
    y: f64,
    half_width: f64,
    platforms: &[crate::street::types::PlatformLine],
) -> &str {
    let mut best: Option<(f64, usize)> = None; // (plat_y, index)
    for (i, platform) in platforms.iter().enumerate() {
        if !platform.solid_from_top() {
            continue;
        }
        // Use body-width overlap (matching physics collision in movement.rs)
        // so platform edges within half_width still resolve correctly.
        if x + half_width < platform.min_x() || x - half_width > platform.max_x() {
            continue;
        }
        let plat_y = platform.y_at(x);
        if (plat_y - y).abs() < 1.0 {
            // Prefer the highest platform (most-negative Y), matching
            // physics Phase 1 slope-following priority in movement.rs.
            match best {
                Some((best_y, _)) if plat_y < best_y => best = Some((plat_y, i)),
                None => best = Some((plat_y, i)),
                _ => {}
            }
        }
    }
    match best {
        Some((_, idx)) => &platforms[idx].surface,
        None => "default",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::transition::TransitionPhase;
    use crate::item::types::{EntityDefs, ItemDefs};
    use crate::street::types::*;
    use std::collections::HashMap;

    fn empty_catalog() -> TrackCatalog {
        TrackCatalog {
            tracks: HashMap::new(),
        }
    }

    fn empty_store_catalog() -> StoreCatalog {
        StoreCatalog {
            stores: HashMap::new(),
        }
    }

    fn empty_skill_defs() -> SkillDefs {
        HashMap::new()
    }

    fn test_street() -> StreetData {
        StreetData {
            tsid: "test".into(),
            name: "Test".into(),
            left: -3000.0,
            right: 3000.0,
            top: -1000.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![Layer {
                name: "middleground".into(),
                z: 0,
                w: 6000.0,
                h: 1000.0,
                is_middleground: true,
                decos: vec![],
                platform_lines: vec![PlatformLine {
                    id: "ground".into(),
                    start: Point { x: -2800.0, y: 0.0 },
                    end: Point { x: 2800.0, y: 0.0 },
                    pc_perm: None,
                    item_perm: None,
                    surface: "default".into(),
                }],
                walls: vec![],
                ladders: vec![],
                filters: None,
            }],
            signposts: vec![],
            default_spawn: None,
        }
    }

    #[test]
    fn tick_produces_render_frame() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert!(frame.is_some());
    }

    #[test]
    fn load_street_sets_last_arrival_from_default_spawn() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );

        let mut street = test_street();
        street.default_spawn = Some(crate::street::types::SpawnPoint {
            x: 123.0,
            y: -50.0,
            facing: Some(crate::street::types::Facing::Left),
        });

        state.load_street(street, vec![], vec![]);

        assert_eq!(state.last_arrival.x, 123.0);
        assert_eq!(state.last_arrival.y, -50.0);
        assert_eq!(state.player.x, 123.0);
        assert_eq!(state.player.y, -50.0);
        assert_eq!(state.oob_ticks, 0);
        assert!(state.pending_arrival.is_none());
    }

    #[test]
    fn load_street_without_default_spawn_uses_center() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let street = test_street(); // test_street has default_spawn: None
        let expected_x = (street.left + street.right) / 2.0;
        let expected_y = street.ground_y;

        state.load_street(street, vec![], vec![]);

        assert_eq!(state.player.x, expected_x);
        assert_eq!(state.player.y, expected_y);
        assert_eq!(state.last_arrival.x, expected_x);
        assert_eq!(state.last_arrival.y, expected_y);
    }

    #[test]
    fn tick_returns_none_without_street() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let input = InputState::default();
        assert!(state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .is_none());
    }

    #[test]
    fn facing_updates_from_input() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);

        let input = InputState {
            left: true,
            ..Default::default()
        };
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert_eq!(state.facing, Direction::Left);

        let input = InputState {
            right: true,
            ..Default::default()
        };
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert_eq!(state.facing, Direction::Right);
    }

    #[test]
    fn animation_idle_on_ground() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        state.player.on_ground = true;
        state.player.y = 0.0;
        state.player.vy = 0.0;

        let input = InputState::default();
        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        assert_eq!(frame.player.animation, AnimationState::Idle);
    }

    #[test]
    fn animation_walking() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        state.player.on_ground = true;
        state.player.y = 0.0;

        let input = InputState {
            right: true,
            ..Default::default()
        };
        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        assert_eq!(frame.player.animation, AnimationState::Walking);
    }

    #[test]
    fn camera_does_not_panic_on_small_street() {
        // Street smaller than viewport (600px wide, 400px tall vs 1280x720 viewport)
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let small_street = StreetData {
            tsid: "small".into(),
            name: "Tiny".into(),
            left: -300.0,
            right: 300.0,
            top: -400.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![Layer {
                name: "middleground".into(),
                z: 0,
                w: 600.0,
                h: 400.0,
                is_middleground: true,
                decos: vec![],
                platform_lines: vec![PlatformLine {
                    id: "ground".into(),
                    start: Point { x: -300.0, y: 0.0 },
                    end: Point { x: 300.0, y: 0.0 },
                    pc_perm: None,
                    item_perm: None,
                    surface: "default".into(),
                }],
                walls: vec![],
                ladders: vec![],
                filters: None,
            }],
            signposts: vec![],
            default_spawn: None,
        };
        state.load_street(small_street, vec![], vec![]);

        // Should not panic — camera clamp handles min > max gracefully
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert!(frame.is_some());
    }

    #[test]
    fn camera_frames_grounded_player_near_bottom() {
        // Viewport 1280x720, street 6000x1000. Player on ground (y=0).
        // PLAYER_VERTICAL_FRAMING = 0.85, so cam_y should clamp to
        // street.bottom - vh * 0.85 = 0 - 612 = -612. That places the
        // player's world-y=0 at screen_y = player.y - cam_y = 612,
        // i.e. 85% down the viewport (15% from the bottom).
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        state.player.y = 0.0;
        state.player.vy = 0.0;
        state.player.on_ground = true;

        let frame = state
            .tick(1.0 / 60.0, &InputState::default(), &mut rand::thread_rng())
            .unwrap();
        let expected_cam_y = -(720.0 * PLAYER_VERTICAL_FRAMING);
        assert!(
            (frame.camera.y - expected_cam_y).abs() < 0.001,
            "cam_y {} vs expected {}",
            frame.camera.y,
            expected_cam_y
        );
        // Screen-Y check: player lands 85% down the viewport.
        let player_screen_y = state.player.y - frame.camera.y;
        assert!((player_screen_y - 720.0 * PLAYER_VERTICAL_FRAMING).abs() < 0.001);
    }

    #[test]
    fn camera_clamps_to_street_top_when_player_high() {
        // Player teleported well above the street top — camera should
        // clamp to street.top, not follow the player off the map.
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        state.player.y = -5000.0; // far above street.top = -1000

        let frame = state
            .tick(1.0 / 60.0, &InputState::default(), &mut rand::thread_rng())
            .unwrap();
        assert!(
            (frame.camera.y - (-1000.0)).abs() < 0.001,
            "cam_y clamped to street.top: got {}",
            frame.camera.y
        );
    }

    #[test]
    fn load_street_places_player() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        // Player should be at center of street
        assert!((state.player.x - 0.0).abs() < 1.0);
    }

    #[test]
    fn interaction_adds_to_inventory() {
        use crate::item::types::{EntityDef, ItemDef, WorldEntity, YieldEntry};
        use rand::SeedableRng;

        let mut item_defs = ItemDefs::new();
        item_defs.insert(
            "cherry".into(),
            ItemDef {
                id: "cherry".into(),
                name: "Cherry".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 50,
                icon: "cherry".into(),
                base_cost: None,
                energy_value: None,
                mood_value: None,
            },
        );
        let mut entity_defs = EntityDefs::new();
        entity_defs.insert(
            "fruit_tree".into(),
            EntityDef {
                id: "fruit_tree".into(),
                name: "Fruit Tree".into(),
                verb: "Harvest".into(),
                yields: vec![YieldEntry {
                    item: "cherry".into(),
                    min: 1,
                    max: 1,
                }],
                cooldown_secs: 0.0,
                max_harvests: 0,
                respawn_secs: 0.0,
                sprite_class: "tree_fruit".into(),
                interact_radius: 80.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
                dialogue: None,
            },
        );

        let mut state = GameState::new(
            1280.0,
            720.0,
            item_defs,
            entity_defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let street = test_street();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 0.0,
            y: 0.0,
        }];
        state.load_street(street, entities, vec![]);

        // Stand next to tree and press interact
        let input = InputState {
            interact: true,
            ..Default::default()
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();

        assert_eq!(frame.inventory.slots[0].as_ref().unwrap().item_id, "cherry");
        assert!(frame.pickup_feedback.iter().any(|f| f.success));
    }

    #[test]
    fn render_frame_has_no_transition_by_default() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        let input = InputState::default();
        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        assert!(frame.transition.is_none());
    }

    #[test]
    fn game_state_has_transition_state() {
        let state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        assert_eq!(state.transition.phase, TransitionPhase::None);
    }

    #[test]
    fn tick_detects_signpost_pre_subscribe() {
        use crate::street::types::{Signpost, SignpostConnection};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let mut street = test_street();
        street.signposts = vec![Signpost {
            id: "sign_right".into(),
            x: 1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO002".into(),
                target_label: "To the Heights".into(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        }];
        state.load_street(street, vec![], vec![]);
        state.player.x = 1500.0;
        state.player.on_ground = true;

        let input = InputState::default();
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

        assert!(matches!(
            state.transition.phase,
            TransitionPhase::PreSubscribed { .. }
        ));
    }

    #[test]
    fn tick_triggers_swoop_on_crossing_signpost() {
        use crate::street::types::{Signpost, SignpostConnection};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let mut street = test_street();
        street.signposts = vec![Signpost {
            id: "sign_right".into(),
            x: 1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO002".into(),
                target_label: "To the Heights".into(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        }];
        state.load_street(street, vec![], vec![]);
        state.player.x = 1950.0;
        state.player.on_ground = true;

        let input = InputState::default();
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

        // check_signposts puts us in PreSubscribed, then the crossing
        // check triggers the swoop — both happen in the same tick.
        assert!(matches!(
            state.transition.phase,
            TransitionPhase::Swooping { .. }
        ));
    }

    #[test]
    fn signpost_crossing_captures_pending_arrival_when_connection_has_fields() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );

        // Build a street where a signpost at x=900 connects to TARGET with
        // explicit arrival fields.
        use crate::street::types::{Facing, Signpost, SignpostConnection};
        let mut street = test_street();
        street.signposts.push(Signpost {
            id: "east".to_string(),
            x: 900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "TARGETTSID".to_string(),
                target_label: "To Target".to_string(),
                arrival_x: Some(-777.0),
                arrival_y: Some(-5.0),
                arrival_facing: Some(Facing::Right),
            }],
        });
        state.load_street(street, vec![], vec![]);

        // Walk player to the signpost and cross it.
        state.player.x = 850.0;
        state.player.on_ground = true;
        let input = InputState {
            right: true,
            ..Default::default()
        };
        // First tick: pre-subscribe (player within 500px of signpost).
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        // Force player past signpost.x to trigger crossing.
        state.player.x = 950.0;
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

        assert!(
            state.pending_arrival.is_some(),
            "pending_arrival should be populated"
        );
        let sp = state.pending_arrival.unwrap();
        assert_eq!(sp.x, -777.0);
        assert_eq!(sp.y, -5.0);
        assert_eq!(sp.facing, Some(Facing::Right));
    }

    #[test]
    fn signpost_crossing_leaves_pending_arrival_none_when_no_arrival_fields() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        use crate::street::types::{Signpost, SignpostConnection};
        let mut street = test_street();
        street.signposts.push(Signpost {
            id: "east".to_string(),
            x: 900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "TARGETTSID".to_string(),
                target_label: "To Target".to_string(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        });
        state.load_street(street, vec![], vec![]);

        state.player.x = 850.0;
        state.player.on_ground = true;
        let input = InputState {
            right: true,
            ..Default::default()
        };
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        state.player.x = 950.0;
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

        assert!(
            state.pending_arrival.is_none(),
            "pending_arrival should remain None without explicit arrival fields"
        );
        // But origin_tsid should still be set — legacy path will run at completion.
        assert!(state.transition_origin_tsid.is_some());
    }

    #[test]
    fn tick_freezes_input_during_swoop() {
        use crate::street::types::{Signpost, SignpostConnection};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let mut street = test_street();
        street.signposts = vec![Signpost {
            id: "sign_right".into(),
            x: 1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO002".into(),
                target_label: "To the Heights".into(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        }];
        state.load_street(street, vec![], vec![]);
        state.player.x = 1950.0;
        state.player.on_ground = true;
        let input = InputState::default();
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert!(matches!(
            state.transition.phase,
            TransitionPhase::Swooping { .. }
        ));

        let pos_before = state.player.x;
        let input = InputState {
            left: true,
            ..Default::default()
        };
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

        assert!(
            (state.player.x - pos_before).abs() < 0.01,
            "Player moved during swoop: {} -> {}",
            pos_before,
            state.player.x
        );
    }

    #[test]
    fn render_frame_contains_transition_during_swoop() {
        use crate::street::types::{Signpost, SignpostConnection};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let mut street = test_street();
        street.signposts = vec![Signpost {
            id: "sign_right".into(),
            x: 1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO002".into(),
                target_label: "To the Heights".into(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        }];
        state.load_street(street, vec![], vec![]);
        state.player.x = 1950.0;
        state.player.on_ground = true;
        let input = InputState::default();
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        let transition = frame.transition.unwrap();
        assert!(transition.progress > 0.0);
        assert_eq!(transition.to_street, "demo_heights");
    }

    #[test]
    fn game_time_accumulates() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        let input = InputState::default();

        state.tick(0.5, &input, &mut rand::thread_rng());
        assert!((state.game_time - 0.5).abs() < 0.001);

        state.tick(0.25, &input, &mut rand::thread_rng());
        assert!((state.game_time - 0.75).abs() < 0.001);
    }

    #[test]
    fn entity_states_cleared_on_load_street() {
        use crate::item::types::EntityInstanceState;

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state
            .entity_states
            .insert("tree_1".into(), EntityInstanceState::new(3));
        assert_eq!(state.entity_states.len(), 1);

        state.load_street(test_street(), vec![], vec![]);
        assert!(state.entity_states.is_empty());
    }

    #[test]
    fn transition_complete_repositions_player() {
        use crate::street::types::{Signpost, SignpostConnection};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let mut street = test_street();
        street.tsid = "LADEMO001".into();
        street.signposts = vec![Signpost {
            id: "sign_right".into(),
            x: 1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO002".into(),
                target_label: "To the Heights".into(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        }];
        state.load_street(street, vec![], vec![]);
        state.player.x = 1950.0;
        state.player.on_ground = true;
        let input = InputState::default();
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert!(matches!(
            state.transition.phase,
            TransitionPhase::Swooping { .. }
        ));

        state
            .transition
            .mark_street_ready(state.transition.generation);

        let mut new_street = test_street();
        new_street.tsid = "LADEMO002".into();
        new_street.signposts = vec![Signpost {
            id: "sign_left".into(),
            x: -1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO001".into(),
                target_label: "Back to Meadow".into(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        }];
        state.load_street(new_street, vec![], vec![]);

        // Tick through the remaining swoop (mark_street_ready shrinks it to
        // MIN_SWOOP_SECS = 0.3s ≈ 18 frames) + one Complete-handler tick. After
        // Complete runs: pending_arrival was None (the original signpost had no
        // explicit arrival_x/y), so resolve_arrival walks the reciprocal-signpost
        // chain and places the player at the return signpost x=-1900.
        //
        // We stop ticking AT completion because x=-1900 is within
        // PRE_SUBSCRIBE_DISTANCE of that signpost — further ticks would retrigger
        // a swoop back to LADEMO001 (a known behavior the OOB/re-entry plan
        // accepts; callers are expected to handle the immediate re-approach).
        let mut saw_complete = false;
        for _ in 0..60 {
            state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
            if state.transition_origin_tsid.is_none() && !saw_complete {
                saw_complete = true;
                break;
            }
        }
        assert!(
            saw_complete,
            "Complete handler should have cleared transition_origin_tsid"
        );

        // Player lands at the return signpost x=-1900 (reciprocal fallback).
        assert!(
            (state.player.x - (-1900.0)).abs() < 1.0,
            "Player should be at reciprocal signpost x=-1900, got {}",
            state.player.x
        );
        // last_arrival records the arrival for the OOB safety net.
        assert_eq!(state.last_arrival.x, -1900.0);
    }

    #[test]
    fn entity_frame_includes_cooldown_remaining() {
        use crate::item::types::{EntityDef, ItemDef, WorldEntity, YieldEntry};
        use rand::SeedableRng;

        let mut item_defs = ItemDefs::new();
        item_defs.insert(
            "cherry".into(),
            ItemDef {
                id: "cherry".into(),
                name: "Cherry".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 50,
                icon: "cherry".into(),
                base_cost: None,
                energy_value: None,
                mood_value: None,
            },
        );
        let mut entity_defs = EntityDefs::new();
        entity_defs.insert(
            "fruit_tree".into(),
            EntityDef {
                id: "fruit_tree".into(),
                name: "Fruit Tree".into(),
                verb: "Harvest".into(),
                yields: vec![YieldEntry {
                    item: "cherry".into(),
                    min: 1,
                    max: 1,
                }],
                cooldown_secs: 5.0,
                max_harvests: 3,
                respawn_secs: 30.0,
                sprite_class: "tree_fruit".into(),
                interact_radius: 80.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
                dialogue: None,
            },
        );

        let mut state = GameState::new(
            1280.0,
            720.0,
            item_defs,
            entity_defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 0.0,
            y: 0.0,
        }];
        state.load_street(test_street(), entities, vec![]);
        state.player.x = 0.0;
        state.player.on_ground = true;

        // Harvest the entity
        let input = InputState {
            interact: true,
            ..Default::default()
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();

        // After harvest, entity should have cooldown remaining
        let entity_frame = &frame.world_entities[0];
        assert!(entity_frame.cooldown_remaining.is_some());
        assert!(!entity_frame.depleted);

        // Advance past cooldown (tick with no interact)
        let input = InputState::default();
        let mut last_frame = None;
        for _ in 0..400 {
            last_frame = state.tick(1.0 / 60.0, &input, &mut rng);
        }
        let frame = last_frame.unwrap();
        let entity_frame = &frame.world_entities[0];
        assert!(entity_frame.cooldown_remaining.is_none());
        assert!(!entity_frame.depleted);
    }

    #[test]
    fn entity_frame_shows_depleted() {
        use crate::item::types::{EntityDef, ItemDef, WorldEntity, YieldEntry};
        use rand::SeedableRng;

        let mut item_defs = ItemDefs::new();
        item_defs.insert(
            "cherry".into(),
            ItemDef {
                id: "cherry".into(),
                name: "Cherry".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 50,
                icon: "cherry".into(),
                base_cost: None,
                energy_value: None,
                mood_value: None,
            },
        );
        let mut entity_defs = EntityDefs::new();
        entity_defs.insert(
            "fruit_tree".into(),
            EntityDef {
                id: "fruit_tree".into(),
                name: "Fruit Tree".into(),
                verb: "Harvest".into(),
                yields: vec![YieldEntry {
                    item: "cherry".into(),
                    min: 1,
                    max: 1,
                }],
                cooldown_secs: 0.0,
                max_harvests: 1,
                respawn_secs: 30.0,
                sprite_class: "tree_fruit".into(),
                interact_radius: 80.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
                dialogue: None,
            },
        );

        let mut state = GameState::new(
            1280.0,
            720.0,
            item_defs,
            entity_defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 0.0,
            y: 0.0,
        }];
        state.load_street(test_street(), entities, vec![]);
        state.player.x = 0.0;
        state.player.on_ground = true;

        // Single harvest depletes (max_harvests=1)
        let input = InputState {
            interact: true,
            ..Default::default()
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        state.tick(1.0 / 60.0, &input, &mut rng);

        // Next frame should show depleted
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();
        let entity_frame = &frame.world_entities[0];
        assert!(entity_frame.cooldown_remaining.is_some());
        assert!(entity_frame.depleted);
    }

    fn movable_entity_defs() -> EntityDefs {
        use crate::item::types::EntityDef;

        let mut defs = EntityDefs::new();
        defs.insert(
            "chicken".into(),
            EntityDef {
                id: "chicken".into(),
                name: "Chicken".into(),
                verb: "Squeeze".into(),
                yields: vec![],
                cooldown_secs: 0.0,
                max_harvests: 0,
                respawn_secs: 0.0,
                sprite_class: "npc_chicken".into(),
                interact_radius: 60.0,
                walk_speed: Some(40.0),
                wander_radius: Some(120.0),
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
                dialogue: None,
            },
        );
        defs.insert(
            "fruit_tree".into(),
            EntityDef {
                id: "fruit_tree".into(),
                name: "Fruit Tree".into(),
                verb: "Harvest".into(),
                yields: vec![],
                cooldown_secs: 0.0,
                max_harvests: 0,
                respawn_secs: 0.0,
                sprite_class: "tree_fruit".into(),
                interact_radius: 80.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
                dialogue: None,
            },
        );
        defs
    }

    #[test]
    fn tick_entities_moves_movable_entity() {
        use rand::SeedableRng;

        let defs = movable_entity_defs();
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "c1".into(),
            entity_type: "chicken".into(),
            x: 200.0,
            y: -2.0,
        }];
        state.load_street(test_street(), entities, vec![]);

        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Tick several times to get past initial idle
        for _ in 0..200 {
            state.tick(1.0 / 60.0, &input, &mut rng);
        }

        // Entity should have moved from spawn
        let chicken = &state.world_entities[0];
        assert!(
            (chicken.x - 200.0).abs() > 1.0,
            "Chicken should have moved from spawn x=200, got x={}",
            chicken.x
        );
    }

    #[test]
    fn tick_entities_static_entity_stays_put() {
        use rand::SeedableRng;

        let defs = movable_entity_defs();
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: -800.0,
            y: -2.0,
        }];
        state.load_street(test_street(), entities, vec![]);

        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        for _ in 0..200 {
            state.tick(1.0 / 60.0, &input, &mut rng);
        }

        let tree = &state.world_entities[0];
        assert!(
            (tree.x - (-800.0)).abs() < 0.01,
            "Tree should stay at x=-800, got x={}",
            tree.x
        );
    }

    #[test]
    fn tick_entities_respects_wander_radius() {
        use rand::SeedableRng;

        let defs = movable_entity_defs();
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "c1".into(),
            entity_type: "chicken".into(),
            x: 200.0,
            y: -2.0,
        }];
        state.load_street(test_street(), entities, vec![]);

        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Tick many times — entity should never exceed wander_radius (120) from spawn (200)
        for _ in 0..2000 {
            state.tick(1.0 / 60.0, &input, &mut rng);
        }

        let chicken = &state.world_entities[0];
        let distance = (chicken.x - 200.0).abs();
        assert!(
            distance <= 121.0, // 1px tolerance for float
            "Chicken at x={} is {}px from spawn (max 120)",
            chicken.x,
            distance
        );
    }

    #[test]
    fn tick_entities_facing_matches_direction() {
        use rand::SeedableRng;

        let defs = movable_entity_defs();
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "c1".into(),
            entity_type: "chicken".into(),
            x: 200.0,
            y: -2.0,
        }];
        state.load_street(test_street(), entities, vec![]);

        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Tick until entity is moving (past initial idle)
        for _ in 0..200 {
            state.tick(1.0 / 60.0, &input, &mut rng);
        }

        let entity_state = state.entity_states.get("c1").unwrap();
        if entity_state.velocity_x > 0.0 {
            assert_eq!(entity_state.facing, Direction::Right);
        } else if entity_state.velocity_x < 0.0 {
            assert_eq!(entity_state.facing, Direction::Left);
        }
        // velocity_x == 0 means idle — facing can be either, don't assert
    }

    #[test]
    fn tick_entities_idle_pause_at_boundary() {
        use crate::item::types::EntityDef;
        use rand::SeedableRng;

        let mut defs = EntityDefs::new();
        defs.insert(
            "fast_npc".into(),
            EntityDef {
                id: "fast_npc".into(),
                name: "Fast".into(),
                verb: "Poke".into(),
                yields: vec![],
                cooldown_secs: 0.0,
                max_harvests: 0,
                respawn_secs: 0.0,
                sprite_class: "npc_fast".into(),
                interact_radius: 60.0,
                walk_speed: Some(200.0), // Very fast so it reaches boundary quickly
                wander_radius: Some(20.0), // Very small radius
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
                dialogue: None,
            },
        );

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "f1".into(),
            entity_type: "fast_npc".into(),
            x: 0.0,
            y: -2.0,
        }];
        state.load_street(test_street(), entities, vec![]);

        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Tick past initial idle and long enough to reach boundary
        for _ in 0..300 {
            state.tick(1.0 / 60.0, &input, &mut rng);
        }

        // After hitting boundary, entity should have had at least one idle pause
        let entity_state = state.entity_states.get("f1").unwrap();
        // Entity must be within wander radius
        let dist = (entity_state.current_x - entity_state.wander_origin).abs();
        // Overshoot can be up to one tick of movement (200 * 1/60 ≈ 3.33px)
        // because boundary check fires on the next tick after the overshoot.
        let max_overshoot = 200.0 * (1.0 / 60.0);
        assert!(
            dist <= 20.0 + max_overshoot + 0.01,
            "Entity outside wander radius: dist={}",
            dist
        );
    }

    #[test]
    fn tick_entities_write_back_to_world_entity() {
        use rand::SeedableRng;

        let defs = movable_entity_defs();
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "c1".into(),
            entity_type: "chicken".into(),
            x: 200.0,
            y: -2.0,
        }];
        state.load_street(test_street(), entities, vec![]);

        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        for _ in 0..200 {
            state.tick(1.0 / 60.0, &input, &mut rng);
        }

        // WorldEntity.x should match the movement state's current_x
        let entity_state = state.entity_states.get("c1").unwrap();
        let world_entity = &state.world_entities[0];
        assert!(
            (world_entity.x - entity_state.current_x).abs() < 0.01,
            "WorldEntity.x ({}) should match current_x ({})",
            world_entity.x,
            entity_state.current_x
        );
    }

    #[test]
    fn tick_entities_initial_direction_varies_with_seed() {
        use rand::SeedableRng;

        let defs = movable_entity_defs();

        // Run with two different seeds and collect initial facing
        let mut facings = Vec::new();
        for seed in [1u64, 2, 3, 4, 5, 6, 7, 8] {
            let mut state = GameState::new(
                1280.0,
                720.0,
                ItemDefs::new(),
                defs.clone(),
                HashMap::new(),
                empty_catalog(),
                empty_store_catalog(),
                empty_skill_defs(),
                HashMap::new(),
                HashMap::new(),
            );
            let entities = vec![WorldEntity {
                id: "c1".into(),
                entity_type: "chicken".into(),
                x: 200.0,
                y: -2.0,
            }];
            state.load_street(test_street(), entities, vec![]);
            let input = InputState::default();
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            state.tick(1.0 / 60.0, &input, &mut rng);
            let entity_state = state.entity_states.get("c1").unwrap();
            facings.push(entity_state.facing);
        }

        // With 8 different seeds, we should see both Left and Right
        let has_left = facings.iter().any(|f| *f == Direction::Left);
        let has_right = facings.iter().any(|f| *f == Direction::Right);
        assert!(
            has_left && has_right,
            "Expected both Left and Right facings across seeds, got {:?}",
            facings
        );
    }

    #[test]
    fn build_entity_frames_defaults_facing_right_for_static() {
        use rand::SeedableRng;

        let defs = movable_entity_defs();
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: -800.0,
            y: -2.0,
        }];
        state.load_street(test_street(), entities, vec![]);

        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();

        // Static entity (tree) should default to facing Right
        assert_eq!(frame.world_entities[0].facing, Direction::Right);
    }

    #[test]
    fn prompt_shows_cooldown_text_through_tick() {
        use crate::item::types::{EntityDef, ItemDef, WorldEntity, YieldEntry};
        use rand::SeedableRng;

        let mut item_defs = ItemDefs::new();
        item_defs.insert(
            "cherry".into(),
            ItemDef {
                id: "cherry".into(),
                name: "Cherry".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 50,
                icon: "cherry".into(),
                base_cost: None,
                energy_value: None,
                mood_value: None,
            },
        );
        let mut entity_defs = EntityDefs::new();
        entity_defs.insert(
            "fruit_tree".into(),
            EntityDef {
                id: "fruit_tree".into(),
                name: "Fruit Tree".into(),
                verb: "Harvest".into(),
                yields: vec![YieldEntry {
                    item: "cherry".into(),
                    min: 1,
                    max: 1,
                }],
                cooldown_secs: 5.0,
                max_harvests: 3,
                respawn_secs: 30.0,
                sprite_class: "tree_fruit".into(),
                interact_radius: 80.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
                dialogue: None,
            },
        );

        let mut state = GameState::new(
            1280.0,
            720.0,
            item_defs,
            entity_defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 0.0,
            y: 0.0,
        }];
        state.load_street(test_street(), entities, vec![]);
        state.player.x = 0.0;
        state.player.on_ground = true;

        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Harvest on first tick
        let input = InputState {
            interact: true,
            ..Default::default()
        };
        state.tick(1.0 / 60.0, &input, &mut rng);

        // Next tick: still near entity, prompt should show cooldown text
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();
        let prompt = frame.interaction_prompt.unwrap();
        assert!(!prompt.actionable);
        assert!(prompt.verb.contains("Available"));
    }

    #[test]
    fn build_entity_frames_applies_bob_offset() {
        use rand::SeedableRng;

        let mut defs = EntityDefs::new();
        defs.insert(
            "butterfly".into(),
            crate::item::types::EntityDef {
                id: "butterfly".into(),
                name: "Butterfly".into(),
                verb: "Milk".into(),
                yields: vec![],
                cooldown_secs: 0.0,
                max_harvests: 0,
                respawn_secs: 0.0,
                sprite_class: "npc_butterfly".into(),
                interact_radius: 90.0,
                walk_speed: Some(25.0),
                wander_radius: Some(150.0),
                bob_amplitude: Some(15.0),
                bob_frequency: Some(1.5),
                playlist: None,
                audio_radius: None,
                store: None,
                dialogue: None,
            },
        );

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "b1".into(),
            entity_type: "butterfly".into(),
            x: 600.0,
            y: -80.0,
        }];
        state.load_street(test_street(), entities, vec![]);

        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Collect y values over several ticks — should vary due to sine bob
        let mut y_values: Vec<f64> = Vec::new();
        for _ in 0..120 {
            if let Some(frame) = state.tick(1.0 / 60.0, &input, &mut rng) {
                y_values.push(frame.world_entities[0].y);
            }
        }

        let min_y = y_values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_y = y_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let y_range = max_y - min_y;

        assert!(
            y_range > 1.0,
            "Butterfly y should oscillate due to bob, but range was only {}",
            y_range
        );
    }

    #[test]
    fn build_entity_frames_no_bob_for_ground_entity() {
        use rand::SeedableRng;

        let defs = movable_entity_defs(); // chicken + fruit_tree, no bob fields
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            defs,
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let entities = vec![WorldEntity {
            id: "c1".into(),
            entity_type: "chicken".into(),
            x: 200.0,
            y: -2.0,
        }];
        state.load_street(test_street(), entities, vec![]);

        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // All y values should be the same (no bob)
        let mut y_values: Vec<f64> = Vec::new();
        for _ in 0..120 {
            if let Some(frame) = state.tick(1.0 / 60.0, &input, &mut rng) {
                y_values.push(frame.world_entities[0].y);
            }
        }

        let first_y = y_values[0];
        for (i, &y) in y_values.iter().enumerate() {
            assert!(
                (y - first_y).abs() < 0.01,
                "Chicken y should not bob, but frame {} had y={} vs first y={}",
                i,
                y,
                first_y
            );
        }
    }

    #[test]
    fn craft_recipe_success_creates_feedback() {
        let item_defs =
            crate::item::loader::parse_item_defs(include_str!("../../../assets/items.json"))
                .unwrap();
        let entity_defs =
            crate::item::loader::parse_entity_defs(include_str!("../../../assets/entities.json"))
                .unwrap();
        let recipe_defs =
            crate::item::loader::parse_recipe_defs(include_str!("../../../assets/recipes.json"))
                .unwrap();

        let mut state = GameState::new(
            1280.0,
            720.0,
            item_defs,
            entity_defs,
            recipe_defs,
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        state.inventory.add("cherry", 10, &state.item_defs);
        state.inventory.add("grain", 5, &state.item_defs);
        state.inventory.add("pot", 1, &state.item_defs);

        let result = state.craft_recipe("cherry_pie");
        assert!(result.is_ok());

        // Inputs consumed immediately, outputs deferred
        assert_eq!(state.inventory.count_item("cherry"), 5);
        assert_eq!(state.inventory.count_item("pot"), 1);
        assert_eq!(state.inventory.count_item("cherry_pie"), 0);
        assert!(state.active_craft.is_some());

        // Energy deducted
        assert!(state.energy < 600.0);

        // Advance game_time to just before completion, then do a small tick
        // so the feedback doesn't age out (retained for 1.5s).
        let input = InputState::default();
        state.game_time = 9.9;
        let frame = state.tick(0.2, &input, &mut rand::thread_rng()).unwrap();

        // Now outputs delivered
        assert_eq!(state.inventory.count_item("cherry_pie"), 1);
        assert!(state.active_craft.is_none());
        assert!(frame
            .pickup_feedback
            .iter()
            .any(|f| f.text.contains("Cherry Pie") && f.success));
        assert!(frame
            .audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::CraftSuccess { .. })));
        // iMG earned from craft (cherry_pie base_cost=20, 2x = 40)
        assert!(state.imagination > 0);
        assert!(frame.pickup_feedback.iter().any(|f| f.text.contains("iMG")));
    }

    #[test]
    fn craft_recipe_unknown_returns_error() {
        let item_defs =
            crate::item::loader::parse_item_defs(include_str!("../../../assets/items.json"))
                .unwrap();
        let entity_defs =
            crate::item::loader::parse_entity_defs(include_str!("../../../assets/entities.json"))
                .unwrap();
        let recipe_defs =
            crate::item::loader::parse_recipe_defs(include_str!("../../../assets/recipes.json"))
                .unwrap();

        let mut state = GameState::new(
            1280.0,
            720.0,
            item_defs,
            entity_defs,
            recipe_defs,
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );

        let result = state.craft_recipe("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn audio_events_empty_by_default() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        let input = InputState::default();
        // First tick drains the StreetChanged event from load_street
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        // Second tick should have no audio events
        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        assert!(frame.audio_events.is_empty());
    }

    #[test]
    fn audio_event_jump() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        state.player.on_ground = true;

        // First tick: on ground, no Jump event (StreetChanged is drained but no Jump)
        let input = InputState::default();
        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        assert!(!frame
            .audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::Jump)));

        // Simulate jump: player leaves ground with upward velocity
        state.player.on_ground = false;
        state.player.vy = -200.0;
        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        assert!(frame
            .audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::Jump)));
    }

    #[test]
    fn audio_event_land() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);

        // Position player high above ground so physics keeps them airborne
        state.player.y = -500.0;
        state.player.on_ground = false;
        let input = InputState::default();
        // Tick while airborne — establishes prev_on_ground = false
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

        // Now simulate landing: snap player to ground
        state.player.y = 0.0;
        state.player.on_ground = true;
        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        assert!(frame
            .audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::Land)));
    }

    #[test]
    fn audio_event_no_duplicate_land() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        state.player.on_ground = true;

        let input = InputState::default();
        // Two ticks on ground — no Land event
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        assert!(!frame
            .audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::Land)));
    }

    #[test]
    fn audio_event_craft_success_drains_next_tick() {
        let item_defs =
            crate::item::loader::parse_item_defs(include_str!("../../../assets/items.json"))
                .unwrap();
        let entity_defs =
            crate::item::loader::parse_entity_defs(include_str!("../../../assets/entities.json"))
                .unwrap();
        let recipe_defs =
            crate::item::loader::parse_recipe_defs(include_str!("../../../assets/recipes.json"))
                .unwrap();

        let mut state = GameState::new(
            1280.0,
            720.0,
            item_defs,
            entity_defs,
            recipe_defs,
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        // Clear the StreetChanged event from load_street
        state.pending_audio_events.clear();

        state.inventory.add("wood", 3, &state.item_defs);
        state.craft_recipe("plank").unwrap();

        // Craft is now timed — no immediate CraftSuccess event
        assert!(state.active_craft.is_some());
        assert!(!state
            .pending_audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::CraftSuccess { .. })));

        // Advance past the craft duration (plank = 4s)
        let input = InputState::default();
        let frame = state.tick(5.0, &input, &mut rand::thread_rng()).unwrap();
        assert!(frame
            .audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::CraftSuccess { .. })));

        // Next tick should NOT have CraftSuccess
        let frame2 = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
        assert!(!frame2
            .audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::CraftSuccess { .. })));
    }

    #[test]
    fn no_spurious_land_after_transition() {
        use crate::street::types::{Signpost, SignpostConnection};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        let mut street = test_street();
        street.tsid = "LADEMO001".into();
        street.signposts = vec![Signpost {
            id: "sign_right".into(),
            x: 1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO002".into(),
                target_label: "To the Heights".into(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        }];
        state.load_street(street, vec![], vec![]);

        // Player is airborne near the signpost when swoop triggers
        state.player.x = 1950.0;
        state.player.on_ground = false;
        state.player.vy = -100.0;
        state.prev_on_ground = false;

        let input = InputState::default();
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert!(matches!(
            state.transition.phase,
            TransitionPhase::Swooping { .. }
        ));

        state
            .transition
            .mark_street_ready(state.transition.generation);

        let mut new_street = test_street();
        new_street.tsid = "LADEMO002".into();
        new_street.signposts = vec![Signpost {
            id: "sign_left".into(),
            x: -1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO001".into(),
                target_label: "Back to Meadow".into(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        }];
        state.load_street(new_street, vec![], vec![]);

        // Tick through swoop to completion and reset
        let mut frames = vec![];
        for _ in 0..60 {
            if let Some(frame) = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng()) {
                frames.push(frame);
            }
        }

        // No frame should contain a spurious Land event
        let has_land = frames
            .iter()
            .any(|f| f.audio_events.iter().any(|e| matches!(e, AudioEvent::Land)));
        assert!(
            !has_land,
            "Expected no spurious Land event after transition"
        );
    }

    #[test]
    fn audio_event_street_changed_on_load() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);

        assert!(state
            .pending_audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::StreetChanged { .. })));
    }

    #[test]
    fn surface_at_finds_matching_platform() {
        use crate::street::types::{PlatformLine, Point};

        let platforms = vec![
            PlatformLine {
                id: "grass_plat".into(),
                start: Point { x: -200.0, y: 0.0 },
                end: Point { x: 0.0, y: 0.0 },
                pc_perm: None,
                item_perm: None,
                surface: "grass".into(),
            },
            PlatformLine {
                id: "stone_plat".into(),
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 200.0, y: 0.0 },
                pc_perm: None,
                item_perm: None,
                surface: "stone".into(),
            },
        ];

        assert_eq!(surface_at(-100.0, 0.0, 15.0, &platforms), "grass");
        assert_eq!(surface_at(100.0, 0.0, 15.0, &platforms), "stone");
        assert_eq!(surface_at(500.0, 0.0, 15.0, &platforms), "default");
    }

    #[test]
    fn surface_at_returns_default_for_no_platforms() {
        assert_eq!(surface_at(0.0, 0.0, 15.0, &[]), "default");
    }

    #[test]
    fn surface_at_prefers_highest_overlapping_platform() {
        use crate::street::types::{PlatformLine, Point};

        // Flat ground at y=0 (grass) and a slope starting at y=0 rising to y=-100 (stone).
        // At x=500 both overlap — the slope is higher (more negative Y), so physics
        // snaps to the slope. surface_at should return "stone", not "grass".
        let platforms = vec![
            PlatformLine {
                id: "ground".into(),
                start: Point { x: 0.0, y: 0.0 },
                end: Point { x: 1000.0, y: 0.0 },
                pc_perm: None,
                item_perm: None,
                surface: "grass".into(),
            },
            PlatformLine {
                id: "slope".into(),
                start: Point { x: 400.0, y: 0.0 },
                end: Point {
                    x: 800.0,
                    y: -100.0,
                },
                pc_perm: None,
                item_perm: None,
                surface: "stone".into(),
            },
        ];

        // At x=600 (midpoint of slope), slope y = -50, ground y = 0.
        // Player snapped to slope at y=-50 — should return "stone".
        assert_eq!(surface_at(600.0, -50.0, 15.0, &platforms), "stone");

        // At x=400 (start of slope), both are at y=0 — slope should win
        // since it matches and is encountered, but they're at the same Y.
        // Either is acceptable; what matters is we don't return "default".
        let result = surface_at(400.0, 0.0, 15.0, &platforms);
        assert!(result == "grass" || result == "stone");
    }

    fn footstep_test_street() -> StreetData {
        StreetData {
            tsid: "LTEST".into(),
            name: "Test".into(),
            left: -500.0,
            right: 500.0,
            top: -200.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![Layer {
                name: "middleground".into(),
                z: 0,
                w: 1000.0,
                h: 200.0,
                is_middleground: true,
                decos: vec![],
                platform_lines: vec![PlatformLine {
                    id: "plat".into(),
                    start: Point { x: -500.0, y: 0.0 },
                    end: Point { x: 500.0, y: 0.0 },
                    pc_perm: Some(-1),
                    item_perm: None,
                    surface: "grass".into(),
                }],
                walls: vec![],
                ladders: vec![],
                filters: None,
            }],
            signposts: vec![],
            default_spawn: None,
        }
    }

    fn count_footsteps(frame: &Option<RenderFrame>) -> usize {
        frame.as_ref().map_or(0, |f| {
            f.audio_events
                .iter()
                .filter(|e| matches!(e, AudioEvent::Footstep { .. }))
                .count()
        })
    }

    #[test]
    fn footstep_emits_after_stride_distance() {
        use crate::physics::movement::{InputState, FOOTSTEP_STRIDE, WALK_SPEED};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(footstep_test_street(), vec![], vec![]);
        state.player = crate::physics::movement::PhysicsBody::new(0.0, 0.0);
        state.player.on_ground = true;
        state.prev_on_ground = true;

        let dt = 1.0 / 60.0;
        // At WALK_SPEED px/s and dt=1/60 s, each tick moves WALK_SPEED/60 px.
        // Ticks needed to cover one FOOTSTEP_STRIDE = ceil(FOOTSTEP_STRIDE / (WALK_SPEED * dt)).
        let ticks_for_stride = (FOOTSTEP_STRIDE / (WALK_SPEED * dt)).ceil() as usize + 5;

        let walking_right = InputState {
            left: false,
            right: true,
            jump: false,
            interact: false,
        };

        let mut total_footsteps = 0usize;
        let mut rng = rand::thread_rng();
        for _ in 0..ticks_for_stride {
            let frame = state.tick(dt, &walking_right, &mut rng);
            total_footsteps += count_footsteps(&frame);
        }

        assert!(
            total_footsteps >= 1,
            "Expected at least one footstep after walking {ticks_for_stride} ticks; got {total_footsteps}"
        );
    }

    #[test]
    fn no_footstep_while_airborne() {
        use crate::physics::movement::InputState;

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(footstep_test_street(), vec![], vec![]);
        state.player = crate::physics::movement::PhysicsBody::new(0.0, -50.0);
        state.player.on_ground = false;
        state.player.vy = 100.0;
        state.prev_on_ground = false;

        let dt = 1.0 / 60.0;
        let walking_right = InputState {
            left: false,
            right: true,
            jump: false,
            interact: false,
        };

        let mut total_footsteps = 0usize;
        let mut rng = rand::thread_rng();
        for _ in 0..60 {
            let frame = state.tick(dt, &walking_right, &mut rng);
            // Only count ticks where player was still airborne
            if !state.player.on_ground {
                total_footsteps += count_footsteps(&frame);
            }
        }

        assert_eq!(
            total_footsteps, 0,
            "Expected no footsteps while airborne; got {total_footsteps}"
        );
    }

    #[test]
    fn accumulator_resets_on_stop() {
        use crate::physics::movement::{InputState, FOOTSTEP_STRIDE, WALK_SPEED};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(footstep_test_street(), vec![], vec![]);
        state.player = crate::physics::movement::PhysicsBody::new(0.0, 0.0);
        state.player.on_ground = true;
        state.prev_on_ground = true;

        let dt = 1.0 / 60.0;
        // Walk for a few ticks — not enough to reach a full stride
        let partial_ticks = ((FOOTSTEP_STRIDE * 0.5) / (WALK_SPEED * dt)).ceil() as usize;

        let walking_right = InputState {
            left: false,
            right: true,
            jump: false,
            interact: false,
        };
        let stopped = InputState {
            left: false,
            right: false,
            jump: false,
            interact: false,
        };

        let mut rng = rand::thread_rng();
        for _ in 0..partial_ticks {
            state.tick(dt, &walking_right, &mut rng);
        }

        // Stop — accumulator should reset to zero
        state.tick(dt, &stopped, &mut rng);
        let acc_after_stop = state.player.distance_since_footstep;
        assert!(
            acc_after_stop < 1.0,
            "Expected accumulator near zero after stopping; got {acc_after_stop}"
        );

        // Walk again — footstep should come only after a fresh full stride, not
        // immediately from leftover partial distance.
        let ticks_for_stride = (FOOTSTEP_STRIDE / (WALK_SPEED * dt)).ceil() as usize + 5;
        let mut footstep_tick: Option<usize> = None;
        for i in 0..ticks_for_stride {
            let frame = state.tick(dt, &walking_right, &mut rng);
            if count_footsteps(&frame) > 0 && footstep_tick.is_none() {
                footstep_tick = Some(i);
            }
        }

        // The footstep should arrive after a meaningful number of ticks
        // (i.e. the partial distance was not carried over).
        let ticks_floor = (FOOTSTEP_STRIDE * 0.4 / (WALK_SPEED * dt)) as usize;
        if let Some(tick) = footstep_tick {
            assert!(
                tick >= ticks_floor,
                "Footstep came too early (tick {tick}), suggests partial distance was not reset"
            );
        } else {
            panic!("No footstep emitted after walking a full stride from reset");
        }
    }

    #[test]
    fn footstep_surface_is_grass() {
        use crate::physics::movement::{InputState, FOOTSTEP_STRIDE, WALK_SPEED};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(footstep_test_street(), vec![], vec![]);
        state.player = crate::physics::movement::PhysicsBody::new(0.0, 0.0);
        state.player.on_ground = true;
        state.prev_on_ground = true;

        let dt = 1.0 / 60.0;
        let ticks_for_stride = (FOOTSTEP_STRIDE / (WALK_SPEED * dt)).ceil() as usize + 5;

        let walking_right = InputState {
            left: false,
            right: true,
            jump: false,
            interact: false,
        };

        let mut rng = rand::thread_rng();
        let mut footstep_surfaces: Vec<String> = vec![];
        for _ in 0..ticks_for_stride {
            if let Some(frame) = state.tick(dt, &walking_right, &mut rng) {
                for event in &frame.audio_events {
                    if let AudioEvent::Footstep { surface } = event {
                        footstep_surfaces.push(surface.clone());
                    }
                }
            }
        }

        assert!(
            !footstep_surfaces.is_empty(),
            "Expected at least one footstep event"
        );
        for surface in &footstep_surfaces {
            assert_eq!(
                surface, "grass",
                "Expected surface 'grass', got '{surface}'"
            );
        }
    }

    #[test]
    fn energy_decays_per_tick() {
        let mut state = GameState::new(
            800.0,
            600.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);

        let initial_energy = state.energy;
        let input = InputState {
            left: false,
            right: false,
            jump: false,
            interact: false,
        };
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);

        // Tick for 1 second at 60fps
        for _ in 0..60 {
            state.tick(1.0 / 60.0, &input, &mut rng);
        }

        // After 1s at 0.1/sec decay: should lose ~0.1 energy
        let lost = initial_energy - state.energy;
        assert!(
            lost > 0.09 && lost < 0.11,
            "Expected ~0.1 energy loss, got {lost}"
        );
    }

    #[test]
    fn energy_does_not_decay_below_zero() {
        let mut state = GameState::new(
            800.0,
            600.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);
        state.energy = 0.01; // Almost empty

        let input = InputState {
            left: false,
            right: false,
            jump: false,
            interact: false,
        };
        let mut rng = rand::rngs::mock::StepRng::new(0, 1);

        // Tick for 10 seconds — should clamp at 0, not go negative
        for _ in 0..600 {
            state.tick(1.0 / 60.0, &input, &mut rng);
        }

        assert_eq!(state.energy, 0.0);
    }

    #[test]
    fn transition_completion_uses_pending_arrival_when_set() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);

        // Simulate mid-transition state: pending_arrival set, origin_tsid set,
        // transition phase Complete.
        state.pending_arrival = Some(crate::street::types::SpawnPoint {
            x: 333.0,
            y: -17.0,
            facing: Some(crate::street::types::Facing::Left),
        });
        state.transition_origin_tsid = Some("OTHERTSID".to_string());
        state.transition.phase = crate::engine::transition::TransitionPhase::Complete {
            new_street: "test".to_string(),
            direction: crate::engine::transition::TransitionDirection::Right,
        };

        let input = InputState::default();
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

        // Player x is applied from pending_arrival and isn't changed by physics
        // (no horizontal input). Player y is applied pre-physics; after the
        // physics sub-tick, gravity nudges y toward the ground platform, so we
        // only check that last_arrival (which isn't touched by physics) records
        // the exact pending_arrival values.
        assert_eq!(state.player.x, 333.0);
        assert_eq!(state.last_arrival.x, 333.0);
        assert_eq!(state.last_arrival.y, -17.0);
        // origin_tsid consumed, pending_arrival cleared.
        assert!(state.transition_origin_tsid.is_none());
        assert!(state.pending_arrival.is_none());
        assert_eq!(state.oob_ticks, 0);
    }

    #[test]
    fn transition_completion_falls_back_to_reciprocal_signpost() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        // Street where a signpost at x=600 connects back to OTHERTSID.
        use crate::street::types::{Signpost, SignpostConnection};
        let mut street = test_street();
        street.signposts.push(Signpost {
            id: "reciprocal".to_string(),
            x: 600.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "OTHERTSID".to_string(),
                target_label: "Back".to_string(),
                arrival_x: None,
                arrival_y: None,
                arrival_facing: None,
            }],
        });
        state.load_street(street, vec![], vec![]);

        state.pending_arrival = None; // legacy path
        state.transition_origin_tsid = Some("OTHERTSID".to_string());
        state.transition.phase = crate::engine::transition::TransitionPhase::Complete {
            new_street: "test".to_string(),
            direction: crate::engine::transition::TransitionDirection::Right,
        };

        let input = InputState::default();
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

        assert_eq!(state.player.x, 600.0);
        assert_eq!(state.player.y, 0.0); // ground_y
        assert_eq!(state.last_arrival.x, 600.0);
        assert!(state.transition_origin_tsid.is_none());
    }
}

#[cfg(test)]
mod save_tests {
    use super::*;
    use crate::avatar::types::AvatarAppearance;
    use crate::item::types::ItemStack;
    use std::collections::HashMap;

    fn empty_catalog() -> TrackCatalog {
        TrackCatalog {
            tracks: HashMap::new(),
        }
    }

    fn empty_store_catalog() -> StoreCatalog {
        StoreCatalog {
            stores: HashMap::new(),
        }
    }

    fn empty_skill_defs() -> SkillDefs {
        HashMap::new()
    }

    #[test]
    fn save_state_round_trip() {
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: -500.0,
            y: -100.0,
            facing: Direction::Right,
            inventory: vec![
                Some(ItemStack {
                    item_id: "cherry".to_string(),
                    count: 5,
                }),
                None,
                Some(ItemStack {
                    item_id: "grain".to_string(),
                    count: 2,
                }),
            ],
            avatar: AvatarAppearance::default(),
            currants: 50,
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            skill_progress: SkillProgress::default(),
            upgrades: PlayerUpgrades::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };
        let json = serde_json::to_string(&save).unwrap();
        let loaded: SaveState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.street_id, "demo_meadow");
        assert!((loaded.x - (-500.0)).abs() < f64::EPSILON);
        assert!((loaded.y - (-100.0)).abs() < f64::EPSILON);
        assert_eq!(loaded.facing, Direction::Right);
        assert_eq!(loaded.inventory.len(), 3);
        assert_eq!(loaded.inventory[0].as_ref().unwrap().item_id, "cherry");
        assert!(loaded.inventory[1].is_none());
    }

    #[test]
    fn save_state_uses_camel_case() {
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Left,
            inventory: vec![Some(ItemStack {
                item_id: "cherry".to_string(),
                count: 1,
            })],
            avatar: AvatarAppearance::default(),
            currants: 50,
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            skill_progress: SkillProgress::default(),
            upgrades: PlayerUpgrades::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };
        let json = serde_json::to_string(&save).unwrap();
        assert!(
            json.contains("\"streetId\""),
            "Should use camelCase: {json}"
        );
        assert!(
            json.contains("\"itemId\""),
            "ItemStack should use camelCase: {json}"
        );
    }

    #[test]
    fn empty_inventory_round_trip() {
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Left,
            inventory: vec![None; 16],
            avatar: AvatarAppearance::default(),
            currants: 50,
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            skill_progress: SkillProgress::default(),
            upgrades: PlayerUpgrades::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };
        let json = serde_json::to_string(&save).unwrap();
        let loaded: SaveState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.inventory.len(), 16);
        assert!(loaded.inventory.iter().all(|s| s.is_none()));
    }

    #[test]
    fn write_and_read_save_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("savegame.json");

        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 100.0,
            y: -50.0,
            facing: Direction::Right,
            inventory: vec![
                Some(ItemStack {
                    item_id: "cherry".to_string(),
                    count: 3,
                }),
                None,
            ],
            avatar: AvatarAppearance::default(),
            currants: 50,
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            skill_progress: SkillProgress::default(),
            upgrades: PlayerUpgrades::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };

        write_save_state(&path, &save).unwrap();
        let loaded = read_save_state(&path).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.street_id, "demo_meadow");
        assert!((loaded.x - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn missing_save_file_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        let result = read_save_state(&path);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn corrupted_save_file_returns_none() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("savegame.json");
        std::fs::write(&path, "not valid json!!!").unwrap();
        let result = read_save_state(&path);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn restore_save_clamps_position() {
        let item_defs = crate::item::types::ItemDefs::new();
        let entity_defs = crate::item::types::EntityDefs::new();
        let recipe_defs = crate::item::types::RecipeDefs::new();
        let mut state = GameState::new(
            1280.0,
            720.0,
            item_defs,
            entity_defs,
            recipe_defs,
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );

        // Load a street to get bounds
        let xml = include_str!("../../../assets/streets/demo_meadow.xml");
        let street = crate::street::parser::parse_street(xml).unwrap();
        state.load_street(street, vec![], vec![]);

        // Try to restore a position way outside bounds.
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 99999.0,
            y: -99999.0,
            facing: Direction::Left,
            inventory: vec![],
            avatar: AvatarAppearance::default(),
            currants: 50,
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            skill_progress: SkillProgress::default(),
            upgrades: PlayerUpgrades::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };
        state.restore_save(&save);

        // Position should be clamped to street bounds.
        let street = state.street.as_ref().unwrap();
        assert!(
            state.player.x <= street.right - 1.0,
            "x should be clamped to right bound"
        );
        assert!(
            state.player.y >= street.top + 1.0,
            "y should be clamped to top bound"
        );
        assert_eq!(state.facing, Direction::Left);
    }

    #[test]
    fn restore_save_fills_inventory() {
        let item_defs = crate::item::types::ItemDefs::new();
        let entity_defs = crate::item::types::EntityDefs::new();
        let recipe_defs = crate::item::types::RecipeDefs::new();
        let mut state = GameState::new(
            1280.0,
            720.0,
            item_defs,
            entity_defs,
            recipe_defs,
            empty_catalog(),
            empty_store_catalog(),
            empty_skill_defs(),
            HashMap::new(),
            HashMap::new(),
        );
        assert_eq!(state.inventory.slots.len(), 16);

        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Right,
            inventory: vec![Some(ItemStack {
                item_id: "cherry".to_string(),
                count: 5,
            })],
            avatar: AvatarAppearance::default(),
            currants: 50,
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            skill_progress: SkillProgress::default(),
            upgrades: PlayerUpgrades::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };
        state.restore_save(&save);

        // Capacity preserved at 16, first slot has cherry, rest are None.
        assert_eq!(state.inventory.slots.len(), 16);
        assert_eq!(state.inventory.slots[0].as_ref().unwrap().item_id, "cherry");
        assert!(state.inventory.slots[1].is_none());
    }

    #[test]
    fn save_state_currants_default() {
        // Deserializing a SaveState JSON without currants should default to 50.
        // Serialize a default SaveState (without currants field) and strip the currants field.
        let full = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Right,
            inventory: vec![],
            avatar: AvatarAppearance::default(),
            currants: 999, // will be stripped below
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            skill_progress: SkillProgress::default(),
            upgrades: PlayerUpgrades::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };
        let mut value: serde_json::Value = serde_json::to_value(&full).unwrap();
        value.as_object_mut().unwrap().remove("currants");
        let json = serde_json::to_string(&value).unwrap();
        let save: SaveState = serde_json::from_str(&json).unwrap();
        assert_eq!(save.currants, 50);
    }

    #[test]
    fn save_state_currants_round_trip() {
        // Serializing and deserializing SaveState preserves currants value.
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Right,
            inventory: vec![],
            avatar: AvatarAppearance::default(),
            currants: 999,
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            skill_progress: SkillProgress::default(),
            upgrades: PlayerUpgrades::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };
        let json = serde_json::to_string(&save).unwrap();
        let loaded: SaveState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.currants, 999);
    }

    #[test]
    fn save_state_energy_default() {
        let json =
            r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50}"#;
        let save: SaveState = serde_json::from_str(json).unwrap();
        assert_eq!(save.energy, 600.0);
    }

    #[test]
    fn save_state_energy_round_trip() {
        let save = SaveState {
            street_id: "demo".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Right,
            inventory: vec![],
            avatar: AvatarAppearance::default(),
            currants: 50,
            energy: 123.4,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 0,
            skill_progress: SkillProgress::default(),
            upgrades: PlayerUpgrades::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };
        let json = serde_json::to_string(&save).unwrap();
        let restored: SaveState = serde_json::from_str(&json).unwrap();
        assert!((restored.energy - 123.4).abs() < f64::EPSILON);
    }

    #[test]
    fn save_state_imagination_default() {
        let json = r#"{
            "streetId": "demo_meadow",
            "x": 0, "y": 0,
            "facing": "right",
            "inventory": []
        }"#;
        let save: SaveState = serde_json::from_str(json).unwrap();
        assert_eq!(save.imagination, 0);
        assert_eq!(save.upgrades.energy_tank_tier, 0);
        assert_eq!(save.upgrades.haggling_tier, 0);
    }

    #[test]
    fn save_state_imagination_round_trip() {
        use crate::item::imagination::PlayerUpgrades;
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Right,
            inventory: vec![],
            avatar: AvatarAppearance::default(),
            currants: 50,
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 42,
            upgrades: PlayerUpgrades {
                energy_tank_tier: 2,
                haggling_tier: 1,
            },
            skill_progress: SkillProgress::default(),
            quest_progress: crate::quest::types::QuestProgress::default(),
            mood: 100.0,
            max_mood: 100.0,
            buddies: vec![],
            blocked: vec![],
            last_hi_date: None,
        };
        let json = serde_json::to_string(&save).unwrap();
        let parsed: SaveState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.imagination, 42);
        assert_eq!(parsed.upgrades.energy_tank_tier, 2);
        assert_eq!(parsed.upgrades.haggling_tier, 1);
    }

    #[test]
    fn craft_earns_imagination() {
        use crate::street::types::{Gradient, StreetData};
        let item_defs =
            crate::item::loader::parse_item_defs(include_str!("../../../assets/items.json"))
                .unwrap();
        let entity_defs =
            crate::item::loader::parse_entity_defs(include_str!("../../../assets/entities.json"))
                .unwrap();
        let recipe_defs =
            crate::item::loader::parse_recipe_defs(include_str!("../../../assets/recipes.json"))
                .unwrap();
        let track_catalog = crate::engine::jukebox::TrackCatalog {
            tracks: std::collections::HashMap::new(),
        };
        let store_catalog = crate::item::types::StoreCatalog {
            stores: std::collections::HashMap::new(),
        };
        let skill_defs =
            crate::skill::loader::parse_skill_defs(include_str!("../../../assets/skills.json"))
                .unwrap();
        let mut state = GameState::new(
            800.0,
            600.0,
            item_defs,
            entity_defs,
            recipe_defs,
            track_catalog,
            store_catalog,
            skill_defs,
            HashMap::new(),
            HashMap::new(),
        );
        state.load_street(
            StreetData {
                tsid: "test".into(),
                name: "Test".into(),
                left: -3000.0,
                right: 3000.0,
                top: -1000.0,
                bottom: 0.0,
                ground_y: 0.0,
                gradient: Some(Gradient {
                    top: "#000".into(),
                    bottom: "#111".into(),
                }),
                layers: vec![],
                signposts: vec![],
                default_spawn: None,
            },
            vec![],
            vec![],
        );

        // Pre-learn required skills for cherry_pie (cooking_1 → cooking_2)
        state.skill_progress.learned.push("cooking_1".to_string());
        state.skill_progress.learned.push("cooking_2".to_string());

        // Give player cherry_pie ingredients
        state.inventory.add("cherry", 10, &state.item_defs);
        state.inventory.add("grain", 5, &state.item_defs);
        state.inventory.add("pot", 1, &state.item_defs);

        let before = state.imagination;
        // Start timed craft
        state.craft_recipe("cherry_pie").unwrap();
        // iMG not earned yet (craft is in progress)
        assert_eq!(state.imagination, before);

        // Advance past craft completion time, then tick to deliver outputs
        state.game_time = 100.0;
        let input = InputState::default();
        let frame = state.tick(0.1, &input, &mut rand::thread_rng()).unwrap();

        // iMG earned on craft completion: cherry_pie base_cost=20, 2x = 40
        assert_eq!(state.imagination - before, 40);
        assert!(frame.pickup_feedback.iter().any(|f| f.text.contains("iMG")));
    }

    #[test]
    fn save_state_mood_default() {
        let json = r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50}"#;
        let save: SaveState = serde_json::from_str(json).unwrap();
        assert_eq!(save.mood, 100.0);
        assert_eq!(save.max_mood, 100.0);
    }

    #[test]
    fn save_state_mood_round_trip() {
        let json = r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50,"mood":72.5,"maxMood":100.0}"#;
        let save: SaveState = serde_json::from_str(json).unwrap();
        assert!((save.mood - 72.5).abs() < f64::EPSILON);
        assert!((save.max_mood - 100.0).abs() < f64::EPSILON);
        let reserialized = serde_json::to_string(&save).unwrap();
        let restored: SaveState = serde_json::from_str(&reserialized).unwrap();
        assert!((restored.mood - 72.5).abs() < f64::EPSILON);
    }

    #[test]
    fn save_state_buddies_default_empty() {
        let json = r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50}"#;
        let save: SaveState = serde_json::from_str(json).unwrap();
        assert!(save.buddies.is_empty());
        assert!(save.blocked.is_empty());
    }

    #[test]
    fn save_state_buddies_round_trip() {
        let json = r#"{"streetId":"demo","x":0,"y":0,"facing":"right","inventory":[],"currants":50,"buddies":[{"addressHash":"01010101010101010101010101010101","displayName":"Alice","addedDate":"2026-04-10","coPresenceTotal":100.0,"lastSeenDate":"2026-04-10"}],"blocked":["02020202020202020202020202020202"]}"#;
        let save: SaveState = serde_json::from_str(json).unwrap();
        assert_eq!(save.buddies.len(), 1);
        assert_eq!(save.blocked.len(), 1);
        let reserialized = serde_json::to_string(&save).unwrap();
        let restored: SaveState = serde_json::from_str(&reserialized).unwrap();
        assert_eq!(restored.buddies.len(), 1);
        assert_eq!(restored.blocked.len(), 1);
    }
}
