# Entity State & Cooldowns — Design Spec

**Bead:** glitch-6a3
**Date:** 2026-03-17
**Status:** Approved

## Problem

Entities (fruit trees, chickens, pigs, etc.) can be harvested infinitely with no cooldown or depletion. Spamming the interact key yields unlimited items every tick. This removes resource management from gameplay and makes the item loop feel hollow.

## Goals

- Enforce per-entity-instance cooldowns between harvests
- Add depletion after N harvests, with a longer respawn period before the entity resets
- Communicate entity state visually (opacity) and textually (prompt)
- Preserve the sans-I/O architecture: all logic timestamp-based, no system clock in game state
- Design for forward compatibility with a full entity state machine (growth stages, entity-specific behaviors)

## Non-Goals

- Full state machine with multiple visual stages (future work)
- Entity movement or AI
- Persistence across sessions or street transitions
- Multiplayer entity state synchronization

## Data Model

### EntityInstanceState (new)

```rust
/// Per-instance runtime state for a world entity.
/// Stored in GameState::entity_states, keyed by entity instance ID.
pub struct EntityInstanceState {
    /// Harvests remaining before depletion. Initialized from EntityDef::max_harvests.
    pub harvests_remaining: u32,
    /// Game-time timestamp when cooldown expires. 0.0 = not on cooldown.
    pub cooldown_until: f64,
    /// Game-time timestamp when respawn completes. 0.0 = not depleted.
    pub depleted_until: f64,
}
```

Timestamps use `game_time: f64` accumulated on `GameState` via `self.game_time += dt` each tick. This is a session-global monotonic clock — no `Instant::now()` in game logic.

### EntityDef changes

Two new fields alongside the existing `cooldown_secs`:

| Field | Type | Description |
|-------|------|-------------|
| `max_harvests` | `u32` | Harvests before depletion. 0 = infinite (no depletion). |
| `respawn_secs` | `f64` | Seconds to respawn after depletion. |

Both fields are required on `EntityDef` (no `serde(default)`), matching the existing `cooldown_secs` convention. All entries in `entities.json` must include them.

Example `entities.json` entry (note: JSON uses camelCase per `#[serde(rename_all = "camelCase")]` on `EntityDef`):

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
    "interactRadius": 80.0
  }
}
```

### GameState changes

```rust
pub struct GameState {
    // ... existing fields ...
    pub entity_states: HashMap<String, EntityInstanceState>,
    pub game_time: f64,
}
```

### Storage approach

HashMap keyed by entity instance ID (e.g., `"tree_1"`). Chosen over parallel vec (fragile index coupling) or embedded fields on WorldEntity (mixes static placement with dynamic state).

**Lazy initialization:** On first interaction with an entity, if no entry exists in the HashMap, create one with `harvests_remaining = def.max_harvests`. No pre-population on street load.

**Street load reset:** `entity_states.clear()` in `load_street()`, alongside existing `world_items.clear()`. `game_time` continues accumulating (session-global).

## Interaction Logic

Three checks inserted into `execute_interaction()` **before** existing inventory operations:

### 1. Depletion check

```
if state.depleted_until > game_time:
    reject interaction
    return feedback: "Regrowing... (Xs)"
```

### 2. Cooldown check

```
if state.cooldown_until > game_time:
    reject interaction
    return feedback: "Available in Xs"
```

### 3. Post-harvest state update (after successful yield)

```
state.harvests_remaining -= 1
if harvests_remaining == 0:
    state.depleted_until = game_time + def.respawn_secs
    state.harvests_remaining = def.max_harvests  // pre-set for after respawn
else:
    state.cooldown_until = game_time + def.cooldown_secs
```

Pre-setting `harvests_remaining` on depletion means no tick-based respawn processing. When `depleted_until` expires, the entity is immediately ready with full harvests.

### max_harvests = 0 (infinite)

When `max_harvests` is 0, depletion is disabled. The `harvests_remaining` field is never decremented, and only `cooldown_secs` applies between harvests. This preserves backward compatibility with entities that should be infinite.

### Invariant: depletion and cooldown are mutually exclusive

The post-harvest if/else ensures that on any given harvest, either `cooldown_until` or `depleted_until` is set, never both simultaneously. This makes the `max(cooldown_until, depleted_until)` computation in frame building correct — one of them will always be 0.0 or stale.

### Function signature changes

`execute_interaction()` gains two new parameters:
- `entity_states: &mut HashMap<String, EntityInstanceState>` — read and mutate per-instance state
- `game_time: f64` — for timestamp comparisons

The entity instance ID is resolved from the `NearestInteractable::Entity { index }` via `entities[index].id`.

## Prompt Text

The prompt is built in `build_prompt()` in Rust. The current `InteractionPrompt` struct has `verb` and `target_name` fields, and the renderer combines them as `[E] ${verb} ${targetName}`.

To support non-actionable prompts (cooldown/depleted), add an `actionable: bool` field to `InteractionPrompt`:

```rust
pub struct InteractionPrompt {
    pub verb: String,
    pub target_name: String,
    pub target_x: f64,
    pub target_y: f64,
    pub actionable: bool,  // New: false suppresses [E] prefix in renderer
}
```

The renderer changes its template: if `actionable`, show `[E] ${verb} ${targetName}`; otherwise show `${verb}` only.

| Entity State | `actionable` | `verb` | `target_name` | Rendered |
|-------------|-------------|--------|---------------|----------|
| Ready | `true` | `"Harvest"` | `"Fruit Tree"` | `[E] Harvest Fruit Tree` |
| On cooldown | `false` | `"Available in 4s"` | `""` | `Available in 4s` |
| Depleted | `false` | `"Regrowing... (28s)"` | `""` | `Regrowing... (28s)` |

`build_prompt()` gains `game_time: f64` and `entity_states: &HashMap<String, EntityInstanceState>` parameters. It resolves the entity instance ID from the `NearestInteractable::Entity { index }` via `entities[index].id` to look up state in the HashMap.

The frontend prompt component needs one small change: conditionally render the `[E]` prefix based on the new `actionable` field. The TypeScript `InteractionPrompt` interface gains `actionable: boolean`.

## Rendering

### WorldEntityFrame extension

```rust
pub struct WorldEntityFrame {
    // ... existing fields ...
    /// Seconds until entity is available (cooldown or respawn). None = ready.
    pub cooldown_remaining: Option<f64>,
    /// True if entity is in respawn period (depleted), false if just on cooldown.
    pub depleted: bool,
}
```

`cooldown_remaining` collapses both cooldown and depletion into one value — the renderer doesn't need to distinguish *why* the entity is unavailable, only *how long* and *how severe*.

### Frame building

In `build_entity_frames()`, for each entity:

1. Look up state in `entity_states` HashMap
2. Compute: `remaining = max(cooldown_until, depleted_until) - game_time`
3. If `remaining > 0`: set `cooldown_remaining = Some(remaining)`, `depleted = depleted_until > game_time`
4. Otherwise: `cooldown_remaining = None`, `depleted = false`

### Frontend opacity

In `renderer.ts`, when updating entity sprites:

| State | Opacity |
|-------|---------|
| Ready (`cooldown_remaining` is null/absent) | 1.0 |
| On cooldown (not depleted) | 0.5 |
| Depleted | 0.25 |

No other visual changes. Works with placeholder rectangles and future sprites.

### TypeScript type update

```typescript
export interface WorldEntityFrame {
  // ... existing fields ...
  cooldownRemaining: number | null;
  depleted: boolean;
}
```

## Game Time

`game_time: f64` added to `GameState`, initialized to `0.0`, incremented by `dt` each tick. This is the sole time source for all entity state logic.

No per-tick scanning of entities for expired timers. All checks are timestamp comparisons at interaction time (O(1) per interaction) and frame-build time (O(n) per tick where n is entity count — same as current).

## Tuning Values

Initial entity definitions (subject to playtesting):

| Entity | cooldown_secs | max_harvests | respawn_secs |
|--------|--------------|-------------|-------------|
| fruit_tree | 5.0 | 3 | 30.0 |
| chicken | 8.0 | 2 | 45.0 |
| pig | 8.0 | 2 | 45.0 |
| butterfly | 0.0 | 1 | 20.0 |
| bubble_tree | 3.0 | 4 | 25.0 |
| wood_tree | 6.0 | 3 | 35.0 |

These are starting points. The data-driven design (all values in `entities.json`) makes tuning a JSON edit, not a code change.

## Forward Compatibility (State Machine Path)

This design is structured to evolve into a full entity state machine:

- **HashMap stays:** `EntityInstanceState` becomes an enum with variants (Growing, Ready, Cooldown, Depleted, Respawning, etc.)
- **game_time stays:** Phase transitions use the same timestamp-comparison pattern
- **WorldEntityFrame stays:** `cooldown_remaining` and `depleted` generalize to a `state` field with richer variants
- **Prompt pattern stays:** `build_prompt()` matches on state to produce context-specific text
- **No structural refactoring needed** to go from struct → enum

The key constraint this design establishes: entity state is a separate concern from entity placement, owned by a HashMap in GameState, with game_time as the sole clock source. Future state machine work extends this pattern without breaking it.

## Testing

### Unit tests — interaction.rs

- Harvest decrements `harvests_remaining`
- Interaction rejected during cooldown; returns correct feedback text with remaining seconds
- Interaction rejected during depletion; returns correct feedback text with remaining seconds
- Cooldown expires after `cooldown_secs` game time elapses
- Depletion triggers after last harvest; expires after `respawn_secs`
- Full cycle: harvest to depletion → respawn → harvests available again
- Lazy init: first interaction creates state with `max_harvests`
- `max_harvests = 0`: no depletion, cooldown-only behavior
- `respawn_secs = 0.0` with `max_harvests > 0`: instant respawn after depletion

### Unit tests — state.rs

- `game_time` accumulates correctly across ticks
- `entity_states` clears on `load_street()`
- `WorldEntityFrame` includes correct `cooldown_remaining` and `depleted` values
- Prompt text reflects entity state (ready vs cooldown vs depleted)

### Existing tests

The 140+ existing tests are unaffected. `execute_interaction()` gains `entity_states` and `game_time` parameters; existing test call sites pass an empty `&mut HashMap::new()` and `0.0` (fresh entities have no state entry, so behavior is identical to current infinite harvesting). `build_prompt()` similarly gains these parameters with the same defaults.

## Files Modified

| File | Change |
|------|--------|
| `src-tauri/src/item/types.rs` | Add `EntityInstanceState`, `max_harvests`/`respawn_secs` to `EntityDef`, `actionable` to `InteractionPrompt`, `cooldown_remaining`/`depleted` to `WorldEntityFrame` |
| `src-tauri/src/item/loader.rs` | Serde handles new fields automatically (no code changes needed, just `entities.json` must include them) |
| `src-tauri/src/item/interaction.rs` | Cooldown/depletion checks in `execute_interaction()`, state-aware `build_prompt()`, remove deferred-phase comment |
| `src-tauri/src/engine/state.rs` | Add `entity_states` HashMap, `game_time`, thread through tick loop, extend frame builder |
| `assets/entities.json` | Add `maxHarvests` and `respawnSecs` to all entity definitions, set non-zero `cooldownSecs` |
| `src/lib/types.ts` | Add `cooldownRemaining`/`depleted` to `WorldEntityFrame`, `actionable` to `InteractionPrompt` |
| `src/lib/engine/renderer.ts` | Opacity fade based on cooldown/depletion state, conditional `[E]` prefix based on `actionable` |
