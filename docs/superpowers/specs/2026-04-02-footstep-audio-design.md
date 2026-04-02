# Footstep Audio — Animation-Synced, Surface-Dependent

**Issue:** glitch-1zc
**Date:** 2026-04-02
**Status:** Approved

## Overview

Per-step footstep sounds driven by horizontal distance traveled on the ground. Surface-dependent: platforms carry a `surface` field that selects which footstep sound variant plays. Builds on the existing audio event pipeline and SoundKit variant resolution system.

## Platform Surface Types

### PlatformLine Schema

`PlatformLine` gains an optional `surface` field, defaulting to `"default"`:

```rust
pub struct PlatformLine {
    pub id: String,
    pub start: Point,
    pub end: Point,
    pub pc_perm: Option<i32>,
    pub item_perm: Option<i32>,
    pub surface: String,  // NEW — defaults to "default"
}
```

Uses `String` (not `Option<String>`) because every platform always has a surface — just might be `"default"`.

### Street XML

New optional `<str id="surface">` element on platform objects:

```xml
<object id="plat_main">
  <object id="start"><int id="x">-1800</int><int id="y">0</int></object>
  <object id="end"><int id="x">1800</int><int id="y">0</int></object>
  <str id="surface">grass</str>
</object>
```

Parsed with the same `.get().and_then()` pattern as `pc_perm`, falling back to `"default".to_string()` when absent.

### Recognized Surfaces

Default kit ships with: `grass`, `stone`, `wood`. Any other string value is valid (custom kits can define variants for it) and falls back to the `default` sound.

## Footstep Event Emission

### Distance Accumulator

New field on `PhysicsBody`: `distance_since_footstep: f64`. Tracks absolute horizontal distance traveled while on the ground.

**Cadence constant:** `FOOTSTEP_STRIDE = 40.0` pixels. At walk speed (200 px/s), this produces ~5 footsteps per second.

### Emission Logic

In `GameState::tick()`, after physics update:

1. If `on_ground && |vx| > 0.1`: accumulate `|dx|` into `distance_since_footstep`
2. When accumulator >= `FOOTSTEP_STRIDE`: emit `AudioEvent::Footstep { surface }`, subtract `FOOTSTEP_STRIDE` from accumulator (preserving remainder for smooth cadence)
3. Reset accumulator to 0 when player leaves ground (jump/fall) or stops moving (`|vx| <= 0.1`)

### Surface Lookup

When emitting a footstep, look up which platform the player is standing on (already determined by collision resolution). Use that platform's `surface` field. If standing on `ground_y` (no platform), use `"default"`.

### AudioEvent Variant

```rust
Footstep { surface: String }
```

Serialized as `{ "type": "footstep", "surface": "grass" }` following existing camelCase serde conventions.

## SoundKit & Frontend

### Default Kit

New entry in `default-kit.json`:

```json
"footstep": {
  "default": "sfx/footstep-default.mp3",
  "variants": {
    "grass": "sfx/footstep-grass.mp3",
    "stone": "sfx/footstep-stone.mp3",
    "wood": "sfx/footstep-wood.mp3"
  }
}
```

Four short (0.1–0.3s) percussive audio files in `assets/audio/sfx/`.

### Frontend Processing

`AudioManager.processEvents()` adds a case for `"footstep"` — calls `playSfx('footstep', event.surface)`. The existing variant resolution handles the rest: looks up `variants[surface]`, falls back to `default`.

TypeScript `AudioEvent` union gains:

```typescript
{ type: 'footstep'; surface: string }
```

No new AudioManager logic needed beyond the event case.

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Platform has no `surface` in XML | Defaults to `"default"` — generic footstep plays |
| SoundKit missing `footstep` entry | `playSfx` returns early (existing behavior for unknown event types) |
| Unknown surface string (e.g. `"marble"`) | Variant lookup misses, falls back to `footstep.default` sound |
| Player lands from jump while moving | Accumulator starts at 0, first footstep after 40px — no double-tap with `Land` event |
| Player stops mid-stride | Accumulator resets to 0, no lingering partial stride |
| Rapid direction changes | `|dx|` accumulated as absolute value, reversals count as distance — feels natural |
| Custom kit with no footstep variants | `default` entry plays for all surfaces |

## Testing

### Rust Unit Tests

- `surface` field parses from XML (present → value, absent → `"default"`)
- `PlatformLine` serialization includes `surface`
- Footstep event emits after accumulating `FOOTSTEP_STRIDE` pixels
- Accumulator resets on leaving ground
- Accumulator resets on stopping (`|vx| <= 0.1`)
- Correct surface lookup from current platform
- Remainder preserved across strides (no drift)

### Frontend Tests (audio.test.ts)

- `processEvents` handles `footstep` event type
- `footstep` event passes `surface` as variant key to `playSfx`

### Manual Verification

- Walk on platforms with different surfaces — hear distinct sounds
- Jump mid-walk — no footstep while airborne, resumes on landing
- Stop walking — footsteps stop immediately
- Platform without surface tag — generic footstep plays
- Switch sound kit — footstep sounds update with kit

## Out of Scope

- Footstep volume scaling by speed (constant volume for now)
- Remote player footsteps (local player only)
- Footstep particle effects or visual indicators
- Footstep echo/reverb based on environment
