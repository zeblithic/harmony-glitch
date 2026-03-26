# Game State Persistence

## Overview

Save player position, inventory, current street, and facing direction to disk. Auto-save on street change and app exit. Auto-resume on launch if a save exists, skipping the street picker.

**Goal:** Player progress (position, inventory, street) survives app restarts without manual save/load actions.

**Scope:** Player state only — no per-street entity state (NPC harvests, dropped items). Streets reset on re-entry, matching Glitch's original ephemeral world design.

## Save File

Location: `<app_data_dir>/savegame.json` (alongside existing `profile.json`).

Platform-standard paths via Tauri's `app.path().app_data_dir()`:
- macOS: `~/Library/Application Support/com.zeblithic.harmony-glitch/`
- Linux: `~/.config/com.zeblithic.harmony-glitch/`

### Format

```json
{
  "street_id": "demo_meadow",
  "x": -500.0,
  "y": -100.0,
  "facing": "right",
  "inventory": [
    { "item_id": "cherry", "count": 5 },
    null,
    null
  ]
}
```

Small (~500 bytes). Synchronous write is fine — no game loop stutter at this size.

The `inventory` array is a fixed-length Vec matching `Inventory.slots` (16 entries). `null` = empty slot. Each non-null entry has `item_id` (String) and `count` (u32).

## Save Triggers

1. **Street change** — save before loading the new street, in the `load_street` IPC command handler. Captures the player's position on the OLD street before it's replaced.

2. **App exit** — save in `stop_game` command. Also hook Tauri's window close event (`on_window_event` with `CloseRequested`) so saves happen even if the user closes the window directly.

## Load Flow

### Backend

A new IPC command `get_saved_state()` reads `savegame.json` and returns either:
- `{ streetId, x, y, facing, inventory }` — if save exists and is valid
- `null` — if no save file, or if the file is corrupted (log warning, don't crash)

The actual state restoration happens when the frontend calls `load_street(streetId)` followed by `start_game()` — the existing flow. The save data is applied to `GameState` after the street loads:
- Set `player.x`, `player.y` from save
- Set `facing` from save
- Populate `inventory.slots` from save

### Frontend

On mount in `App.svelte`:
1. Call `get_saved_state()`
2. If non-null: skip the street picker, call `load_street(savedState.streetId)` directly, then `start_game()`
3. If null: show street picker as today (first-time player)
4. The "Back" button (already exists) returns to the street picker — this is how players switch streets

### Save on Load

When `load_street` is called and a game is already running (street transition or explicit street change), save the current state BEFORE loading the new street. This ensures the save always reflects the latest position.

## Serialization

Add `#[derive(Serialize, Deserialize)]` to types that don't already have it:

| Type | Current Derives | Needs Adding |
|------|----------------|--------------|
| `PhysicsBody` | `Debug, Clone` | `Serialize, Deserialize` |
| `Inventory` | `Debug, Clone` | `Serialize, Deserialize` |
| `ItemStack` | `Debug, Clone, Serialize, Deserialize` | Already done |
| `Direction` | `Debug, Clone, Copy, PartialEq, Serialize, Deserialize` | Already done |

A dedicated `SaveState` struct wraps just the fields we save — we don't serialize the entire `GameState`:

```rust
#[derive(Serialize, Deserialize)]
struct SaveState {
    street_id: String,
    x: f64,
    y: f64,
    facing: Direction,
    inventory: Vec<Option<ItemStack>>,
}
```

## Error Handling

- **Missing file:** Return null from `get_saved_state()`. Normal first-run.
- **Corrupted JSON:** Log warning via `eprintln!`, return null. Player sees street picker.
- **Save write failure:** Log error but don't crash. Non-fatal — next save trigger will retry.
- **Unknown street_id in save:** Return null. Street may have been removed between versions.

## Testing

### Rust Unit Tests

1. **Save/load round-trip:** Create `SaveState`, serialize to JSON, deserialize, verify all fields match.
2. **Inventory serialization:** Full inventory, empty inventory, partial inventory (mix of Some/None slots).
3. **Missing file returns None:** `load_save_state` with non-existent path returns None.
4. **Corrupted file returns None:** Write invalid JSON, verify graceful fallback.
5. **Street validation:** Save with unknown street_id treated as no-save.

### Frontend Tests (vitest)

1. **Auto-resume flow:** Mock `get_saved_state()` returning a save → verify `loadStreet` called with saved street.
2. **First-time flow:** Mock returning null → verify street picker shown.

## Files Modified

| File | Change |
|------|--------|
| `src-tauri/src/engine/state.rs` | Add Serialize/Deserialize to `PhysicsBody` |
| `src-tauri/src/item/inventory.rs` | Add Serialize/Deserialize to `Inventory` |
| `src-tauri/src/lib.rs` | New `get_saved_state` and `save_game` commands, save hooks in `load_street`/`stop_game`/window close |
| `src/App.svelte` | Auto-resume logic on mount |
| `src/lib/ipc.ts` | New `getSavedState()` wrapper |
| `src/lib/types.ts` | New `SavedState` interface |

No new crates. No new dependencies.

## What This Does NOT Include

- Per-street entity state (NPC harvests, dropped items) — streets reset on re-entry
- Multiple save slots — single save file
- Cloud sync — local only
- Manual save/load UI — fully automatic
- Save file versioning/migration — v1, will add if format changes
