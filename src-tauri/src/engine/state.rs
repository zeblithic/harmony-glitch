use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::avatar::types::{AnimationState, Direction};
use crate::engine::audio::AudioEvent;
use crate::engine::transition::{
    TransitionDirection, TransitionPhase, TransitionState, PRE_SUBSCRIBE_DISTANCE,
};
use crate::item::interaction;
use crate::item::inventory::Inventory;
use crate::item::types::{
    EntityDefs, EntityInstanceState, InteractionPrompt, InventoryFrame, ItemDefs, ItemStackFrame,
    PickupFeedback, RecipeDefs, WorldEntity, WorldEntityFrame, WorldItem, WorldItemFrame,
};
use crate::physics::movement::{InputState, PhysicsBody};
use crate::street::types::StreetData;

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
    pub tsid_to_name: std::collections::HashMap<String, String>,
    pub entity_states: std::collections::HashMap<String, EntityInstanceState>,
    pub game_time: f64,
    pub audio_events: Vec<AudioEvent>,
    pub pending_audio_events: Vec<AudioEvent>,
    pub prev_on_ground: bool,
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
}

impl GameState {
    pub fn new(
        viewport_width: f64,
        viewport_height: f64,
        item_defs: ItemDefs,
        entity_defs: EntityDefs,
        recipe_defs: RecipeDefs,
    ) -> Self {
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
            tsid_to_name: std::collections::HashMap::from([
                ("LADEMO001".to_string(), "demo_meadow".to_string()),
                ("LADEMO002".to_string(), "demo_heights".to_string()),
            ]),
            entity_states: std::collections::HashMap::new(),
            game_time: 0.0,
            audio_events: vec![],
            pending_audio_events: vec![],
            prev_on_ground: true,
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
            let center_x = (street.left + street.right) / 2.0;
            self.player = PhysicsBody::new(center_x, street.ground_y);
        }
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
    }

    /// Execute a crafting recipe. On success, generates pickup feedback.
    pub fn craft_recipe(
        &mut self,
        recipe_id: &str,
    ) -> Result<Vec<crate::item::types::CraftOutput>, crate::item::types::CraftError> {
        let recipe = self
            .recipe_defs
            .get(recipe_id)
            .ok_or(crate::item::types::CraftError::UnknownRecipe)?
            .clone();
        let result = crate::item::crafting::craft(&recipe, &mut self.inventory, &self.item_defs)?;
        for output in &result {
            self.pickup_feedback.push(PickupFeedback {
                id: self.next_feedback_id,
                text: format!("+{} x{}", output.name, output.count),
                success: true,
                x: self.player.x,
                y: self.player.y,
                age_secs: 0.0,
            });
            self.next_feedback_id += 1;
        }
        self.pending_audio_events.push(AudioEvent::CraftSuccess {
            recipe_id: recipe.id.clone(),
        });
        Ok(result)
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
        // Drain pending audio events from IPC commands (craft_recipe, load_street)
        let mut audio_events: Vec<AudioEvent> = std::mem::take(&mut self.pending_audio_events);

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
                    self.transition_origin_tsid = Some(street.tsid.clone());
                    self.transition.trigger_swoop();
                }
            }
        }

        let was_swooping = matches!(self.transition.phase, TransitionPhase::Swooping { .. });
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
                let return_signpost = street
                    .signposts
                    .iter()
                    .find(|s| s.connects.iter().any(|c| c.target_tsid == origin_tsid));

                if let Some(sp) = return_signpost {
                    let street_mid = (street.left + street.right) / 2.0;
                    let inward = if sp.x < street_mid { 1.0 } else { -1.0 };
                    self.player.x = sp.x + inward * (PRE_SUBSCRIBE_DISTANCE + 50.0);
                    self.player.y = street.ground_y;
                    self.player.vx = 0.0;
                    self.player.vy = 0.0;
                } else {
                    self.player.x = (street.left + street.right) / 2.0;
                    self.player.y = street.ground_y;
                    self.player.vx = 0.0;
                    self.player.vy = 0.0;
                }
            } else {
                self.transition.reset();
            }
        }

        // Timeout path: Swooping → None without visiting Complete.
        // The Complete handler already clears transition_origin_tsid, so this
        // only fires when Complete was never visited (timeout cancellation).
        // Prevents a late-arriving loadStreet from seeing is_transitioning=false
        // and mis-positioning the player.
        if was_swooping
            && self.transition.phase == TransitionPhase::None
            && self.transition_origin_tsid.is_some()
        {
            self.transition_origin_tsid = None;
        }

        // Re-check swooping state after transition system may have changed it.
        let is_swooping = matches!(self.transition.phase, TransitionPhase::Swooping { .. });

        // Transition audio events
        if !was_swooping && is_swooping {
            audio_events.push(AudioEvent::TransitionStart);
        }
        if was_swooping && !is_swooping {
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
            );

            // Jump/Land audio detection
            if self.prev_on_ground && !self.player.on_ground && self.player.vy < 0.0 {
                audio_events.push(AudioEvent::Jump);
            }
            if !self.prev_on_ground && self.player.on_ground {
                audio_events.push(AudioEvent::Land);
            }
            self.prev_on_ground = self.player.on_ground;

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

        // Camera: center on player, clamped to street bounds.
        // When the street is smaller than the viewport, center the street
        // instead of panicking (f64::clamp requires min <= max).
        let cam_x = self.player.x - self.viewport_width / 2.0;
        let cam_y = self.player.y - self.viewport_height * 0.6; // Player in lower 40%
        let cam_x_min = street.left;
        let cam_x_max = (street.right - self.viewport_width).max(cam_x_min);
        let cam_y_min = street.top;
        let cam_y_max = (street.bottom - self.viewport_height).max(cam_y_min);
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
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::transition::TransitionPhase;
    use crate::item::types::{EntityDefs, ItemDefs};
    use crate::street::types::*;
    use std::collections::HashMap;

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
                }],
                walls: vec![],
                ladders: vec![],
                filters: None,
            }],
            signposts: vec![],
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
        );
        state.load_street(test_street(), vec![], vec![]);
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert!(frame.is_some());
    }

    #[test]
    fn tick_returns_none_without_street() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
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
                }],
                walls: vec![],
                ladders: vec![],
                filters: None,
            }],
            signposts: vec![],
        };
        state.load_street(small_street, vec![], vec![]);

        // Should not panic — camera clamp handles min > max gracefully
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert!(frame.is_some());
    }

    #[test]
    fn load_street_places_player() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
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
            },
        );

        let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, HashMap::new());
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
        );
        let mut street = test_street();
        street.signposts = vec![Signpost {
            id: "sign_right".into(),
            x: 1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO002".into(),
                target_label: "To the Heights".into(),
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
        );
        let mut street = test_street();
        street.signposts = vec![Signpost {
            id: "sign_right".into(),
            x: 1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO002".into(),
                target_label: "To the Heights".into(),
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
    fn tick_freezes_input_during_swoop() {
        use crate::street::types::{Signpost, SignpostConnection};

        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
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
        );
        let mut street = test_street();
        street.signposts = vec![Signpost {
            id: "sign_right".into(),
            x: 1900.0,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: "LADEMO002".into(),
                target_label: "To the Heights".into(),
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
            }],
        }];
        state.load_street(new_street, vec![], vec![]);

        for _ in 0..30 {
            state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        }

        assert_eq!(state.transition.phase, TransitionPhase::None);
        assert!(state.transition_origin_tsid.is_none());
        // Player is placed PRE_SUBSCRIBE_DISTANCE + 50px inward from the return
        // signpost (x=-1900), i.e. at x=-1350, so signpost detection resets cleanly.
        let expected_x = -1900.0 + (PRE_SUBSCRIBE_DISTANCE + 50.0);
        assert!(
            (state.player.x - expected_x).abs() < 1.0,
            "Player should be at x={} (just outside pre-subscribe zone), got {}",
            expected_x,
            state.player.x
        );
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
            },
        );

        let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, HashMap::new());
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
            },
        );

        let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, HashMap::new());
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
            },
        );
        defs
    }

    #[test]
    fn tick_entities_moves_movable_entity() {
        use rand::SeedableRng;

        let defs = movable_entity_defs();
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs, HashMap::new());
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
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs, HashMap::new());
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
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs, HashMap::new());
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
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs, HashMap::new());
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
            },
        );

        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs, HashMap::new());
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
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs, HashMap::new());
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
            let mut state =
                GameState::new(1280.0, 720.0, ItemDefs::new(), defs.clone(), HashMap::new());
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
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs, HashMap::new());
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
            },
        );

        let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, HashMap::new());
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
            },
        );

        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs, HashMap::new());
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
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs, HashMap::new());
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

        let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, recipe_defs);
        state.inventory.add("cherry", 10, &state.item_defs);
        state.inventory.add("grain", 5, &state.item_defs);
        state.inventory.add("pot", 1, &state.item_defs);

        let result = state.craft_recipe("cherry_pie");
        assert!(result.is_ok());

        assert_eq!(state.pickup_feedback.len(), 1);
        assert!(state.pickup_feedback[0].text.contains("Cherry Pie"));
        assert!(state.pickup_feedback[0].success);

        assert_eq!(state.inventory.count_item("cherry_pie"), 1);
        assert_eq!(state.inventory.count_item("cherry"), 5);
        assert_eq!(state.inventory.count_item("pot"), 1);
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

        let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, recipe_defs);

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

        let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, recipe_defs);
        state.load_street(test_street(), vec![], vec![]);
        // Clear the StreetChanged event from load_street
        state.pending_audio_events.clear();

        state.inventory.add("wood", 3, &state.item_defs);
        state.craft_recipe("plank").unwrap();

        assert!(state
            .pending_audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::CraftSuccess { .. })));

        let input = InputState::default();
        let frame = state
            .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
            .unwrap();
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
    fn audio_event_street_changed_on_load() {
        let mut state = GameState::new(
            1280.0,
            720.0,
            ItemDefs::new(),
            EntityDefs::new(),
            HashMap::new(),
        );
        state.load_street(test_street(), vec![], vec![]);

        assert!(state
            .pending_audio_events
            .iter()
            .any(|e| matches!(e, AudioEvent::StreetChanged { .. })));
    }
}
