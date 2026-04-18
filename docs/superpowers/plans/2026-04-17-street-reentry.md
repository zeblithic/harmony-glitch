# Street Re-entry & Fall-Into-Void Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make streets closed containers with predictable re-entry — physics clamps `player.y` to `street.bottom`, signpost connections own their arrival location, and a safety-net teleport recovers players stuck on the invisible floor.

**Architecture:** Three tightly-coupled changes sharing one new runtime concept (`last_arrival: Point`): a Y-clamp in `PhysicsBody::tick`, a new `engine::arrival` module with `resolve_arrival` / `resolve_default_spawn`, and an OOB detector in `GameState::tick` that teleports to `last_arrival` after 30 consecutive stuck-on-floor ticks. Additive `Option<…>` fields on `SignpostConnection` and `StreetData` keep legacy XMLs parsing unchanged.

**Tech Stack:** Rust (Tauri), `src-tauri/src/{physics,engine,street}/`. Tests via `cargo test --manifest-path src-tauri/Cargo.toml`.

**Spec:** `docs/superpowers/specs/2026-04-17-street-reentry-design.md`

---

## File Structure

**Created:**
- `src-tauri/src/engine/arrival.rs` — pure functions `resolve_arrival` and `resolve_default_spawn`. Kept separate from `state.rs` (which is already 1500+ lines) to keep the logic isolated and easy to unit-test without a full `GameState` fixture.

**Modified:**
- `src-tauri/src/street/types.rs` — add `Facing` enum, `SpawnPoint` struct, optional fields on `SignpostConnection` and `StreetData`.
- `src-tauri/src/street/parser.rs` — read new XML attributes/elements with defaults.
- `src-tauri/src/physics/movement.rs` — add `street_bottom: f64` parameter to `tick`, add Y-clamp.
- `src-tauri/src/engine/mod.rs` — register the new `arrival` module.
- `src-tauri/src/engine/state.rs` — add `last_arrival`, `oob_ticks`, `pending_arrival` fields; rewrite the signpost-crossing capture and transition-completion blocks; add OOB detector after the physics tick; wire `resolve_default_spawn` into `load_street`.
- `assets/streets/demo_meadow.xml`, `assets/streets/demo_heights.xml` — add `<default_spawn>` element and arrival fields on each connection (using the real Glitch `<object>` / `<int>` / `<str>` / `<objref>` XML dialect, not the illustrative shorthand in the spec).

---

## Task 1: Add `Facing` enum and `SpawnPoint` struct

**Files:**
- Modify: `src-tauri/src/street/types.rs`
- Test: `src-tauri/src/street/types.rs` (tests module at bottom, or create one)

- [ ] **Step 1: Write the failing tests**

Add to `src-tauri/src/street/types.rs` (at the bottom — create a `#[cfg(test)] mod tests {}` block if one doesn't exist there):

```rust
#[cfg(test)]
mod types_tests {
    use super::*;

    #[test]
    fn facing_serializes_lowercase() {
        let json = serde_json::to_string(&Facing::Left).unwrap();
        assert_eq!(json, "\"left\"");
        let json = serde_json::to_string(&Facing::Right).unwrap();
        assert_eq!(json, "\"right\"");
    }

    #[test]
    fn spawn_point_roundtrips_through_serde() {
        let sp = SpawnPoint { x: 123.5, y: -4.0, facing: Some(Facing::Right) };
        let json = serde_json::to_string(&sp).unwrap();
        let back: SpawnPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back.x, 123.5);
        assert_eq!(back.y, -4.0);
        assert_eq!(back.facing, Some(Facing::Right));
    }

    #[test]
    fn spawn_point_roundtrips_without_facing() {
        let sp = SpawnPoint { x: 0.0, y: 0.0, facing: None };
        let json = serde_json::to_string(&sp).unwrap();
        let back: SpawnPoint = serde_json::from_str(&json).unwrap();
        assert!(back.facing.is_none());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib street::types`
Expected: FAIL — `cannot find type 'Facing'`, `cannot find struct 'SpawnPoint'`.

- [ ] **Step 3: Add the types**

Add to `src-tauri/src/street/types.rs` near the existing `Point` struct (around line 72, after `Point`):

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Facing {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SpawnPoint {
    pub x: f64,
    pub y: f64,
    pub facing: Option<Facing>,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib street::types`
Expected: PASS — all three tests green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/street/types.rs
git commit -m "feat(types): add Facing enum and SpawnPoint struct

Prerequisite for ZEB-132 — used by signpost arrival data and
StreetData.default_spawn."
```

---

## Task 2: Add optional arrival fields to `SignpostConnection` + parser

**Files:**
- Modify: `src-tauri/src/street/types.rs` (extend `SignpostConnection`)
- Modify: `src-tauri/src/street/parser.rs` (`parse_signposts` function, ~lines 289-320)
- Test: `src-tauri/src/street/parser.rs` tests module

- [ ] **Step 1: Write the failing test**

Add to the tests module at the bottom of `src-tauri/src/street/parser.rs`:

```rust
#[test]
fn parse_signpost_connection_with_arrival_fields() {
    let xml = r#"<?xml version="1.0"?>
<object id="dynamic">
  <str id="label">TestStreet</str>
  <str id="tsid">LATEST001</str>
  <int id="l">-1000</int>
  <int id="r">1000</int>
  <int id="t">-500</int>
  <int id="b">0</int>
  <object id="signposts">
    <object id="sign_a">
      <int id="x">500</int>
      <int id="y">0</int>
      <object id="connects">
        <object id="connect_1">
          <objref id="target" tsid="OTHERTSID" label="To Other" />
          <int id="arrival_x">-800</int>
          <int id="arrival_y">0</int>
          <str id="arrival_facing">right</str>
        </object>
      </object>
    </object>
  </object>
</object>"#;
    let street = parse_street(xml).unwrap();
    assert_eq!(street.signposts.len(), 1);
    let conn = &street.signposts[0].connects[0];
    assert_eq!(conn.target_tsid, "OTHERTSID");
    assert_eq!(conn.arrival_x, Some(-800.0));
    assert_eq!(conn.arrival_y, Some(0.0));
    assert_eq!(conn.arrival_facing, Some(crate::street::types::Facing::Right));
}

#[test]
fn parse_signpost_connection_without_arrival_fields() {
    let xml = r#"<?xml version="1.0"?>
<object id="dynamic">
  <str id="label">TestStreet</str>
  <str id="tsid">LATEST001</str>
  <int id="l">-1000</int>
  <int id="r">1000</int>
  <int id="t">-500</int>
  <int id="b">0</int>
  <object id="signposts">
    <object id="sign_a">
      <int id="x">500</int>
      <int id="y">0</int>
      <object id="connects">
        <object id="connect_1">
          <objref id="target" tsid="OTHERTSID" label="To Other" />
        </object>
      </object>
    </object>
  </object>
</object>"#;
    let street = parse_street(xml).unwrap();
    let conn = &street.signposts[0].connects[0];
    assert!(conn.arrival_x.is_none());
    assert!(conn.arrival_y.is_none());
    assert!(conn.arrival_facing.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib parse_signpost_connection`
Expected: FAIL — compile errors for `conn.arrival_x`, `conn.arrival_y`, `conn.arrival_facing` (fields don't exist yet).

- [ ] **Step 3: Extend `SignpostConnection`**

In `src-tauri/src/street/types.rs`, replace the existing `SignpostConnection` struct (~lines 155-160):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignpostConnection {
    pub target_tsid: String,
    pub target_label: String,
    /// Where on the target street the player arrives when using this
    /// connection. If None, runtime falls back to the target signpost's
    /// own (x, y) — preserves legacy behavior for imported Glitch XMLs.
    #[serde(default)]
    pub arrival_x: Option<f64>,
    #[serde(default)]
    pub arrival_y: Option<f64>,
    /// Player facing on arrival. If None, inferred from which half of the
    /// target street arrival_x sits in.
    #[serde(default)]
    pub arrival_facing: Option<Facing>,
}
```

- [ ] **Step 4: Update the parser**

In `src-tauri/src/street/parser.rs`, replace the `SignpostConnection` construction inside `parse_signposts` (~lines 299-308). Find the block:

```rust
.filter_map(|c| match c.get("target") {
    Some(XmlValue::ObjRef { tsid, label }) => Some(SignpostConnection {
        target_tsid: tsid.clone(),
        target_label: label.clone(),
    }),
    _ => None,
})
```

Replace with:

```rust
.filter_map(|c| match c.get("target") {
    Some(XmlValue::ObjRef { tsid, label }) => Some(SignpostConnection {
        target_tsid: tsid.clone(),
        target_label: label.clone(),
        arrival_x: c.get("arrival_x").and_then(|v| v.as_f64()),
        arrival_y: c.get("arrival_y").and_then(|v| v.as_f64()),
        arrival_facing: c
            .get("arrival_facing")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "left" => Some(crate::street::types::Facing::Left),
                "right" => Some(crate::street::types::Facing::Right),
                _ => None,
            }),
    }),
    _ => None,
})
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib parse_signpost_connection`
Expected: PASS — both tests green.

Also run all parser tests to check nothing regressed:
Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib street::parser`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/street/types.rs src-tauri/src/street/parser.rs
git commit -m "feat(street): add optional arrival fields to SignpostConnection

arrival_x, arrival_y, arrival_facing on <connect> let the origin signpost
specify where on the target street the player lands. Absent fields preserve
legacy .find()-on-reciprocal-signpost behavior."
```

---

## Task 3: Add optional `default_spawn` to `StreetData` + parser

**Files:**
- Modify: `src-tauri/src/street/types.rs` (extend `StreetData`)
- Modify: `src-tauri/src/street/parser.rs` (`parse_street` function)
- Test: parser tests module

- [ ] **Step 1: Write the failing tests**

Add to the tests module at the bottom of `src-tauri/src/street/parser.rs`:

```rust
#[test]
fn parse_street_with_default_spawn() {
    let xml = r#"<?xml version="1.0"?>
<object id="dynamic">
  <str id="label">TestStreet</str>
  <str id="tsid">LATEST001</str>
  <int id="l">-1000</int>
  <int id="r">1000</int>
  <int id="t">-500</int>
  <int id="b">0</int>
  <object id="default_spawn">
    <int id="x">250</int>
    <int id="y">-50</int>
    <str id="facing">left</str>
  </object>
</object>"#;
    let street = parse_street(xml).unwrap();
    let sp = street.default_spawn.expect("default_spawn should parse");
    assert_eq!(sp.x, 250.0);
    assert_eq!(sp.y, -50.0);
    assert_eq!(sp.facing, Some(crate::street::types::Facing::Left));
}

#[test]
fn parse_street_without_default_spawn() {
    let xml = r#"<?xml version="1.0"?>
<object id="dynamic">
  <str id="label">TestStreet</str>
  <str id="tsid">LATEST001</str>
  <int id="l">-1000</int>
  <int id="r">1000</int>
  <int id="t">-500</int>
  <int id="b">0</int>
</object>"#;
    let street = parse_street(xml).unwrap();
    assert!(street.default_spawn.is_none());
}

#[test]
fn parse_street_default_spawn_without_facing() {
    let xml = r#"<?xml version="1.0"?>
<object id="dynamic">
  <str id="label">TestStreet</str>
  <str id="tsid">LATEST001</str>
  <int id="l">-1000</int>
  <int id="r">1000</int>
  <int id="t">-500</int>
  <int id="b">0</int>
  <object id="default_spawn">
    <int id="x">0</int>
    <int id="y">0</int>
  </object>
</object>"#;
    let street = parse_street(xml).unwrap();
    let sp = street.default_spawn.unwrap();
    assert!(sp.facing.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib parse_street_with_default_spawn parse_street_without_default_spawn parse_street_default_spawn_without_facing`
Expected: FAIL — compile error on `street.default_spawn` (field doesn't exist).

- [ ] **Step 3: Extend `StreetData`**

In `src-tauri/src/street/types.rs`, add the `default_spawn` field to the `StreetData` struct (insert after the `signposts` field at line 24):

```rust
pub struct StreetData {
    pub tsid: String,
    pub name: String,
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub ground_y: f64,
    pub gradient: Option<Gradient>,
    pub layers: Vec<Layer>,
    pub signposts: Vec<Signpost>,
    /// Where the player spawns on first entry (before any signpost traversal)
    /// and the last-resort fallback for resolve_arrival. If None, runtime
    /// resolves to first signpost position or (center_x, ground_y).
    #[serde(default)]
    pub default_spawn: Option<SpawnPoint>,
}
```

Also update every existing `StreetData { ... }` construction in this file (search for `StreetData {` inside `types.rs` — around lines 310, 352, 521 based on earlier grep) to include `default_spawn: None,`.

Run `cargo check --manifest-path src-tauri/Cargo.toml` after this edit — the compiler will list every other missed construction site across the codebase. Add `default_spawn: None,` to each.

- [ ] **Step 4: Update the parser**

In `src-tauri/src/street/parser.rs`, add a helper above `parse_street`:

```rust
fn parse_default_spawn(dynamic: &XmlValue) -> Option<SpawnPoint> {
    let obj = dynamic.get("default_spawn").and_then(|v| v.as_object())?;
    let x = obj.get("x").and_then(|v| v.as_f64())?;
    let y = obj.get("y").and_then(|v| v.as_f64())?;
    let facing = obj
        .get("facing")
        .and_then(|v| v.as_str())
        .and_then(|s| match s {
            "left" => Some(crate::street::types::Facing::Left),
            "right" => Some(crate::street::types::Facing::Right),
            _ => None,
        });
    Some(SpawnPoint { x, y, facing })
}
```

Add the import at the top of `parser.rs` if not already present:

```rust
use crate::street::types::SpawnPoint;
```

Then inside `parse_street`, before the final `Ok(StreetData { ... })`, add:

```rust
let default_spawn = parse_default_spawn(dynamic);
```

And add `default_spawn,` to the returned struct literal.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib street::parser`
Expected: all PASS (including the 3 new default_spawn tests).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/street/types.rs src-tauri/src/street/parser.rs
git commit -m "feat(street): add optional default_spawn to StreetData

<default_spawn x y facing/> at the street root specifies where the player
lands on first entry before any signpost traversal. Absent field is resolved
at runtime to first-signpost-position or street-center."
```

---

## Task 4: Add Y-clamp to `PhysicsBody::tick`

**Files:**
- Modify: `src-tauri/src/physics/movement.rs` (add `street_bottom` param, Y-clamp)
- Modify: `src-tauri/src/engine/state.rs:707` (only call site)
- Test: `src-tauri/src/physics/movement.rs` tests module

- [ ] **Step 1: Write the failing tests**

Add to the tests module at the bottom of `src-tauri/src/physics/movement.rs` (near existing tests like `wall_blocks_movement_from_left` at ~line 281):

```rust
#[test]
fn player_y_clamped_to_street_bottom_when_no_platform_below() {
    // Player starts mid-air above street.bottom with no platform to catch them.
    let mut body = PhysicsBody::new(0.0, -500.0);
    body.on_ground = false;
    body.vy = 0.0;
    let input = InputState::default();
    let platforms: Vec<PlatformLine> = vec![]; // no ground anywhere
    let walls: Vec<Wall> = vec![];

    // Let gravity run for 5 seconds — plenty of time to pass street.bottom.
    for _ in 0..300 {
        body.tick(
            1.0 / 60.0,
            &input,
            &platforms,
            &walls,
            -1000.0,
            1000.0,
            0.0, // street_bottom
        );
    }

    assert!(
        body.y <= 0.0 + 0.01,
        "Player should be clamped to street_bottom=0, actual y={}",
        body.y
    );
    assert!(
        body.vy.abs() < 0.01,
        "Vertical velocity should be reset to 0 after clamp, actual vy={}",
        body.vy
    );
}

#[test]
fn player_vx_preserved_when_y_clamp_engages() {
    // Player hits the floor with horizontal input — they should still move sideways.
    let mut body = PhysicsBody::new(0.0, -100.0);
    body.on_ground = false;
    let input = InputState {
        right: true,
        ..Default::default()
    };
    let platforms: Vec<PlatformLine> = vec![];
    let walls: Vec<Wall> = vec![];

    let initial_x = body.x;
    for _ in 0..60 {
        body.tick(
            1.0 / 60.0,
            &input,
            &platforms,
            &walls,
            -1000.0,
            1000.0,
            0.0,
        );
    }

    assert!(
        body.x > initial_x + 10.0,
        "Player should move right on the clamp floor, x={} (started {})",
        body.x,
        initial_x
    );
}

#[test]
fn higher_platform_wins_over_y_clamp() {
    // Player falling — a platform at y=-50 should catch them before they
    // reach street_bottom=0.
    let mut body = PhysicsBody::new(0.0, -200.0);
    body.on_ground = false;
    body.vy = 0.0;
    let input = InputState::default();
    let platforms = vec![PlatformLine {
        start: Point { x: -500.0, y: -50.0 },
        end: Point { x: 500.0, y: -50.0 },
        pc_perm: None,
    }];
    let walls: Vec<Wall> = vec![];

    for _ in 0..120 {
        body.tick(
            1.0 / 60.0,
            &input,
            &platforms,
            &walls,
            -1000.0,
            1000.0,
            0.0,
        );
    }

    assert!(
        (body.y - (-50.0)).abs() < 0.5,
        "Player should land on platform at y=-50, actual y={}",
        body.y
    );
    assert!(body.on_ground, "Player should be grounded on the platform");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib player_y_clamped_to_street_bottom player_vx_preserved_when_y_clamp higher_platform_wins_over_y_clamp`
Expected: FAIL — compile error because `tick` doesn't take 7 args yet.

- [ ] **Step 3: Add `street_bottom` parameter and Y-clamp**

In `src-tauri/src/physics/movement.rs`, update the `tick` signature (lines 68-76):

```rust
pub fn tick(
    &mut self,
    dt: f64,
    input: &InputState,
    platforms: &[PlatformLine],
    walls: &[Wall],
    street_left: f64,
    street_right: f64,
    street_bottom: f64,
)
```

At the END of the `tick` function body (after Phase 3 ceiling handling, i.e., after all existing collision work is complete), add:

```rust
// Hard floor: street.bottom is the absolute lowest Y any entity can occupy.
// Ground geometry should cover the play area, but this clamp prevents the
// infinite-fall bug if a player walks off an un-platformed gap. vx is
// deliberately preserved so the player can walk off the invisible floor
// back onto a real platform.
if self.y > street_bottom {
    self.y = street_bottom;
    self.vy = 0.0;
}
```

- [ ] **Step 4: Update the sole call site in `state.rs`**

In `src-tauri/src/engine/state.rs:707`, replace:

```rust
self.player.tick(
    dt,
    input,
    street.platforms(),
    street.walls(),
    street.left,
    street.right,
);
```

with:

```rust
self.player.tick(
    dt,
    input,
    street.platforms(),
    street.walls(),
    street.left,
    street.right,
    street.bottom,
);
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib movement`
Expected: all PASS (3 new tests + existing tests still green).

Run full test suite to catch any other broken call sites:
Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/physics/movement.rs src-tauri/src/engine/state.rs
git commit -m "feat(physics): clamp player.y to street.bottom

Closes the infinite-fall bug at the physics layer. If a player walks off
an un-platformed gap, they come to rest at street.bottom with vy=0 and
horizontal control intact, instead of accumulating downward velocity forever.
Higher platforms still win via the existing Phase 1/2 collision resolution."
```

---

## Task 5: Add `engine::arrival` module with `resolve_arrival` and `resolve_default_spawn`

**Files:**
- Create: `src-tauri/src/engine/arrival.rs`
- Modify: `src-tauri/src/engine/mod.rs` (register the module)
- Test: `src-tauri/src/engine/arrival.rs` tests module

- [ ] **Step 1: Create `arrival.rs` with failing tests first**

Create `src-tauri/src/engine/arrival.rs`:

```rust
use crate::street::types::{Facing, SignpostConnection, SpawnPoint, StreetData};

/// Resolve the point at which a player should land on `street` when arriving
/// via `connection`. Prefers the connection's explicit arrival_x/arrival_y;
/// falls back to the reciprocal signpost on the target street (legacy
/// Glitch-XML path); last-resorts to the street's default_spawn.
pub fn resolve_arrival(
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

/// Resolve the player's spawn location on `street` when there is no
/// origin signpost context — first entry, initial load, or final fallback
/// for resolve_arrival.
pub fn resolve_default_spawn(street: &StreetData) -> SpawnPoint {
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

/// Infer player facing from an arrival x-coordinate when the SpawnPoint
/// doesn't specify one. Faces inward from the nearer street edge.
pub fn infer_facing(arrival_x: f64, street: &StreetData) -> Facing {
    let street_mid = (street.left + street.right) / 2.0;
    if arrival_x < street_mid { Facing::Right } else { Facing::Left }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::types::{Gradient, Layer, Point, Signpost};

    fn test_street(signposts: Vec<Signpost>, default_spawn: Option<SpawnPoint>) -> StreetData {
        StreetData {
            tsid: "LATEST001".to_string(),
            name: "Test Street".to_string(),
            left: -1000.0,
            right: 1000.0,
            top: -500.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![],
            signposts,
            default_spawn,
        }
    }

    fn connection(
        target_tsid: &str,
        arrival_x: Option<f64>,
        arrival_y: Option<f64>,
        arrival_facing: Option<Facing>,
    ) -> SignpostConnection {
        SignpostConnection {
            target_tsid: target_tsid.to_string(),
            target_label: "Label".to_string(),
            arrival_x,
            arrival_y,
            arrival_facing,
        }
    }

    #[test]
    fn resolve_arrival_uses_explicit_connection_fields() {
        let street = test_street(vec![], None);
        let conn = connection("ORIGIN", Some(-750.0), Some(-25.0), Some(Facing::Right));
        let sp = resolve_arrival(&street, "ORIGIN", Some(&conn));
        assert_eq!(sp.x, -750.0);
        assert_eq!(sp.y, -25.0);
        assert_eq!(sp.facing, Some(Facing::Right));
    }

    #[test]
    fn resolve_arrival_falls_back_to_reciprocal_signpost() {
        // Connection has no arrival fields; street has a signpost connecting back to ORIGIN.
        let reciprocal = Signpost {
            id: "back_gate".to_string(),
            x: 800.0,
            y: 0.0,
            connects: vec![connection("ORIGIN", None, None, None)],
        };
        let street = test_street(vec![reciprocal], None);
        let conn = connection("TARGETTSID", None, None, None);
        let sp = resolve_arrival(&street, "ORIGIN", Some(&conn));
        assert_eq!(sp.x, 800.0);
        assert_eq!(sp.y, 0.0); // ground_y
    }

    #[test]
    fn resolve_arrival_last_resorts_to_default_spawn() {
        // No connection, no reciprocal signpost — falls to default_spawn.
        let street = test_street(
            vec![],
            Some(SpawnPoint { x: 42.0, y: -10.0, facing: Some(Facing::Left) }),
        );
        let sp = resolve_arrival(&street, "ORIGIN", None);
        assert_eq!(sp.x, 42.0);
        assert_eq!(sp.y, -10.0);
        assert_eq!(sp.facing, Some(Facing::Left));
    }

    #[test]
    fn resolve_default_spawn_prefers_explicit() {
        let street = test_street(
            vec![],
            Some(SpawnPoint { x: 7.0, y: 0.0, facing: None }),
        );
        let sp = resolve_default_spawn(&street);
        assert_eq!(sp.x, 7.0);
    }

    #[test]
    fn resolve_default_spawn_falls_back_to_first_signpost() {
        let first = Signpost {
            id: "first".to_string(),
            x: -600.0,
            y: 0.0,
            connects: vec![],
        };
        let second = Signpost {
            id: "second".to_string(),
            x: 600.0,
            y: 0.0,
            connects: vec![],
        };
        let street = test_street(vec![first, second], None);
        let sp = resolve_default_spawn(&street);
        assert_eq!(sp.x, -600.0);
        assert_eq!(sp.y, 0.0); // ground_y
    }

    #[test]
    fn resolve_default_spawn_last_resorts_to_center() {
        let street = test_street(vec![], None);
        let sp = resolve_default_spawn(&street);
        assert_eq!(sp.x, 0.0); // (left=-1000 + right=1000) / 2
        assert_eq!(sp.y, 0.0); // ground_y
    }

    #[test]
    fn infer_facing_left_half_faces_right() {
        let street = test_street(vec![], None);
        assert_eq!(infer_facing(-500.0, &street), Facing::Right);
    }

    #[test]
    fn infer_facing_right_half_faces_left() {
        let street = test_street(vec![], None);
        assert_eq!(infer_facing(500.0, &street), Facing::Left);
    }
}
```

- [ ] **Step 2: Register the module**

In `src-tauri/src/engine/mod.rs`, add the new module. Find the existing `pub mod …;` lines at the top of the file and add:

```rust
pub mod arrival;
```

- [ ] **Step 3: Run tests to verify they fail (then pass)**

Since the module was created with the tests and the implementation together, both should pass on first run:

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib engine::arrival`
Expected: all 8 tests PASS.

If they fail, check the imports and module registration. If they pass, proceed to commit.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/engine/arrival.rs src-tauri/src/engine/mod.rs
git commit -m "feat(engine): add arrival module with resolve_arrival helpers

Pure functions that resolve where a player lands on a street, given an
optional origin signpost connection. Preference order:
  1. Explicit arrival_x/arrival_y on the connection
  2. Reciprocal signpost on the target street (legacy Glitch XMLs)
  3. street.default_spawn
  4. First signpost on the street
  5. Street center + ground_y"
```

---

## Task 6: Add `last_arrival`, `oob_ticks`, `pending_arrival` to `GameState` and wire into `load_street`

**Files:**
- Modify: `src-tauri/src/engine/state.rs` (`GameState` struct, `new`, `load_street`)
- Test: `src-tauri/src/engine/state.rs` tests module

- [ ] **Step 1: Write the failing test**

Add to the tests module at the bottom of `src-tauri/src/engine/state.rs` (near existing tests like `tick_produces_render_frame` at ~line 1497):

```rust
#[test]
fn load_street_sets_last_arrival_from_default_spawn() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
        empty_catalog(),
        empty_store_catalog(),
        empty_skill_defs(),
        HashMap::new(),
        HashMap::new(),
    );

    let mut street = test_street();
    street.default_spawn = Some(crate::street::types::SpawnPoint {
        x: 123.0,
        y: -50.0,
        facing: Some(crate::street::types::Facing::Left),
    });

    state.load_street(street, vec![], vec![]);

    assert_eq!(state.last_arrival.x, 123.0);
    assert_eq!(state.last_arrival.y, -50.0);
    assert_eq!(state.player.x, 123.0);
    assert_eq!(state.player.y, -50.0);
    assert_eq!(state.oob_ticks, 0);
    assert!(state.pending_arrival.is_none());
}

#[test]
fn load_street_without_default_spawn_uses_center() {
    // Existing behavior: fallback to street center when no default_spawn.
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
        empty_catalog(),
        empty_store_catalog(),
        empty_skill_defs(),
        HashMap::new(),
        HashMap::new(),
    );
    let street = test_street(); // test_street has default_spawn: None
    let expected_x = (street.left + street.right) / 2.0;
    let expected_y = street.ground_y;

    state.load_street(street, vec![], vec![]);

    assert_eq!(state.player.x, expected_x);
    assert_eq!(state.player.y, expected_y);
    assert_eq!(state.last_arrival.x, expected_x);
    assert_eq!(state.last_arrival.y, expected_y);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib load_street_sets_last_arrival load_street_without_default_spawn`
Expected: FAIL — compile error on `state.last_arrival`, `state.oob_ticks`, `state.pending_arrival` (fields don't exist).

- [ ] **Step 3: Add the fields to `GameState`**

In `src-tauri/src/engine/state.rs`, add three fields to the `GameState` struct (insert them near `transition_origin_tsid` at line 115):

```rust
pub transition_origin_tsid: Option<String>,
/// Last valid landing location on the current street — where the safety
/// net teleports the player when OOB is detected. Initialized from
/// resolve_default_spawn on street load, updated on every signpost
/// traversal via the active connection's arrival fields.
pub last_arrival: crate::street::types::Point,
/// Counter for consecutive ticks the player has been stuck on the
/// floor clamp with nothing to stand on. Reset when grounded or below
/// the clamp threshold; teleport fires at OOB_THRESHOLD_TICKS.
pub oob_ticks: u32,
/// Arrival SpawnPoint captured at signpost-crossing time, used by the
/// transition-completion handler to place the player. None when not
/// transitioning or when the active connection has no explicit arrival.
pub pending_arrival: Option<crate::street::types::SpawnPoint>,
```

- [ ] **Step 4: Initialize in `GameState::new`**

Find the `GameState { ... }` struct literal inside `GameState::new` (around line 270-320). Add to the initializer:

```rust
transition_origin_tsid: None,
last_arrival: crate::street::types::Point { x: 0.0, y: 0.0 },
oob_ticks: 0,
pending_arrival: None,
```

- [ ] **Step 5: Wire `resolve_default_spawn` into `load_street`**

Find `load_street` in `src-tauri/src/engine/state.rs:324`. Replace the current player-positioning block (currently lines 336-343):

```rust
let is_transitioning = matches!(
    self.transition.phase,
    TransitionPhase::Swooping { .. } | TransitionPhase::Complete { .. }
);
if !is_transitioning {
    let center_x = (street.left + street.right) / 2.0;
    self.player = PhysicsBody::new(center_x, street.ground_y);
}
self.street = Some(street);
```

with:

```rust
let is_transitioning = matches!(
    self.transition.phase,
    TransitionPhase::Swooping { .. } | TransitionPhase::Complete { .. }
);
if !is_transitioning {
    let spawn = crate::engine::arrival::resolve_default_spawn(&street);
    self.player = PhysicsBody::new(spawn.x, spawn.y);
    self.last_arrival = crate::street::types::Point { x: spawn.x, y: spawn.y };
    self.facing = match spawn.facing.unwrap_or_else(|| {
        crate::engine::arrival::infer_facing(spawn.x, &street)
    }) {
        crate::street::types::Facing::Left => crate::engine::Direction::Left,
        crate::street::types::Facing::Right => crate::engine::Direction::Right,
    };
}
self.oob_ticks = 0;
self.pending_arrival = None;
self.street = Some(street);
```

(If `engine::Direction` isn't the correct enum name for `self.facing`, check `engine/mod.rs` or search for `pub enum Direction` to confirm — adjust the match-arm namespaces accordingly.)

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib load_street_sets_last_arrival load_street_without_default_spawn`
Expected: PASS.

Run full state tests to catch regressions:
Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib engine::state`
Expected: all PASS (some existing tests may need adjustment if they hard-coded center-x spawn — update them to match the new behavior; `test_street()` has no default_spawn so center-x should still be computed).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat(engine): track last_arrival + wire default_spawn into load_street

GameState now carries last_arrival (for safety-net respawn), oob_ticks
(counter for stuck-on-floor detection), and pending_arrival (captured at
signpost-crossing, consumed at transition completion). load_street uses
resolve_default_spawn to position the player instead of hard-coded center_x."
```

---

## Task 7: Capture `pending_arrival` at signpost crossing

**Files:**
- Modify: `src-tauri/src/engine/state.rs` (signpost-crossing block ~line 626-629)
- Test: `src-tauri/src/engine/state.rs` tests module

- [ ] **Step 1: Write the failing test**

Add to the tests module:

```rust
#[test]
fn signpost_crossing_captures_pending_arrival_when_connection_has_fields() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
        empty_catalog(),
        empty_store_catalog(),
        empty_skill_defs(),
        HashMap::new(),
        HashMap::new(),
    );

    // Build a street where a signpost at x=900 connects to TARGET with
    // explicit arrival fields.
    use crate::street::types::{Facing, Signpost, SignpostConnection};
    let mut street = test_street();
    street.signposts.push(Signpost {
        id: "east".to_string(),
        x: 900.0,
        y: 0.0,
        connects: vec![SignpostConnection {
            target_tsid: "TARGETTSID".to_string(),
            target_label: "To Target".to_string(),
            arrival_x: Some(-777.0),
            arrival_y: Some(-5.0),
            arrival_facing: Some(Facing::Right),
        }],
    });
    state.load_street(street, vec![], vec![]);

    // Walk player to the signpost and cross it.
    state.player.x = 850.0;
    state.player.on_ground = true;
    // First tick: pre-subscribe (player within 500px of signpost).
    let input = InputState { right: true, ..Default::default() };
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    // Force player past signpost.x to trigger crossing.
    state.player.x = 950.0;
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

    assert!(state.pending_arrival.is_some(), "pending_arrival should be populated");
    let sp = state.pending_arrival.unwrap();
    assert_eq!(sp.x, -777.0);
    assert_eq!(sp.y, -5.0);
    assert_eq!(sp.facing, Some(Facing::Right));
}

#[test]
fn signpost_crossing_leaves_pending_arrival_none_when_no_arrival_fields() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
        empty_catalog(),
        empty_store_catalog(),
        empty_skill_defs(),
        HashMap::new(),
        HashMap::new(),
    );
    use crate::street::types::{Signpost, SignpostConnection};
    let mut street = test_street();
    street.signposts.push(Signpost {
        id: "east".to_string(),
        x: 900.0,
        y: 0.0,
        connects: vec![SignpostConnection {
            target_tsid: "TARGETTSID".to_string(),
            target_label: "To Target".to_string(),
            arrival_x: None,
            arrival_y: None,
            arrival_facing: None,
        }],
    });
    state.load_street(street, vec![], vec![]);

    state.player.x = 850.0;
    state.player.on_ground = true;
    let input = InputState { right: true, ..Default::default() };
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    state.player.x = 950.0;
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

    assert!(
        state.pending_arrival.is_none(),
        "pending_arrival should remain None without explicit arrival fields"
    );
    // But origin_tsid should still be set — legacy path will run at completion.
    assert!(state.transition_origin_tsid.is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib signpost_crossing_captures signpost_crossing_leaves_pending`
Expected: FAIL — `pending_arrival` is always `None` because nothing populates it yet.

- [ ] **Step 3: Update the signpost-crossing handler**

In `src-tauri/src/engine/state.rs`, replace the `if crossed { ... }` block (~lines 626-629):

```rust
if crossed {
    self.transition_origin_tsid = Some(street.tsid.clone());
    self.transition.trigger_swoop();
}
```

with:

```rust
if crossed {
    // Capture the arrival SpawnPoint from the active connection (if it has
    // explicit arrival fields) so the transition-completion handler can
    // place the player without falling back to .find()-on-reciprocal.
    // First connection wins — multi-connection UI is a separate ticket.
    self.pending_arrival = street
        .signposts
        .iter()
        .find(|s| (s.x - signpost_x).abs() < 0.5)
        .and_then(|s| s.connects.first())
        .and_then(|c| {
            if let (Some(x), Some(y)) = (c.arrival_x, c.arrival_y) {
                Some(crate::street::types::SpawnPoint {
                    x,
                    y,
                    facing: c.arrival_facing,
                })
            } else {
                None
            }
        });
    self.transition_origin_tsid = Some(street.tsid.clone());
    self.transition.trigger_swoop();
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib signpost_crossing_captures signpost_crossing_leaves_pending`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat(engine): capture pending_arrival on signpost crossing

When the player crosses a signpost, the active connection's arrival
fields (if any) are copied into GameState.pending_arrival for the
transition-completion handler to consume. Legacy connections without
arrival fields leave pending_arrival as None; the completion handler
will fall through to the existing .find()-on-reciprocal path."
```

---

## Task 8: Replace `state.rs:646` arrival block with `resolve_arrival`-based logic

**Files:**
- Modify: `src-tauri/src/engine/state.rs` (lines ~637-670)
- Test: `src-tauri/src/engine/state.rs` tests module

- [ ] **Step 1: Write the failing tests**

Add to the tests module:

```rust
#[test]
fn transition_completion_uses_pending_arrival_when_set() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
        empty_catalog(),
        empty_store_catalog(),
        empty_skill_defs(),
        HashMap::new(),
        HashMap::new(),
    );
    state.load_street(test_street(), vec![], vec![]);

    // Simulate mid-transition state: pending_arrival set, origin_tsid set,
    // transition phase Complete.
    state.pending_arrival = Some(crate::street::types::SpawnPoint {
        x: 333.0,
        y: -17.0,
        facing: Some(crate::street::types::Facing::Left),
    });
    state.transition_origin_tsid = Some("OTHERTSID".to_string());
    state.transition.phase = crate::engine::transition::TransitionPhase::Complete {
        swoop_progress: 1.0,
    };

    let input = InputState::default();
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

    assert_eq!(state.player.x, 333.0);
    assert_eq!(state.player.y, -17.0);
    assert_eq!(state.last_arrival.x, 333.0);
    assert_eq!(state.last_arrival.y, -17.0);
    // origin_tsid consumed, pending_arrival cleared.
    assert!(state.transition_origin_tsid.is_none());
    assert!(state.pending_arrival.is_none());
    assert_eq!(state.oob_ticks, 0);
}

#[test]
fn transition_completion_falls_back_to_reciprocal_signpost() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
        empty_catalog(),
        empty_store_catalog(),
        empty_skill_defs(),
        HashMap::new(),
        HashMap::new(),
    );
    // Street where a signpost at x=600 connects back to OTHERTSID.
    use crate::street::types::{Signpost, SignpostConnection};
    let mut street = test_street();
    street.signposts.push(Signpost {
        id: "reciprocal".to_string(),
        x: 600.0,
        y: 0.0,
        connects: vec![SignpostConnection {
            target_tsid: "OTHERTSID".to_string(),
            target_label: "Back".to_string(),
            arrival_x: None,
            arrival_y: None,
            arrival_facing: None,
        }],
    });
    state.load_street(street, vec![], vec![]);

    state.pending_arrival = None; // legacy path
    state.transition_origin_tsid = Some("OTHERTSID".to_string());
    state.transition.phase = crate::engine::transition::TransitionPhase::Complete {
        swoop_progress: 1.0,
    };

    let input = InputState::default();
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

    assert_eq!(state.player.x, 600.0);
    assert_eq!(state.player.y, 0.0); // ground_y
    assert_eq!(state.last_arrival.x, 600.0);
    assert!(state.transition_origin_tsid.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib transition_completion_uses_pending transition_completion_falls_back`
Expected: FAIL — current behavior places player via `.find()` and doesn't set `last_arrival`; `pending_arrival` is ignored.

- [ ] **Step 3: Rewrite the completion block**

In `src-tauri/src/engine/state.rs`, replace the block starting "Handle transition completion" (~lines 637-673). Find this block:

```rust
if let TransitionPhase::Complete { .. } = &self.transition.phase {
    if self.transition_origin_tsid.is_some() {
        let origin_tsid = self.transition_origin_tsid.take().unwrap();
        let street = self.street.as_ref().unwrap();
        let return_signpost = street
            .signposts
            .iter()
            .find(|s| s.connects.iter().any(|c| c.target_tsid == origin_tsid));

        if let Some(sp) = return_signpost {
            let street_mid = (street.left + street.right) / 2.0;
            let inward = if sp.x < street_mid { 1.0 } else { -1.0 };
            self.player.x = sp.x + inward * (PRE_SUBSCRIBE_DISTANCE + 50.0);
            self.player.y = street.ground_y;
            self.player.vx = 0.0;
            self.player.vy = 0.0;
        } else {
            self.player.x = (street.left + street.right) / 2.0;
            self.player.y = street.ground_y;
            self.player.vx = 0.0;
            self.player.vy = 0.0;
        }
        self.prev_on_ground = true;
        self.player.distance_since_footstep = 0.0;
    } else {
        self.transition.reset();
    }
}
```

Replace with:

```rust
if let TransitionPhase::Complete { .. } = &self.transition.phase {
    if self.transition_origin_tsid.is_some() {
        let origin_tsid = self.transition_origin_tsid.take().unwrap();
        let street = self.street.as_ref().unwrap();

        // Prefer the pending_arrival captured at signpost-crossing time;
        // fall back to resolve_arrival, which walks the (reciprocal → default
        // → center) chain.
        let arrival = self.pending_arrival.take().unwrap_or_else(|| {
            crate::engine::arrival::resolve_arrival(street, &origin_tsid, None)
        });

        let facing = arrival.facing.unwrap_or_else(|| {
            crate::engine::arrival::infer_facing(arrival.x, street)
        });

        self.player.x = arrival.x;
        self.player.y = arrival.y;
        self.player.vx = 0.0;
        self.player.vy = 0.0;
        self.facing = match facing {
            crate::street::types::Facing::Left => crate::engine::Direction::Left,
            crate::street::types::Facing::Right => crate::engine::Direction::Right,
        };
        self.last_arrival = crate::street::types::Point {
            x: arrival.x,
            y: arrival.y,
        };
        self.oob_ticks = 0;
        self.prev_on_ground = true;
        self.player.distance_since_footstep = 0.0;
    } else {
        self.transition.reset();
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib transition_completion_uses_pending transition_completion_falls_back`
Expected: PASS.

Run full state tests to catch regressions:
Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib engine::state`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat(engine): transition-completion uses resolve_arrival

Replaces the .find()-based disambiguation at state.rs:646 with a single
call chain: pending_arrival (captured at crossing) -> resolve_arrival
(which handles the reciprocal-signpost legacy fallback and final center
fallback). last_arrival is now set on every arrival, ready for the OOB
safety net."
```

---

## Task 9: Add OOB detector + safety-net teleport

**Files:**
- Modify: `src-tauri/src/engine/state.rs` (`GameState::tick` after `self.player.tick(…)` at line 707)
- Test: `src-tauri/src/engine/state.rs` tests module

- [ ] **Step 1: Write the failing tests**

Add to the tests module:

```rust
#[test]
fn oob_detector_fires_after_threshold_ticks() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
        empty_catalog(),
        empty_store_catalog(),
        empty_skill_defs(),
        HashMap::new(),
        HashMap::new(),
    );
    state.load_street(test_street(), vec![], vec![]);
    state.last_arrival = crate::street::types::Point { x: 100.0, y: -10.0 };

    // Simulate player stuck on the floor clamp: y == street.bottom (0.0),
    // not grounded (no platform below).
    state.player.x = -500.0;
    state.player.y = 0.0;
    state.player.on_ground = false;

    let input = InputState::default();
    // Tick 30+ times — hold player at floor, no platform.
    for _ in 0..35 {
        state.player.y = 0.0;
        state.player.on_ground = false;
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    }

    // Safety net should have fired: player at last_arrival.
    assert_eq!(state.player.x, 100.0);
    assert_eq!(state.player.y, -10.0);
    assert_eq!(state.oob_ticks, 0);
}

#[test]
fn oob_detector_resets_when_grounded() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
        empty_catalog(),
        empty_store_catalog(),
        empty_skill_defs(),
        HashMap::new(),
        HashMap::new(),
    );
    state.load_street(test_street(), vec![], vec![]);
    state.last_arrival = crate::street::types::Point { x: 100.0, y: -10.0 };

    state.player.x = 0.0;
    state.player.y = 0.0;
    let input = InputState::default();

    // 20 ticks stuck, then grounded for 20 ticks — counter should reset.
    for _ in 0..20 {
        state.player.y = 0.0;
        state.player.on_ground = false;
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    }
    assert!(state.oob_ticks > 0, "counter should increment during stuck ticks");

    for _ in 0..20 {
        state.player.on_ground = true;
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    }
    assert_eq!(state.oob_ticks, 0, "counter should reset when grounded");
    // Player should NOT have been teleported.
    assert_ne!(state.player.x, 100.0);
}

#[test]
fn oob_detector_does_not_fire_when_walking_on_bottom_platform() {
    // Walking on a platform that happens to sit at street.bottom (like
    // plat_main at y=0 in demo_meadow). on_ground=true → no trigger.
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
        empty_catalog(),
        empty_store_catalog(),
        empty_skill_defs(),
        HashMap::new(),
        HashMap::new(),
    );
    state.load_street(test_street(), vec![], vec![]);
    state.last_arrival = crate::street::types::Point { x: 100.0, y: -10.0 };
    let initial_x = 250.0;
    state.player.x = initial_x;

    let input = InputState::default();
    for _ in 0..60 {
        state.player.y = 0.0;
        state.player.on_ground = true; // on a real platform at y=0
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    }

    assert_eq!(state.oob_ticks, 0);
    assert_eq!(state.player.x, initial_x, "player should not be teleported");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib oob_detector_fires oob_detector_resets oob_detector_does_not_fire`
Expected: FAIL — no OOB detector exists yet.

- [ ] **Step 3: Add the OOB detector**

In `src-tauri/src/engine/state.rs`, first add a module-level constant near the top (after the existing `use` statements / before `GameState`):

```rust
const OOB_THRESHOLD_TICKS: u32 = 30; // ~500ms at 60fps
```

Then find the physics-tick line in `GameState::tick` at line 707:

```rust
self.player.tick(
    dt,
    input,
    street.platforms(),
    street.walls(),
    street.left,
    street.right,
    street.bottom,
);
```

Immediately after this block (before the jump/land audio detection), add:

```rust
// --- OOB safety-net detector ---
// If the player is sitting on the street.bottom clamp with no platform
// catching them (on_ground=false), they're on the invisible floor with
// nowhere to go. After OOB_THRESHOLD_TICKS consecutive frames in that
// state, teleport to last_arrival.
{
    let street = self.street.as_ref().unwrap();
    let at_floor = (self.player.y - street.bottom).abs() < 1.0;
    if at_floor && !self.player.on_ground {
        self.oob_ticks += 1;
        if self.oob_ticks >= OOB_THRESHOLD_TICKS {
            log::info!(
                "OOB respawn: street={} player=({:.1},{:.1}) -> last_arrival=({:.1},{:.1})",
                street.tsid,
                self.player.x,
                self.player.y,
                self.last_arrival.x,
                self.last_arrival.y
            );
            self.player.x = self.last_arrival.x;
            self.player.y = self.last_arrival.y;
            self.player.vx = 0.0;
            self.player.vy = 0.0;
            self.oob_ticks = 0;
        }
    } else {
        self.oob_ticks = 0;
    }
}
```

(If `log::info!` isn't available — check the top of the file for `use log::…` or similar — swap for whatever logging facade the crate uses. Alternatives that may be in scope: `tracing::info!`, a bespoke `info!` macro, or `eprintln!` as last resort.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib oob_detector_fires oob_detector_resets oob_detector_does_not_fire`
Expected: PASS.

Run full state tests:
Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib engine::state`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat(engine): OOB safety-net teleport to last_arrival

After 30 consecutive ticks (~500ms at 60fps) stuck at street.bottom with
on_ground=false, the player is teleported to last_arrival with zeroed
velocity. Walking onto a real platform within the window resets the
counter — only persistent stuck-on-invisible-floor triggers recovery.

Fires a log::info! on respawn for playtest diagnostics."
```

---

## Task 10: Migrate `demo_meadow.xml` and `demo_heights.xml`

**Files:**
- Modify: `assets/streets/demo_meadow.xml`
- Modify: `assets/streets/demo_heights.xml`

Note: this task has no unit test (it's a data migration). Verification is through the parser tests (which will continue to pass since the new fields are all optional) and through manual playtesting.

- [ ] **Step 1: Update `demo_meadow.xml`**

Open `assets/streets/demo_meadow.xml`. Find the signposts block (~lines 135-145):

```xml
<object id="signposts">
  <object id="sign_to_heights">
    <int id="x">1900</int>
    <int id="y">0</int>
    <object id="connects">
      <object id="connect_1">
        <objref id="target" tsid="LADEMO002" label="To the Heights" />
      </object>
    </object>
  </object>
</object>
```

Replace with:

```xml
<object id="signposts">
  <object id="sign_to_heights">
    <int id="x">1900</int>
    <int id="y">0</int>
    <object id="connects">
      <object id="connect_1">
        <objref id="target" tsid="LADEMO002" label="To the Heights" />
        <int id="arrival_x">-1700</int>
        <int id="arrival_y">0</int>
        <str id="arrival_facing">right</str>
      </object>
    </object>
  </object>
</object>
```

Also add a `<default_spawn>` element directly inside `<object id="dynamic">` (as a sibling of `<object id="signposts">`, `<object id="layers">`, etc.). Place it near the top of the dynamic block, after the metadata `<int>` fields:

```xml
<object id="default_spawn">
  <int id="x">0</int>
  <int id="y">0</int>
  <str id="facing">right</str>
</object>
```

- [ ] **Step 2: Update `demo_heights.xml`**

Open `assets/streets/demo_heights.xml`. Find the signposts block (~lines 145-155):

```xml
<object id="signposts">
  <object id="sign_to_meadow">
    <int id="x">-1900</int>
    <int id="y">0</int>
    <object id="connects">
      <object id="connect_1">
        <objref id="target" tsid="LADEMO001" label="Back to the Meadow" />
      </object>
    </object>
  </object>
</object>
```

Replace with:

```xml
<object id="signposts">
  <object id="sign_to_meadow">
    <int id="x">-1900</int>
    <int id="y">0</int>
    <object id="connects">
      <object id="connect_1">
        <objref id="target" tsid="LADEMO001" label="Back to the Meadow" />
        <int id="arrival_x">1700</int>
        <int id="arrival_y">0</int>
        <str id="arrival_facing">left</str>
      </object>
    </object>
  </object>
</object>
```

Also add a `<default_spawn>` element inside `<object id="dynamic">` (sibling of signposts):

```xml
<object id="default_spawn">
  <int id="x">0</int>
  <int id="y">0</int>
  <str id="facing">left</str>
</object>
```

- [ ] **Step 3: Verify XML still parses**

Run the full parser + engine test suite to confirm the XMLs parse and the streets load correctly:

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all PASS.

- [ ] **Step 4: Manual verification (app)**

Install dependencies if the worktree hasn't had `npm install` yet:

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/.claude/worktrees/zeb-132-street-reentry && npm install`

Then launch the app:

Run: `npm run tauri dev`

Verify all four of these behaviors in-game:

1. **Fresh spawn position** — start a new game on Demo Meadow. Player should spawn at `x=0, y=0` (center, facing right), matching the `<default_spawn>` values. Not near the signpost at `x=1900`.

2. **Signpost arrival position** — walk to the east signpost (x=1900), cross it. After the swoop lands on Demo Heights, player should be at `x=1700, y=0, facing=left` (from the connection's arrival fields), NOT at the reciprocal signpost's position at `x=-1900`.

3. **Round-trip arrival** — in Demo Heights, walk to the west signpost (x=-1900), cross it. On Demo Meadow, player should land at `x=-1700, y=0, facing=right`.

4. **OOB recovery** — in Demo Heights (which has no wall on the side opposite the signpost — check the XML), walk off the open edge, fall. Physics clamps y to 0. After ~500ms of being off-platform, player should teleport to `last_arrival` (the position from the most recent arrival). Check the terminal for the `OOB respawn:` log line.

If any behavior diverges from expected, stop and investigate before committing.

- [ ] **Step 5: Commit**

```bash
git add assets/streets/demo_meadow.xml assets/streets/demo_heights.xml
git commit -m "feat(streets): migrate demo streets to new arrival schema

demo_meadow and demo_heights now carry explicit <default_spawn> and
per-connection <arrival_x/arrival_y/arrival_facing> fields. This exercises
the ZEB-132 arrival machinery and closes the fall-into-void bug via the
new physics Y-clamp + OOB safety net."
```

---

## Final Verification

After all 10 tasks are complete, run the full suite one more time:

- [ ] **Step: Full test sweep**

Run:
```bash
cargo test --manifest-path src-tauri/Cargo.toml
npm test --prefix /Users/zeblith/work/zeblithic/harmony-glitch/.claude/worktrees/zeb-132-street-reentry
```

Expected: all PASS.

- [ ] **Step: Clippy**

Run: `cd src-tauri && cargo clippy --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step: Full manual playtest**

Repeat the four behaviors from Task 10 Step 4, then additionally:

- Run across Demo Meadow east→Heights west→Meadow east multiple times. `last_arrival` should update on each traversal; OOB respawn after any fall should place you at the most recent arrival point, not the first one.
- Open a save from before this branch (if one exists) and restore it. Player should land correctly; OOB respawn should work. (The new fields are `Option` + `#[serde(default)]` so old saves deserialize cleanly with `None` → runtime uses `resolve_default_spawn`.)
