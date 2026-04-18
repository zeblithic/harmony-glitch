# Street Re-entry & Fall-Into-Void Design (ZEB-132)

## Goal

Streets should behave as closed containers. A player can never end up in a
state where they have no ground to recover from — no infinite falls, no
out-of-bounds dead zones. When a player uses a signpost to enter a street,
the arrival location is authored on the connection, not reverse-engineered
by scanning the destination street for a reciprocal signpost.

## Scope

Three tightly-coupled threads land together:

1. **Physics containment** — the vertical axis gets the same treatment the
   horizontal axis already has. `player.y` is clamped to `street.bottom` at
   the end of the physics tick, closing the infinite-fall bug at the lowest
   layer.

2. **Arrival-owned-by-connection** — each `SignpostConnection` declares where
   on the target street the player lands. The origin signpost is authoritative.
   The `.find()`-based disambiguation currently in `state.rs:646` becomes a
   legacy fallback for XMLs that don't yet have arrival fields.

3. **Safety-net respawn** — when a player ends up stuck on the invisible floor
   at `street.bottom` with no platform above them, the engine teleports them
   to the street's `last_arrival` point (initialized from a new
   `default_spawn`, overwritten on every signpost traversal).

The invariant these establish: **a player can never end up in a location
they can't walk out of.** Either geometry gives them ground (author's job),
physics gives them `street.bottom` (our guarantee), or the safety net gives
them `last_arrival` (our backstop).

## Out of scope

- **Trapezoidal re-entry geometry** — streets where a ramp's upper end
  leaves a "gap over nothing" at the edge. Handled by street-design
  convention: ramps never intersect flat ground; taking a ramp requires a
  small jump, so players default to staying on the main road. No code change.

- **Multiple-connection disambiguation at the UI level** — a single signpost
  with several outgoing connections still presents as one point of
  interaction. Picking among connections is separate UI territory (signpost
  menu / confirmation dialog), a different ticket.

- **Real-Glitch-XML arrival ingest** — the schema is additive with `Option`
  fields. Imported Glitch streets that don't carry arrival data continue to
  work via the legacy fallback. When we want to eliminate the fallback for
  real Glitch data, the `street-import` tool gains a pass to emit arrival
  coordinates from Glitch's own XML; that's a separate ticket.

- **Platform permeability changes** — `PlatformLine.pc_perm` already models
  top-only vs. both-sides solidity correctly. No change to floor/ceiling
  semantics.

## Architecture

The three threads form one coherent change because they share a single new
concept: `last_arrival: Point`, the most recent "valid landing location on
this street" the engine has recorded.

- On fresh street load (not via signpost), `last_arrival` is initialized
  from `resolve_default_spawn(&street)`.
- On signpost traversal, `last_arrival` is overwritten with
  `resolve_arrival(target_street, origin_tsid, active_connection)`.
- The safety-net teleport sets `player` position to `last_arrival`.

This threads the signpost-arrival-spec and the OOB respawn through the same
mechanism. An author only has to say "this connection takes players to
`(arrival_x, arrival_y)`" once; the runtime uses that same point for both
normal arrival and safety-net recovery.

## Data model

Current `StreetData` and `SignpostConnection` (from
`src-tauri/src/street/types.rs`) get additive, `Option`-typed fields so that
existing XMLs, serialized saves, and IPC payloads continue to parse.

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Facing { Left, Right }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub x: f64,
    pub y: f64,
    pub facing: Option<Facing>,
}

pub struct StreetData {
    // ... existing fields (tsid, name, left, right, top, bottom, ground_y,
    //     gradient, layers, signposts) ...

    /// Where the player spawns on first entry (before any signpost traversal)
    /// and the last-resort fallback when resolve_arrival can't find anything
    /// else. If None, runtime resolves to the first signpost's (x, y) or
    /// (street_center, ground_y) if the street has no signposts.
    pub default_spawn: Option<SpawnPoint>,
}

pub struct SignpostConnection {
    pub target_tsid: String,
    pub target_label: String,

    /// Where on the target street the player arrives when using this
    /// connection. If None, runtime falls back to the target signpost's
    /// own (x, y) — preserves legacy behavior for imported Glitch XMLs.
    pub arrival_x: Option<f64>,
    pub arrival_y: Option<f64>,
    /// Player facing on arrival. If None, inferred from which half of the
    /// target street arrival_x sits in (inward-from-center, same rule as
    /// state.rs:646 uses today).
    pub arrival_facing: Option<Facing>,
}
```

### XML representation

New attributes on `<connect>`, plus a new top-level `<default_spawn>`
element sibling to `<dynamic>` / `<layers>`:

```xml
<street tsid="demo_meadow" name="Demo Meadow" l="-2000" r="2000" t="-800" b="0">
  <default_spawn x="0" y="0" facing="right"/>

  <dynamic>
    <signpost id="east_gate" x="1900" y="0">
      <connect target_tsid="demo_heights"
               target_label="to Heights"
               arrival_x="-1700" arrival_y="0" arrival_facing="right"/>
    </signpost>
    <!-- ... -->
  </dynamic>

  <!-- ... -->
</street>
```

All new attributes/elements are optional. The parser treats absence as `None`
and the runtime resolves defaults. Existing demo_meadow.xml and demo_heights.xml
get a migration pass (see Migration below).

### Runtime state

One new field on `GameState`:

```rust
pub struct GameState {
    // ... existing fields ...
    pub last_arrival: Point,
    pub oob_ticks: u32,
}
```

`last_arrival` is set on street load (from default_spawn) and updated on
signpost traversal (from the connection's resolved arrival). `oob_ticks`
is the counter the safety-net detector uses; reset on every street load.

## Physics changes

File: `src-tauri/src/physics/movement.rs`, `PhysicsBody::tick`.

`tick` already receives `street_left: f64` and `street_right: f64`. Add a
third: `street_bottom: f64`. All call sites pass it through from
`StreetData::bottom`.

At the end of the tick — after Phase 1 (slope-follow), Phase 2 (swept
platform landing), and Phase 3 (ceiling block) — add:

```rust
// Hard floor: street.bottom is the absolute lowest Y any entity can occupy.
// Ground geometry should cover the play area, but this clamp prevents the
// infinite-fall bug if a player walks off an un-platformed gap.
if self.y > street_bottom {
    self.y = street_bottom;
    self.vy = 0.0;
}
```

`vx` is deliberately preserved; the player retains horizontal control on
the invisible floor, so they can attempt to walk toward a real platform
before the safety-net timer fires.

**Interaction with existing collision phases:** the clamp runs *after*
Phases 1 and 2. If either lands the player on a platform higher than
`street.bottom`, they stay there. The clamp only engages when nothing else
caught them — exactly the fall-into-void scenario.

## Engine & state changes

Files: `src-tauri/src/engine/state.rs`, `src-tauri/src/engine/transition.rs`.

### 1. Arrival resolution helper

```rust
/// Resolve the point at which a player should land on `street` when arriving
/// via `connection`. Prefers the connection's explicit arrival_x/arrival_y;
/// falls back to the target signpost's own (x, y) for legacy XMLs without
/// arrival fields; last-resorts to the street's default_spawn.
fn resolve_arrival(
    street: &StreetData,
    origin_tsid: &str,
    connection: Option<&SignpostConnection>,
) -> SpawnPoint {
    if let Some(c) = connection {
        if let (Some(x), Some(y)) = (c.arrival_x, c.arrival_y) {
            return SpawnPoint { x, y, facing: c.arrival_facing };
        }
    }
    if let Some(sp) = street.signposts.iter().find(|s|
        s.connects.iter().any(|c| c.target_tsid == origin_tsid)
    ) {
        return SpawnPoint { x: sp.x, y: street.ground_y, facing: None };
    }
    resolve_default_spawn(street)
}
```

### 2. Default-spawn resolution

```rust
fn resolve_default_spawn(street: &StreetData) -> SpawnPoint {
    if let Some(s) = street.default_spawn {
        return s;
    }
    if let Some(sp) = street.signposts.first() {
        return SpawnPoint { x: sp.x, y: street.ground_y, facing: None };
    }
    SpawnPoint {
        x: (street.left + street.right) / 2.0,
        y: street.ground_y,
        facing: None,
    }
}
```

### 3. `state.rs:646` rewrite

The post-signpost arrival block currently uses `.find()` on the target
street's signposts to figure out where to place the player. That block
is replaced with a single call to `resolve_arrival`:

```rust
// origin_tsid is the TSID of the street the player just left.
// active_connection is the SignpostConnection they used to traverse.
let arrival = resolve_arrival(&target_street, &origin_tsid, Some(active_connection));
self.last_arrival = Point { x: arrival.x, y: arrival.y };
self.player.x = arrival.x;
self.player.y = arrival.y;
self.player.facing = arrival.facing.unwrap_or_else(|| {
    // Inward-from-center inference, same as today
    let street_mid = (target_street.left + target_street.right) / 2.0;
    if arrival.x < street_mid { Facing::Right } else { Facing::Left }
});
self.oob_ticks = 0;
```

On fresh street load (not via signpost, e.g., game start or save restore):

```rust
let spawn = resolve_default_spawn(&street);
self.last_arrival = Point { x: spawn.x, y: spawn.y };
self.player.x = spawn.x;
self.player.y = spawn.y;
self.player.facing = spawn.facing.unwrap_or(Facing::Right);
self.oob_ticks = 0;
```

### 4. OOB detector + safety-net teleport

Added to the tick loop in `GameState::tick`, immediately after the physics
step:

```rust
const OOB_THRESHOLD_TICKS: u32 = 30; // ~500ms at 60fps

let at_floor = (self.player.y - street.bottom).abs() < 1.0;
if at_floor && !self.physics.grounded {
    self.oob_ticks += 1;
    if self.oob_ticks >= OOB_THRESHOLD_TICKS {
        self.player.x = self.last_arrival.x;
        self.player.y = self.last_arrival.y;
        self.physics.vx = 0.0;
        self.physics.vy = 0.0;
        self.oob_ticks = 0;
        log_oob_respawn(street.tsid, &self.player, &self.last_arrival);
    }
} else {
    self.oob_ticks = 0;
}
```

The threshold window matters: walking off a platform, hitting the clamp,
and running onto a low adjacent platform within half a second should NOT
respawn the player — that's valid recovery. Only persistent
stuck-on-void-floor triggers the net.

`PhysicsBody::grounded` already reflects the result of Phase 1/2 collision
resolution and is the right signal here. If the field is currently private,
expose it (`pub fn grounded(&self) -> bool`) for the engine's read-only use.

## Testing

### Rust unit tests (`src-tauri`)

`physics::movement`:
- `player.y` never exceeds `street.bottom` after `tick`, given a high initial `vy` with no platforms below
- Hard floor does NOT engage when Phase 2 swept landing catches the player on a higher platform
- `vx` is preserved when the clamp fires (horizontal input still works on the invisible floor)

`engine::state::resolve_arrival`:
- Explicit `arrival_x`/`arrival_y` on the connection wins over legacy fallback
- Legacy `.find()` fallback fires when connection has no arrival fields and a reciprocal signpost exists on the target street
- Last-resort returns `resolve_default_spawn(street)` when neither connection nor reciprocal signpost resolves

`engine::state::resolve_default_spawn`:
- Returns `street.default_spawn` when present
- Falls back to first signpost's (x, y) + ground_y when no default_spawn
- Returns (center_x, ground_y) when street has no signposts at all

`engine::state::oob_detector`:
- Player stuck at `street.bottom` with `!grounded` for 30+ ticks triggers teleport to `last_arrival`
- Player grounded on a platform resets the counter (no respawn)
- Player walking on a platform that happens to sit at `street.bottom` (like `plat_main` at y=0) does NOT trigger — `grounded` is true

### Integration tests (`src-tauri`)

- Fresh street load sets `last_arrival` from `resolve_default_spawn`; `player.x/y/facing` match
- Signpost traversal sets `last_arrival` from the active connection's resolved arrival; player lands at the XML's arrival coords, not the reciprocal signpost's position
- Falling off an un-platformed test street edge eventually triggers respawn to `last_arrival`

### Frontend tests

None required. Rendering is agnostic to the change — player coordinates update through the existing render-frame pipeline.

### Manual verification (PR description checklist)

- [ ] `npm run tauri dev` → walk off an un-walled edge of a test street; respawn fires within ~0.5s rather than infinite fall
- [ ] Walk off a platform but land on a low adjacent platform within the 30-tick window; no respawn
- [ ] Traverse both signposts in demo_meadow ↔ demo_heights round-trip; arrival position matches the XML's arrival fields, not the reciprocal-signpost position
- [ ] Fresh game start on each demo street; initial position matches `<default_spawn>`

## Migration

1. **Demo streets** — `assets/streets/demo_meadow.xml` and
   `assets/streets/demo_heights.xml`. Add `arrival_x`, `arrival_y`,
   `arrival_facing` to each `<connect>` element, and a `<default_spawn>`
   element to each street root. Arrival positions chosen so the player
   lands inward of the signpost, clear of the signpost sprite, at
   `ground_y`.

2. **Imported Glitch streets** — anything under `streets/` produced by the
   `street-import` tool. No migration. They hit the legacy `.find()`
   fallback and behave exactly as today. Upgrading these to carry arrival
   data (so the fallback can eventually be removed) is a separate ticket
   scoped to `street-import`.

3. **Breaking changes** — none. All new struct fields are `Option<…>`,
   new XML attributes/elements are optional, and serialized saves that
   predate this change deserialize fine with `None` defaults.

## Observability

On safety-net fire, emit one `info`-level log line with the street tsid,
`last_arrival` coordinates, and the player's OOB position at the moment of
teleport. This surfaces geometry gaps found during playtesting without
adding per-tick noise.

Example:

```
[INFO] OOB respawn: street=demo_meadow player=(1243.2, 0.0) → last_arrival=(0.0, 0.0)
```

No additional UI is required; if the teleport feels jarring during
playtesting, a fade-to-black transition can be added later.
