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
- Changes to the existing interaction system (proximity, cooldown, depletion)
- Runtime asset hot-reload

## Architecture

Follows the project's sans-I/O pattern: movement is a pure state machine in
Rust. The caller provides dt (tick delta) and RNG. The frontend is a dumb
renderer that reads position and facing from WorldEntityFrame.

Movement config lives in `EntityDef` (static, loaded once). Movement runtime
state lives in `EntityInstanceState` (per-instance, updated each tick). This
follows the existing pattern where EntityDef holds config and entity_states
holds runtime state.

## Data Model Changes

### EntityDef additions (`entities.json` / `item/types.rs`)

```rust
pub walk_speed: Option<f64>,    // pixels/sec, None = static entity
pub wander_radius: Option<f64>, // pixels from spawn point, None = no movement
```

Only animals receive these fields. Trees omit them and remain static.

Example `entities.json` entries:

```json
{ "id": "chicken", "walk_speed": 40.0, "wander_radius": 120.0, ... }
{ "id": "pig", "walk_speed": 35.0, "wander_radius": 100.0, ... }
{ "id": "butterfly", "walk_speed": 25.0, "wander_radius": 150.0, ... }
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

These fields are lazy-initialized on first tick from the entity's spawn position.

### WorldEntityFrame additions (Rust → frontend)

```rust
pub facing: Direction,    // "left" or "right"
```

The existing `x` field reports `current_x` from movement state instead of the
static spawn position. The existing `y` field is modified for butterflies only
(sine-wave offset).

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

1. **Lazy init**: If no movement state exists in entity_states, initialize
   `current_x` and `wander_origin` from `WorldEntity.x`, `velocity_x = 0`,
   `facing = Right`, `idle_until = 0`.

2. **Idle check**: If `game_time < idle_until`, skip movement (velocity stays
   at 0, facing unchanged).

3. **Boundary check**: If `|current_x - wander_origin| >= wander_radius`,
   reverse direction and enter idle pause:
   - Clamp `current_x` to boundary
   - Set `velocity_x = 0`
   - Set `idle_until = game_time + random(1.0..3.0)`

4. **Apply movement**: Otherwise:
   - `velocity_x = walk_speed * direction_sign`
   - `current_x += velocity_x * dt`
   - `facing = if velocity_x > 0 { Right } else { Left }`

### Butterfly sine-wave

Butterflies use the same wander_radius patrol for horizontal movement. In
addition, `build_entity_frames()` applies a vertical sine-wave offset to the
butterfly's y position:

```
display_y = spawn_y + sin(game_time * frequency) * amplitude
```

Suggested values: frequency = 1.5, amplitude = 15.0 pixels.

This keeps the sine-wave computation out of the movement tick (it's purely a
display transform applied when building the frame).

### build_entity_frames() changes

- Use `current_x` from movement state (if initialized) instead of static
  `WorldEntity.x`
- Apply butterfly sine offset to `y`
- Include `facing` field in WorldEntityFrame
- Default `facing` to `Right` for static entities (trees)

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
- **Boundary reversal**: Entity at wander boundary reverses velocity and enters
  idle pause
- **Idle pause**: Entity stops moving while game_time < idle_until; resumes
  after idle expires
- **Facing direction**: Matches velocity direction; unchanged during idle pause
- **build_entity_frames()**: WorldEntityFrame uses current_x from movement
  state; includes facing field
- **Butterfly sine offset**: y position oscillates over time
- **Lazy init**: First tick initializes movement state from spawn position
- **Static entity**: Entity without walk_speed has no movement state, position
  unchanged

### Frontend

No new frontend tests needed. `createEntity()` behavior is unchanged. The
facing flip is a trivial field read in the renderer.

### Integration

Manual verification: `npm run tauri dev`, observe chickens/pigs wandering and
pausing at boundaries, butterfly floating in sine wave, trees stationary.

## Files Modified

### Rust
- `src-tauri/src/item/types.rs` — Add walk_speed, wander_radius to EntityDef;
  add movement fields to EntityInstanceState; add facing to WorldEntityFrame
- `src-tauri/src/item/loader.rs` — Parse new EntityDef fields from JSON
- `src-tauri/src/engine/state.rs` — Add tick_entities() call in tick();
  update build_entity_frames() for current_x, facing, butterfly sine
- `assets/entities.json` — Add walk_speed and wander_radius to animal entities

### Frontend
- `src/lib/types.ts` — Add facing to WorldEntityFrame interface
- `src/lib/engine/renderer.ts` — Apply facing flip to entity sprites

### Tests
- `src-tauri/src/engine/state.rs` or new test module — Movement unit tests
