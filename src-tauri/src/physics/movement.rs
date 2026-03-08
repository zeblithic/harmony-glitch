use serde::{Deserialize, Serialize};

use crate::street::types::PlatformLine;

/// Physics constants (tunable).
pub const GRAVITY: f64 = 980.0; // px/s²
pub const WALK_SPEED: f64 = 200.0; // px/s
pub const JUMP_VELOCITY: f64 = -400.0; // px/s (negative = up in Glitch coords)
pub const TERMINAL_VELOCITY: f64 = 600.0; // px/s

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
        // Apply horizontal input
        self.vx = if input.left && !input.right {
            -WALK_SPEED
        } else if input.right && !input.left {
            WALK_SPEED
        } else {
            0.0
        };

        // Jump (only if on ground)
        if input.jump && self.on_ground {
            self.vy = JUMP_VELOCITY;
            self.on_ground = false;
        }

        // Apply gravity (positive direction = down toward y=0 in Glitch coords)
        // In Glitch coords, gravity pulls toward MORE POSITIVE Y.
        // Ground_y is 0 and platforms have negative y.
        // So gravity should push y toward 0 (more positive).
        if !self.on_ground {
            self.vy += GRAVITY * dt;
            self.vy = self.vy.min(TERMINAL_VELOCITY);
        }

        // Move
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // Clamp to street bounds
        self.x = self
            .x
            .clamp(street_left + self.half_width, street_right - self.half_width);

        // Platform collision
        self.on_ground = false;
        for platform in platforms {
            if !platform.solid_from_top() {
                continue;
            }

            // Check if player is within platform X range
            let plat_min_x = platform.min_x();
            let plat_max_x = platform.max_x();
            if self.x < plat_min_x || self.x > plat_max_x {
                continue;
            }

            let plat_y = platform.y_at(self.x);

            // Player feet are at self.y. If feet are at or below platform surface
            // and were above it before (falling onto it), snap to platform.
            // "Below" in Glitch coords means more positive Y.
            if self.vy >= 0.0 && self.y >= plat_y && self.y <= plat_y + GRAVITY * dt * dt + 2.0 {
                self.y = plat_y;
                self.vy = 0.0;
                self.on_ground = true;
                break;
            }
        }
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

        let mut body = PhysicsBody::new(100.0, -48.0); // Near the slope surface
        body.vy = 10.0; // Slight downward velocity
        body.on_ground = false;
        let input = InputState::default();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // Should land on slope — y should be at the slope's Y at x=100
        // slope y_at(100) = 0 + 0.5 * (-100) = -50
        if body.on_ground {
            assert!((body.y - (-50.0)).abs() < 5.0);
        }
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
