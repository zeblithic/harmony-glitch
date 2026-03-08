use serde::{Deserialize, Serialize};

use crate::street::types::PlatformLine;

/// Physics constants (tunable).
pub const GRAVITY: f64 = 980.0; // px/s²
pub const WALK_SPEED: f64 = 200.0; // px/s
pub const JUMP_VELOCITY: f64 = -400.0; // px/s (negative = up in Glitch coords)
pub const TERMINAL_VELOCITY: f64 = 600.0; // px/s
/// Max Y displacement to snap to when walking along a slope.
/// Must exceed the steepest slope's Y change per frame at walk speed.
const SLOPE_SNAP_TOLERANCE: f64 = 10.0;

/// Player physics state.
#[derive(Debug, Clone)]
pub struct PhysicsBody {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub on_ground: bool,
    /// Half-width for collision (avatar is centered on x).
    pub half_width: f64,
    /// Height from feet (y) to head.
    pub height: f64,
    /// Previous tick's jump input — used for rising-edge detection
    /// so holding jump doesn't auto-repeat.
    prev_jump: bool,
}

/// Input state from the player.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputState {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

impl PhysicsBody {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            on_ground: false,
            half_width: 15.0,
            height: 60.0,
            prev_jump: false,
        }
    }

    /// Run one physics tick.
    /// Glitch coordinate system: Y=0 at bottom, negative Y = up.
    /// Gravity pulls toward positive Y (toward ground_y=0).
    pub fn tick(
        &mut self,
        dt: f64,
        input: &InputState,
        platforms: &[PlatformLine],
        street_left: f64,
        street_right: f64,
    ) {
        let was_on_ground = self.on_ground;

        // Apply horizontal input
        self.vx = if input.left && !input.right {
            -WALK_SPEED
        } else if input.right && !input.left {
            WALK_SPEED
        } else {
            0.0
        };

        // Jump (rising edge only — prevents auto-repeat while key is held)
        if input.jump && !self.prev_jump && self.on_ground {
            self.vy = JUMP_VELOCITY;
            self.on_ground = false;
        }

        // Apply gravity (positive direction = down toward y=0 in Glitch coords)
        if !self.on_ground {
            self.vy += GRAVITY * dt;
            self.vy = self.vy.min(TERMINAL_VELOCITY);
        }

        // Save pre-move Y for swept collision detection
        let prev_y = self.y;

        // Move
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // Clamp to street bounds
        self.x = self
            .x
            .clamp(street_left + self.half_width, street_right - self.half_width);

        // --- Platform collision (3 phases) ---
        self.on_ground = false;

        // Phase 1: Slope following — if player was on ground and didn't jump,
        // snap to the platform surface at the new X position. Without this,
        // walking on a slope detaches the player because vy=0 means no
        // vertical sweep occurs, but the platform Y changed with X.
        if was_on_ground && self.vy >= 0.0 {
            let mut best_snap: Option<f64> = None;
            let mut best_dist = f64::MAX;
            for platform in platforms {
                if !platform.solid_from_top() {
                    continue;
                }
                if self.x < platform.min_x() || self.x > platform.max_x() {
                    continue;
                }
                let plat_y = platform.y_at(self.x);
                let dist = (self.y - plat_y).abs();
                if dist < SLOPE_SNAP_TOLERANCE && dist < best_dist {
                    best_snap = Some(plat_y);
                    best_dist = dist;
                }
            }
            if let Some(plat_y) = best_snap {
                self.y = plat_y;
                self.vy = 0.0;
                self.on_ground = true;
            }
        }

        // Phase 2: Swept collision for falling players — compare Y before
        // and after movement to detect platform crossings at any speed.
        if !self.on_ground {
            let mut best_plat_y: Option<f64> = None;
            for platform in platforms {
                if !platform.solid_from_top() {
                    continue;
                }
                if self.x < platform.min_x() || self.x > platform.max_x() {
                    continue;
                }
                let plat_y = platform.y_at(self.x);
                if self.vy >= 0.0 && prev_y <= plat_y && self.y >= plat_y {
                    match best_plat_y {
                        Some(best) if plat_y < best => {
                            best_plat_y = Some(plat_y);
                        }
                        None => {
                            best_plat_y = Some(plat_y);
                        }
                        _ => {}
                    }
                }
            }
            if let Some(plat_y) = best_plat_y {
                self.y = plat_y;
                self.vy = 0.0;
                self.on_ground = true;
            }
        }

        // Phase 3: Ceiling collision — block upward movement through
        // platforms that are solid from below (pc_perm = None or 1).
        if self.vy < 0.0 {
            let prev_head = prev_y - self.height;
            let new_head = self.y - self.height;
            for platform in platforms {
                if !platform.solid_from_bottom() {
                    continue;
                }
                if self.x < platform.min_x() || self.x > platform.max_x() {
                    continue;
                }
                let plat_y = platform.y_at(self.x);
                // Head swept through platform from below (more positive)
                // to above (more negative)
                if prev_head >= plat_y && new_head <= plat_y {
                    self.y = plat_y + self.height;
                    self.vy = 0.0;
                    break;
                }
            }
        }

        self.prev_jump = input.jump;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::types::Point;

    fn flat_ground() -> Vec<PlatformLine> {
        vec![PlatformLine {
            id: "ground".into(),
            start: Point { x: -1000.0, y: 0.0 },
            end: Point { x: 1000.0, y: 0.0 },
            pc_perm: None,
            item_perm: None,
        }]
    }

    #[test]
    fn falls_with_gravity() {
        let mut body = PhysicsBody::new(0.0, -200.0); // High up
        let input = InputState::default();
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // Should have moved downward (vy positive, y increased)
        assert!(body.vy > 0.0);
    }

    #[test]
    fn lands_on_platform() {
        let mut body = PhysicsBody::new(0.0, -0.5); // Just above ground
        body.vy = 100.0; // Falling
        let input = InputState::default();
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        assert!(body.on_ground);
        assert_eq!(body.y, 0.0);
        assert_eq!(body.vy, 0.0);
    }

    #[test]
    fn walks_right() {
        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let input = InputState {
            right: true,
            ..Default::default()
        };
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        assert!(body.x > 0.0);
    }

    #[test]
    fn walks_left() {
        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let input = InputState {
            left: true,
            ..Default::default()
        };
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        assert!(body.x < 0.0);
    }

    #[test]
    fn jumps() {
        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let input = InputState {
            jump: true,
            ..Default::default()
        };
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // Should have negative vy (going up) and moved up (negative y)
        assert!(body.vy < 0.0 || body.y < 0.0);
        assert!(!body.on_ground);
    }

    #[test]
    fn jump_does_not_auto_repeat_while_held() {
        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let jump_input = InputState {
            jump: true,
            ..Default::default()
        };
        let platforms = flat_ground();

        // First tick with jump pressed: should jump
        body.tick(1.0 / 60.0, &jump_input, &platforms, -1000.0, 1000.0);
        assert!(!body.on_ground, "Should jump on rising edge");
        assert!(body.vy < 0.0, "Should have upward velocity");

        // Simulate landing back on ground while key is still held
        body.y = 0.0;
        body.vy = 0.0;
        body.on_ground = true;

        // Tick again with jump still held — should NOT re-jump
        body.tick(1.0 / 60.0, &jump_input, &platforms, -1000.0, 1000.0);
        assert!(body.on_ground, "Should NOT re-jump while key is held");
    }

    #[test]
    fn cannot_jump_in_air() {
        let mut body = PhysicsBody::new(0.0, -100.0);
        body.on_ground = false;
        let initial_vy = body.vy;
        let input = InputState {
            jump: true,
            ..Default::default()
        };
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // vy should not have gotten the jump impulse
        // (it gets gravity instead)
        assert!(body.vy >= initial_vy);
    }

    #[test]
    fn clamped_to_street_bounds() {
        let mut body = PhysicsBody::new(999.0, 0.0);
        body.on_ground = true;
        let input = InputState {
            right: true,
            ..Default::default()
        };
        let platforms = flat_ground();

        // Run many ticks to push past boundary
        for _ in 0..100 {
            body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);
        }

        assert!(body.x <= 1000.0 - body.half_width);
    }

    #[test]
    fn walks_on_slope() {
        let platforms = vec![PlatformLine {
            id: "slope".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point {
                x: 200.0,
                y: -100.0,
            },
            pc_perm: None,
            item_perm: None,
        }];

        // Player starts ABOVE the slope surface (y=-52 is above plat_y=-50
        // in Glitch coords where more-negative = higher) with enough downward
        // velocity to cross through the platform in one tick via swept collision.
        let mut body = PhysicsBody::new(100.0, -52.0);
        body.vy = 200.0;
        body.on_ground = false;
        let input = InputState::default();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // Should land on slope via swept collision — y snapped to slope surface
        // slope y_at(100) = 0 + 0.5 * (-100) = -50
        assert!(body.on_ground, "Player should land on slope");
        assert!((body.y - (-50.0)).abs() < 1.0, "Player Y should snap to slope surface, got {}", body.y);
    }

    #[test]
    fn terminal_velocity_caps_falling() {
        let mut body = PhysicsBody::new(0.0, -10000.0);
        let input = InputState::default();
        let platforms: Vec<PlatformLine> = vec![]; // No platforms to land on

        // Fall for a long time
        for _ in 0..600 {
            body.tick(1.0 / 60.0, &input, &platforms, -100000.0, 100000.0);
        }

        assert!(body.vy <= TERMINAL_VELOCITY);
    }

    #[test]
    fn standing_still_on_slope_does_not_slide() {
        // Slope from (0, 0) to (200, -100) — rises 100px over 200px
        let platforms = vec![PlatformLine {
            id: "slope".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 200.0, y: -100.0 },
            pc_perm: None,
            item_perm: None,
        }];

        let slope_y_at_100 = -50.0;
        let mut body = PhysicsBody::new(100.0, slope_y_at_100);
        body.on_ground = true;
        let input = InputState::default(); // No input

        let initial_x = body.x;
        let initial_y = body.y;

        // Run several ticks with no input
        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);
        }

        // Player should not have slid — position essentially unchanged
        assert!(
            (body.x - initial_x).abs() < 0.01,
            "Player slid horizontally: {} -> {}",
            initial_x,
            body.x
        );
        assert!(
            (body.y - initial_y).abs() < 0.01,
            "Player slid vertically: {} -> {}",
            initial_y,
            body.y
        );
        assert!(body.on_ground);
    }

    #[test]
    fn lands_on_higher_of_overlapping_platforms() {
        // Two platforms at the same X range, different heights.
        // Body starts just above the higher one so it doesn't tunnel through.
        let platforms = vec![
            PlatformLine {
                id: "high".into(),
                start: Point { x: -100.0, y: -50.0 },
                end: Point { x: 100.0, y: -50.0 },
                pc_perm: None,
                item_perm: None,
            },
            PlatformLine {
                id: "low".into(),
                start: Point { x: -100.0, y: 0.0 },
                end: Point { x: 100.0, y: 0.0 },
                pc_perm: None,
                item_perm: None,
            },
        ];

        // Start just above the high platform with a gentle falling velocity
        let mut body = PhysicsBody::new(0.0, -51.0);
        body.vy = 50.0; // Gentle fall
        let input = InputState::default();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        assert!(body.on_ground);
        // Should have landed on the higher platform (y = -50), not the lower one (y = 0)
        assert!(
            (body.y - (-50.0)).abs() < 1.0,
            "Expected to land on high platform at y=-50, got y={}",
            body.y
        );
    }

    #[test]
    fn walks_along_slope_stays_grounded() {
        // Slope from (0, 0) to (200, -100)
        let platforms = vec![PlatformLine {
            id: "slope".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 200.0, y: -100.0 },
            pc_perm: None,
            item_perm: None,
        }];

        let mut body = PhysicsBody::new(50.0, -25.0); // On slope at x=50 (y_at(50) = -25)
        body.on_ground = true;
        let input = InputState {
            right: true,
            ..Default::default()
        };

        // Walk right for several frames
        for _ in 0..30 {
            body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);
            assert!(
                body.on_ground,
                "Player fell off slope at x={}, y={}",
                body.x,
                body.y
            );
        }

        // Player should have moved right and followed the slope upward
        assert!(body.x > 50.0, "Player should have moved right");
    }

    #[test]
    fn ceiling_blocks_jump_through_solid_platform() {
        // Solid platform (pc_perm = None) above the player
        let platforms = vec![
            PlatformLine {
                id: "ground".into(),
                start: Point { x: -1000.0, y: 0.0 },
                end: Point { x: 1000.0, y: 0.0 },
                pc_perm: None,
                item_perm: None,
            },
            PlatformLine {
                id: "ceiling".into(),
                start: Point { x: -1000.0, y: -100.0 },
                end: Point { x: 1000.0, y: -100.0 },
                pc_perm: None, // Solid from both directions
                item_perm: None,
            },
        ];

        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let input = InputState {
            jump: true,
            ..Default::default()
        };

        // Jump — player should be blocked by the ceiling
        for _ in 0..30 {
            body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);
        }

        // Player's head should not have passed above the ceiling platform
        let head_y = body.y - body.height;
        assert!(
            head_y >= -100.0,
            "Player's head passed through solid ceiling: head_y={}",
            head_y
        );
    }

    #[test]
    fn can_jump_through_one_way_platform() {
        // One-way from top platform (pc_perm = -1) — should be jumpable from below
        let platforms = vec![
            PlatformLine {
                id: "ground".into(),
                start: Point { x: -1000.0, y: 0.0 },
                end: Point { x: 1000.0, y: 0.0 },
                pc_perm: Some(-1), // One-way from top
                item_perm: None,
            },
            PlatformLine {
                id: "upper".into(),
                start: Point { x: -1000.0, y: -80.0 },
                end: Point { x: 1000.0, y: -80.0 },
                pc_perm: Some(-1), // One-way from top
                item_perm: None,
            },
        ];

        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let input = InputState {
            jump: true,
            ..Default::default()
        };

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // Player should be jumping upward, not blocked
        assert!(body.vy < 0.0, "Player should be moving upward");
        assert!(!body.on_ground);
    }

    #[test]
    fn lands_at_terminal_velocity() {
        // A player falling at terminal velocity (600 px/s = 10px/frame)
        // must still collide with a platform via swept collision.
        // Start at y=-5 so the player sweeps through y=0 in one frame.
        let mut body = PhysicsBody::new(0.0, -5.0);
        body.vy = TERMINAL_VELOCITY;
        let input = InputState::default();
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        assert!(body.on_ground, "Player at terminal velocity should land, not tunnel through");
        assert_eq!(body.y, 0.0, "Player should snap to platform surface");
        assert_eq!(body.vy, 0.0);
    }

    #[test]
    fn walking_off_platform_edge_starts_falling() {
        // Short platform from x=0 to x=100
        let platforms = vec![PlatformLine {
            id: "short".into(),
            start: Point { x: 0.0, y: -50.0 },
            end: Point { x: 100.0, y: -50.0 },
            pc_perm: None,
            item_perm: None,
        }];

        let mut body = PhysicsBody::new(90.0, -50.0); // Near the right edge, on platform
        body.on_ground = true;
        let input = InputState {
            right: true,
            ..Default::default()
        };

        // Walk right until past the platform edge
        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);
        }

        // Player should have walked past x=100 and started falling
        assert!(body.x > 100.0, "Player should have walked past edge");
        assert!(!body.on_ground, "Player should be falling after walking off edge");
        assert!(body.vy > 0.0, "Player should have downward velocity");
    }
}
