use serde::{Deserialize, Serialize};

use crate::avatar::types::{AnimationState, Direction};
use crate::physics::movement::{InputState, PhysicsBody};
use crate::street::types::StreetData;

/// The complete game state.
pub struct GameState {
    pub player: PhysicsBody,
    pub facing: Direction,
    pub street: Option<StreetData>,
    pub viewport_width: f64,
    pub viewport_height: f64,
}

/// Data sent to the frontend each tick for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFrame {
    pub player: PlayerFrame,
    pub camera: CameraFrame,
    pub street_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerFrame {
    pub x: f64,
    pub y: f64,
    pub facing: Direction,
    pub animation: AnimationState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraFrame {
    pub x: f64,
    pub y: f64,
}

impl GameState {
    pub fn new(viewport_width: f64, viewport_height: f64) -> Self {
        Self {
            player: PhysicsBody::new(0.0, -100.0),
            facing: Direction::Right,
            street: None,
            viewport_width,
            viewport_height,
        }
    }

    pub fn load_street(&mut self, street: StreetData) {
        // Place player at ground level, center of street
        let center_x = (street.left + street.right) / 2.0;
        self.player = PhysicsBody::new(center_x, street.ground_y - 100.0);
        self.street = Some(street);
    }

    /// Run one tick of the game loop.
    pub fn tick(&mut self, dt: f64, input: &InputState) -> Option<RenderFrame> {
        let street = self.street.as_ref()?;

        // Update facing direction
        if input.left && !input.right {
            self.facing = Direction::Left;
        } else if input.right && !input.left {
            self.facing = Direction::Right;
        }

        // Physics tick
        self.player
            .tick(dt, input, street.platforms(), street.left, street.right);

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
                facing: self.facing,
                animation,
            },
            camera: CameraFrame { x: cam_x, y: cam_y },
            street_id: street.tsid.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input);
        assert!(frame.is_some());
    }

    #[test]
    fn tick_returns_none_without_street() {
        let mut state = GameState::new(1280.0, 720.0);
        let input = InputState::default();
        assert!(state.tick(1.0 / 60.0, &input).is_none());
    }

    #[test]
    fn facing_updates_from_input() {
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());

        let input = InputState {
            left: true,
            ..Default::default()
        };
        state.tick(1.0 / 60.0, &input);
        assert_eq!(state.facing, Direction::Left);

        let input = InputState {
            right: true,
            ..Default::default()
        };
        state.tick(1.0 / 60.0, &input);
        assert_eq!(state.facing, Direction::Right);
    }

    #[test]
    fn animation_idle_on_ground() {
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());
        state.player.on_ground = true;
        state.player.y = 0.0;
        state.player.vy = 0.0;

        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input).unwrap();
        assert_eq!(frame.player.animation, AnimationState::Idle);
    }

    #[test]
    fn animation_walking() {
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());
        state.player.on_ground = true;
        state.player.y = 0.0;

        let input = InputState {
            right: true,
            ..Default::default()
        };
        let frame = state.tick(1.0 / 60.0, &input).unwrap();
        assert_eq!(frame.player.animation, AnimationState::Walking);
    }

    #[test]
    fn camera_does_not_panic_on_small_street() {
        // Street smaller than viewport (600px wide, 400px tall vs 1280x720 viewport)
        let mut state = GameState::new(1280.0, 720.0);
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
        state.load_street(small_street);

        // Should not panic — camera clamp handles min > max gracefully
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input);
        assert!(frame.is_some());
    }

    #[test]
    fn load_street_places_player() {
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());
        // Player should be at center of street
        assert!((state.player.x - 0.0).abs() < 1.0);
    }
}
