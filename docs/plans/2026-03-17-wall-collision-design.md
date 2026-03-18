# Wall Collision Enforcement — Design Spec

**Bead:** glitch-h2m
**Date:** 2026-03-17
**Status:** Approved

## Problem

Walls are parsed from street XML into `Wall { x, y, h, pc_perm, item_perm }` structs and stored on the middleground layer, but the physics system ignores them. Players walk freely past wall positions. The only horizontal constraint is the street-bounds clamp (`l`/`r`), which is typically wider than the wall positions (e.g., walls at x=-1900/1900 within a street bounded at -3000/3000).

## Goals

- Enforce wall collision in `PhysicsBody::tick()` alongside existing platform collision
- Support directional walls (`pc_perm`: solid, one-way left, one-way right, pass-through)
- Respect wall vertical extent — walls only block when player overlaps vertically
- Preserve street-bounds clamp as a safety net behind wall enforcement

## Non-Goals

- Wall rendering or visual indicators (walls are invisible barriers, matching original Glitch)
- Item collision with walls (`item_perm` — future work)
- Ladder interaction (separate feature)

## Wall Geometry

A wall is a vertical line segment at position `(x, y)` extending downward by `h` pixels. In Glitch coordinates (Y=0 at ground, negative Y = up):

- Wall top: `y` (more negative = higher)
- Wall bottom: `y + h` (closer to 0 = lower)

Example from demo streets: `wall_left { x: -1900, y: -400, h: 400 }` spans from y=-400 to y=0 (ground level).

### Vertical Overlap

A wall blocks the player only when their vertical extents overlap:

```
player_top = body.y - body.height    (head position)
player_bottom = body.y               (feet position)
wall_top = wall.y
wall_bottom = wall.y + wall.h

overlaps = player_bottom > wall_top && player_top < wall_bottom
```

### Directional Permissions (`pc_perm`)

Same bit-pattern structure as `PlatformLine`, but applied to horizontal directions instead of vertical:

| `pc_perm` | Platform meaning (vertical) | Wall meaning (horizontal) |
|-----------|----------------------------|---------------------------|
| `None`    | Solid from both sides | Solid from both sides |
| `-1`      | One-way from top | Blocks movement from left only |
| `1`       | One-way from bottom | Blocks movement from right only |
| `0`       | Pass-through | Pass-through |

## Collision Logic

Wall collision runs as a new phase inside `PhysicsBody::tick()`, placed between the horizontal move (`self.x += self.vx * dt`) and the existing street-bounds clamp. This ordering ensures walls take priority within the street, and the street-bounds clamp catches anything beyond.

### Horizontal Sweep

After the horizontal move, for each wall with vertical overlap:

1. **Moving right** (player was left of wall, now overlaps): if `prev_x + half_width <= wall.x` and `new_x + half_width > wall.x` and `wall.blocks_from_left()`: push back to `wall.x - half_width`, zero `vx`.
2. **Moving left** (player was right of wall, now overlaps): if `prev_x - half_width >= wall.x` and `new_x - half_width < wall.x` and `wall.blocks_from_right()`: push back to `wall.x + half_width`, zero `vx`.

The `prev_x` comparison ensures the wall only blocks movement *through* it, not players already on the other side (which matters for one-way walls).

### Phase Ordering in `tick()`

```
1. Apply horizontal input (vx)
2. Apply jump / gravity (vy)
3. Save prev_x, prev_y          ← prev_x is NEW (only prev_y exists currently)
4. Move (x += vx*dt, y += vy*dt)
5. ** Wall collision (NEW) **    ← horizontal push-back using prev_x
6. Street-bounds clamp           ← safety net
7. Platform collision (3 phases: slope-follow, swept, ceiling)
```

Walls are processed in slice order. If multiple walls apply simultaneously (rare — typically 1-2 walls per street), the last push wins.

Wall collision runs before the street-bounds clamp so that walls are the primary constraint. The clamp remains as a fallback for streets without walls or edge cases.

## Data Model Changes

### Wall Helper Methods (new)

```rust
impl Wall {
    /// Whether this wall blocks movement from the left.
    pub fn blocks_from_left(&self) -> bool {
        !matches!(self.pc_perm, Some(1) | Some(0))
    }

    /// Whether this wall blocks movement from the right.
    pub fn blocks_from_right(&self) -> bool {
        !matches!(self.pc_perm, Some(-1) | Some(0))
    }

    /// Bottom Y extent of the wall.
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
}
```

### StreetData Accessor (new)

```rust
impl StreetData {
    pub fn walls(&self) -> &[Wall] {
        self.middleground()
            .map(|l| l.walls.as_slice())
            .unwrap_or(&[])
    }
}
```

### PhysicsBody::tick() Signature Change

```rust
pub fn tick(
    &mut self,
    dt: f64,
    input: &InputState,
    platforms: &[PlatformLine],
    walls: &[Wall],        // NEW
    street_left: f64,
    street_right: f64,
)
```

## Call Site Changes

### state.rs

```rust
self.player.tick(dt, input, street.platforms(), street.walls(), street.left, street.right);
```

### Existing Tests

All existing `tick()` call sites in movement.rs tests and state.rs tests pass `&[]` for the new `walls` parameter. Behavior is identical to current (no walls = no wall collision).

## Testing

### Unit tests — types.rs

- `wall_blocks_from_left`: `None` and `-1` block from left, `1` and `0` don't
- `wall_blocks_from_right`: `None` and `1` block from right, `-1` and `0` don't
- `wall_bottom_extent`: `bottom()` returns `y + h`

### Unit tests — types.rs (additional)

- `walls_accessor_returns_middleground_walls`: `StreetData::walls()` returns walls from middleground layer

### Unit tests — movement.rs

- `wall_blocks_movement_from_left`: player walks right into solid wall, pushed back to `wall.x - half_width`
- `wall_blocks_movement_from_right`: player walks left into solid wall, pushed back to `wall.x + half_width`
- `wall_does_not_block_when_above`: player head and feet both above wall top (player_bottom < wall.y), passes through horizontally
- `wall_does_not_block_when_below`: player head and feet both below wall bottom (player_top > wall.y + wall.h), not blocked
- `wall_one_way_left_blocks_from_left_only`: `pc_perm = -1` blocks left-to-right, allows right-to-left
- `wall_one_way_right_blocks_from_right_only`: `pc_perm = 1` blocks right-to-left, allows left-to-right
- `wall_passthrough_allows_all`: `pc_perm = 0` never blocks
- `wall_does_not_push_player_already_past`: player spawned on far side of solid wall is not retroactively pushed back (verifies prev_x guard)
- `street_bounds_still_clamp_beyond_walls`: walls inside street, player still clamped by outer bounds

## Files Modified

| File | Change |
|------|--------|
| `src-tauri/src/street/types.rs` | Add `blocks_from_left()`, `blocks_from_right()`, `bottom()` to `Wall`, add `walls()` to `StreetData` |
| `src-tauri/src/physics/movement.rs` | Add `walls: &[Wall]` param to `tick()`, save `prev_x` before move, wall collision phase, update existing test call sites |
| `src-tauri/src/engine/state.rs` | Pass `street.walls()` to `tick()`, remove stale "walls not yet enforced" comment, update test call sites |

No frontend changes. No data changes. Demo streets already define walls that will start being enforced.
