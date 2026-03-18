use serde::{Deserialize, Serialize};

use crate::street::types::{PlatformLine, Wall};

/// Physics constants (tunable).
pub const GRAVITY: f64 = 980.0; // px/s²
pub const WALK_SPEED: f64 = 200.0; // px/s
pub const JUMP_VELOCITY: f64 = -400.0; // px/s (negative = up in Glitch coords)
pub const TERMINAL_VELOCITY: f64 = 600.0; // px/s
/// Max Y displacement to snap to when walking along a slope.
/// Must exceed the steepest slope's Y change per frame at walk speed:
///   WALK_SPEED * (dy/dx) / 60 fps.
/// At 200 px/s and dy/dx=0.5 (steepest demo slope): 200*0.5/60 ≈ 1.7 px.
/// 10 px gives comfortable headroom. If this is reduced below the per-frame
/// Y change of any slope, players will briefly detach until Phase 2 catches them.
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
    pub interact: bool,
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
        walls: &[Wall],
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

        // Save pre-move position for swept collision detection
        let prev_x = self.x;
        let prev_y = self.y;

        // Move
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // --- Wall collision ---
        let mut wall_prev_x = prev_x;
        for wall in walls {
            if matches!(wall.pc_perm, Some(0)) {
                continue;
            }
            // Vertical overlap check
            let player_top = self.y - self.height;
            let player_bottom = self.y;
            if player_bottom <= wall.y || player_top >= wall.bottom() {
                continue;
            }
            // Horizontal sweep
            if wall_prev_x + self.half_width <= wall.x
                && self.x + self.half_width > wall.x
                && wall.blocks_from_left()
            {
                self.x = wall.x - self.half_width;
                self.vx = 0.0;
                wall_prev_x = self.x;
            } else if wall_prev_x - self.half_width >= wall.x
                && self.x - self.half_width < wall.x
                && wall.blocks_from_right()
            {
                self.x = wall.x + self.half_width;
                self.vx = 0.0;
                wall_prev_x = self.x;
            }
        }

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
            for platform in platforms {
                if !platform.solid_from_top() {
                    continue;
                }
                if self.x + self.half_width < platform.min_x()
                    || self.x - self.half_width > platform.max_x()
                {
                    continue;
                }
                let plat_y = platform.y_at(self.x);
                let dist = (self.y - plat_y).abs();
                if dist < SLOPE_SNAP_TOLERANCE {
                    // Prefer the highest platform (most-negative Y) so slopes
                    // win over flat ground when they overlap in X range.
                    match best_snap {
                        Some(best) if plat_y < best => best_snap = Some(plat_y),
                        None => best_snap = Some(plat_y),
                        _ => {}
                    }
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
                if self.x + self.half_width < platform.min_x()
                    || self.x - self.half_width > platform.max_x()
                {
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
                if self.x + self.half_width < platform.min_x()
                    || self.x - self.half_width > platform.max_x()
                {
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
    use crate::street::types::{Point, Wall};

    fn flat_ground() -> Vec<PlatformLine> {
        vec![PlatformLine {
            id: "ground".into(),
            start: Point { x: -1000.0, y: 0.0 },
            end: Point { x: 1000.0, y: 0.0 },
            pc_perm: None,
            item_perm: None,
        }]
    }

    fn solid_wall(x: f64, y: f64, h: f64) -> Vec<Wall> {
        vec![Wall {
            id: "wall".into(), x, y, h,
            pc_perm: None, item_perm: None,
        }]
    }

    // --- Wall collision tests ---

    #[test]
    fn wall_blocks_movement_from_left() {
        // Wall at x=100, spanning y=-400 to y=0 (full height).
        // Player starts at x=80 on the ground (y=0), walks right into wall.
        let mut body = PhysicsBody::new(80.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };
        let platforms = flat_ground();
        let walls = solid_wall(100.0, -400.0, 400.0);

        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        assert!(
            body.x <= 100.0 - body.half_width + 0.01,
            "Player should be blocked by wall from left, x={}",
            body.x
        );
    }

    #[test]
    fn wall_blocks_movement_from_right() {
        // Wall at x=100, player starts at x=120, walks left into wall.
        let mut body = PhysicsBody::new(120.0, 0.0);
        body.on_ground = true;
        let input = InputState { left: true, ..Default::default() };
        let platforms = flat_ground();
        let walls = solid_wall(100.0, -400.0, 400.0);

        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        assert!(
            body.x >= 100.0 + body.half_width - 0.01,
            "Player should be blocked by wall from right, x={}",
            body.x
        );
    }

    #[test]
    fn wall_does_not_block_when_above() {
        // Wall at y=-100, h=100 → spans -100 to 0.
        // Player at y=-200 on a high platform — entirely above the wall's Y range.
        let platforms = vec![PlatformLine {
            id: "high".into(),
            start: Point { x: -1000.0, y: -200.0 },
            end: Point { x: 1000.0, y: -200.0 },
            pc_perm: None,
            item_perm: None,
        }];
        let walls = solid_wall(100.0, -100.0, 100.0);

        let mut body = PhysicsBody::new(80.0, -200.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };

        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        // Should have walked past x=100 — wall does not block because player is above it
        assert!(body.x > 100.0, "Player above wall should not be blocked, x={}", body.x);
    }

    #[test]
    fn wall_does_not_block_when_below() {
        // Wall at y=-400, h=100 → spans -400 to -300.
        // Player at y=0 (feet), head at y=-60 — entirely below the wall's Y range.
        let mut body = PhysicsBody::new(80.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };
        let platforms = flat_ground();
        let walls = solid_wall(100.0, -400.0, 100.0);

        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        // Should have walked past x=100 — wall does not block because player is below it
        assert!(body.x > 100.0, "Player below wall should not be blocked, x={}", body.x);
    }

    #[test]
    fn wall_one_way_left_blocks_from_left_only() {
        // pc_perm=-1: blocks from left (blocks_from_left=true), passes from right.
        let walls = vec![Wall {
            id: "wall".into(), x: 100.0, y: -400.0, h: 400.0,
            pc_perm: Some(-1), item_perm: None,
        }];

        // From left: player at x=80 walks right → should be blocked
        let mut body = PhysicsBody::new(80.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };
        let platforms = flat_ground();
        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(
            body.x <= 100.0 - body.half_width + 0.01,
            "One-way left wall should block from left, x={}",
            body.x
        );

        // From right: player at x=120 walks left → should pass through
        let mut body2 = PhysicsBody::new(120.0, 0.0);
        body2.on_ground = true;
        let input2 = InputState { left: true, ..Default::default() };
        for _ in 0..60 {
            body2.tick(1.0 / 60.0, &input2, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(
            body2.x < 100.0 - body2.half_width,
            "One-way left wall should pass from right, x={}",
            body2.x
        );
    }

    #[test]
    fn wall_one_way_right_blocks_from_right_only() {
        // pc_perm=1: blocks from right (blocks_from_right=true), passes from left.
        let walls = vec![Wall {
            id: "wall".into(), x: 100.0, y: -400.0, h: 400.0,
            pc_perm: Some(1), item_perm: None,
        }];
        let platforms = flat_ground();

        // From right: player at x=120 walks left → should be blocked
        let mut body = PhysicsBody::new(120.0, 0.0);
        body.on_ground = true;
        let input = InputState { left: true, ..Default::default() };
        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(
            body.x >= 100.0 + body.half_width - 0.01,
            "One-way right wall should block from right, x={}",
            body.x
        );

        // From left: player at x=80 walks right → should pass through
        let mut body2 = PhysicsBody::new(80.0, 0.0);
        body2.on_ground = true;
        let input2 = InputState { right: true, ..Default::default() };
        for _ in 0..60 {
            body2.tick(1.0 / 60.0, &input2, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(
            body2.x > 100.0 + body2.half_width,
            "One-way right wall should pass from left, x={}",
            body2.x
        );
    }

    #[test]
    fn wall_passthrough_allows_all() {
        // pc_perm=0: pass-through — no blocking from either direction.
        let walls = vec![Wall {
            id: "wall".into(), x: 100.0, y: -400.0, h: 400.0,
            pc_perm: Some(0), item_perm: None,
        }];
        let platforms = flat_ground();

        // From left → should pass
        let mut body = PhysicsBody::new(80.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };
        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(body.x > 100.0 + body.half_width, "Passthrough wall should not block from left, x={}", body.x);

        // From right → should pass
        let mut body2 = PhysicsBody::new(120.0, 0.0);
        body2.on_ground = true;
        let input2 = InputState { left: true, ..Default::default() };
        for _ in 0..60 {
            body2.tick(1.0 / 60.0, &input2, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(body2.x < 100.0 - body2.half_width, "Passthrough wall should not block from right, x={}", body2.x);
    }

    #[test]
    fn wall_does_not_push_player_already_past() {
        // Player already past (right of) the wall with no input — should stay put.
        let mut body = PhysicsBody::new(200.0, 0.0);
        body.on_ground = true;
        let input = InputState::default();
        let platforms = flat_ground();
        let walls = solid_wall(100.0, -400.0, 400.0);

        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        assert!(
            (body.x - 200.0).abs() < 0.01,
            "Player already past wall should not be moved, x={}",
            body.x
        );
    }

    #[test]
    fn wall_correction_updates_prev_x_for_subsequent_walls() {
        // Two walls close together processed in order: wall_a at x=100, wall_b at x=99.
        // Player walks right; at WALK_SPEED a single frame sweeps ~3.3px. Position the
        // player so right-edge (x + half_width) crosses both walls in one frame.
        // Wall A should stop the player. Wall B should NOT fire because the corrected
        // position never crossed wall B from the left.
        let half = 15.0; // default half_width
        // Start just left of wall_b: right-edge at 98.5, one frame moves to ~101.8
        let start_x = 99.0 - half - 0.5; // 83.5 → right-edge 98.5
        let mut body = PhysicsBody::new(start_x, 0.0);
        body.on_ground = true;
        let walls = vec![
            Wall { id: "a".into(), x: 100.0, y: -400.0, h: 400.0, pc_perm: None, item_perm: None },
            Wall { id: "b".into(), x: 99.0, y: -400.0, h: 400.0, pc_perm: None, item_perm: None },
        ];
        let input = InputState { right: true, ..Default::default() };
        let platforms = flat_ground();
        body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);

        // Player should stop at wall_a (the first wall hit), not wall_b
        let expected = 100.0 - half;
        assert!(
            (body.x - expected).abs() < 0.01,
            "Player should stop at wall_a (x={}), not be pushed further back by wall_b, x={}",
            expected, body.x
        );
    }

    #[test]
    fn street_bounds_still_clamp_beyond_walls() {
        // Street right=150, wall at x=100. Player should be clamped by street bound, not wall.
        let mut body = PhysicsBody::new(80.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };
        let platforms = flat_ground();
        // Wall is inside the street bounds — player can't reach street edge past the wall
        // Use a wall at x=200 but street right=150, so street clamp applies first
        let walls = solid_wall(200.0, -400.0, 400.0);

        for _ in 0..120 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 150.0);
        }

        assert!(
            body.x <= 150.0 - body.half_width + 0.01,
            "Street bounds should clamp player, x={}",
            body.x
        );
    }

    #[test]
    fn falls_with_gravity() {
        let mut body = PhysicsBody::new(0.0, -200.0); // High up
        let input = InputState::default();
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

        // Should have moved downward (vy positive, y increased)
        assert!(body.vy > 0.0);
    }

    #[test]
    fn lands_on_platform() {
        let mut body = PhysicsBody::new(0.0, -0.5); // Just above ground
        body.vy = 100.0; // Falling
        let input = InputState::default();
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

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

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

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

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

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

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

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
        body.tick(1.0 / 60.0, &jump_input, &platforms, &[], -1000.0, 1000.0);
        assert!(!body.on_ground, "Should jump on rising edge");
        assert!(body.vy < 0.0, "Should have upward velocity");

        // Simulate landing back on ground while key is still held
        body.y = 0.0;
        body.vy = 0.0;
        body.on_ground = true;

        // Tick again with jump still held — should NOT re-jump
        body.tick(1.0 / 60.0, &jump_input, &platforms, &[], -1000.0, 1000.0);
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

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

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
            body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);
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

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

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
            body.tick(1.0 / 60.0, &input, &platforms, &[], -100000.0, 100000.0);
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
            body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);
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

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

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
            body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);
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
    fn walks_onto_slope_overlapping_flat_ground() {
        // Flat ground at y=0 from x=-1800 to x=1800, and a slope from
        // (400, 0) to (800, -120). The slope starts at the same Y as
        // the flat ground but rises. The player should follow the slope
        // (highest platform = most-negative Y), not stay stuck on flat ground.
        let platforms = vec![
            PlatformLine {
                id: "flat".into(),
                start: Point { x: -1800.0, y: 0.0 },
                end: Point { x: 1800.0, y: 0.0 },
                pc_perm: None,
                item_perm: None,
            },
            PlatformLine {
                id: "hill".into(),
                start: Point { x: 400.0, y: 0.0 },
                end: Point { x: 800.0, y: -120.0 },
                pc_perm: None,
                item_perm: None,
            },
        ];

        // Start on flat ground just before the slope begins
        let mut body = PhysicsBody::new(390.0, 0.0);
        body.on_ground = true;
        let input = InputState {
            right: true,
            ..Default::default()
        };

        // Walk right for enough frames to enter the slope region
        for _ in 0..120 {
            body.tick(1.0 / 60.0, &input, &platforms, &[], -1800.0, 1800.0);
        }

        // Player should have moved past x=400 and followed the slope upward
        assert!(body.x > 500.0, "Player should have walked into slope region, x={}", body.x);
        assert!(body.y < -10.0, "Player should have followed slope upward, y={}", body.y);
        assert!(body.on_ground, "Player should still be grounded on slope");
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
            body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);
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

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

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

        body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);

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
            body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);
        }

        // Player should have walked past x=100 and started falling
        assert!(body.x > 100.0, "Player should have walked past edge");
        assert!(!body.on_ground, "Player should be falling after walking off edge");
        assert!(body.vy > 0.0, "Player should have downward velocity");
    }
}
