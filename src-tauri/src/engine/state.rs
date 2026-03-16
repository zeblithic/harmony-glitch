use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::avatar::types::{AnimationState, Direction};
use crate::item::interaction;
use crate::item::inventory::Inventory;
use crate::item::types::{
    EntityDefs, InteractionPrompt, InventoryFrame, ItemDefs, ItemStackFrame, PickupFeedback,
    WorldEntity, WorldEntityFrame, WorldItem, WorldItemFrame,
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
    pub prev_interact: bool,
    pub next_item_id: u64,
    pub next_feedback_id: u64,
    pub pickup_feedback: Vec<PickupFeedback>,
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
            prev_interact: false,
            next_item_id: 0,
            next_feedback_id: 0,
            pickup_feedback: vec![],
        }
    }

    pub fn load_street(&mut self, street: StreetData, entities: Vec<WorldEntity>) {
        // Place player at ground level, center of street.
        // Spawning directly at ground_y ensures the first physics tick's
        // swept collision snaps to the nearest platform below.
        let center_x = (street.left + street.right) / 2.0;
        self.player = PhysicsBody::new(center_x, street.ground_y);
        self.street = Some(street);
        self.world_entities = entities;
        self.world_items.clear();
        self.pickup_feedback.clear();
    }

    /// Run one tick of the game loop.
    pub fn tick(&mut self, dt: f64, input: &InputState, rng: &mut impl Rng) -> Option<RenderFrame> {
        let street = self.street.as_ref()?;

        // Update facing direction
        if input.left && !input.right {
            self.facing = Direction::Left;
        } else if input.right && !input.left {
            self.facing = Direction::Right;
        }

        // Physics tick — walls are parsed from street data but not yet enforced
        // in the collision system (Phase A scope: platforms only).
        self.player
            .tick(dt, input, street.platforms(), street.left, street.right);

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
            )
        });

        // Rising edge detection for interact
        let interact_pressed = input.interact && !self.prev_interact;
        self.prev_interact = input.interact;

        // Execute interaction on rising edge
        let mut interacted = false;
        if interact_pressed {
            if let Some(nearest) = &nearest {
                let result = interaction::execute_interaction(
                    nearest,
                    &mut self.inventory,
                    &self.world_entities,
                    &self.entity_defs,
                    &self.world_items,
                    &self.item_defs,
                    rng,
                );

                // Apply results — assign unique IDs to feedback
                for mut fb in result.feedback {
                    fb.id = self.next_feedback_id;
                    self.next_feedback_id += 1;
                    self.pickup_feedback.push(fb);
                }

                // Spawn overflow items
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

                // Remove or update ground items
                if let Some(idx) = result.remove_ground_item {
                    self.world_items.remove(idx);
                } else if let Some((idx, new_count)) = result.update_ground_item {
                    self.world_items[idx].count = new_count;
                }

                interacted = true;
            }
        }

        // Clear prompt on the frame where interaction happened — the target
        // may have been removed or changed, so the pre-interaction prompt is stale.
        let interaction_prompt = if interacted { None } else { interaction_prompt };

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
        })
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
                WorldEntityFrame {
                    id: e.id.clone(),
                    entity_type: e.entity_type.clone(),
                    name: def.map(|d| d.name.clone()).unwrap_or_default(),
                    sprite_class: def.map(|d| d.sprite_class.clone()).unwrap_or_default(),
                    x: e.x,
                    y: e.y,
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
    use crate::item::types::{EntityDefs, ItemDefs};
    use crate::street::types::*;

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
                    start: Point {
                        x: -2800.0,
                        y: 0.0,
                    },
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
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
        state.load_street(test_street(), vec![]);
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert!(frame.is_some());
    }

    #[test]
    fn tick_returns_none_without_street() {
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
        let input = InputState::default();
        assert!(state.tick(1.0 / 60.0, &input, &mut rand::thread_rng()).is_none());
    }

    #[test]
    fn facing_updates_from_input() {
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
        state.load_street(test_street(), vec![]);

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
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
        state.load_street(test_street(), vec![]);
        state.player.on_ground = true;
        state.player.y = 0.0;
        state.player.vy = 0.0;

        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng()).unwrap();
        assert_eq!(frame.player.animation, AnimationState::Idle);
    }

    #[test]
    fn animation_walking() {
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
        state.load_street(test_street(), vec![]);
        state.player.on_ground = true;
        state.player.y = 0.0;

        let input = InputState {
            right: true,
            ..Default::default()
        };
        let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng()).unwrap();
        assert_eq!(frame.player.animation, AnimationState::Walking);
    }

    #[test]
    fn camera_does_not_panic_on_small_street() {
        // Street smaller than viewport (600px wide, 400px tall vs 1280x720 viewport)
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
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
        state.load_street(small_street, vec![]);

        // Should not panic — camera clamp handles min > max gracefully
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
        assert!(frame.is_some());
    }

    #[test]
    fn load_street_places_player() {
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
        state.load_street(test_street(), vec![]);
        // Player should be at center of street
        assert!((state.player.x - 0.0).abs() < 1.0);
    }

    #[test]
    fn interaction_adds_to_inventory() {
        use crate::item::types::{EntityDef, ItemDef, YieldEntry, WorldEntity};
        use rand::SeedableRng;

        let mut item_defs = ItemDefs::new();
        item_defs.insert("cherry".into(), ItemDef {
            id: "cherry".into(),
            name: "Cherry".into(),
            description: "".into(),
            category: "food".into(),
            stack_limit: 50,
            icon: "cherry".into(),
        });
        let mut entity_defs = EntityDefs::new();
        entity_defs.insert("fruit_tree".into(), EntityDef {
            id: "fruit_tree".into(),
            name: "Fruit Tree".into(),
            verb: "Harvest".into(),
            yields: vec![YieldEntry { item: "cherry".into(), min: 1, max: 1 }],
            cooldown_secs: 0.0,
            sprite_class: "tree_fruit".into(),
            interact_radius: 80.0,
        });

        let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs);
        let street = test_street();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 0.0,
            y: 0.0,
        }];
        state.load_street(street, entities);

        // Stand next to tree and press interact
        let input = InputState { interact: true, ..Default::default() };
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();

        assert_eq!(frame.inventory.slots[0].as_ref().unwrap().item_id, "cherry");
        assert!(frame.pickup_feedback.iter().any(|f| f.success));
    }
}
