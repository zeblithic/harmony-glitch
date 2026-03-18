# NPC Entity Movement Design

## Goal

Add movement patterns to NPC entities so chickens, pigs, and butterflies wander
within a radius of their spawn point, pausing at boundaries. Trees and other
static entities remain motionless. Rust owns all movement logic; the frontend
only reads the updated positions and facing direction from each RenderFrame.

## Non-Goals

- NPC gravity or platform collision (ground NPCs stay at spawn y)
- NPC spritesheets or frame animation (single sprite + horizontal flip)
- Waypoint paths or behavioral AI
- Runtime asset hot-reload

## Architecture

Follows the project's sans-I/O pattern: movement is a pure state machine in
Rust. The caller provides dt (tick delta) and RNG. The frontend is a dumb
renderer that reads position and facing from WorldEntityFrame.

Movement config lives in `EntityDef` (static, loaded once). Movement runtime
state lives in `EntityInstanceState` (per-instance, updated each tick). This
follows the existing pattern where EntityDef holds config and entity_states
holds runtime state.

### Interaction compatibility

`tick_entities()` writes the live position back to `WorldEntity.x` each tick.
This ensures `proximity_scan()`, `build_prompt()`, and `execute_interaction()`
use the entity's current position without any changes to the interaction code.
The original spawn position is preserved in `EntityInstanceState.wander_origin`.

### Street transition behavior

`load_street()` clears `entity_states`, so movement state is lost on
re-entering a street. Entities reinitialize at their spawn positions. This is
consistent with how harvest state is also reset and is acceptable for Phase A.

## Data Model Changes

### EntityDef additions (`entities.json` / `item/types.rs`)

```rust
pub walk_speed: Option<f64>,      // pixels/sec, None = static entity
pub wander_radius: Option<f64>,   // pixels from spawn point, None = no movement
pub bob_amplitude: Option<f64>,   // vertical sine-wave amplitude in pixels
pub bob_frequency: Option<f64>,   // vertical sine-wave frequency in Hz
```

Only animals receive movement fields. Trees omit them and remain static.
Only floating entities (butterfly) receive bob fields.

Example `entities.json` entries (note: serde `rename_all = "camelCase"`):

```json
{ "id": "chicken", "walkSpeed": 40.0, "wanderRadius": 120.0, ... }
{ "id": "pig", "walkSpeed": 35.0, "wanderRadius": 100.0, ... }
{ "id": "butterfly", "walkSpeed": 25.0, "wanderRadius": 150.0, "bobAmplitude": 15.0, "bobFrequency": 1.5, ... }
{ "id": "fruit_tree", ... }
```

### EntityInstanceState additions (`item/types.rs`)

```rust
pub current_x: f64,       // live position (initialized from WorldEntity.x)
pub velocity_x: f64,      // current horizontal velocity
pub facing: Direction,     // Left or Right
pub wander_origin: f64,   // spawn x, center of patrol range
pub idle_until: f64,       // game_time at which idle pause ends
```

These fields are lazy-initialized on first tick from the entity's spawn
position. Initial facing direction is randomized via RNG. Initial idle period
is randomized (0.0..2.0 seconds) so entities don't all start walking
simultaneously on street load.

### WorldEntityFrame additions (Rust → frontend)

```rust
pub facing: Direction,    // "left" or "right"
```

The existing `x` field reports `current_x` from movement state (written back
to `WorldEntity.x` each tick). The existing `y` field is modified for entities
with `bob_amplitude`/`bob_frequency` (sine-wave offset applied in
`build_entity_frames()`).

### Frontend type change (`types.ts`)

```typescript
export interface WorldEntityFrame {
  // ... existing fields ...
  facing: Direction;  // new
}
```

## Movement Logic

### tick_entities()

New function called from `GameState::tick()`, after player physics but before
building render frames. Iterates over all entities that have `walk_speed` and
`wander_radius` defined in their EntityDef.

For each movable entity:

1. **Lazy init**: If no movement state exists in entity_states, initialize:
   - `current_x` and `wander_origin` from `WorldEntity.x`
   - `velocity_x = 0`
   - `facing = random(Left, Right)` via RNG
   - `idle_until = game_time + random(0.0..2.0)` (staggered start)

2. **Idle check**: If `game_time < idle_until`, set `velocity_x = 0` and skip
   to step 5 (write-back). Facing is unchanged during idle.

3. **Boundary check**: Only when moving (`velocity_x != 0`). If
   `|current_x - wander_origin| >= wander_radius`:
   - Clamp `current_x` to `wander_origin +/- wander_radius`
   - Reverse `facing`
   - Set `velocity_x = 0`
   - Set `idle_until = game_time + random(1.0..3.0)`

4. **Apply movement**:
   - `direction_sign = if facing == Right { 1.0 } else { -1.0 }`
   - `velocity_x = walk_speed * direction_sign`
   - `current_x += velocity_x * dt`

5. **Write-back**: `WorldEntity.x = current_x` (keeps interaction system
   compatible without changes to proximity_scan/build_prompt/execute_interaction)

### Vertical bob (sine-wave)

Entities with `bob_amplitude` and `bob_frequency` defined in their EntityDef
get a vertical sine-wave offset applied in `build_entity_frames()`:

```
display_y = spawn_y + sin(game_time * bob_frequency * 2π) * bob_amplitude
```

This is a pure display transform, not stored in movement state. The entity's
`WorldEntity.y` is not modified.

### build_entity_frames() changes

- Use `WorldEntity.x` (which now reflects `current_x` after write-back) for
  the x position — no change needed
- Apply bob offset to `y` for entities with bob config
- Include `facing` field in WorldEntityFrame (from movement state, or default
  `Right` for static entities)

## Frontend Rendering Changes

Minimal — Rust owns all logic.

**Renderer entity loop** (`renderer.ts`): Apply facing direction by flipping
the entity container, same pattern as the avatar:

```typescript
sprite.scale.x = entity.facing === 'right' ? 1 : -1;
```

Position application is unchanged — `x` and `y` already come from
WorldEntityFrame each frame.

**No new sprite logic needed.** `SpriteManager.createEntity()` is unchanged.
The fallback upgrade path continues to work as before.

## Testing Strategy

### Rust unit tests (all movement logic)

- **Movement tick**: Entity with walk_speed/wander_radius moves each tick;
  static entity (tree) stays at spawn position
- **Boundary reversal**: Entity at wander boundary reverses facing, enters
  idle pause, and velocity drops to zero
- **Boundary only when moving**: Boundary check does not trigger when entity
  is idle at the boundary (prevents re-trigger loop)
- **Idle pause**: Entity stops moving while game_time < idle_until; resumes
  walking in reversed direction after idle expires
- **Facing direction**: Matches movement direction; unchanged during idle pause
- **Initial randomization**: Different RNG seeds produce different initial
  facing directions and idle periods
- **Write-back**: WorldEntity.x reflects current_x after tick
- **build_entity_frames()**: WorldEntityFrame includes facing field; default
  Right for static entities
- **Bob offset**: Entity with bob_amplitude/bob_frequency has oscillating y
  position over time; entity without bob fields has unchanged y
- **Lazy init**: First tick initializes movement state from spawn position
- **Static entity**: Entity without walk_speed has no movement state, position
  unchanged

### Frontend

No new frontend tests needed. `createEntity()` behavior is unchanged. The
facing flip is a trivial field read in the renderer.

### Integration

Manual verification: `npm run tauri dev`, observe chickens/pigs wandering and
pausing at boundaries, butterfly floating with sine-wave bob, trees stationary.

## Files Modified

### Rust
- `src-tauri/src/item/types.rs` — Add walk_speed, wander_radius, bob_amplitude,
  bob_frequency to EntityDef; add movement fields to EntityInstanceState; add
  facing to WorldEntityFrame
- `src-tauri/src/item/loader.rs` — Parse new EntityDef fields from JSON
- `src-tauri/src/engine/state.rs` — Add tick_entities() call in tick();
  update build_entity_frames() for facing and bob offset
- `assets/entities.json` — Add walkSpeed, wanderRadius, bobAmplitude,
  bobFrequency to animal entities

### Frontend
- `src/lib/types.ts` — Add facing to WorldEntityFrame interface
- `src/lib/engine/renderer.ts` — Apply facing flip to entity sprites

### Tests
- `src-tauri/src/engine/state.rs` or new test module — Movement unit tests
