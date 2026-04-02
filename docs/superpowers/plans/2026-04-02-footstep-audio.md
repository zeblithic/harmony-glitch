# Footstep Audio Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add distance-based, surface-dependent footstep sounds that play while the player walks on platforms.

**Architecture:** Rust backend tracks horizontal distance traveled on ground and emits `Footstep { surface }` audio events when stride threshold is reached. Surface comes from a new `surface` field on `PlatformLine`, parsed from street XML. Frontend handles the new event type through existing variant resolution in `AudioManager.processEvents()`.

**Tech Stack:** Rust (Tauri v2), Svelte 5, TypeScript, Howler.js, quick-xml

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `src-tauri/src/street/types.rs` | Modify | Add `surface: String` to `PlatformLine`, update test constructions |
| `src-tauri/src/street/parser.rs` | Modify | Parse `surface` from XML, update test constructions |
| `src-tauri/src/engine/audio.rs` | Modify | Add `Footstep { surface }` variant to `AudioEvent` |
| `src-tauri/src/physics/movement.rs` | Modify | Add `distance_since_footstep` accumulator to `PhysicsBody` |
| `src-tauri/src/engine/state.rs` | Modify | Surface lookup, footstep emission logic in `tick()` |
| `src/lib/types.ts` | Modify | Add `footstep` to `AudioEvent` union |
| `src/lib/engine/audio.ts` | Modify | Add `footstep` case in `processEvents` |
| `src/lib/engine/audio.test.ts` | Modify | Test footstep event handling |
| `assets/audio/default-kit.json` | Modify | Add `footstep` entry with surface variants |
| `assets/audio/sfx/footstep-default.mp3` | Create | Placeholder audio (~0.15s) |
| `assets/audio/sfx/footstep-grass.mp3` | Create | Placeholder audio (~0.15s) |
| `assets/audio/sfx/footstep-stone.mp3` | Create | Placeholder audio (~0.15s) |
| `assets/audio/sfx/footstep-wood.mp3` | Create | Placeholder audio (~0.15s) |
| `assets/streets/demo_meadow.xml` | Modify | Add `surface` tags to platforms |
| `assets/streets/demo_heights.xml` | Modify | Add `surface` tags to platforms |

---

### Task 1: Add `surface` field to PlatformLine struct

**Files:**
- Modify: `src-tauri/src/street/types.rs:54-63`

- [ ] **Step 1: Add `surface` field to PlatformLine**

In `src-tauri/src/street/types.rs`, add the `surface` field to `PlatformLine`:

```rust
/// A platform line segment. Players walk along these.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformLine {
    pub id: String,
    pub start: Point,
    pub end: Point,
    /// -1 = one-way from top, 1 = one-way from bottom, 0 = pass-through, None = solid
    pub pc_perm: Option<i32>,
    pub item_perm: Option<i32>,
    /// Surface material for footstep sounds (e.g. "grass", "stone", "wood").
    /// Defaults to "default" when not specified in street XML.
    pub surface: String,
}
```

- [ ] **Step 2: Fix all existing PlatformLine constructions in types.rs tests**

Every test in `types.rs` that constructs a `PlatformLine` needs the new field. Add `surface: "default".into()` to each. There are 5 constructions in tests: `platform_y_at_flat`, `platform_y_at_sloped`, `platform_one_way_from_top`, `platform_fully_solid`, and `serializes_to_camel_case`.

Example (apply to all 5):

```rust
let p = PlatformLine {
    id: "test".into(),
    start: Point { x: 0.0, y: -100.0 },
    end: Point { x: 200.0, y: -100.0 },
    pc_perm: None,
    item_perm: None,
    surface: "default".into(),
};
```

- [ ] **Step 3: Add test for surface serialization**

Add a test in the `types.rs` `mod tests` block:

```rust
#[test]
fn platform_surface_serializes() {
    let p = PlatformLine {
        id: "p1".into(),
        start: Point { x: 0.0, y: 0.0 },
        end: Point { x: 100.0, y: 0.0 },
        pc_perm: None,
        item_perm: None,
        surface: "grass".into(),
    };
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains(r#""surface":"grass""#));
}
```

- [ ] **Step 4: Run tests to verify**

Run: `cd src-tauri && cargo test street::types`
Expected: All tests pass (existing + new surface test). Some tests in other modules may fail due to missing `surface` field — those are fixed in subsequent tasks.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/street/types.rs
git commit -m "feat(street): add surface field to PlatformLine for footstep audio"
```

---

### Task 2: Parse `surface` from street XML

**Files:**
- Modify: `src-tauri/src/street/parser.rs:148-200`

- [ ] **Step 1: Update parse_platform_lines to extract surface**

In `src-tauri/src/street/parser.rs`, inside the `parse_platform_lines` function, add surface extraction in the map closure (after `item_perm`):

```rust
let surface = p
    .get("surface")
    .and_then(|v| v.as_str())
    .unwrap_or("default")
    .to_string();
```

And add `surface` to the `PlatformLine` construction:

```rust
PlatformLine {
    id: plat_id.clone(),
    start,
    end,
    pc_perm,
    item_perm,
    surface,
}
```

- [ ] **Step 2: Add test for surface parsing (present)**

Add a test in `parser.rs` `mod tests`:

```rust
#[test]
fn parse_platform_surface_from_xml() {
    let xml = r#"
    <game_object tsid="GTEST" label="Surface">
      <object id="dynamic">
        <str id="tsid">LSURF</str>
        <str id="label">Surface Street</str>
        <int id="l">-500</int>
        <int id="r">500</int>
        <int id="t">-200</int>
        <int id="b">0</int>
        <int id="ground_y">0</int>
        <object id="layers">
          <object id="middleground">
            <int id="w">1000</int>
            <int id="h">200</int>
            <int id="z">0</int>
            <str id="name">middleground</str>
            <object id="platform_lines">
              <object id="plat_grass">
                <object id="start">
                  <int id="x">-400</int>
                  <int id="y">0</int>
                </object>
                <object id="end">
                  <int id="x">0</int>
                  <int id="y">0</int>
                </object>
                <str id="surface">grass</str>
              </object>
              <object id="plat_plain">
                <object id="start">
                  <int id="x">0</int>
                  <int id="y">0</int>
                </object>
                <object id="end">
                  <int id="x">400</int>
                  <int id="y">0</int>
                </object>
              </object>
            </object>
          </object>
        </object>
      </object>
    </game_object>
    "#;

    let street = parse_street(xml).unwrap();
    let platforms = street.platforms();
    assert_eq!(platforms.len(), 2);

    let grass = platforms.iter().find(|p| p.id == "plat_grass").unwrap();
    assert_eq!(grass.surface, "grass");

    let plain = platforms.iter().find(|p| p.id == "plat_plain").unwrap();
    assert_eq!(plain.surface, "default");
}
```

- [ ] **Step 3: Run tests to verify**

Run: `cd src-tauri && cargo test street::parser`
Expected: All parser tests pass. The existing `SAMPLE_STREET_XML` tests will pass because platforms without `<str id="surface">` correctly default to `"default"`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/street/parser.rs
git commit -m "feat(street): parse surface field from platform XML"
```

---

### Task 3: Add `Footstep` variant to AudioEvent

**Files:**
- Modify: `src-tauri/src/engine/audio.rs:7-29`

- [ ] **Step 1: Add Footstep variant**

In `src-tauri/src/engine/audio.rs`, add the new variant inside the `AudioEvent` enum, after `StreetChanged`:

```rust
#[serde(rename_all = "camelCase")]
Footstep {
    surface: String,
},
```

- [ ] **Step 2: Add serialization test**

Add in the `mod tests` block:

```rust
#[test]
fn serialize_footstep() {
    let event = AudioEvent::Footstep {
        surface: "grass".into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains(r#""type":"footstep""#));
    assert!(json.contains(r#""surface":"grass""#));
}
```

- [ ] **Step 3: Add Footstep to roundtrip test**

In the `roundtrip_all_variants` test, add to the `events` vec:

```rust
AudioEvent::Footstep {
    surface: "stone".into(),
},
```

- [ ] **Step 4: Run tests to verify**

Run: `cd src-tauri && cargo test engine::audio`
Expected: All tests pass including new serialization test and updated roundtrip.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/engine/audio.rs
git commit -m "feat(audio): add Footstep variant to AudioEvent"
```

---

### Task 4: Add distance accumulator to PhysicsBody

**Files:**
- Modify: `src-tauri/src/physics/movement.rs:18-57`

- [ ] **Step 1: Add field and constant**

In `src-tauri/src/physics/movement.rs`, add the stride constant after `SLOPE_SNAP_TOLERANCE`:

```rust
/// Horizontal distance (px) the player must travel on ground before a footstep event fires.
/// At WALK_SPEED (200 px/s), this gives ~5 footsteps per second.
pub const FOOTSTEP_STRIDE: f64 = 40.0;
```

Add the field to `PhysicsBody` (after `prev_jump`):

```rust
/// Accumulated horizontal distance on ground since last footstep event.
pub distance_since_footstep: f64,
```

Initialize it to `0.0` in `PhysicsBody::new()`:

```rust
distance_since_footstep: 0.0,
```

- [ ] **Step 2: Add accumulation and reset logic in tick()**

In `PhysicsBody::tick()`, after `self.prev_jump = input.jump;` (line 235) and before the closing `}` of `tick()`, add:

```rust
// Footstep distance accumulator
if self.on_ground && self.vx.abs() > 0.1 {
    self.distance_since_footstep += (self.x - prev_x).abs();
} else {
    self.distance_since_footstep = 0.0;
}
```

Note: the accumulator only accumulates — it doesn't emit events. The `GameState::tick()` method (Task 5) reads and resets it when emitting footstep events.

- [ ] **Step 3: Run physics tests to verify**

Run: `cd src-tauri && cargo test physics::movement`
Expected: All existing tests pass (the new field doesn't affect existing behavior).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/physics/movement.rs
git commit -m "feat(physics): add distance_since_footstep accumulator"
```

---

### Task 5: Emit footstep events in GameState::tick()

**Files:**
- Modify: `src-tauri/src/engine/state.rs:351-370`

- [ ] **Step 1: Add surface lookup helper**

In `src-tauri/src/engine/state.rs`, add a free function before the `impl GameState` block (or after it — outside the impl). This finds the surface of the platform the player is standing on:

```rust
/// Find the surface material of the platform under the player.
/// Returns "default" if on ground_y or no platform matches.
fn surface_at(x: f64, y: f64, platforms: &[crate::street::types::PlatformLine]) -> &str {
    for platform in platforms {
        if !platform.solid_from_top() {
            continue;
        }
        if x < platform.min_x() || x > platform.max_x() {
            continue;
        }
        if (platform.y_at(x) - y).abs() < 1.0 {
            return &platform.surface;
        }
    }
    "default"
}
```

The 1.0 pixel tolerance handles floating-point rounding after slope snapping.

- [ ] **Step 2: Add footstep emission logic in tick()**

In `GameState::tick()`, after the Jump/Land audio detection block (after `self.prev_on_ground = self.player.on_ground;` at line 370), add:

```rust
// Footstep audio — emit when stride distance reached
if self.player.on_ground
    && self.player.distance_since_footstep >= crate::physics::movement::FOOTSTEP_STRIDE
{
    let surface = surface_at(
        self.player.x,
        self.player.y,
        street.platforms(),
    );
    audio_events.push(AudioEvent::Footstep {
        surface: surface.to_string(),
    });
    self.player.distance_since_footstep -= crate::physics::movement::FOOTSTEP_STRIDE;
}
```

Note: `street` is already borrowed immutably at line 351 (`let street = self.street.as_ref().unwrap();`). The subtraction (not reset) preserves remainder for smooth cadence.

- [ ] **Step 3: Add test for surface_at helper**

In `src-tauri/src/engine/state.rs` tests (or add a `#[cfg(test)] mod tests` block if one doesn't exist — check where existing tests live), add:

```rust
#[test]
fn surface_at_finds_matching_platform() {
    use crate::street::types::{PlatformLine, Point};

    let platforms = vec![
        PlatformLine {
            id: "grass_plat".into(),
            start: Point { x: -200.0, y: 0.0 },
            end: Point { x: 0.0, y: 0.0 },
            pc_perm: None,
            item_perm: None,
            surface: "grass".into(),
        },
        PlatformLine {
            id: "stone_plat".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 200.0, y: 0.0 },
            pc_perm: None,
            item_perm: None,
            surface: "stone".into(),
        },
    ];

    assert_eq!(surface_at(-100.0, 0.0, &platforms), "grass");
    assert_eq!(surface_at(100.0, 0.0, &platforms), "stone");
    // Off all platforms — returns default
    assert_eq!(surface_at(500.0, 0.0, &platforms), "default");
}

#[test]
fn surface_at_returns_default_for_no_platforms() {
    assert_eq!(surface_at(0.0, 0.0, &[]), "default");
}
```

- [ ] **Step 4: Add test for footstep emission cadence**

This tests that the GameState emits a Footstep event after walking the stride distance. You'll need to construct a minimal GameState with a street and drive tick() calls. Use the existing `GameState::new()` constructor and `load_street()` pattern from the codebase. If there are existing integration-style tests in state.rs, follow the same pattern.

```rust
#[test]
fn footstep_emits_after_stride_distance() {
    use crate::physics::movement::{InputState, FOOTSTEP_STRIDE, WALK_SPEED};

    let mut state = GameState::new();
    // Load a minimal street with a flat platform
    let street = crate::street::types::StreetData {
        tsid: "LTEST".into(),
        name: "Test".into(),
        left: -500.0,
        right: 500.0,
        top: -200.0,
        bottom: 0.0,
        ground_y: 0.0,
        gradient: None,
        layers: vec![crate::street::types::Layer {
            name: "middleground".into(),
            z: 0,
            w: 1000.0,
            h: 200.0,
            is_middleground: true,
            decos: vec![],
            platform_lines: vec![crate::street::types::PlatformLine {
                id: "plat".into(),
                start: crate::street::types::Point { x: -500.0, y: 0.0 },
                end: crate::street::types::Point { x: 500.0, y: 0.0 },
                pc_perm: Some(-1),
                item_perm: None,
                surface: "grass".into(),
            }],
            walls: vec![],
            ladders: vec![],
            filters: None,
        }],
        signposts: vec![],
    };
    state.street = Some(street);
    state.player = crate::physics::movement::PhysicsBody::new(0.0, 0.0);
    state.player.on_ground = true;
    state.prev_on_ground = true;

    let dt = 1.0 / 60.0;
    let walking_right = InputState {
        left: false,
        right: true,
        jump: false,
        interact: false,
    };
    let mut rng = rand::thread_rng();

    // Walk until we should have passed the stride distance
    let ticks_for_stride = (FOOTSTEP_STRIDE / (WALK_SPEED * dt)).ceil() as usize + 1;
    let mut footstep_count = 0;
    for _ in 0..ticks_for_stride {
        if let Some(frame) = state.tick(dt, &walking_right, &mut rng) {
            footstep_count += frame
                .audio_events
                .iter()
                .filter(|e| matches!(e, AudioEvent::Footstep { .. }))
                .count();
        }
    }
    assert!(footstep_count >= 1, "Expected at least 1 footstep event after walking {ticks_for_stride} ticks");

    // Check surface is correct
    let last_frame = state.tick(dt, &walking_right, &mut rng);
    // Keep walking to get another footstep
    let mut found_surface = None;
    for _ in 0..(ticks_for_stride * 2) {
        if let Some(frame) = state.tick(dt, &walking_right, &mut rng) {
            for event in &frame.audio_events {
                if let AudioEvent::Footstep { surface } = event {
                    found_surface = Some(surface.clone());
                }
            }
        }
    }
    assert_eq!(found_surface, Some("grass".into()));
}

#[test]
fn no_footstep_while_airborne() {
    use crate::physics::movement::InputState;

    let mut state = GameState::new();
    let street = crate::street::types::StreetData {
        tsid: "LTEST".into(),
        name: "Test".into(),
        left: -500.0,
        right: 500.0,
        top: -200.0,
        bottom: 0.0,
        ground_y: 0.0,
        gradient: None,
        layers: vec![crate::street::types::Layer {
            name: "middleground".into(),
            z: 0,
            w: 1000.0,
            h: 200.0,
            is_middleground: true,
            decos: vec![],
            platform_lines: vec![crate::street::types::PlatformLine {
                id: "plat".into(),
                start: crate::street::types::Point { x: -500.0, y: 0.0 },
                end: crate::street::types::Point { x: 500.0, y: 0.0 },
                pc_perm: Some(-1),
                item_perm: None,
                surface: "stone".into(),
            }],
            walls: vec![],
            ladders: vec![],
            filters: None,
        }],
        signposts: vec![],
    };
    state.street = Some(street);
    state.player = crate::physics::movement::PhysicsBody::new(0.0, 0.0);
    // Player is airborne
    state.player.on_ground = false;
    state.prev_on_ground = false;
    state.player.vy = 100.0; // falling

    let dt = 1.0 / 60.0;
    let walking_right = InputState {
        left: false,
        right: true,
        jump: false,
        interact: false,
    };
    let mut rng = rand::thread_rng();

    // Tick many times while falling
    let mut footstep_count = 0;
    for _ in 0..60 {
        if let Some(frame) = state.tick(dt, &walking_right, &mut rng) {
            footstep_count += frame
                .audio_events
                .iter()
                .filter(|e| matches!(e, AudioEvent::Footstep { .. }))
                .count();
        }
    }
    assert_eq!(footstep_count, 0, "No footsteps should emit while airborne");
}
```

- [ ] **Step 5: Run all Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass. Clippy clean: `cargo clippy`

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat(audio): emit footstep events with surface lookup in game tick"
```

---

### Task 6: Frontend — add `footstep` to AudioEvent type and processEvents

**Files:**
- Modify: `src/lib/types.ts:252-261`
- Modify: `src/lib/engine/audio.ts:90-124`
- Modify: `src/lib/engine/audio.test.ts`

- [ ] **Step 1: Add footstep to AudioEvent union type**

In `src/lib/types.ts`, add to the `AudioEvent` type union (after the `streetChanged` line):

```typescript
| { type: 'footstep'; surface: string };
```

- [ ] **Step 2: Add footstep case in processEvents**

In `src/lib/engine/audio.ts`, inside `processEvents()`, add a case in the switch statement (after the `streetChanged` case):

```typescript
case 'footstep':
  this.playSfx('footstep', event.surface);
  break;
```

- [ ] **Step 3: Add footstep entry to test kit**

In `src/lib/engine/audio.test.ts`, update the `makeKit()` function to include the footstep event. Add to the `events` object:

```typescript
footstep: {
  default: 'sfx/footstep-default.mp3',
  variants: { grass: 'sfx/footstep-grass.mp3', stone: 'sfx/footstep-stone.mp3' },
},
```

- [ ] **Step 4: Add test for footstep event processing**

In `src/lib/engine/audio.test.ts`, inside the `describe('AudioManager')` block, add:

```typescript
it('plays footstep with surface variant', () => {
  const manager = new AudioManager(makeKit(), '/audio/');
  manager.processEvents([{ type: 'footstep', surface: 'grass' }]);

  const grassHowl = findHowlBySrc('footstep-grass');
  expect(grassHowl).toBeDefined();
  expect(grassHowl!.play).toHaveBeenCalled();
});

it('falls back to default footstep for unknown surface', () => {
  const manager = new AudioManager(makeKit(), '/audio/');
  manager.processEvents([{ type: 'footstep', surface: 'marble' }]);

  const defaultHowl = findHowlBySrc('footstep-default');
  expect(defaultHowl).toBeDefined();
  expect(defaultHowl!.play).toHaveBeenCalled();
});
```

- [ ] **Step 5: Run frontend tests**

Run: `npx vitest run`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/lib/types.ts src/lib/engine/audio.ts src/lib/engine/audio.test.ts
git commit -m "feat(audio): handle footstep events in frontend AudioManager"
```

---

### Task 7: Add footstep entry to default sound kit + placeholder audio

**Files:**
- Modify: `assets/audio/default-kit.json`
- Create: `assets/audio/sfx/footstep-default.mp3`
- Create: `assets/audio/sfx/footstep-grass.mp3`
- Create: `assets/audio/sfx/footstep-stone.mp3`
- Create: `assets/audio/sfx/footstep-wood.mp3`

- [ ] **Step 1: Add footstep entry to default-kit.json**

In `assets/audio/default-kit.json`, add to the `events` object (after `entityInteract`):

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

- [ ] **Step 2: Create placeholder audio files**

Generate 4 minimal valid MP3 files as placeholders. Use `ffmpeg` to create short silent files (these will be replaced with real sounds later):

```bash
ffmpeg -f lavfi -i "sine=frequency=200:duration=0.15" -b:a 32k assets/audio/sfx/footstep-default.mp3
ffmpeg -f lavfi -i "sine=frequency=250:duration=0.15" -b:a 32k assets/audio/sfx/footstep-grass.mp3
ffmpeg -f lavfi -i "sine=frequency=300:duration=0.15" -b:a 32k assets/audio/sfx/footstep-stone.mp3
ffmpeg -f lavfi -i "sine=frequency=350:duration=0.15" -b:a 32k assets/audio/sfx/footstep-wood.mp3
```

Each has a slightly different pitch so you can tell them apart during testing. They'll be ~2KB each.

If `ffmpeg` is not available, create minimal valid MP3 files another way (e.g. copy an existing short MP3 and rename). The audio system handles missing files gracefully, so even empty placeholders work for development.

- [ ] **Step 3: Verify kit JSON is valid**

Run: `python3 -c "import json; json.load(open('assets/audio/default-kit.json'))"`
Expected: No error.

- [ ] **Step 4: Commit**

```bash
git add assets/audio/default-kit.json assets/audio/sfx/footstep-*.mp3
git commit -m "feat(audio): add footstep sounds to default kit with placeholder audio"
```

---

### Task 8: Add surface tags to demo street XML files

**Files:**
- Modify: `assets/streets/demo_meadow.xml`
- Modify: `assets/streets/demo_heights.xml`

- [ ] **Step 1: Add surfaces to demo_meadow.xml platforms**

Open `assets/streets/demo_meadow.xml` and add `<str id="surface">` elements to platform objects. The main ground platform should be `grass`, any elevated/floating platforms can be `wood` or `stone`:

For the main ground platform, add inside its `<object>` (alongside `start`, `end`, etc.):
```xml
<str id="surface">grass</str>
```

For any floating/elevated platforms:
```xml
<str id="surface">wood</str>
```

- [ ] **Step 2: Add surfaces to demo_heights.xml platforms**

Open `assets/streets/demo_heights.xml` and add surface tags. Use `stone` for ground-level platforms and `wood` for floating/elevated ones. Follow the same pattern as Step 1.

- [ ] **Step 3: Verify parsing still works**

Run: `cd src-tauri && cargo test street::parser::tests::parses_demo_heights -- --exact`
Run: `cd src-tauri && cargo test street::parser::tests::demo_meadow_has_signpost_to_heights -- --exact`
Expected: Both pass (existing tests validate structure, and `surface` defaults gracefully).

- [ ] **Step 4: Commit**

```bash
git add assets/streets/demo_meadow.xml assets/streets/demo_heights.xml
git commit -m "feat(street): add surface tags to demo street platforms"
```

---

### Task 9: Update TypeScript PlatformLine type

**Files:**
- Modify: `src/lib/types.ts:34-40`

- [ ] **Step 1: Add surface to PlatformLine interface**

In `src/lib/types.ts`, add the `surface` field to `PlatformLine`:

```typescript
export interface PlatformLine {
  id: string;
  start: Point;
  end: Point;
  pcPerm: number | null;
  itemPerm: number | null;
  surface: string;
}
```

This keeps the TypeScript type in sync with the Rust struct. The frontend doesn't use this field directly (footstep logic is all in Rust), but the types should match for correctness.

- [ ] **Step 2: Run frontend tests**

Run: `npx vitest run`
Expected: All tests pass.

- [ ] **Step 3: Run all tests (Rust + frontend)**

Run: `cd src-tauri && cargo test`
Run: `cd /home/zeblith/work/zeblithic/harmony-glitch && npx vitest run`
Run: `cd src-tauri && cargo clippy`
Expected: All pass, clippy clean.

- [ ] **Step 4: Commit**

```bash
git add src/lib/types.ts
git commit -m "feat(types): add surface field to TypeScript PlatformLine"
```
