# Street Transitions — Design Document

## Overview

Wire together the existing TransitionState state machine, PixiJS swoop renderer, and App.svelte transition handler to enable seamless street-to-street transitions. Players walk to a signpost at the street edge and the swoop animation carries them to the connected street.

**Goal:** Two isolated demo rooms become a connected world you can walk between.

## What Already Exists

The heavy lifting is done — this project is integration glue.

- **TransitionState** (`src-tauri/src/engine/transition.rs`): Pure state machine with 8 tests. Handles PreSubscribed → Swooping → Complete lifecycle, stall-at-0.9 while waiting for street data, timeout cancellation, min swoop duration.
- **Swoop rendering** (`src/lib/engine/renderer.ts:414-429`): Reads `frame.transition`, applies viewport-based slide offset to world and parallax containers.
- **TransitionInfo TypeScript type** (`src/lib/types.ts`): `{ progress, direction, toStreet }` already defined.
- **App.svelte handler** (`src/App.svelte:42-52`): Detects transition completion, calls `loadStreet`. Needs minor adjustment to trigger earlier.
- **Signpost data**: Both demo streets have bidirectional signpost connections parsed from XML.
- **load_street command**: Handles runtime street switching, preserves inventory.

## Design Decisions

1. **Walk-to-edge trigger (Glitch-authentic)**: Transition triggers automatically when player crosses the signpost X coordinate. No button press. Faithful to the original game.
2. **Spawn at target signpost**: Player appears at the destination street's signpost that connects back to the origin. Data-driven, works for any street layout.
3. **No wall collision**: Signpost X is the trigger, not wall enforcement. Walls are a separate concern.
4. **Streets-only scope**: No entity cooldowns, no signpost visuals, no additional streets.

## Trigger Mechanism

Each tick, `GameState::tick()` calls `transition.check_signposts()`. When the player enters the 500px pre-subscribe zone, the state machine moves to `PreSubscribed`. When the player's X crosses the signpost X (toward the edge), `trigger_swoop()` fires.

- Right-edge signpost (`direction == Right`): trigger when `player_x >= signpost_x`
- Left-edge signpost (`direction == Left`): trigger when `player_x <= signpost_x`

During swoop, player input is frozen — no movement, interaction, or physics updates.

## Data Flow

### Rust Side

1. `TransitionState` added as field on `GameState`, ticked every frame
2. `check_signposts()` called each tick with player position and street bounds
3. When player crosses signpost X → `trigger_swoop(current_street_tsid)`
4. `TransitionFrame` struct added to `RenderFrame`:
   ```rust
   pub struct TransitionFrame {
       pub progress: f64,
       pub direction: TransitionDirection,
       pub to_street: String,  // resolved name, not TSID
   }
   ```
5. TSID→name mapping (`HashMap<String, String>`) for resolving signpost targets to loadable street names
6. New `street_transition_ready` command calls `transition.mark_street_ready()`
7. On `Complete` phase: reposition player at target signpost X, reset transition state

### Frontend Side

1. App.svelte triggers `loadStreet` as soon as `frame.transition` appears (not at `progress >= 1.0`)
2. `transitionPending` flag prevents duplicate loads
3. After `loadStreet` returns → renderer rebuilds scene → call `streetTransitionReady()`
4. Rust finishes swoop, new street revealed

### Spawn Positioning

On `Complete`, find the destination street's signpost whose `target_tsid` matches the street we came from. Position player at that signpost's X coordinate, at ground level. Fallback to street center if no matching return signpost found.

## Files Modified

| File | Change |
|------|--------|
| `src-tauri/src/engine/state.rs` | Add `TransitionState` field, `TransitionFrame` struct, tick integration, input freeze during swoop, `Complete` handler with spawn positioning, transition data in RenderFrame |
| `src-tauri/src/lib.rs` | Add `street_transition_ready` command, TSID→name map |
| `src/App.svelte` | Change transition trigger to fire on first `frame.transition` appearance, call `streetTransitionReady()` after load |
| `src/lib/ipc.ts` | Add `streetTransitionReady()` wrapper |

No changes needed to: `renderer.ts` (swoop already implemented), `transition.rs` (state machine complete), `movement.rs` (no wall collision), `types.ts` (TransitionInfo already defined).

## Testing

- **Rust unit tests**: Signpost detection triggers PreSubscribed, crossing signpost X triggers swoop, input frozen during swoop, Complete repositions player at target signpost, TransitionFrame present in RenderFrame during swoop and absent otherwise
- **Existing tests**: 8 transition.rs tests unchanged, 128 existing tests unaffected
- **Manual**: Walk between demo_meadow and demo_heights in both directions, verify swoop animation, spawn position, round-trip

## Not In Scope

- Wall collision enforcement
- Entity cooldowns
- Additional streets beyond the two demos
- Signpost visual indicators (signs, arrows, labels)
- Pre-subscribe network prefetch (multiplayer concern)
- Camera smoothing or dead zones
