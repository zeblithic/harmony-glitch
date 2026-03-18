# NPC Entity Movement Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add wander-within-radius movement to NPC entities (chicken, pig, butterfly) with idle pauses, facing direction, and vertical bob for flying entities.

**Architecture:** Rust owns all movement logic (sans-I/O). `tick_entities()` updates positions each tick and writes back to `WorldEntity.x` for interaction compatibility. Frontend reads `facing` from WorldEntityFrame and flips sprites.

**Tech Stack:** Rust (Tauri v2), serde, rand; TypeScript, PixiJS v8, Svelte 5

**Spec:** `docs/plans/2026-03-18-npc-movement-design.md`

---

## Chunk 1: Data Model & JSON

### Task 1: Add movement fields to EntityDef

**Files:**
- Modify: `src-tauri/src/item/types.rs:25-39`
- Test: `src-tauri/src/item/types.rs` (existing tests module)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/item/types.rs`:

```rust
#[test]
fn entity_def_deserializes_movement_fields() {
    let json = r#"{
        "name": "Chicken",
        "verb": "Squeeze",
        "yields": [{ "item": "grain", "min": 1, "max": 2 }],
        "cooldownSecs": 8.0,
        "maxHarvests": 2,
        "respawnSecs": 45.0,
        "spriteClass": "npc_chicken",
        "interactRadius": 60,
        "walkSpeed": 40.0,
        "wanderRadius": 120.0
    }"#;
    let def: EntityDef = serde_json::from_str(json).unwrap();
    assert!((def.walk_speed.unwrap() - 40.0).abs() < 0.01);
    assert!((def.wander_radius.unwrap() - 120.0).abs() < 0.01);
    assert!(def.bob_amplitude.is_none());
    assert!(def.bob_frequency.is_none());
}

#[test]
fn entity_def_deserializes_bob_fields() {
    let json = r#"{
        "name": "Butterfly",
        "verb": "Milk",
        "yields": [{ "item": "milk", "min": 1, "max": 1 }],
        "cooldownSecs": 0.0,
        "maxHarvests": 1,
        "respawnSecs": 20.0,
        "spriteClass": "npc_butterfly",
        "interactRadius": 90,
        "walkSpeed": 25.0,
        "wanderRadius": 150.0,
        "bobAmplitude": 15.0,
        "bobFrequency": 1.5
    }"#;
    let def: EntityDef = serde_json::from_str(json).unwrap();
    assert!((def.bob_amplitude.unwrap() - 15.0).abs() < 0.01);
    assert!((def.bob_frequency.unwrap() - 1.5).abs() < 0.01);
}

#[test]
fn entity_def_omits_movement_fields_for_static() {
    let json = r#"{
        "name": "Fruit Tree",
        "verb": "Harvest",
        "yields": [{ "item": "cherry", "min": 1, "max": 3 }],
        "cooldownSecs": 5.0,
        "maxHarvests": 3,
        "respawnSecs": 30.0,
        "spriteClass": "tree_fruit",
        "interactRadius": 80
    }"#;
    let def: EntityDef = serde_json::from_str(json).unwrap();
    assert!(def.walk_speed.is_none());
    assert!(def.wander_radius.is_none());
    assert!(def.bob_amplitude.is_none());
    assert!(def.bob_frequency.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p harmony-glitch entity_def_deserializes_movement -- --nocapture`
Expected: FAIL — `EntityDef` has no `walk_speed` field

- [ ] **Step 3: Add movement fields to EntityDef**

In `src-tauri/src/item/types.rs`, add four `Option` fields to the `EntityDef` struct after `interact_radius`:

```rust
pub struct EntityDef {
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub verb: String,
    pub yields: Vec<YieldEntry>,
    pub cooldown_secs: f64,
    pub max_harvests: u32,
    pub respawn_secs: f64,
    pub sprite_class: String,
    pub interact_radius: f64,
    pub walk_speed: Option<f64>,
    pub wander_radius: Option<f64>,
    pub bob_amplitude: Option<f64>,
    pub bob_frequency: Option<f64>,
}
```

**Important:** Every place that constructs an `EntityDef` literal must be updated to include the four new fields set to `None`. There are 7 sites across 2 files:

- `src-tauri/src/engine/state.rs` — 4 sites (in tests: `interaction_adds_to_inventory`, `entity_frame_includes_cooldown_remaining`, `entity_frame_shows_depleted`, `prompt_shows_cooldown_text_through_tick`)
- `src-tauri/src/item/interaction.rs` — 3 sites (in tests)

Add to each `EntityDef { ... }` literal:
```rust
walk_speed: None,
wander_radius: None,
bob_amplitude: None,
bob_frequency: None,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: ALL tests pass (including the three new ones and all existing tests that construct EntityDef)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/item/types.rs src-tauri/src/engine/state.rs src-tauri/src/item/interaction.rs
git commit -m "feat: add movement and bob fields to EntityDef"
```

---

### Task 2: Add movement state to EntityInstanceState

**Files:**
- Modify: `src-tauri/src/item/types.rs:58-78`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/item/types.rs`:

```rust
#[test]
fn entity_instance_state_has_movement_fields() {
    use crate::avatar::types::Direction;

    let state = EntityInstanceState::new(3);
    assert_eq!(state.current_x, 0.0);
    assert_eq!(state.velocity_x, 0.0);
    assert_eq!(state.wander_origin, 0.0);
    assert_eq!(state.idle_until, 0.0);
    assert!(matches!(state.facing, Direction::Right));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p harmony-glitch entity_instance_state_has_movement -- --nocapture`
Expected: FAIL — `EntityInstanceState` has no `current_x` field

- [ ] **Step 3: Add movement fields to EntityInstanceState**

Add `use crate::avatar::types::Direction;` to the top of `types.rs` (with the other imports).

Update `EntityInstanceState` and its `new()` constructor:

```rust
#[derive(Debug, Clone)]
pub struct EntityInstanceState {
    pub harvests_remaining: u32,
    pub cooldown_until: f64,
    pub depleted_until: f64,
    pub current_x: f64,
    pub velocity_x: f64,
    pub facing: Direction,
    pub wander_origin: f64,
    pub idle_until: f64,
}

impl EntityInstanceState {
    pub fn new(max_harvests: u32) -> Self {
        Self {
            harvests_remaining: max_harvests,
            cooldown_until: 0.0,
            depleted_until: 0.0,
            current_x: 0.0,
            velocity_x: 0.0,
            facing: Direction::Right,
            wander_origin: 0.0,
            idle_until: 0.0,
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: ALL pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/item/types.rs
git commit -m "feat: add movement state to EntityInstanceState"
```

---

### Task 3: Add facing to WorldEntityFrame

**Files:**
- Modify: `src-tauri/src/item/types.rs:94-106`
- Modify: `src-tauri/src/engine/state.rs:438-467` (build_entity_frames)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/item/types.rs`:

```rust
#[test]
fn world_entity_frame_serializes_facing() {
    use crate::avatar::types::Direction;

    let frame = WorldEntityFrame {
        id: "t1".into(),
        entity_type: "fruit_tree".into(),
        name: "Fruit Tree".into(),
        sprite_class: "tree_fruit".into(),
        x: 100.0,
        y: -2.0,
        cooldown_remaining: None,
        depleted: false,
        facing: Direction::Left,
    };
    let json = serde_json::to_string(&frame).unwrap();
    assert!(json.contains(r#""facing":"left""#));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p harmony-glitch world_entity_frame_serializes_facing -- --nocapture`
Expected: FAIL — `WorldEntityFrame` has no `facing` field

- [ ] **Step 3: Add facing field to WorldEntityFrame**

In `src-tauri/src/item/types.rs`, add `facing` to `WorldEntityFrame`:

```rust
pub struct WorldEntityFrame {
    pub id: String,
    pub entity_type: String,
    pub name: String,
    pub sprite_class: String,
    pub x: f64,
    pub y: f64,
    pub cooldown_remaining: Option<f64>,
    pub depleted: bool,
    pub facing: Direction,
}
```

Then update `build_entity_frames()` in `src-tauri/src/engine/state.rs` to include the `facing` field. For now, look up the movement state from `entity_states` and use its facing, defaulting to `Direction::Right` for static entities:

```rust
fn build_entity_frames(&self) -> Vec<WorldEntityFrame> {
    self.world_entities
        .iter()
        .map(|e| {
            let def = self.entity_defs.get(&e.entity_type);

            let (cooldown_remaining, depleted) = if let Some(state) = self.entity_states.get(&e.id) {
                let remaining = (state.cooldown_until.max(state.depleted_until)) - self.game_time;
                if remaining > 0.0 {
                    (Some(remaining), state.depleted_until > self.game_time)
                } else {
                    (None, false)
                }
            } else {
                (None, false)
            };

            let facing = self.entity_states
                .get(&e.id)
                .map(|s| s.facing)
                .unwrap_or(Direction::Right);

            WorldEntityFrame {
                id: e.id.clone(),
                entity_type: e.entity_type.clone(),
                name: def.map(|d| d.name.clone()).unwrap_or_default(),
                sprite_class: def.map(|d| d.sprite_class.clone()).unwrap_or_default(),
                x: e.x,
                y: e.y,
                cooldown_remaining,
                depleted,
                facing,
            }
        })
        .collect()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: ALL pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/item/types.rs src-tauri/src/engine/state.rs
git commit -m "feat: add facing direction to WorldEntityFrame"
```

---

### Task 4: Update entities.json with movement data

**Files:**
- Modify: `assets/entities.json`
- Modify: `src-tauri/src/item/loader.rs` (test updates)

- [ ] **Step 1: Update entities.json**

Add `walkSpeed` and `wanderRadius` to the three animal entities. Add `bobAmplitude` and `bobFrequency` to butterfly only. Trees get no new fields.

```json
{
  "fruit_tree": {
    "name": "Fruit Tree",
    "verb": "Harvest",
    "yields": [{ "item": "cherry", "min": 1, "max": 3 }],
    "cooldownSecs": 5.0,
    "maxHarvests": 3,
    "respawnSecs": 30.0,
    "spriteClass": "tree_fruit",
    "interactRadius": 80
  },
  "chicken": {
    "name": "Chicken",
    "verb": "Squeeze",
    "yields": [{ "item": "grain", "min": 1, "max": 2 }],
    "cooldownSecs": 8.0,
    "maxHarvests": 2,
    "respawnSecs": 45.0,
    "spriteClass": "npc_chicken",
    "interactRadius": 60,
    "walkSpeed": 40.0,
    "wanderRadius": 120.0
  },
  "pig": {
    "name": "Pig",
    "verb": "Nibble",
    "yields": [{ "item": "meat", "min": 1, "max": 2 }],
    "cooldownSecs": 8.0,
    "maxHarvests": 2,
    "respawnSecs": 45.0,
    "spriteClass": "npc_pig",
    "interactRadius": 60,
    "walkSpeed": 35.0,
    "wanderRadius": 100.0
  },
  "butterfly": {
    "name": "Butterfly",
    "verb": "Milk",
    "yields": [{ "item": "milk", "min": 1, "max": 1 }],
    "cooldownSecs": 0.0,
    "maxHarvests": 1,
    "respawnSecs": 20.0,
    "spriteClass": "npc_butterfly",
    "interactRadius": 90,
    "walkSpeed": 25.0,
    "wanderRadius": 150.0,
    "bobAmplitude": 15.0,
    "bobFrequency": 1.5
  },
  "bubble_tree": {
    "name": "Bubble Tree",
    "verb": "Harvest",
    "yields": [{ "item": "bubble", "min": 1, "max": 4 }],
    "cooldownSecs": 3.0,
    "maxHarvests": 4,
    "respawnSecs": 25.0,
    "spriteClass": "tree_bubble",
    "interactRadius": 80
  },
  "wood_tree": {
    "name": "Wood Tree",
    "verb": "Chop",
    "yields": [{ "item": "wood", "min": 1, "max": 2 }],
    "cooldownSecs": 6.0,
    "maxHarvests": 3,
    "respawnSecs": 35.0,
    "spriteClass": "tree_wood",
    "interactRadius": 80
  }
}
```

- [ ] **Step 2: Add loader test for movement fields**

Add to the `tests` module in `src-tauri/src/item/loader.rs`:

```rust
#[test]
fn parse_bundled_entities_has_movement_fields() {
    let json = include_str!("../../../assets/entities.json");
    let defs = parse_entity_defs(json).unwrap();

    let chicken = &defs["chicken"];
    assert!((chicken.walk_speed.unwrap() - 40.0).abs() < 0.01);
    assert!((chicken.wander_radius.unwrap() - 120.0).abs() < 0.01);
    assert!(chicken.bob_amplitude.is_none());

    let butterfly = &defs["butterfly"];
    assert!((butterfly.bob_amplitude.unwrap() - 15.0).abs() < 0.01);
    assert!((butterfly.bob_frequency.unwrap() - 1.5).abs() < 0.01);

    let fruit_tree = &defs["fruit_tree"];
    assert!(fruit_tree.walk_speed.is_none());
    assert!(fruit_tree.wander_radius.is_none());
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: ALL pass (JSON fields match struct with serde camelCase)

- [ ] **Step 4: Commit**

```bash
git add assets/entities.json src-tauri/src/item/loader.rs
git commit -m "feat: add movement and bob data to entities.json"
```

---

## Chunk 2: Movement Logic (Rust)

### Task 5: Implement tick_entities()

This is the core movement logic. All tests use a seeded RNG for deterministic assertions.

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

- [ ] **Step 1: Write failing tests for movement**

Add these tests to the `tests` module in `src-tauri/src/engine/state.rs`. They need a helper to build entity defs with movement fields:

```rust
fn movable_entity_defs() -> EntityDefs {
    let mut defs = EntityDefs::new();
    defs.insert("chicken".into(), EntityDef {
        id: "chicken".into(),
        name: "Chicken".into(),
        verb: "Squeeze".into(),
        yields: vec![],
        cooldown_secs: 0.0,
        max_harvests: 0,
        respawn_secs: 0.0,
        sprite_class: "npc_chicken".into(),
        interact_radius: 60.0,
        walk_speed: Some(40.0),
        wander_radius: Some(120.0),
        bob_amplitude: None,
        bob_frequency: None,
    });
    defs.insert("fruit_tree".into(), EntityDef {
        id: "fruit_tree".into(),
        name: "Fruit Tree".into(),
        verb: "Harvest".into(),
        yields: vec![],
        cooldown_secs: 0.0,
        max_harvests: 0,
        respawn_secs: 0.0,
        sprite_class: "tree_fruit".into(),
        interact_radius: 80.0,
        walk_speed: None,
        wander_radius: None,
        bob_amplitude: None,
        bob_frequency: None,
    });
    defs
}

#[test]
fn tick_entities_moves_movable_entity() {
    use rand::SeedableRng;

    let defs = movable_entity_defs();
    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs);
    let entities = vec![
        WorldEntity { id: "c1".into(), entity_type: "chicken".into(), x: 200.0, y: -2.0 },
    ];
    state.load_street(test_street(), entities);

    let input = InputState::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    // Tick several times to get past initial idle
    for _ in 0..200 {
        state.tick(1.0 / 60.0, &input, &mut rng);
    }

    // Entity should have moved from spawn
    let chicken = &state.world_entities[0];
    assert!((chicken.x - 200.0).abs() > 1.0,
        "Chicken should have moved from spawn x=200, got x={}", chicken.x);
}

#[test]
fn tick_entities_static_entity_stays_put() {
    use rand::SeedableRng;

    let defs = movable_entity_defs();
    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs);
    let entities = vec![
        WorldEntity { id: "t1".into(), entity_type: "fruit_tree".into(), x: -800.0, y: -2.0 },
    ];
    state.load_street(test_street(), entities);

    let input = InputState::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    for _ in 0..200 {
        state.tick(1.0 / 60.0, &input, &mut rng);
    }

    let tree = &state.world_entities[0];
    assert!((tree.x - (-800.0)).abs() < 0.01,
        "Tree should stay at x=-800, got x={}", tree.x);
}

#[test]
fn tick_entities_respects_wander_radius() {
    use rand::SeedableRng;

    let defs = movable_entity_defs();
    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs);
    let entities = vec![
        WorldEntity { id: "c1".into(), entity_type: "chicken".into(), x: 200.0, y: -2.0 },
    ];
    state.load_street(test_street(), entities);

    let input = InputState::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    // Tick many times — entity should never exceed wander_radius (120) from spawn (200)
    for _ in 0..2000 {
        state.tick(1.0 / 60.0, &input, &mut rng);
    }

    let chicken = &state.world_entities[0];
    let distance = (chicken.x - 200.0).abs();
    assert!(distance <= 121.0,  // 1px tolerance for float
        "Chicken at x={} is {}px from spawn (max 120)", chicken.x, distance);
}

#[test]
fn tick_entities_facing_matches_direction() {
    use rand::SeedableRng;

    let defs = movable_entity_defs();
    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs);
    let entities = vec![
        WorldEntity { id: "c1".into(), entity_type: "chicken".into(), x: 200.0, y: -2.0 },
    ];
    state.load_street(test_street(), entities);

    let input = InputState::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    // Tick until entity is moving (past initial idle)
    for _ in 0..200 {
        state.tick(1.0 / 60.0, &input, &mut rng);
    }

    let entity_state = state.entity_states.get("c1").unwrap();
    if entity_state.velocity_x > 0.0 {
        assert_eq!(entity_state.facing, Direction::Right);
    } else if entity_state.velocity_x < 0.0 {
        assert_eq!(entity_state.facing, Direction::Left);
    }
    // velocity_x == 0 means idle — facing can be either, don't assert
}

#[test]
fn tick_entities_idle_pause_at_boundary() {
    use rand::SeedableRng;

    let mut defs = EntityDefs::new();
    defs.insert("fast_npc".into(), EntityDef {
        id: "fast_npc".into(),
        name: "Fast".into(),
        verb: "Poke".into(),
        yields: vec![],
        cooldown_secs: 0.0,
        max_harvests: 0,
        respawn_secs: 0.0,
        sprite_class: "npc_fast".into(),
        interact_radius: 60.0,
        walk_speed: Some(200.0),   // Very fast so it reaches boundary quickly
        wander_radius: Some(20.0), // Very small radius
        bob_amplitude: None,
        bob_frequency: None,
    });

    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs);
    let entities = vec![
        WorldEntity { id: "f1".into(), entity_type: "fast_npc".into(), x: 0.0, y: -2.0 },
    ];
    state.load_street(test_street(), entities);

    let input = InputState::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    // Tick past initial idle and long enough to reach boundary
    for _ in 0..300 {
        state.tick(1.0 / 60.0, &input, &mut rng);
    }

    // After hitting boundary, entity should have had at least one idle pause
    // (velocity_x = 0 at some point). We can verify it reversed direction
    // by checking it's been on both sides of spawn.
    let entity_state = state.entity_states.get("f1").unwrap();
    // Entity must be within wander radius
    let dist = (entity_state.current_x - entity_state.wander_origin).abs();
    assert!(dist <= 21.0, "Entity outside wander radius: dist={}", dist);
}

#[test]
fn tick_entities_write_back_to_world_entity() {
    use rand::SeedableRng;

    let defs = movable_entity_defs();
    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs);
    let entities = vec![
        WorldEntity { id: "c1".into(), entity_type: "chicken".into(), x: 200.0, y: -2.0 },
    ];
    state.load_street(test_street(), entities);

    let input = InputState::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    for _ in 0..200 {
        state.tick(1.0 / 60.0, &input, &mut rng);
    }

    // WorldEntity.x should match the movement state's current_x
    let entity_state = state.entity_states.get("c1").unwrap();
    let world_entity = &state.world_entities[0];
    assert!((world_entity.x - entity_state.current_x).abs() < 0.01,
        "WorldEntity.x ({}) should match current_x ({})", world_entity.x, entity_state.current_x);
}

#[test]
fn tick_entities_initial_direction_varies_with_seed() {
    use rand::SeedableRng;

    let defs = movable_entity_defs();

    // Run with two different seeds and collect initial facing
    let mut facings = Vec::new();
    for seed in [1u64, 2, 3, 4, 5, 6, 7, 8] {
        let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs.clone());
        let entities = vec![
            WorldEntity { id: "c1".into(), entity_type: "chicken".into(), x: 200.0, y: -2.0 },
        ];
        state.load_street(test_street(), entities);
        let input = InputState::default();
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        state.tick(1.0 / 60.0, &input, &mut rng);
        let entity_state = state.entity_states.get("c1").unwrap();
        facings.push(entity_state.facing);
    }

    // With 8 different seeds, we should see both Left and Right
    let has_left = facings.iter().any(|f| *f == Direction::Left);
    let has_right = facings.iter().any(|f| *f == Direction::Right);
    assert!(has_left && has_right,
        "Expected both Left and Right facings across seeds, got {:?}", facings);
}

#[test]
fn build_entity_frames_defaults_facing_right_for_static() {
    use rand::SeedableRng;

    let defs = movable_entity_defs();
    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs);
    let entities = vec![
        WorldEntity { id: "t1".into(), entity_type: "fruit_tree".into(), x: -800.0, y: -2.0 },
    ];
    state.load_street(test_street(), entities);

    let input = InputState::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let frame = state.tick(1.0 / 60.0, &input, &mut rng).unwrap();

    // Static entity (tree) should default to facing Right
    assert_eq!(frame.world_entities[0].facing, Direction::Right);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p harmony-glitch tick_entities -- --nocapture`
Expected: FAIL — no `tick_entities` method exists yet

- [ ] **Step 3: Implement tick_entities()**

Add this method to the `impl GameState` block in `src-tauri/src/engine/state.rs`. Uses index-based iteration to satisfy the borrow checker (iterating `&mut self.world_entities` while accessing `self.entity_defs` and `self.entity_states` requires splitting the borrows):

```rust
fn tick_entities(&mut self, dt: f64, rng: &mut impl Rng) {
    for i in 0..self.world_entities.len() {
        let entity_type = self.world_entities[i].entity_type.clone();
        let entity_id = self.world_entities[i].id.clone();
        let entity_x = self.world_entities[i].x;

        let def = match self.entity_defs.get(&entity_type) {
            Some(d) => d,
            None => continue,
        };

        let (walk_speed, wander_radius, max_harvests) = match (def.walk_speed, def.wander_radius) {
            (Some(ws), Some(wr)) => (ws, wr, def.max_harvests),
            _ => continue,
        };

        let game_time = self.game_time;
        let state = self.entity_states
            .entry(entity_id)
            .or_insert_with(|| {
                let mut s = EntityInstanceState::new(max_harvests);
                s.current_x = entity_x;
                s.wander_origin = entity_x;
                s.facing = if rng.gen::<bool>() { Direction::Right } else { Direction::Left };
                s.idle_until = game_time + rng.gen_range(0.0..2.0);
                s
            });

        // Idle check
        if game_time < state.idle_until {
            state.velocity_x = 0.0;
            self.world_entities[i].x = state.current_x;
            continue;
        }

        // Boundary check — only when moving
        if state.velocity_x != 0.0
            && (state.current_x - state.wander_origin).abs() >= wander_radius
        {
            if state.current_x > state.wander_origin {
                state.current_x = state.wander_origin + wander_radius;
            } else {
                state.current_x = state.wander_origin - wander_radius;
            }
            state.facing = match state.facing {
                Direction::Right => Direction::Left,
                Direction::Left => Direction::Right,
            };
            state.velocity_x = 0.0;
            state.idle_until = game_time + rng.gen_range(1.0..3.0);
            self.world_entities[i].x = state.current_x;
            continue;
        }

        // Apply movement
        let direction_sign = if state.facing == Direction::Right { 1.0 } else { -1.0 };
        state.velocity_x = walk_speed * direction_sign;
        state.current_x += state.velocity_x * dt;
        self.world_entities[i].x = state.current_x;
    }
}
```

Then call `tick_entities` from `tick()`. Place the call **before** the interaction system block and **outside** the `if !is_swooping` guard (entities keep wandering during swoops). Insert it right after the transition system's swooping-state recheck (after `let is_swooping = matches!(...)` around line 244) and before `let interaction_prompt = if !is_swooping {`:

```rust
        // Re-check swooping state after transition system may have changed it.
        let is_swooping = matches!(self.transition.phase, TransitionPhase::Swooping { .. });

        // Tick entity movement — runs even during swoops so NPCs keep wandering.
        // Must run BEFORE the interaction block so that lazy-init of movement
        // state happens before execute_interaction can create a partial state.
        self.tick_entities(dt, rng);

        let interaction_prompt = if !is_swooping {
```

**Why before interactions:** Both `tick_entities` and `execute_interaction` lazy-init `EntityInstanceState` via `entry().or_insert_with()`. If interactions run first on the first tick, the interaction system creates an `EntityInstanceState` with `current_x = 0.0` and `wander_origin = 0.0` (defaults from `new()`), and `tick_entities` would see it already exists in the map and skip its spawn-position init. Placing `tick_entities` first ensures movement state is correctly initialized from the entity's spawn position.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: ALL pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: implement tick_entities() wander movement"
```

---

### Task 6: Add bob offset to build_entity_frames

**Files:**
- Modify: `src-tauri/src/engine/state.rs` (build_entity_frames)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src-tauri/src/engine/state.rs`:

```rust
#[test]
fn build_entity_frames_applies_bob_offset() {
    use rand::SeedableRng;

    let mut defs = EntityDefs::new();
    defs.insert("butterfly".into(), EntityDef {
        id: "butterfly".into(),
        name: "Butterfly".into(),
        verb: "Milk".into(),
        yields: vec![],
        cooldown_secs: 0.0,
        max_harvests: 0,
        respawn_secs: 0.0,
        sprite_class: "npc_butterfly".into(),
        interact_radius: 90.0,
        walk_speed: Some(25.0),
        wander_radius: Some(150.0),
        bob_amplitude: Some(15.0),
        bob_frequency: Some(1.5),
    });

    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs);
    let entities = vec![
        WorldEntity { id: "b1".into(), entity_type: "butterfly".into(), x: 600.0, y: -80.0 },
    ];
    state.load_street(test_street(), entities);

    let input = InputState::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    // Collect y values over several ticks — should vary due to sine bob
    let mut y_values: Vec<f64> = Vec::new();
    for _ in 0..120 {
        if let Some(frame) = state.tick(1.0 / 60.0, &input, &mut rng) {
            y_values.push(frame.world_entities[0].y);
        }
    }

    let min_y = y_values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_y = y_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let y_range = max_y - min_y;

    assert!(y_range > 1.0,
        "Butterfly y should oscillate due to bob, but range was only {}", y_range);
}

#[test]
fn build_entity_frames_no_bob_for_ground_entity() {
    use rand::SeedableRng;

    let defs = movable_entity_defs(); // chicken + fruit_tree, no bob fields
    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), defs);
    let entities = vec![
        WorldEntity { id: "c1".into(), entity_type: "chicken".into(), x: 200.0, y: -2.0 },
    ];
    state.load_street(test_street(), entities);

    let input = InputState::default();
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);

    // All y values should be the same (no bob)
    let mut y_values: Vec<f64> = Vec::new();
    for _ in 0..120 {
        if let Some(frame) = state.tick(1.0 / 60.0, &input, &mut rng) {
            y_values.push(frame.world_entities[0].y);
        }
    }

    let first_y = y_values[0];
    for (i, &y) in y_values.iter().enumerate() {
        assert!((y - first_y).abs() < 0.01,
            "Chicken y should not bob, but frame {} had y={} vs first y={}", i, y, first_y);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p harmony-glitch build_entity_frames_applies_bob -- --nocapture`
Expected: FAIL — bob offset not implemented

- [ ] **Step 3: Implement bob offset in build_entity_frames**

Update `build_entity_frames()` in `src-tauri/src/engine/state.rs`. Add bob offset computation for entities with `bob_amplitude` and `bob_frequency`:

```rust
fn build_entity_frames(&self) -> Vec<WorldEntityFrame> {
    self.world_entities
        .iter()
        .map(|e| {
            let def = self.entity_defs.get(&e.entity_type);

            let (cooldown_remaining, depleted) = if let Some(state) = self.entity_states.get(&e.id) {
                let remaining = (state.cooldown_until.max(state.depleted_until)) - self.game_time;
                if remaining > 0.0 {
                    (Some(remaining), state.depleted_until > self.game_time)
                } else {
                    (None, false)
                }
            } else {
                (None, false)
            };

            let facing = self.entity_states
                .get(&e.id)
                .map(|s| s.facing)
                .unwrap_or(Direction::Right);

            // Apply vertical bob for entities with bob config
            let y = if let Some(d) = def {
                match (d.bob_amplitude, d.bob_frequency) {
                    (Some(amp), Some(freq)) => {
                        e.y + (self.game_time * freq * std::f64::consts::TAU).sin() * amp
                    }
                    _ => e.y,
                }
            } else {
                e.y
            };

            WorldEntityFrame {
                id: e.id.clone(),
                entity_type: e.entity_type.clone(),
                name: def.map(|d| d.name.clone()).unwrap_or_default(),
                sprite_class: def.map(|d| d.sprite_class.clone()).unwrap_or_default(),
                x: e.x,
                y,
                cooldown_remaining,
                depleted,
                facing,
            }
        })
        .collect()
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace`
Expected: ALL pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: add vertical bob offset for flying entities"
```

---

## Chunk 3: Frontend Integration

### Task 7: Update frontend types and renderer

**Files:**
- Modify: `src/lib/types.ts:182-191`
- Modify: `src/lib/engine/renderer.ts:349-357`

- [ ] **Step 1: Add facing to WorldEntityFrame TypeScript type**

In `src/lib/types.ts`, add `facing` to the `WorldEntityFrame` interface:

```typescript
export interface WorldEntityFrame {
  id: string;
  entityType: string;
  name: string;
  spriteClass: string;
  x: number;
  y: number;
  cooldownRemaining: number | null;
  depleted: boolean;
  facing: Direction;
}
```

- [ ] **Step 2: Apply facing flip in renderer**

In `src/lib/engine/renderer.ts`, after the entity position assignment (line ~349-350), add the facing flip. The block should look like:

```typescript
sprite.x = entity.x - this.street.left;
sprite.y = entity.y - this.street.top;
sprite.scale.x = entity.facing === 'right' ? 1 : -1;
```

- [ ] **Step 3: Update sprites.test.ts entity test data**

In `src/lib/engine/sprites.test.ts`, add `facing: 'right' as const` to all entity objects in the tests (createEntity fallback, fallback upgrade path, missing texture dedup). For example:

```typescript
const entity = {
    id: 'e1', entityType: 'tree', name: 'Fruit Tree',
    spriteClass: 'tree_fruit', x: 100, y: 0,
    cooldownRemaining: null, depleted: false,
    facing: 'right' as const,
};
```

Update ALL entity objects in the test file to include the `facing` field.

- [ ] **Step 4: Run frontend tests and build**

Run: `npx vitest run && npm run build`
Expected: ALL 9 tests pass, build succeeds with no type errors

- [ ] **Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/engine/renderer.ts src/lib/engine/sprites.test.ts
git commit -m "feat: apply entity facing direction in renderer"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run full Rust test suite**

Run: `cargo test --workspace`
Expected: ALL pass

- [ ] **Step 2: Run full frontend test suite and build**

Run: `npx vitest run && npm run build`
Expected: ALL pass, build clean

- [ ] **Step 3: Run Rust linter**

Run: `cd src-tauri && cargo clippy --workspace`
Expected: No warnings

- [ ] **Step 4: Commit any clippy fixes if needed**

```bash
git add -A
git commit -m "fix: address clippy warnings"
```
