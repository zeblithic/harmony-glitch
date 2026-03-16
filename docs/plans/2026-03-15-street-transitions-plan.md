# Street Transitions Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire together existing TransitionState, swoop renderer, and App.svelte handler so players can walk between demo_meadow and demo_heights seamlessly.

**Architecture:** Integration of existing pure state machine (`TransitionState`) into `GameState.tick()`. Player crossing a signpost X coordinate triggers a swoop animation. Frontend pre-loads the target street during the swoop, signals readiness, and the player spawns at the return signpost on the new street.

**Tech Stack:** Rust (Tauri v2), Svelte 5, TypeScript, PixiJS v8

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `src-tauri/src/engine/state.rs` | GameState, RenderFrame, tick loop | Modify: add TransitionState, TransitionFrame, TSID map, input freeze, swoop trigger, Complete handler |
| `src-tauri/src/lib.rs` | Tauri commands, game loop | Modify: add `street_transition_ready` command, register it |
| `src/App.svelte` | Top-level app, transition orchestration | Modify: trigger loadStreet on first frame.transition, call streetTransitionReady |
| `src/lib/ipc.ts` | IPC wrappers | Modify: add `streetTransitionReady()` |

No new files. No changes to `transition.rs`, `renderer.ts`, `types.ts`, or `movement.rs`.

**Note:** Tasks MUST be executed in order — later tasks depend on fields and imports added by earlier tasks. The TSID→name map is hardcoded for Phase A's two demo streets; this will be dynamically populated when more streets are added.

**Important:** `load_street()` must NOT reset `transition` or `transition_origin_tsid` fields on `GameState`. These fields track in-flight transition state that spans across the street swap.

---

## Chunk 1: Rust Backend Integration

### Task 1: Add TransitionFrame struct and transition field to RenderFrame

**Files:**
- Modify: `src-tauri/src/engine/state.rs:1-45`

- [ ] **Step 1: Write the failing test**

Add to the existing `tests` module in `state.rs`:

```rust
#[test]
fn render_frame_has_no_transition_by_default() {
    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
    state.load_street(test_street(), vec![]);
    let input = InputState::default();
    let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng()).unwrap();
    assert!(frame.transition.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch render_frame_has_no_transition -- --nocapture`
Expected: FAIL — `transition` field doesn't exist on `RenderFrame`

- [ ] **Step 3: Add TransitionFrame struct and transition field**

Add the `TransitionFrame` struct after `CameraFrame` (around line 64):

```rust
/// Transition animation data sent to the frontend during a swoop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionFrame {
    pub progress: f64,
    pub direction: TransitionDirection,
    pub to_street: String,
}
```

Add import at top of file:

```rust
use crate::engine::transition::TransitionDirection;
```

Add field to `RenderFrame`:

```rust
pub transition: Option<TransitionFrame>,
```

Add `transition: None` to the `RenderFrame` construction in `tick()` (around line 258):

```rust
pickup_feedback: self.pickup_feedback.clone(),
transition: None,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test -p harmony-glitch render_frame_has_no_transition -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full test suite**

Run: `cd src-tauri && cargo test --workspace`
Expected: All 136+ tests pass (existing 128 + 8 transition.rs)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: add TransitionFrame struct and transition field to RenderFrame"
```

---

### Task 2: Add TransitionState and TSID map to GameState

**Files:**
- Modify: `src-tauri/src/engine/state.rs:14-101`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn game_state_has_transition_state() {
    let state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
    // TransitionState starts at None phase
    assert_eq!(state.transition.phase, TransitionPhase::None);
}
```

Add import to test module:

```rust
use crate::engine::transition::TransitionPhase;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch game_state_has_transition_state -- --nocapture`
Expected: FAIL — `transition` field doesn't exist on `GameState`

- [ ] **Step 3: Add TransitionState and TSID map fields**

Add import at the top of `state.rs`:

```rust
use crate::engine::transition::{TransitionDirection, TransitionState};
```

(Update the existing `TransitionDirection` import to include `TransitionState`.)

Add to `GameState` struct:

```rust
pub transition: TransitionState,
pub tsid_to_name: std::collections::HashMap<String, String>,
```

Add to `GameState::new()`:

```rust
transition: TransitionState::new(),
tsid_to_name: std::collections::HashMap::from([
    ("LADEMO001".to_string(), "demo_meadow".to_string()),
    ("LADEMO002".to_string(), "demo_heights".to_string()),
]),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test -p harmony-glitch game_state_has_transition_state -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full test suite**

Run: `cd src-tauri && cargo test --workspace`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: add TransitionState and TSID map to GameState"
```

---

### Task 3: Integrate signpost detection and swoop trigger into tick()

**Files:**
- Modify: `src-tauri/src/engine/state.rs:116-260`

- [ ] **Step 1: Write the failing test — signpost detection during tick**

```rust
#[test]
fn tick_detects_signpost_pre_subscribe() {
    use crate::street::types::{Signpost, SignpostConnection};

    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
    let mut street = test_street();
    street.signposts = vec![Signpost {
        id: "sign_right".into(),
        x: 1900.0,
        y: 0.0,
        connects: vec![SignpostConnection {
            target_tsid: "LADEMO002".into(),
            target_label: "To the Heights".into(),
        }],
    }];
    state.load_street(street, vec![]);

    // Move player near the signpost (within 500px)
    state.player.x = 1500.0;
    state.player.on_ground = true;

    let input = InputState::default();
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

    assert!(matches!(
        state.transition.phase,
        TransitionPhase::PreSubscribed { .. }
    ));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch tick_detects_signpost_pre_subscribe -- --nocapture`
Expected: FAIL — tick doesn't call `check_signposts`

- [ ] **Step 3: Write the second failing test — swoop triggers on crossing signpost X**

```rust
#[test]
fn tick_triggers_swoop_on_crossing_signpost() {
    use crate::street::types::{Signpost, SignpostConnection};
    use crate::engine::transition::TransitionPhase;

    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
    let mut street = test_street();
    street.signposts = vec![Signpost {
        id: "sign_right".into(),
        x: 1900.0,
        y: 0.0,
        connects: vec![SignpostConnection {
            target_tsid: "LADEMO002".into(),
            target_label: "To the Heights".into(),
        }],
    }];
    state.load_street(street, vec![]);

    // Place player past the signpost X (crossed it)
    state.player.x = 1950.0;
    state.player.on_ground = true;

    let input = InputState::default();
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

    // check_signposts puts us in PreSubscribed, then the crossing
    // check triggers the swoop — both happen in the same tick.
    assert!(matches!(
        state.transition.phase,
        TransitionPhase::Swooping { .. }
    ));
}
```

- [ ] **Step 4: Implement signpost detection and swoop trigger in tick()**

Add the following BEFORE the physics tick in `tick()` (after the facing update, around line 126):

```rust
// --- Street transition system ---
// Check signpost proximity and trigger swoop if player crosses signpost X.
self.transition.check_signposts(
    self.player.x,
    &street.signposts,
    street.left,
    street.right,
);

// Trigger swoop when player crosses the signpost X coordinate.
// Copy values out of the pattern match BEFORE calling trigger_swoop,
// because the if-let borrows self.transition immutably while
// trigger_swoop needs &mut self.transition.
if let TransitionPhase::PreSubscribed {
    signpost_x,
    direction,
    ..
} = &self.transition.phase
{
    let signpost_x = *signpost_x;
    let direction = *direction;
    let crossed = match direction {
        TransitionDirection::Right => self.player.x >= signpost_x,
        TransitionDirection::Left => self.player.x <= signpost_x,
    };
    if crossed {
        self.transition.trigger_swoop(street.tsid.clone());
    }
}

// Tick transition animation
self.transition.tick(dt);
```

Update the existing `use` statement at the top of `state.rs` to include `TransitionPhase`:

```rust
use crate::engine::transition::{TransitionDirection, TransitionPhase, TransitionState};
```

- [ ] **Step 5: Run both tests to verify they pass**

Run: `cd src-tauri && cargo test -p harmony-glitch tick_detects_signpost -- --nocapture`
Run: `cd src-tauri && cargo test -p harmony-glitch tick_triggers_swoop -- --nocapture`
Expected: Both PASS

- [ ] **Step 6: Run full test suite**

Run: `cd src-tauri && cargo test --workspace`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: integrate signpost detection and swoop trigger into tick loop"
```

---

### Task 4: Freeze input during swoop and populate TransitionFrame in RenderFrame

**Files:**
- Modify: `src-tauri/src/engine/state.rs:116-260`

- [ ] **Step 1: Write the failing test — input frozen during swoop**

```rust
#[test]
fn tick_freezes_input_during_swoop() {
    use crate::street::types::{Signpost, SignpostConnection};

    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
    let mut street = test_street();
    street.signposts = vec![Signpost {
        id: "sign_right".into(),
        x: 1900.0,
        y: 0.0,
        connects: vec![SignpostConnection {
            target_tsid: "LADEMO002".into(),
            target_label: "To the Heights".into(),
        }],
    }];
    state.load_street(street, vec![]);

    // Trigger swoop by crossing signpost
    state.player.x = 1950.0;
    state.player.on_ground = true;
    let input = InputState::default();
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    assert!(matches!(state.transition.phase, TransitionPhase::Swooping { .. }));

    // Record position, then tick with movement input
    let pos_before = state.player.x;
    let input = InputState { left: true, ..Default::default() };
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

    // Player should NOT have moved
    assert!(
        (state.player.x - pos_before).abs() < 0.01,
        "Player moved during swoop: {} -> {}",
        pos_before,
        state.player.x
    );
}
```

- [ ] **Step 2: Write the failing test — TransitionFrame present during swoop**

```rust
#[test]
fn render_frame_contains_transition_during_swoop() {
    use crate::street::types::{Signpost, SignpostConnection};

    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
    let mut street = test_street();
    street.signposts = vec![Signpost {
        id: "sign_right".into(),
        x: 1900.0,
        y: 0.0,
        connects: vec![SignpostConnection {
            target_tsid: "LADEMO002".into(),
            target_label: "To the Heights".into(),
        }],
    }];
    state.load_street(street, vec![]);

    // Trigger swoop
    state.player.x = 1950.0;
    state.player.on_ground = true;
    let input = InputState::default();
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());

    // Next tick should have transition in frame
    let frame = state.tick(1.0 / 60.0, &input, &mut rand::thread_rng()).unwrap();
    let transition = frame.transition.unwrap();
    assert!(transition.progress > 0.0);
    assert_eq!(transition.to_street, "demo_heights");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd src-tauri && cargo test -p harmony-glitch tick_freezes_input_during_swoop -- --nocapture`
Run: `cd src-tauri && cargo test -p harmony-glitch render_frame_contains_transition -- --nocapture`
Expected: Both FAIL

- [ ] **Step 4: Implement input freeze and TransitionFrame population**

Restructure `tick()` to skip physics and interaction when swooping. Wrap the existing physics + interaction blocks in a guard:

```rust
let is_swooping = matches!(self.transition.phase, TransitionPhase::Swooping { .. });

if !is_swooping {
    // Physics tick
    self.player.tick(dt, input, street.platforms(), street.left, street.right);

    // --- Interaction system ---
    // [entire interaction block stays here, unchanged]
}
```

Populate `TransitionFrame` in the RenderFrame construction. Replace `transition: None` with:

```rust
transition: self.transition.swoop_progress().map(|(progress, direction)| {
    let to_street_tsid = match &self.transition.phase {
        TransitionPhase::Swooping { to_street, .. } => to_street.clone(),
        _ => String::new(),
    };
    TransitionFrame {
        progress,
        direction,
        to_street: self.tsid_to_name
            .get(&to_street_tsid)
            .cloned()
            .unwrap_or(to_street_tsid),
    }
}),
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -p harmony-glitch tick_freezes_input -- --nocapture`
Run: `cd src-tauri && cargo test -p harmony-glitch render_frame_contains_transition -- --nocapture`
Expected: Both PASS

- [ ] **Step 6: Run full test suite**

Run: `cd src-tauri && cargo test --workspace`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: freeze input during swoop, populate TransitionFrame in RenderFrame"
```

---

### Task 5: Handle transition Complete phase — spawn at target signpost

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn transition_complete_repositions_player() {
    use crate::street::types::{Signpost, SignpostConnection};

    let mut state = GameState::new(1280.0, 720.0, ItemDefs::new(), EntityDefs::new());
    let mut street = test_street();
    street.tsid = "LADEMO001".into();
    street.signposts = vec![Signpost {
        id: "sign_right".into(),
        x: 1900.0,
        y: 0.0,
        connects: vec![SignpostConnection {
            target_tsid: "LADEMO002".into(),
            target_label: "To the Heights".into(),
        }],
    }];
    state.load_street(street, vec![]);

    // Trigger swoop
    state.player.x = 1950.0;
    state.player.on_ground = true;
    let input = InputState::default();
    state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    assert!(matches!(state.transition.phase, TransitionPhase::Swooping { .. }));

    // Mark street ready — this will cause swoop to complete
    state.transition.mark_street_ready();

    // Simulate a new street having been loaded with a return signpost
    let mut new_street = test_street();
    new_street.tsid = "LADEMO002".into();
    new_street.signposts = vec![Signpost {
        id: "sign_left".into(),
        x: -1900.0,
        y: 0.0,
        connects: vec![SignpostConnection {
            target_tsid: "LADEMO001".into(),
            target_label: "Back to Meadow".into(),
        }],
    }];
    state.load_street(new_street, vec![]);

    // Tick enough frames for swoop to complete (MIN_SWOOP_SECS = 0.3)
    for _ in 0..30 {
        state.tick(1.0 / 60.0, &input, &mut rand::thread_rng());
    }

    // Transition should be back to None, origin cleared
    assert_eq!(state.transition.phase, TransitionPhase::None);
    assert!(state.transition_origin_tsid.is_none());
    // Player should be at the return signpost X (-1900), not center (0)
    assert!(
        (state.player.x - (-1900.0)).abs() < 1.0,
        "Player should be at return signpost x=-1900, got {}",
        state.player.x
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch transition_complete_repositions -- --nocapture`
Expected: FAIL — Complete phase not handled

- [ ] **Step 3: Implement Complete handler**

The `Complete { new_street }` variant stores the TSID of the street we ARRIVED at, but we need the signpost connecting BACK to the street we came FROM. Store the origin TSID on `GameState`.

Add a field to `GameState`:

```rust
pub transition_origin_tsid: Option<String>,
```

Initialize in `GameState::new()`:

```rust
transition_origin_tsid: None,
```

Update the swoop trigger in the crossing detection code (Task 3) to also set the origin:

```rust
if crossed {
    self.transition_origin_tsid = Some(street.tsid.clone());
    self.transition.trigger_swoop(street.tsid.clone());
}
```

Add the Complete handler in `tick()`, after `self.transition.tick(dt)` and before the `is_swooping` guard:

```rust
// Handle transition completion — reposition player at return signpost
if let TransitionPhase::Complete { .. } = &self.transition.phase {
    if let Some(origin_tsid) = &self.transition_origin_tsid {
        let return_signpost_x = street.signposts.iter()
            .find(|s| s.connects.iter().any(|c| c.target_tsid == *origin_tsid))
            .map(|s| s.x);

        if let Some(x) = return_signpost_x {
            self.player.x = x;
            self.player.y = street.ground_y;
        }
    }
    // If no return signpost found, player stays at load_street's default (center)

    self.transition_origin_tsid = None;
    self.transition.reset();
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd src-tauri && cargo test -p harmony-glitch transition_complete_repositions -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full test suite**

Run: `cd src-tauri && cargo test --workspace`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat: handle transition Complete — reposition player at return signpost"
```

---

### Task 6: Add street_transition_ready Tauri command

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add the command**

Add after the `drop_item` command (around line 262):

```rust
#[tauri::command]
fn street_transition_ready(app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.transition.mark_street_ready();
    Ok(())
}
```

- [ ] **Step 2: Register the command in the invoke handler**

Add `street_transition_ready` to the `tauri::generate_handler!` list (around line 488):

```rust
.invoke_handler(tauri::generate_handler![
    list_streets,
    load_street,
    send_input,
    start_game,
    stop_game,
    get_identity,
    set_display_name,
    send_chat,
    drop_item,
    get_network_status,
    street_transition_ready,
])
```

- [ ] **Step 3: Verify it compiles**

Run: `cd src-tauri && cargo clippy --workspace`
Expected: No errors, no warnings

- [ ] **Step 4: Run full test suite**

Run: `cd src-tauri && cargo test --workspace`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add street_transition_ready Tauri command"
```

---

## Chunk 2: Frontend Integration

### Task 7: Add streetTransitionReady IPC wrapper

**Files:**
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add the function**

Add after `dropItem` (around line 59):

```typescript
export async function streetTransitionReady(): Promise<void> {
  return invoke('street_transition_ready');
}
```

- [ ] **Step 2: Verify build**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add src/lib/ipc.ts
git commit -m "feat: add streetTransitionReady IPC wrapper"
```

---

### Task 8: Update App.svelte to orchestrate street transitions

**Files:**
- Modify: `src/App.svelte`

- [ ] **Step 1: Update imports**

Add `streetTransitionReady` to the import from `./lib/ipc`:

```typescript
import { stopGame, loadStreet, getIdentity, streetTransitionReady } from './lib/ipc';
```

- [ ] **Step 2: Update handleFrame to trigger transition on first appearance**

Replace the existing transition handler (lines 41-52):

```typescript
// When a transition appears, pre-load the target street immediately.
// The TransitionState stalls at progress 0.9 until we signal ready.
if (frame.transition && !transitionPending) {
  transitionPending = true;
  loadStreet(frame.transition.toStreet)
    .then((street) => {
      currentStreet = street;
      return streetTransitionReady();
    })
    .then(() => {
      transitionPending = false;
    })
    .catch((e) => {
      console.error('Street transition failed:', e);
      transitionPending = false;
    });
}
```

Key changes from the original:
- Trigger on `frame.transition` (any progress), not `progress >= 1.0`
- `transitionPending` cleared only after BOTH `loadStreet` AND `streetTransitionReady` complete
- Error handling resets `transitionPending` to allow retry (timeout handles stuck state)

- [ ] **Step 3: Verify build**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add src/App.svelte
git commit -m "feat: orchestrate street transitions — pre-load on first transition frame"
```

---

### Task 9: Verify full integration

- [ ] **Step 1: Run all Rust tests**

Run: `cd src-tauri && cargo test --workspace`
Expected: All tests pass (128 original + 8 transition + ~5 new = ~141)

- [ ] **Step 2: Run clippy**

Run: `cd src-tauri && cargo clippy --workspace`
Expected: No warnings

- [ ] **Step 3: Build frontend**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: Build succeeds

- [ ] **Step 4: Manual testing**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run tauri dev`

Test plan:
1. Select demo_meadow from street picker
2. Walk right toward x=1900 — verify swoop animation starts
3. Verify arrival on demo_heights, player at left edge (x=-1900)
4. Walk left toward x=-1900 — verify swoop back to demo_meadow
5. Verify arrival at right edge (x=1900)
6. Verify inventory persists across transitions
7. Verify entities reload correctly on each street
8. Verify no visual flash when renderer rebuilds scene mid-swoop (the swoop offset should cover it)

- [ ] **Step 5: Final commit if any cleanup needed**

```bash
git add -A
git commit -m "chore: street transitions integration cleanup"
```
