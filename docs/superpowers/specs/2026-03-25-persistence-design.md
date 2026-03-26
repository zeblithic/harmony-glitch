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
- Windows: `%APPDATA%\com.zeblithic.harmony-glitch\`

### Format

`SaveState` uses `#[serde(rename_all = "camelCase")]` to match the frontend convention. `ItemStack` already has `rename_all = "camelCase"`, so `item_id` serializes as `"itemId"`.

```json
{
  "streetId": "demo_meadow",
  "x": -500.0,
  "y": -100.0,
  "facing": "right",
  "inventory": [
    { "itemId": "cherry", "count": 5 },
    null,
    null
  ]
}
```

Small (~500 bytes). Synchronous write is fine — no game loop stutter at this size.

The `inventory` array is a fixed-length Vec matching `Inventory.slots` (16 entries). `null` = empty slot. The `streetId` uses the short name (e.g. `"demo_meadow"`, not the TSID `"LADEMO001"`) — `load_street_xml` accepts both, but short names are more readable in the save file.

## Save Triggers

1. **Street change** — in the `load_street` IPC command, if `GameState.street.is_some()` (a street was previously loaded), save current state BEFORE loading the new street. This captures the player's position on the old street.

2. **App exit** — in `stop_game` command and Tauri's `on_window_event` with `CloseRequested`. Saves even if the user closes the window directly.

## Load Flow

### Backend

A new IPC command `get_saved_state()` reads `savegame.json` and returns either:
- `{ streetId, x, y, facing, inventory }` — if save exists and is valid
- `null` — if no save file, or if the file is corrupted (log warning, don't crash)

State restoration: `load_street` gains an optional `saved_state` parameter. When the frontend passes save data, `load_street` applies the saved position, facing, and inventory to `GameState` AFTER loading the street XML (which resets player to spawn point). This keeps the restoration logic inside the existing `load_street` flow — no separate `restore_save` command needed.

Specifically, after `GameState::load_street()` sets the default spawn position:
- Override `player.x`, `player.y` from save (clamped to street bounds)
- Override `facing` from save
- Replace `inventory.slots` from save

### Frontend

On mount in `App.svelte`:
1. Call `getSavedState()`
2. If non-null: skip the street picker, call `loadStreet(savedState.streetId, savedState)` directly, then `startGame()`
3. If null: show street picker as today (first-time player)
4. The "Back" button (already exists) returns to the street picker — this is how players switch streets

## Serialization

A dedicated `SaveState` struct wraps just the fields we save — we don't serialize the entire `GameState`:

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveState {
    pub street_id: String,
    pub x: f64,
    pub y: f64,
    pub facing: Direction,
    pub inventory: Vec<Option<ItemStack>>,
}
```

`PhysicsBody` and `Inventory` do NOT need `Serialize`/`Deserialize` derives — `SaveState` extracts the relevant fields directly, avoiding issues with private fields like `PhysicsBody.prev_jump`.

`ItemStack` and `Direction` already have `Serialize`/`Deserialize`.

## Error Handling

- **Missing file:** Return null from `get_saved_state()`. Normal first-run.
- **Corrupted JSON:** Log warning via `eprintln!`, return null. Player sees street picker.
- **Save write failure:** Log error but don't crash. Non-fatal — next save trigger will retry.
- **Unknown street_id in save:** Return null. Street may have been removed between versions.
- **Position out of bounds:** Clamp to street bounds after loading. Prevents stuck players if street geometry changed.

## Testing

### Rust Unit Tests

1. **Save/load round-trip:** Create `SaveState`, serialize to JSON, deserialize, verify all fields match.
2. **Inventory serialization:** Full inventory, empty inventory, partial inventory (mix of Some/None slots).
3. **Missing file returns None:** `load_save_state` with non-existent path returns None.
4. **Corrupted file returns None:** Write invalid JSON, verify graceful fallback.
5. **Street validation:** Save with unknown street_id treated as no-save.
6. **Position clamping:** Saved position outside street bounds gets clamped.

### Frontend Tests (vitest)

1. **Auto-resume flow:** Mock `getSavedState()` returning a save → verify `loadStreet` called with saved street.
2. **First-time flow:** Mock returning null → verify street picker shown.

## Files Modified

| File | Change |
|------|--------|
| `src-tauri/src/lib.rs` | New `get_saved_state` command, save logic in `load_street`/`stop_game`/window close, `load_street` gains optional save restoration |
| `src-tauri/src/engine/state.rs` | New `SaveState` struct, `save_state()` and `restore_save()` methods on `GameState` |
| `src/App.svelte` | Auto-resume logic on mount |
| `src/lib/ipc.ts` | New `getSavedState()` wrapper, update `loadStreet` signature |
| `src/lib/types.ts` | New `SavedState` interface |

No new crates. No new dependencies. No derives needed on `PhysicsBody` or `Inventory`.

## What This Does NOT Include

- Per-street entity state (NPC harvests, dropped items) — streets reset on re-entry
- Multiple save slots — single save file
- Cloud sync — local only
- Manual save/load UI — fully automatic
- Save file versioning/migration — v1, will add if format changes
