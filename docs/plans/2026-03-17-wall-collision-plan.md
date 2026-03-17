# Wall Collision Enforcement Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enforce wall collision in the physics engine so players can't walk through vertical barriers parsed from street XML.

**Architecture:** Walls are vertical line segments already parsed into `Wall` structs. Add helper methods to `Wall` and `StreetData`, then add a wall collision phase to `PhysicsBody::tick()` between the horizontal move and the street-bounds clamp. All logic is pure — no I/O, no frontend changes.

**Tech Stack:** Rust (src-tauri), TDD with `cargo test`

**Design spec:** `docs/plans/2026-03-17-wall-collision-design.md`

---

## File Structure

| File | Role | Change |
|------|------|--------|
| `src-tauri/src/street/types.rs` | Wall/Street data types | Add `Wall` helper methods + `StreetData::walls()` accessor |
| `src-tauri/src/physics/movement.rs` | Physics tick loop | Add `walls` param, `prev_x` save, wall collision phase |
| `src-tauri/src/engine/state.rs` | Game state tick | Pass walls to physics, update stale comment, fix test call sites |

---

## Task 1: Wall Helper Methods (types.rs)

**Files:**
- Modify: `src-tauri/src/street/types.rs`

**Context:** `Wall` is defined at line 74 with fields `id`, `x`, `y`, `h`, `pc_perm`, `item_perm`. `PlatformLine` already has `solid_from_top()`/`solid_from_bottom()` helpers using the same `pc_perm` bit pattern — wall helpers follow the same structure but for horizontal directions. `StreetData` already has a `platforms()` accessor at line 156; `walls()` follows the same pattern.

- [ ] **Step 1: Write failing tests for Wall helpers**

Add to the `#[cfg(test)] mod tests` block in `src-tauri/src/street/types.rs` (after the existing `serializes_to_camel_case` test):

```rust
    #[test]
    fn wall_blocks_from_left() {
        // None = solid both sides, blocks from left
        let solid = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: None, item_perm: None };
        assert!(solid.blocks_from_left());

        // -1 = blocks from left
        let left_only = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(-1), item_perm: None };
        assert!(left_only.blocks_from_left());

        // 1 = blocks from right only, does NOT block from left
        let right_only = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(1), item_perm: None };
        assert!(!right_only.blocks_from_left());

        // 0 = pass-through, does NOT block from left
        let passthrough = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(0), item_perm: None };
        assert!(!passthrough.blocks_from_left());
    }

    #[test]
    fn wall_blocks_from_right() {
        let solid = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: None, item_perm: None };
        assert!(solid.blocks_from_right());

        let right_only = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(1), item_perm: None };
        assert!(right_only.blocks_from_right());

        // -1 = blocks from left only, does NOT block from right
        let left_only = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(-1), item_perm: None };
        assert!(!left_only.blocks_from_right());

        let passthrough = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(0), item_perm: None };
        assert!(!passthrough.blocks_from_right());
    }

    #[test]
    fn wall_bottom_extent() {
        let wall = Wall { id: "w".into(), x: 0.0, y: -400.0, h: 400.0, pc_perm: None, item_perm: None };
        assert!((wall.bottom() - 0.0).abs() < 0.001);

        let wall2 = Wall { id: "w".into(), x: 0.0, y: -200.0, h: 100.0, pc_perm: None, item_perm: None };
        assert!((wall2.bottom() - (-100.0)).abs() < 0.001);
    }

    #[test]
    fn walls_accessor_returns_middleground_walls() {
        let wall = Wall { id: "w1".into(), x: -100.0, y: -50.0, h: 50.0, pc_perm: None, item_perm: None };
        let mg = Layer {
            name: "middleground".into(),
            z: 0,
            w: 200.0,
            h: 50.0,
            is_middleground: true,
            decos: vec![],
            platform_lines: vec![],
            walls: vec![wall.clone()],
            ladders: vec![],
            filters: None,
        };
        let s = StreetData {
            tsid: "test".into(),
            name: "Test".into(),
            left: -100.0,
            right: 100.0,
            top: -50.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![mg],
            signposts: vec![],
        };
        assert_eq!(s.walls().len(), 1);
        assert_eq!(s.walls()[0].id, "w1");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test -p harmony-glitch street::types::tests::wall_blocks_from_left street::types::tests::wall_blocks_from_right street::types::tests::wall_bottom_extent street::types::tests::walls_accessor 2>&1`
Expected: FAIL — `blocks_from_left`, `blocks_from_right`, `bottom`, and `walls` methods don't exist yet.

- [ ] **Step 3: Implement Wall helper methods and StreetData::walls()**

Add to `impl Wall` block (create new impl block after line 82 in `src-tauri/src/street/types.rs`):

```rust
impl Wall {
    /// Whether this wall blocks movement from the left.
    /// Same bit-pattern as PlatformLine::solid_from_top but for horizontal direction.
    pub fn blocks_from_left(&self) -> bool {
        !matches!(self.pc_perm, Some(1) | Some(0))
    }

    /// Whether this wall blocks movement from the right.
    pub fn blocks_from_right(&self) -> bool {
        !matches!(self.pc_perm, Some(-1) | Some(0))
    }

    /// Bottom Y extent of the wall (y + h).
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
}
```

Add `walls()` to the existing `impl StreetData` block (after the `platforms()` method around line 160):

```rust
    /// All walls from the middleground layer.
    pub fn walls(&self) -> &[Wall] {
        self.middleground()
            .map(|l| l.walls.as_slice())
            .unwrap_or(&[])
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -p harmony-glitch street::types::tests 2>&1`
Expected: All types tests pass (existing + 4 new).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/street/types.rs
git commit -m "feat: add Wall helper methods and StreetData::walls() accessor"
```

---

## Task 2: Wall Collision Phase in PhysicsBody::tick() (movement.rs)

**Files:**
- Modify: `src-tauri/src/physics/movement.rs`

**Context:** `PhysicsBody::tick()` runs at line 62. The horizontal move is at line 97 (`self.x += self.vx * dt`), followed by the street-bounds clamp at line 101. The wall collision phase goes between them. Currently only `prev_y` is saved (line 94); we also need `prev_x`. The import at line 3 only brings in `PlatformLine` — needs `Wall` too. All 19 existing `body.tick()` call sites in movement.rs tests gain a `&[]` walls parameter (`jump_does_not_auto_repeat_while_held` has 2 calls). The state.rs production call site must also be updated in this same task to keep the codebase compiling.

- [ ] **Step 1: Write failing tests for wall collision**

Add these tests to the existing `#[cfg(test)] mod tests` block at the end of `src-tauri/src/physics/movement.rs`. Also add `use crate::street::types::Wall;` to the test module imports (alongside the existing `use crate::street::types::Point;` at line 207).

Helper function (add after the existing `flat_ground()` helper):

```rust
    fn solid_wall(x: f64, y: f64, h: f64) -> Vec<Wall> {
        vec![Wall {
            id: "wall".into(),
            x,
            y,
            h,
            pc_perm: None,
            item_perm: None,
        }]
    }
```

Tests:

```rust
    #[test]
    fn wall_blocks_movement_from_left() {
        // Wall at x=100, spanning y=-400 to y=0
        let walls = solid_wall(100.0, -400.0, 400.0);
        let platforms = flat_ground();
        let mut body = PhysicsBody::new(80.0, 0.0); // left of wall
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };

        // Walk right for enough frames to reach the wall
        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        // Should be stopped at wall.x - half_width
        assert!(body.x <= 100.0 - body.half_width + 0.01,
            "Player should be blocked by wall, x={}", body.x);
    }

    #[test]
    fn wall_blocks_movement_from_right() {
        let walls = solid_wall(100.0, -400.0, 400.0);
        let platforms = flat_ground();
        let mut body = PhysicsBody::new(120.0, 0.0); // right of wall
        body.on_ground = true;
        let input = InputState { left: true, ..Default::default() };

        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        assert!(body.x >= 100.0 + body.half_width - 0.01,
            "Player should be blocked by wall, x={}", body.x);
    }

    #[test]
    fn wall_does_not_block_when_above() {
        // Wall from y=-100 to y=0 (bottom). Player at y=-200 (head at y=-260).
        // Player is entirely above the wall's vertical extent.
        let walls = solid_wall(100.0, -100.0, 100.0);
        let platforms = vec![PlatformLine {
            id: "high".into(),
            start: Point { x: -1000.0, y: -200.0 },
            end: Point { x: 1000.0, y: -200.0 },
            pc_perm: None,
            item_perm: None,
        }];
        let mut body = PhysicsBody::new(80.0, -200.0); // above wall
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };

        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        // Player should have walked right past x=100
        assert!(body.x > 100.0, "Player above wall should pass through, x={}", body.x);
    }

    #[test]
    fn wall_does_not_block_when_below() {
        // Wall from y=-400 to y=-300. Player at y=0 (feet at ground, head at y=-60).
        // Player is entirely below the wall's vertical extent.
        let walls = solid_wall(100.0, -400.0, 100.0); // wall spans y=-400 to y=-300
        let platforms = flat_ground();
        let mut body = PhysicsBody::new(80.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };

        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        assert!(body.x > 100.0, "Player below wall should pass through, x={}", body.x);
    }

    #[test]
    fn wall_one_way_left_blocks_from_left_only() {
        let walls = vec![Wall {
            id: "w".into(), x: 100.0, y: -400.0, h: 400.0,
            pc_perm: Some(-1), item_perm: None,
        }];
        let platforms = flat_ground();

        // Approach from left — should be blocked
        let mut body = PhysicsBody::new(80.0, 0.0);
        body.on_ground = true;
        let input_right = InputState { right: true, ..Default::default() };
        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input_right, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(body.x <= 100.0 - body.half_width + 0.01,
            "Should be blocked from left, x={}", body.x);

        // Approach from right — should pass through
        let mut body2 = PhysicsBody::new(120.0, 0.0);
        body2.on_ground = true;
        let input_left = InputState { left: true, ..Default::default() };
        for _ in 0..60 {
            body2.tick(1.0 / 60.0, &input_left, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(body2.x < 100.0, "Should pass through from right, x={}", body2.x);
    }

    #[test]
    fn wall_one_way_right_blocks_from_right_only() {
        let walls = vec![Wall {
            id: "w".into(), x: 100.0, y: -400.0, h: 400.0,
            pc_perm: Some(1), item_perm: None,
        }];
        let platforms = flat_ground();

        // Approach from right — should be blocked
        let mut body = PhysicsBody::new(120.0, 0.0);
        body.on_ground = true;
        let input_left = InputState { left: true, ..Default::default() };
        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input_left, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(body.x >= 100.0 + body.half_width - 0.01,
            "Should be blocked from right, x={}", body.x);

        // Approach from left — should pass through
        let mut body2 = PhysicsBody::new(80.0, 0.0);
        body2.on_ground = true;
        let input_right = InputState { right: true, ..Default::default() };
        for _ in 0..60 {
            body2.tick(1.0 / 60.0, &input_right, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(body2.x > 100.0, "Should pass through from left, x={}", body2.x);
    }

    #[test]
    fn wall_passthrough_allows_all() {
        let walls = vec![Wall {
            id: "w".into(), x: 100.0, y: -400.0, h: 400.0,
            pc_perm: Some(0), item_perm: None,
        }];
        let platforms = flat_ground();

        let mut body = PhysicsBody::new(80.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };
        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }
        assert!(body.x > 100.0, "Passthrough wall should not block, x={}", body.x);
    }

    #[test]
    fn wall_does_not_push_player_already_past() {
        // Player spawned on the far side of a solid wall — should NOT be pushed back
        let walls = solid_wall(100.0, -400.0, 400.0);
        let platforms = flat_ground();
        let mut body = PhysicsBody::new(200.0, 0.0); // far right of wall
        body.on_ground = true;
        let input = InputState::default(); // no movement

        for _ in 0..60 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 1000.0);
        }

        assert!((body.x - 200.0).abs() < 0.01,
            "Player already past wall should not be pushed, x={}", body.x);
    }

    #[test]
    fn street_bounds_still_clamp_beyond_walls() {
        // Wall at x=100, but street right bound is at 50.
        // Street bounds should still clamp (safety net).
        let walls = solid_wall(100.0, -400.0, 400.0);
        let platforms = flat_ground();
        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };

        for _ in 0..120 {
            body.tick(1.0 / 60.0, &input, &platforms, &walls, -1000.0, 50.0);
        }

        // Clamped by street bound (50 - half_width), not by wall
        assert!(body.x <= 50.0 - body.half_width + 0.01,
            "Player should be clamped by street bound, x={}", body.x);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test -p harmony-glitch physics::movement::tests::wall_ 2>&1`
Expected: FAIL — `tick()` signature doesn't accept `walls` yet, won't compile.

- [ ] **Step 3: Update tick() signature and add wall collision phase**

In `src-tauri/src/physics/movement.rs`:

**3a.** Update import at line 3:

```rust
use crate::street::types::{PlatformLine, Wall};
```

**3b.** Add `walls: &[Wall]` parameter to `tick()` (line 62). New signature:

```rust
    pub fn tick(
        &mut self,
        dt: f64,
        input: &InputState,
        platforms: &[PlatformLine],
        walls: &[Wall],
        street_left: f64,
        street_right: f64,
    ) {
```

**3c.** Save `prev_x` alongside `prev_y`. Change the line `let prev_y = self.y;` (around line 94) to:

```rust
        let prev_x = self.x;
        let prev_y = self.y;
```

**3d.** Add wall collision phase between the move (`self.y += self.vy * dt`) and the street-bounds clamp (`self.x = self.x.clamp(...)`). Insert after line 98 (`self.y += self.vy * dt;`):

```rust
        // --- Wall collision ---
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
            if prev_x + self.half_width <= wall.x
                && self.x + self.half_width > wall.x
                && wall.blocks_from_left()
            {
                self.x = wall.x - self.half_width;
                self.vx = 0.0;
            } else if prev_x - self.half_width >= wall.x
                && self.x - self.half_width < wall.x
                && wall.blocks_from_right()
            {
                self.x = wall.x + self.half_width;
                self.vx = 0.0;
            }
        }
```

**3e.** Update ALL 19 existing test call sites in movement.rs `mod tests` to pass `&[]` for walls. Every `body.tick(...)` call that currently has 5 arguments gets `&[]` inserted after `&platforms`:

Before: `body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);`
After:  `body.tick(1.0 / 60.0, &input, &platforms, &[], -1000.0, 1000.0);`

The affected tests (19 call sites across 18 tests): `falls_with_gravity`, `lands_on_platform`, `walks_right`, `walks_left`, `jumps`, `jump_does_not_auto_repeat_while_held` (**2 calls** — lines 303 and 313), `cannot_jump_in_air`, `clamped_to_street_bounds`, `walks_on_slope`, `terminal_velocity_caps_falling`, `standing_still_on_slope_does_not_slide`, `lands_on_higher_of_overlapping_platforms`, `walks_along_slope_stays_grounded`, `walks_onto_slope_overlapping_flat_ground`, `ceiling_blocks_jump_through_solid_platform`, `can_jump_through_one_way_platform`, `lands_at_terminal_velocity`, `walking_off_platform_edge_starts_falling`.

**3f.** Update the production call site in `src-tauri/src/engine/state.rs` (must happen in same commit — changing `tick()` signature breaks state.rs otherwise).

At lines 247-250, change:

```rust
            // Physics tick — walls are parsed from street data but not yet enforced
            // in the collision system (Phase A scope: platforms only).
            self.player
                .tick(dt, input, street.platforms(), street.left, street.right);
```

to:

```rust
            self.player
                .tick(dt, input, street.platforms(), street.walls(), street.left, street.right);
```

Note: state.rs tests call `state.tick()` (the `GameState::tick()` method), not `body.tick()` directly, so they do NOT need `&[]` updates — the production code change above handles them.

- [ ] **Step 4: Run ALL tests to verify they pass**

Run: `cd src-tauri && cargo test -p harmony-glitch 2>&1`
Expected: All tests pass (19 existing movement + 9 new wall + all other = ~163 total).

- [ ] **Step 5: Run clippy**

Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/physics/movement.rs src-tauri/src/engine/state.rs
git commit -m "feat: enforce wall collision in PhysicsBody::tick()"
```
