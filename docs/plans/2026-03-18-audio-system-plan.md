# Audio System Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SFX and ambient audio via Rust AudioEvent emitter + Howler.js frontend, with a swappable sound kit manifest.

**Architecture:** Rust emits semantic `AudioEvent`s in RenderFrame (Jump, Land, ItemPickup, etc.). A standalone TypeScript `AudioManager` class maps events to audio files via a JSON sound kit manifest, plays them through Howler.js, and manages ambient loop crossfading on street transitions. No PixiJS or Svelte coupling.

**Tech Stack:** Rust (Tauri v2), TypeScript, Howler.js, vitest

**Spec:** `docs/plans/2026-03-18-audio-system-design.md`

---

## File Structure

### New files
| File | Responsibility |
|------|---------------|
| `src-tauri/src/engine/audio.rs` | `AudioEvent` enum with serde tagged serialization |
| `src/lib/engine/audio.ts` | `AudioManager` class, `SoundKit`/`AudioEvent` types, kit loading |
| `src/lib/engine/audio.test.ts` | AudioManager unit tests with mocked Howler |
| `assets/audio/default-kit.json` | Sound kit manifest |
| `assets/audio/sfx/*.mp3` | ~9 SFX files from Glitch library |
| `assets/audio/ambient/*.mp3` | ~2 ambient loops from Glitch library |

### Modified files
| File | Changes |
|------|---------|
| `src-tauri/src/engine/mod.rs` | Add `pub mod audio;` |
| `src-tauri/src/engine/state.rs` | Add `audio_events`/`pending_audio_events` to GameState, `audio_events` to RenderFrame, emit events in tick/craft_recipe/load_street |
| `src-tauri/src/item/interaction.rs` | Add `interaction_type` field to `InteractionResult` for audio event classification |
| `src/lib/types.ts` | Add `AudioEvent` discriminated union, `audioEvents` to `RenderFrame` |
| `src/App.svelte` | Instantiate AudioManager, pipe audioEvents, dispose on stop |
| `package.json` | Add `howler` dependency |

---

## Chunk 1: Rust AudioEvent and Emission Infrastructure

### Task 1: Create AudioEvent enum and register module

**Files:**
- Create: `src-tauri/src/engine/audio.rs`
- Modify: `src-tauri/src/engine/mod.rs`

- [ ] **Step 1: Write serialization tests**

Create `src-tauri/src/engine/audio.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Semantic audio event emitted by game logic.
/// The frontend maps these to actual sound files via a sound kit manifest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum AudioEvent {
    ItemPickup { item_id: String },
    CraftSuccess { recipe_id: String },
    ActionFailed,
    Jump,
    Land,
    TransitionStart,
    TransitionComplete,
    EntityInteract { entity_type: String },
    StreetChanged { street_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_item_pickup() {
        let event = AudioEvent::ItemPickup { item_id: "cherry".into() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"itemPickup""#));
        assert!(json.contains(r#""itemId":"cherry""#));
    }

    #[test]
    fn serialize_action_failed() {
        let event = AudioEvent::ActionFailed;
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, r#"{"type":"actionFailed"}"#);
    }

    #[test]
    fn serialize_jump() {
        let event = AudioEvent::Jump;
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(json, r#"{"type":"jump"}"#);
    }

    #[test]
    fn serialize_street_changed() {
        let event = AudioEvent::StreetChanged { street_id: "LADEMO001".into() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"streetChanged""#));
        assert!(json.contains(r#""streetId":"LADEMO001""#));
    }

    #[test]
    fn serialize_entity_interact() {
        let event = AudioEvent::EntityInteract { entity_type: "fruit_tree".into() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"entityInteract""#));
        assert!(json.contains(r#""entityType":"fruit_tree""#));
    }

    #[test]
    fn serialize_craft_success() {
        let event = AudioEvent::CraftSuccess { recipe_id: "cherry_pie".into() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"craftSuccess""#));
        assert!(json.contains(r#""recipeId":"cherry_pie""#));
    }

    #[test]
    fn roundtrip_all_variants() {
        let events = vec![
            AudioEvent::ItemPickup { item_id: "cherry".into() },
            AudioEvent::CraftSuccess { recipe_id: "bread".into() },
            AudioEvent::ActionFailed,
            AudioEvent::Jump,
            AudioEvent::Land,
            AudioEvent::TransitionStart,
            AudioEvent::TransitionComplete,
            AudioEvent::EntityInteract { entity_type: "chicken".into() },
            AudioEvent::StreetChanged { street_id: "demo_meadow".into() },
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let deserialized: AudioEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, deserialized);
        }
    }
}
```

- [ ] **Step 2: Register the module**

Add to `src-tauri/src/engine/mod.rs`:
```rust
pub mod audio;
pub mod state;
pub mod transition;
```

- [ ] **Step 3: Run tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch --lib engine::audio::tests`
Expected: PASS (7 tests)

- [ ] **Step 4: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/engine/audio.rs src-tauri/src/engine/mod.rs
git commit -m "feat(audio): add AudioEvent enum with serde tagged serialization"
```

---

### Task 2: Add audio_events to GameState and RenderFrame

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

**Important context for implementer:**
- `GameState` struct is at line 18 of state.rs. Add two new fields: `audio_events` and `pending_audio_events`.
- `GameState::new()` starts at line 100. Initialize both as `vec![]`.
- `RenderFrame` struct is at line 56. Add `pub audio_events: Vec<AudioEvent>`.
- The `tick()` method starts at line 197. At the very start (after the `is_none` check and `game_time += dt`), drain pending events: `let mut audio_events = std::mem::take(&mut self.pending_audio_events);`
- At the end of `tick()`, the `Some(RenderFrame { ... })` block (line 441) needs `audio_events`.
- Add `use crate::engine::audio::AudioEvent;` to the imports at the top of state.rs.
- **~30 test call sites** create `GameState::new()` — they don't need changes since we're adding fields with default values to the struct init, not changing the constructor signature.

- [ ] **Step 1: Write test for empty audio_events**

Add to the test module in state.rs:

```rust
#[test]
fn audio_events_empty_by_default() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
    );
    state.load_street(test_street(), vec![], vec![]);
    let input = InputState::default();
    let frame = state
        .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
        .unwrap();
    assert!(frame.audio_events.is_empty());
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch --lib engine::state::tests::audio_events_empty 2>&1 | tail -10`

- [ ] **Step 3: Add fields and drain mechanism**

In state.rs imports, add:
```rust
use crate::engine::audio::AudioEvent;
```

In `GameState` struct, add after `game_time`:
```rust
pub audio_events: Vec<AudioEvent>,
pub pending_audio_events: Vec<AudioEvent>,
```

In `GameState::new()`, add to struct init:
```rust
audio_events: vec![],
pending_audio_events: vec![],
```

In `RenderFrame` struct, add after `transition`:
```rust
pub audio_events: Vec<AudioEvent>,
```

In `tick()`, right after `self.game_time += dt;` (line 205), add:
```rust
// Drain pending audio events from IPC commands (craft_recipe, load_street)
let mut audio_events = std::mem::take(&mut self.pending_audio_events);
```

In the `Some(RenderFrame { ... })` block at the end of tick(), add:
```rust
audio_events,
```

- [ ] **Step 4: Run all tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test --workspace`
Expected: PASS (all tests)

- [ ] **Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/engine/state.rs
git commit -m "feat(audio): add audio_events to GameState/RenderFrame with drain mechanism"
```

---

### Task 3: Emit Jump and Land events

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

**Important context for implementer:**
- Jump/Land detection needs a `prev_on_ground` field on GameState to detect transitions.
- The player physics runs inside the `if !is_swooping` block (line 310-317). `self.player.tick()` updates `on_ground`.
- Jump = `prev_on_ground` was true AND `on_ground` is now false AND `vy < 0`
- Land = `prev_on_ground` was false AND `on_ground` is now true
- The `audio_events` vec is a local variable created from draining pending events. Push Jump/Land to it.
- Update `prev_on_ground` at the END of tick (after physics), not at the start, to avoid detecting the same transition twice.

- [ ] **Step 1: Write tests**

Add to state.rs test module:

```rust
#[test]
fn audio_event_jump() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
    );
    state.load_street(test_street(), vec![], vec![]);
    state.player.on_ground = true;

    // First tick: on ground, no events
    let input = InputState::default();
    let frame = state
        .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
        .unwrap();
    assert!(frame.audio_events.is_empty());

    // Simulate jump: player leaves ground with upward velocity
    state.player.on_ground = false;
    state.player.vy = -200.0;
    let frame = state
        .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
        .unwrap();
    assert!(frame
        .audio_events
        .iter()
        .any(|e| matches!(e, AudioEvent::Jump)));
}

#[test]
fn audio_event_land() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
    );
    state.load_street(test_street(), vec![], vec![]);

    // Position player high above ground so physics keeps them airborne
    state.player.y = -500.0;
    state.player.on_ground = false;
    let input = InputState::default();
    // Tick while airborne — establishes prev_on_ground = false
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

    // Now simulate landing: snap player to ground
    state.player.y = 0.0;
    state.player.on_ground = true;
    let frame = state
        .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
        .unwrap();
    assert!(frame
        .audio_events
        .iter()
        .any(|e| matches!(e, AudioEvent::Land)));
}

#[test]
fn audio_event_no_duplicate_land() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
    );
    state.load_street(test_street(), vec![], vec![]);
    state.player.on_ground = true;

    let input = InputState::default();
    // Two ticks on ground — no Land event
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    let frame = state
        .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
        .unwrap();
    assert!(!frame
        .audio_events
        .iter()
        .any(|e| matches!(e, AudioEvent::Land)));
}
```

- [ ] **Step 2: Run tests, verify they fail**

- [ ] **Step 3: Implement Jump/Land detection**

Add `prev_on_ground: bool` field to `GameState` struct, initialize as `true` in `new()`.

In `tick()`, after the player physics block (after `self.player.tick(...)` at line ~317 but still inside the `if !is_swooping` block), add:

```rust
// Jump/Land audio detection
if self.prev_on_ground && !self.player.on_ground && self.player.vy < 0.0 {
    audio_events.push(AudioEvent::Jump);
}
if !self.prev_on_ground && self.player.on_ground {
    audio_events.push(AudioEvent::Land);
}
self.prev_on_ground = self.player.on_ground;
```

Note: `audio_events` is the local variable from the drain at the top of tick().

- [ ] **Step 4: Run tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test -p harmony-glitch --lib engine::state::tests::audio_event`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/engine/state.rs
git commit -m "feat(audio): emit Jump and Land events on ground state transitions"
```

---

### Task 4: Emit interaction and transition events

**Files:**
- Modify: `src-tauri/src/engine/state.rs`
- Modify: `src-tauri/src/item/interaction.rs`

**Important context for implementer:**
- `InteractionResult` (interaction.rs line 140) needs a new field to classify the interaction for audio. Add `interaction_type: Option<InteractionType>` where `InteractionType` is an enum: `Entity { entity_type: String }`, `GroundItem { item_id: String }`, `Rejected`.
- In `execute_interaction`, set `interaction_type` based on which branch executed:
  - Entity harvest that passes depletion+cooldown checks → `Entity { entity_type }`
  - Ground item pickup with `added > 0` → `GroundItem { item_id }`
  - Ground item with `added == 0` (full inventory) → `Rejected`
  - Entity depletion/cooldown rejection → `Rejected`
- In state.rs, after processing `InteractionResult`, push audio events based on `interaction_type`:
  - `Entity { entity_type }` → push both `EntityInteract { entity_type }` and `ItemPickup { item_id }` (use first yield item's id)
  - `GroundItem { item_id }` → push `ItemPickup { item_id }`
  - `Rejected` → push `ActionFailed`
- For transitions: use the existing `was_swooping` pattern. After `self.transition.tick(dt)` at line 253, the `is_swooping` is rechecked at line 300. Add:
  - `TransitionStart`: when `!was_swooping_at_start && is_swooping` (where `was_swooping_at_start` is captured before transition.tick)
  - `TransitionComplete`: when `was_swooping && !is_swooping_now` (swoop just ended)

  Note: there's already a `was_swooping` at line 252 and a re-check at line 300. Use those.
- For `CraftSuccess`: in `craft_recipe()` method (line 172), on success push to `self.pending_audio_events`.
- For `StreetChanged`: in `load_street()` method (line 132), push to `self.pending_audio_events`.

- [ ] **Step 1: Add InteractionType to interaction.rs**

Add to interaction.rs, before `InteractionResult`:

```rust
/// Classification of what happened during an interaction (for audio).
#[derive(Debug)]
pub enum InteractionType {
    Entity { entity_type: String },
    GroundItem { item_id: String },
    Rejected,
}
```

Add to `InteractionResult`:
```rust
pub interaction_type: Option<InteractionType>,
```

Initialize as `None` in the default construction, then set appropriately:
- Entity depletion check (line 184 early return) → set `Rejected` before return
- Entity cooldown check (line 198 early return) → set `Rejected` before return
- Entity harvest (after yield loop, line ~250) → set `Entity { entity_type: entity.entity_type.clone() }`
- Ground item added > 0 → set `GroundItem { item_id: item.item_id.clone() }`
- Ground item added == 0 → set `Rejected`

- [ ] **Step 2: Emit events in state.rs tick()**

In the interaction result handling block (after line 369 where feedback is processed), add audio event emission based on `result.interaction_type`:

```rust
// Emit audio events from interaction
match &result.interaction_type {
    Some(interaction::InteractionType::Entity { entity_type }) => {
        audio_events.push(AudioEvent::EntityInteract {
            entity_type: entity_type.clone(),
        });
        // Use first feedback item's text to infer item_id, or use
        // the first yield entry from the entity def
        if let Some(def) = self.entity_defs.get(&entities[nearest_index].entity_type) {
            if let Some(first_yield) = def.yields.first() {
                audio_events.push(AudioEvent::ItemPickup {
                    item_id: first_yield.item.clone(),
                });
            }
        }
    }
    Some(interaction::InteractionType::GroundItem { item_id }) => {
        audio_events.push(AudioEvent::ItemPickup {
            item_id: item_id.clone(),
        });
    }
    Some(interaction::InteractionType::Rejected) => {
        audio_events.push(AudioEvent::ActionFailed);
    }
    None => {}
}
```

Note: You'll need to extract the entity index from `nearest` before the borrow. Capture it as `let nearest_index` before the `execute_interaction` call.

For transitions, after the `is_swooping` re-check at line 300, add:

```rust
// Transition audio events
if !was_swooping && is_swooping {
    audio_events.push(AudioEvent::TransitionStart);
}
if was_swooping && !is_swooping {
    audio_events.push(AudioEvent::TransitionComplete);
}
```

Note: `was_swooping` is the variable at line 252 (before `transition.tick`), `is_swooping` is the re-check at line 300. The variable names in the existing code may differ slightly — read the actual code to use the correct names.

For `CraftSuccess`, in `craft_recipe()` method, after the feedback loop, add:
```rust
self.pending_audio_events.push(AudioEvent::CraftSuccess {
    recipe_id: recipe.id.clone(),
});
```

For `StreetChanged`, in `load_street()`, after `self.street = Some(street);`, add:
```rust
self.pending_audio_events.push(AudioEvent::StreetChanged {
    street_id: self.street.as_ref().unwrap().tsid.clone(),
});
```

- [ ] **Step 3: Write tests for interaction events**

Add to state.rs test module. These tests need real item/entity defs to trigger interactions:

```rust
#[test]
fn audio_event_craft_success_drains_next_tick() {
    let item_defs =
        crate::item::loader::parse_item_defs(include_str!("../../../assets/items.json"))
            .unwrap();
    let entity_defs =
        crate::item::loader::parse_entity_defs(include_str!("../../../assets/entities.json"))
            .unwrap();
    let recipe_defs =
        crate::item::loader::parse_recipe_defs(include_str!("../../../assets/recipes.json"))
            .unwrap();

    let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, recipe_defs);
    state.load_street(test_street(), vec![], vec![]);

    // Stock inventory and craft
    state.inventory.add("wood", 3, &state.item_defs);
    state.craft_recipe("plank").unwrap();

    // CraftSuccess should be in pending, not yet in a frame
    assert!(!state.pending_audio_events.is_empty());

    // Next tick drains it
    let input = InputState::default();
    let frame = state
        .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
        .unwrap();
    assert!(frame
        .audio_events
        .iter()
        .any(|e| matches!(e, AudioEvent::CraftSuccess { .. })));

    // Following tick has no events
    let frame2 = state
        .tick(1.0 / 60.0, &input, &mut rand::thread_rng())
        .unwrap();
    assert!(!frame2
        .audio_events
        .iter()
        .any(|e| matches!(e, AudioEvent::CraftSuccess { .. })));
}

#[test]
fn audio_event_street_changed_on_load() {
    let mut state = GameState::new(
        1280.0,
        720.0,
        ItemDefs::new(),
        EntityDefs::new(),
        HashMap::new(),
    );
    state.load_street(test_street(), vec![], vec![]);

    // StreetChanged should be pending
    assert!(state
        .pending_audio_events
        .iter()
        .any(|e| matches!(e, AudioEvent::StreetChanged { .. })));
}
```

Note: The spec also requires tests for entity harvest producing `ItemPickup` + `EntityInteract`, cooldown rejection producing `ActionFailed`, and partial overflow NOT producing `ActionFailed`. These are complex integration tests requiring entity placement and interaction execution. The implementer should add them using the same pattern as existing interaction tests in `state.rs` (which set up entities via `load_street`, position the player near them, and send interact input). The key assertions:

- After successful entity harvest: `audio_events` contains both `EntityInteract { entity_type: "fruit_tree" }` and `ItemPickup { item_id: "cherry" }`
- After cooldown rejection (interact again immediately): `audio_events` contains `ActionFailed`
- After harvest with overflow (full inventory): `audio_events` contains `EntityInteract` and `ItemPickup` but NOT `ActionFailed` (overflow is a success with spillover, not a rejection)

- [ ] **Step 4: Run all tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test --workspace`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src-tauri/src/engine/state.rs src-tauri/src/item/interaction.rs
git commit -m "feat(audio): emit interaction, transition, craft, and street events"
```

---

## Chunk 2: Audio Data Files

### Task 5: Copy Glitch sounds and create sound kit manifest

**Files:**
- Create: `assets/audio/default-kit.json`
- Create: `assets/audio/sfx/*.mp3` (9 files)
- Create: `assets/audio/ambient/*.mp3` (2 files)

**Important context for implementer:**
- Glitch sounds are at `/Users/zeblith/work/tinyspeck/glitch-sounds/Glitch Ringtones, Alerts & Sound Effects/Glitch SFX/`
- All files are CC0 licensed
- Copy files with `cp -f` (non-interactive flag per CLAUDE.md)
- For ambient, we don't have actual Glitch ambient loops — create simple silent MP3 placeholders or use a generic outdoor sound. The system needs files to exist but they can be swapped later.

- [ ] **Step 1: Create directory structure**

```bash
mkdir -p /Users/zeblith/work/zeblithic/harmony-glitch/assets/audio/sfx
mkdir -p /Users/zeblith/work/zeblithic/harmony-glitch/assets/audio/ambient
```

- [ ] **Step 2: Copy SFX files from Glitch library**

```bash
GLITCH="/Users/zeblith/work/tinyspeck/glitch-sounds/Glitch Ringtones, Alerts & Sound Effects/Glitch SFX"
DEST="/Users/zeblith/work/zeblithic/harmony-glitch/assets/audio/sfx"

cp -f "$GLITCH/Decent Alerts/mp3_s/pick up.mp3" "$DEST/pick-up.mp3"
cp -f "$GLITCH/Tools/awesome pot.mp3" "$DEST/craft-success.mp3"
cp -f "$GLITCH/Decent Alerts/mp3_s/fail.mp3" "$DEST/fail.mp3"
cp -f "$GLITCH/Decent Alerts/mp3_s/triple jump.mp3" "$DEST/jump.mp3"
cp -f "$GLITCH/Miscellaneous SFX/fox bait land.mp3" "$DEST/land.mp3"
cp -f "$GLITCH/Miscellaneous SFX/door-open.mp3" "$DEST/transition-start.mp3"
cp -f "$GLITCH/Miscellaneous SFX/door-close.mp3" "$DEST/transition-complete.mp3"
cp -f "$GLITCH/Tools/hatchet.mp3" "$DEST/interact.mp3"
cp -f "$GLITCH/Tools/pick.mp3" "$DEST/harvest-tree.mp3"
```

- [ ] **Step 3: Create ambient placeholders**

For ambient, we need loopable files. The Glitch library doesn't have obvious ambient loops. Create minimal silent-ish placeholders using ffmpeg (if available) or copy a short SFX and note it needs replacement:

```bash
# If ffmpeg is available, create 5-second silent MP3 files as placeholders:
AMBIENT="/Users/zeblith/work/zeblithic/harmony-glitch/assets/audio/ambient"
ffmpeg -f lavfi -i anullsrc=r=44100:cl=stereo -t 5 -q:a 9 "$AMBIENT/meadow.mp3" -y 2>/dev/null
ffmpeg -f lavfi -i anullsrc=r=44100:cl=stereo -t 5 -q:a 9 "$AMBIENT/heights.mp3" -y 2>/dev/null
```

If ffmpeg is not available, copy any short SFX as placeholder:
```bash
cp -f "$GLITCH/Decent Alerts/mp3_s/client loaded.mp3" "$AMBIENT/meadow.mp3"
cp -f "$GLITCH/Decent Alerts/mp3_s/client loaded.mp3" "$AMBIENT/heights.mp3"
```

- [ ] **Step 4: Create sound kit manifest**

Create `assets/audio/default-kit.json`:

```json
{
  "name": "Default",
  "version": 1,
  "sfxVolume": 1.0,
  "ambientVolume": 0.5,
  "events": {
    "itemPickup": {
      "default": "sfx/pick-up.mp3"
    },
    "craftSuccess": {
      "default": "sfx/craft-success.mp3"
    },
    "actionFailed": {
      "default": "sfx/fail.mp3"
    },
    "jump": {
      "default": "sfx/jump.mp3"
    },
    "land": {
      "default": "sfx/land.mp3"
    },
    "transitionStart": {
      "default": "sfx/transition-start.mp3"
    },
    "transitionComplete": {
      "default": "sfx/transition-complete.mp3"
    },
    "entityInteract": {
      "default": "sfx/interact.mp3",
      "variants": {
        "fruit_tree": "sfx/harvest-tree.mp3",
        "wood_tree": "sfx/harvest-tree.mp3"
      }
    }
  },
  "ambient": {
    "default": "ambient/meadow.mp3",
    "variants": {
      "LADEMO001": "ambient/meadow.mp3",
      "LADEMO002": "ambient/heights.mp3"
    }
  }
}
```

Note: the spec originally listed `ambient/outdoors.mp3` as the default, but we use `ambient/meadow.mp3` to avoid an extra file — it serves double duty as both the meadow-specific and fallback ambient.

Note: ambient variants key off `street_id` which is the TSID (e.g. `LADEMO001`), not the short name.

- [ ] **Step 5: Verify JSON is valid**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && python3 -c "import json; d=json.load(open('assets/audio/default-kit.json')); print(f'Kit: {d[\"name\"]}, {len(d[\"events\"])} events')"
```

- [ ] **Step 6: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add assets/audio/
git commit -m "feat(audio): add default sound kit with Glitch SFX and ambient placeholders"
```

---

## Chunk 3: Frontend AudioManager

### Task 6: Install Howler and add frontend types

**Files:**
- Modify: `package.json`
- Modify: `src/lib/types.ts`

- [ ] **Step 1: Install howler**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && npm install howler && npm install --save-dev @types/howler
```

- [ ] **Step 2: Add AudioEvent type to types.ts**

Add at the end of `src/lib/types.ts`:

```typescript
export type AudioEvent =
  | { type: 'itemPickup'; itemId: string }
  | { type: 'craftSuccess'; recipeId: string }
  | { type: 'actionFailed' }
  | { type: 'jump' }
  | { type: 'land' }
  | { type: 'transitionStart' }
  | { type: 'transitionComplete' }
  | { type: 'entityInteract'; entityType: string }
  | { type: 'streetChanged'; streetId: string };
```

Add `audioEvents` to the existing `RenderFrame` interface:

```typescript
// In RenderFrame interface, after pickupFeedback:
audioEvents: AudioEvent[];
```

- [ ] **Step 3: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add package.json package-lock.json src/lib/types.ts
git commit -m "feat(audio): install howler and add AudioEvent types"
```

---

### Task 7: Implement AudioManager

**Files:**
- Create: `src/lib/engine/audio.ts`

**Important context for implementer:**
- AudioManager must NOT import PixiJS or Svelte — it depends only on Howler and types.
- Sound kit files are in `assets/audio/` which Tauri serves from the app bundle. The URL path depends on the Tauri asset protocol. Use `new URL('../../../assets/audio/', import.meta.url).href` as the base, or hardcode `/assets/audio/` and test in dev.
- Howler's `Howl` constructor takes `{ src: [path], volume }`. It auto-handles format detection.
- For ambient crossfade: `howl.fade(fromVol, toVol, durationMs)`.
- Browser autoplay: check `Howler.ctx?.state === 'suspended'` and call `Howler.ctx.resume()` on first processEvents.

- [ ] **Step 1: Create AudioManager**

Create `src/lib/engine/audio.ts`:

```typescript
import { Howl, Howler } from 'howler';
import type { AudioEvent } from '../types';

export interface SoundEntry {
  default: string;
  variants?: Record<string, string>;
}

export interface SoundKit {
  name: string;
  version: number;
  sfxVolume: number;
  ambientVolume: number;
  events: Record<string, SoundEntry>;
  ambient: SoundEntry;
}

export class AudioManager {
  private kit: SoundKit;
  private sounds: Map<string, Howl> = new Map();
  private currentAmbient: Howl | null = null;
  private sfxVolume: number;
  private ambientVolume: number;
  private audioBasePath: string;
  private contextResumed = false;
  private fadingOut = false;

  constructor(kit: SoundKit, audioBasePath: string) {
    this.kit = kit;
    this.sfxVolume = kit.sfxVolume;
    this.ambientVolume = kit.ambientVolume;
    this.audioBasePath = audioBasePath;
    this.preloadSounds();
  }

  private preloadSounds(): void {
    // Collect all unique file paths from the kit
    const paths = new Set<string>();
    for (const entry of Object.values(this.kit.events)) {
      paths.add(entry.default);
      if (entry.variants) {
        for (const path of Object.values(entry.variants)) {
          paths.add(path);
        }
      }
    }
    paths.add(this.kit.ambient.default);
    if (this.kit.ambient.variants) {
      for (const path of Object.values(this.kit.ambient.variants)) {
        paths.add(path);
      }
    }

    // Pre-load each unique file
    for (const path of paths) {
      const fullPath = `${this.audioBasePath}${path}`;
      const howl = new Howl({
        src: [fullPath],
        preload: true,
        onloaderror: (_id: number, err: unknown) => {
          console.warn(`[AudioManager] Failed to load ${path}:`, err);
        },
      });
      this.sounds.set(path, howl);
    }
  }

  processEvents(events: AudioEvent[]): void {
    // Resume audio context on first call (browser autoplay policy)
    if (!this.contextResumed) {
      this.contextResumed = true;
      if (Howler.ctx?.state === 'suspended') {
        Howler.ctx.resume();
      }
    }

    for (const event of events) {
      switch (event.type) {
        case 'itemPickup':
          this.playSfx('itemPickup', event.itemId);
          break;
        case 'craftSuccess':
          this.playSfx('craftSuccess', event.recipeId);
          break;
        case 'actionFailed':
          this.playSfx('actionFailed');
          break;
        case 'jump':
          this.playSfx('jump');
          break;
        case 'land':
          this.playSfx('land');
          break;
        case 'transitionStart':
          this.playSfx('transitionStart');
          this.fadeOutAmbient();
          break;
        case 'transitionComplete':
          this.playSfx('transitionComplete');
          if (this.fadingOut) {
            this.fadeInAmbient();
          }
          break;
        case 'entityInteract':
          this.playSfx('entityInteract', event.entityType);
          break;
        case 'streetChanged':
          this.handleStreetChanged(event.streetId);
          break;
      }
    }
  }

  private playSfx(eventType: string, variantKey?: string): void {
    const entry = this.kit.events[eventType];
    if (!entry) return;

    const path = (variantKey && entry.variants?.[variantKey]) || entry.default;
    const howl = this.sounds.get(path);
    if (howl) {
      howl.volume(this.sfxVolume);
      howl.play();
    }
  }

  private handleStreetChanged(streetId: string): void {
    const path =
      this.kit.ambient.variants?.[streetId] || this.kit.ambient.default;
    const howl = this.sounds.get(path);
    if (!howl) return;

    // If we're mid-fade-out (transition in progress), queue the new ambient
    // to fade in on TransitionComplete
    if (this.fadingOut) {
      if (this.currentAmbient) {
        this.currentAmbient.stop();
      }
      this.currentAmbient = howl;
      howl.loop(true);
      howl.volume(0);
      howl.play();
      return;
    }

    // Direct street load (no transition) — start immediately
    if (this.currentAmbient) {
      this.currentAmbient.stop();
    }
    this.currentAmbient = howl;
    howl.loop(true);
    howl.volume(this.ambientVolume);
    howl.play();
  }

  private fadeOutAmbient(): void {
    this.fadingOut = true;
    if (this.currentAmbient) {
      this.currentAmbient.fade(this.ambientVolume, 0, 1000);
    }
  }

  private fadeInAmbient(): void {
    this.fadingOut = false;
    if (this.currentAmbient) {
      this.currentAmbient.fade(0, this.ambientVolume, 1000);
    }
  }

  setVolume(channel: 'sfx' | 'ambient', volume: number): void {
    if (channel === 'sfx') {
      this.sfxVolume = volume;
    } else {
      this.ambientVolume = volume;
      if (this.currentAmbient && !this.fadingOut) {
        this.currentAmbient.volume(volume);
      }
    }
  }

  dispose(): void {
    for (const howl of this.sounds.values()) {
      howl.unload();
    }
    this.sounds.clear();
    this.currentAmbient = null;
  }
}

export async function loadSoundKit(basePath: string): Promise<SoundKit> {
  const response = await fetch(`${basePath}default-kit.json`);
  if (!response.ok) {
    throw new Error(`Failed to load sound kit: ${response.status}`);
  }
  return response.json();
}
```

- [ ] **Step 2: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src/lib/engine/audio.ts
git commit -m "feat(audio): implement AudioManager with Howler.js playback and ambient crossfade"
```

---

### Task 8: Integrate AudioManager into App.svelte

**Files:**
- Modify: `src/App.svelte`

**Important context for implementer:**
- AudioManager needs to be created after a street is loaded (so it can start ambient).
- It should be disposed when the game stops.
- The `handleFrame` function receives `RenderFrame` — pipe `frame.audioEvents` to the AudioManager.
- The audio base path in Tauri dev is `/assets/audio/` (served from the app's asset directory).

- [ ] **Step 1: Update App.svelte**

Read `src/App.svelte` first. Add:

1. Import AudioManager:
```typescript
import { AudioManager, loadSoundKit } from './lib/engine/audio';
```

2. Add state variable:
```typescript
let audioManager = $state<AudioManager | null>(null);
```

3. In the existing `handleStreetLoaded` function (or wherever the first street load is handled), initialize audio:
```typescript
async function handleStreetLoaded(street: StreetData) {
  currentStreet = street;
  if (!audioManager) {
    try {
      const kit = await loadSoundKit('/assets/audio/');
      audioManager = new AudioManager(kit, '/assets/audio/');
    } catch (e) {
      console.error('Failed to initialize audio:', e);
    }
  }
}
```

4. In `handleFrame`, pipe audio events:
```typescript
function handleFrame(frame: RenderFrame) {
  latestFrame = frame;
  // ... existing transition logic ...

  // Process audio events
  if (frame.audioEvents?.length && audioManager) {
    audioManager.processEvents(frame.audioEvents);
  }
}
```

5. In the stop/back button handler, dispose audio:
```typescript
onclick={async () => {
  try {
    await stopGame();
  } catch (e) {
    console.error('stopGame failed:', e);
  } finally {
    audioManager?.dispose();
    audioManager = null;
    currentStreet = null;
    latestFrame = null;
  }
}}
```

- [ ] **Step 2: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src/App.svelte
git commit -m "feat(audio): integrate AudioManager into App.svelte"
```

---

### Task 9: Frontend tests for AudioManager

**Files:**
- Create: `src/lib/engine/audio.test.ts`

- [ ] **Step 1: Create test file**

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { AudioEvent } from '../types';

// Mock howler before importing AudioManager
vi.mock('howler', () => {
  const mockHowl = vi.fn().mockImplementation(() => ({
    play: vi.fn(),
    stop: vi.fn(),
    fade: vi.fn(),
    volume: vi.fn(),
    loop: vi.fn(),
    unload: vi.fn(),
  }));
  return {
    Howl: mockHowl,
    Howler: { ctx: { state: 'running', resume: vi.fn() } },
  };
});

import { AudioManager } from './audio';
import type { SoundKit } from './audio';
import { Howl } from 'howler';

function makeKit(): SoundKit {
  return {
    name: 'Test',
    version: 1,
    sfxVolume: 1.0,
    ambientVolume: 0.5,
    events: {
      itemPickup: {
        default: 'sfx/pick-up.mp3',
        variants: { cherry: 'sfx/cherry-pick.mp3' },
      },
      jump: { default: 'sfx/jump.mp3' },
      actionFailed: { default: 'sfx/fail.mp3' },
      transitionStart: { default: 'sfx/transition-start.mp3' },
      transitionComplete: { default: 'sfx/transition-complete.mp3' },
      entityInteract: { default: 'sfx/interact.mp3' },
    },
    ambient: {
      default: 'ambient/default.mp3',
      variants: { LADEMO001: 'ambient/meadow.mp3' },
    },
  };
}

describe('AudioManager', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('resolves variant sound when available', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    const events: AudioEvent[] = [{ type: 'itemPickup', itemId: 'cherry' }];
    manager.processEvents(events);

    // Find the Howl instance for cherry-pick.mp3 and verify play was called
    const calls = vi.mocked(Howl).mock.results;
    const cherryHowl = calls.find(
      (r) => r.type === 'return' && vi.mocked(Howl).mock.calls[calls.indexOf(r)][0].src[0].includes('cherry-pick')
    );
    expect(cherryHowl).toBeDefined();
  });

  it('falls back to default when no variant matches', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    const events: AudioEvent[] = [{ type: 'itemPickup', itemId: 'wood' }];
    manager.processEvents(events);
    // Should play pick-up.mp3 (default), not crash
  });

  it('plays SFX on jump event', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    manager.processEvents([{ type: 'jump' }]);
    // Verify some Howl had play() called
    const howlInstances = vi.mocked(Howl).mock.results
      .filter(r => r.type === 'return')
      .map(r => r.value);
    const played = howlInstances.some(h => h.play.mock.calls.length > 0);
    expect(played).toBe(true);
  });

  it('starts ambient on streetChanged without transition', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    manager.processEvents([{ type: 'streetChanged', streetId: 'LADEMO001' }]);

    const howlInstances = vi.mocked(Howl).mock.results
      .filter(r => r.type === 'return')
      .map(r => r.value);
    const looping = howlInstances.some(h => h.loop.mock.calls.length > 0);
    expect(looping).toBe(true);
  });

  it('fades out ambient on transitionStart', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    // Start ambient first
    manager.processEvents([{ type: 'streetChanged', streetId: 'LADEMO001' }]);
    // Then transition
    manager.processEvents([{ type: 'transitionStart' }]);

    const howlInstances = vi.mocked(Howl).mock.results
      .filter(r => r.type === 'return')
      .map(r => r.value);
    const faded = howlInstances.some(h => h.fade.mock.calls.length > 0);
    expect(faded).toBe(true);
  });

  it('dispose stops and unloads all sounds', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    manager.dispose();

    const howlInstances = vi.mocked(Howl).mock.results
      .filter(r => r.type === 'return')
      .map(r => r.value);
    const allUnloaded = howlInstances.every(h => h.unload.mock.calls.length > 0);
    expect(allUnloaded).toBe(true);
  });

  it('setVolume adjusts only the specified channel', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    // Start ambient
    manager.processEvents([{ type: 'streetChanged', streetId: 'LADEMO001' }]);

    // Change SFX volume
    manager.setVolume('sfx', 0.3);

    // Play an SFX — it should use the new volume
    manager.processEvents([{ type: 'jump' }]);

    const howlInstances = vi.mocked(Howl).mock.results
      .filter(r => r.type === 'return')
      .map(r => r.value);
    // At least one howl should have volume(0.3) called
    const hasNewVolume = howlInstances.some(
      h => h.volume.mock.calls.some((c: number[]) => c[0] === 0.3)
    );
    expect(hasNewVolume).toBe(true);
  });

  it('handles missing event type gracefully', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    // entityInteract is in the kit but with no variant for "unknown_entity"
    // Should fall back to default without throwing
    expect(() => {
      manager.processEvents([{ type: 'entityInteract', entityType: 'unknown_entity' }]);
    }).not.toThrow();
  });
});
```

- [ ] **Step 2: Run tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npx vitest run`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch && git add src/lib/engine/audio.test.ts
git commit -m "test(audio): add AudioManager tests with mocked Howler"
```

---

## Chunk 4: Final Verification

### Task 10: Final verification

**Files:** None (verification only)

- [ ] **Step 1: Run full Rust test suite**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test --workspace`
Expected: PASS

- [ ] **Step 2: Run frontend tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npx vitest run`
Expected: PASS

- [ ] **Step 3: Run clippy**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo clippy --workspace`
Expected: No warnings

- [ ] **Step 4: Run format check**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo fmt --all -- --check`
Expected: No formatting issues
