# Game State Persistence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-save player position, inventory, current street, and facing direction on street change and app exit; auto-resume on launch.

**Architecture:** A `SaveState` struct captures the minimal player state subset. Save/load functions in `engine/state.rs` convert between `GameState` and `SaveState`. The `load_street` IPC command gains optional save restoration. A new `get_saved_state` command reads the save file. Frontend auto-resumes on mount if a save exists.

**Tech Stack:** Rust (Tauri v2), serde_json, Svelte 5, TypeScript

**Spec:** `docs/superpowers/specs/2026-03-25-persistence-design.md`

**Test command:** `cd src-tauri && cargo test -p harmony-glitch`
**Lint command:** `cd src-tauri && cargo clippy -p harmony-glitch`
**Frontend build:** `npm run build`
**Frontend test:** `npx vitest run`

---

## File Structure

| File | Responsibility | Change |
|------|---------------|--------|
| `src-tauri/src/engine/state.rs` | `SaveState` struct + `save_state()`/`restore_save()` on GameState | Modify |
| `src-tauri/src/lib.rs` | `get_saved_state`/`save_game` IPC commands, save hooks | Modify |
| `src/lib/ipc.ts` | `getSavedState()` wrapper | Modify |
| `src/lib/types.ts` | `SavedState` interface | Modify |
| `src/App.svelte` | Auto-resume on mount | Modify |

---

### Task 1: SaveState struct and round-trip serialization

Define `SaveState`, add `save_state()` method to `GameState`, and test round-trip serialization.

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/engine/state.rs` in the existing `mod tests` block (or create one if none exists):

```rust
#[cfg(test)]
mod save_tests {
    use super::*;
    use crate::item::types::ItemStack;

    #[test]
    fn save_state_round_trip() {
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: -500.0,
            y: -100.0,
            facing: Direction::Right,
            inventory: vec![
                Some(ItemStack { item_id: "cherry".to_string(), count: 5 }),
                None,
                Some(ItemStack { item_id: "grain".to_string(), count: 2 }),
            ],
        };
        let json = serde_json::to_string(&save).unwrap();
        let loaded: SaveState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.street_id, "demo_meadow");
        assert!((loaded.x - (-500.0)).abs() < f64::EPSILON);
        assert!((loaded.y - (-100.0)).abs() < f64::EPSILON);
        assert_eq!(loaded.facing, Direction::Right);
        assert_eq!(loaded.inventory.len(), 3);
        assert_eq!(loaded.inventory[0].as_ref().unwrap().item_id, "cherry");
        assert!(loaded.inventory[1].is_none());
    }

    #[test]
    fn save_state_uses_camel_case() {
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Left,
            inventory: vec![Some(ItemStack { item_id: "cherry".to_string(), count: 1 })],
        };
        let json = serde_json::to_string(&save).unwrap();
        assert!(json.contains("\"streetId\""), "Should use camelCase: {json}");
        assert!(json.contains("\"itemId\""), "ItemStack should use camelCase: {json}");
        assert!(!json.contains("\"street_id\""), "Should not use snake_case: {json}");
    }

    #[test]
    fn empty_inventory_round_trip() {
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Left,
            inventory: vec![None; 16],
        };
        let json = serde_json::to_string(&save).unwrap();
        let loaded: SaveState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.inventory.len(), 16);
        assert!(loaded.inventory.iter().all(|s| s.is_none()));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch save_state_round_trip -- --nocapture`
Expected: FAIL — `SaveState` not defined

- [ ] **Step 3: Define `SaveState` and `save_state()` method**

Add to `src-tauri/src/engine/state.rs`, near the top (after the existing imports and before `GameState`):

```rust
use crate::item::types::ItemStack;

/// Minimal player state for save/load. Does not include per-street entity
/// state — streets reset on re-entry (matching Glitch's ephemeral world).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveState {
    pub street_id: String,
    pub x: f64,
    pub y: f64,
    pub facing: Direction,
    pub inventory: Vec<Option<ItemStack>>,
}
```

Add to the `impl GameState` block:

```rust
/// Extract the current save-worthy state.
/// Returns None if no street is loaded (nothing to save).
pub fn save_state(&self) -> Option<SaveState> {
    let street = self.street.as_ref()?;
    Some(SaveState {
        street_id: self
            .tsid_to_name
            .get(&street.tsid)
            .cloned()
            .unwrap_or_else(|| street.tsid.clone()),
        x: self.player.x,
        y: self.player.y,
        facing: self.facing,
        inventory: self.inventory.slots.clone(),
    })
}

/// Restore saved state after a street has been loaded.
/// Position is clamped to street bounds to prevent stuck players.
pub fn restore_save(&mut self, save: &SaveState) {
    if let Some(ref street) = self.street {
        self.player.x = save.x.clamp(street.left + 1.0, street.right - 1.0);
        self.player.y = save.y.clamp(street.top + 1.0, street.bottom);
    } else {
        self.player.x = save.x;
        self.player.y = save.y;
    }
    self.facing = save.facing;
    // Restore inventory slots, preserving the existing capacity.
    let capacity = self.inventory.capacity;
    self.inventory.slots = save.inventory.clone();
    self.inventory.slots.resize(capacity, None);
}
```

Note: `ItemStack` may already be imported in this file. Check and add `use crate::item::types::ItemStack;` only if not already present. Also check if `player.x` and `player.y` are `pub` on `PhysicsBody` — they are (`pub x: f64, pub y: f64`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -p harmony-glitch save_state -- --nocapture`
Expected: ALL PASS

- [ ] **Step 5: Run clippy**

Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: No errors

- [ ] **Step 6: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/engine/state.rs
git commit -m "feat(persistence): add SaveState struct with save_state/restore_save on GameState"
```

---

### Task 2: Save/load file I/O

Add functions to write and read `SaveState` to/from disk, following the same pattern as `identity/persistence.rs`.

**Files:**
- Modify: `src-tauri/src/engine/state.rs` (add file I/O functions and tests)

- [ ] **Step 1: Write the failing tests**

Add to the `save_tests` module:

```rust
#[test]
fn write_and_read_save_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("savegame.json");

    let save = SaveState {
        street_id: "demo_meadow".to_string(),
        x: 100.0,
        y: -50.0,
        facing: Direction::Right,
        inventory: vec![
            Some(ItemStack { item_id: "cherry".to_string(), count: 3 }),
            None,
        ],
    };

    write_save_state(&path, &save).unwrap();
    let loaded = read_save_state(&path).unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.street_id, "demo_meadow");
    assert!((loaded.x - 100.0).abs() < f64::EPSILON);
}

#[test]
fn missing_save_file_returns_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.json");
    let result = read_save_state(&path);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn corrupted_save_file_returns_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("savegame.json");
    std::fs::write(&path, "not valid json!!!").unwrap();
    let result = read_save_state(&path);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test -p harmony-glitch write_and_read_save -- --nocapture`
Expected: FAIL — functions not defined

- [ ] **Step 3: Implement `write_save_state` and `read_save_state`**

Add as free functions in `src-tauri/src/engine/state.rs` (outside the `impl GameState` block, near the bottom before tests):

```rust
/// Write a save state to disk as pretty-printed JSON.
pub fn write_save_state(path: &std::path::Path, save: &SaveState) -> Result<(), String> {
    let json = serde_json::to_string_pretty(save).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Read a save state from disk. Returns Ok(None) if the file is missing
/// or corrupted (graceful degradation — player sees street picker).
pub fn read_save_state(path: &std::path::Path) -> Result<Option<SaveState>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let json = match std::fs::read_to_string(path) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[persistence] Failed to read save file: {e}");
            return Ok(None);
        }
    };
    match serde_json::from_str::<SaveState>(&json) {
        Ok(save) => Ok(Some(save)),
        Err(e) => {
            eprintln!("[persistence] Corrupted save file: {e}");
            Ok(None)
        }
    }
}
```

Also add `use tempfile;` in the test module (add `tempfile` to dev-dependencies in Cargo.toml if not already present — it should be, since `identity/persistence.rs` tests already use it).

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test -p harmony-glitch save_tests -- --nocapture`
Expected: ALL PASS

- [ ] **Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/engine/state.rs src-tauri/Cargo.toml
git commit -m "feat(persistence): add save file read/write with graceful corruption handling"
```

---

### Task 3: IPC commands — get_saved_state and save hooks

Wire the save/load into Tauri IPC: new `get_saved_state` command, save on `load_street` and `stop_game`, restore on `load_street` when save data is available.

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add `get_saved_state` IPC command**

Add to `src-tauri/src/lib.rs`:

```rust
#[tauri::command]
fn get_saved_state(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    let pi = app.state::<PlayerIdentityWrapper>();
    let save_path = pi.data_dir.join("savegame.json");
    let save = crate::engine::state::read_save_state(&save_path)?;
    match save {
        Some(s) => {
            // Validate street_id is loadable before returning.
            if load_street_xml(&s.street_id).is_err() {
                return Ok(None);
            }
            Ok(Some(serde_json::to_value(&s).map_err(|e| e.to_string())?))
        }
        None => Ok(None),
    }
}
```

Register it in `invoke_handler`:

```rust
.invoke_handler(tauri::generate_handler![
    list_streets,
    load_street,
    // ... existing commands ...
    get_saved_state,  // ADD THIS
])
```

- [ ] **Step 2: Add save helper function**

```rust
/// Save the current game state to disk. Non-fatal on failure.
fn save_current_state(app: &AppHandle) {
    let state_wrapper = app.state::<GameStateWrapper>();
    let state = match state_wrapper.0.lock() {
        Ok(s) => s,
        Err(_) => return,
    };
    let save = match state.save_state() {
        Some(s) => s,
        None => return, // No street loaded — nothing to save
    };
    drop(state); // Release lock before file I/O

    let pi = app.state::<PlayerIdentityWrapper>();
    let save_path = pi.data_dir.join("savegame.json");
    if let Err(e) = crate::engine::state::write_save_state(&save_path, &save) {
        eprintln!("[persistence] Save failed: {e}");
    }
}
```

- [ ] **Step 3: Add save hook in `load_street`**

In the `load_street` IPC command, BEFORE the `state.load_street(...)` call, add:

```rust
// Save current state before loading a new street (captures position on old street).
save_current_state(&app);
```

- [ ] **Step 4: Add save hook in `stop_game`**

At the beginning of `stop_game`, before resetting input:

```rust
// Save state on game stop (app exit or Back button).
save_current_state(&app);
```

- [ ] **Step 5: Add restore logic in `load_street`**

The `load_street` command needs to accept an optional save restoration. Add a new command `load_street_with_save` or modify the existing one to accept an optional position.

The simplest approach: add a separate `restore_saved_state` command that applies the save data after `load_street` returns:

```rust
#[tauri::command]
fn restore_saved_state(x: f64, y: f64, facing: String, inventory: Vec<Option<crate::item::types::ItemStack>>, app: AppHandle) -> Result<(), String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let facing_dir = match facing.as_str() {
        "left" => crate::avatar::types::Direction::Left,
        _ => crate::avatar::types::Direction::Right,
    };

    let save = crate::engine::state::SaveState {
        street_id: String::new(), // Not needed for restore
        x,
        y,
        facing: facing_dir,
        inventory,
    };
    state.restore_save(&save);
    Ok(())
}
```

Register in `invoke_handler`.

- [ ] **Step 6: Add window close save hook**

In the `setup` closure (where identity is loaded), add a window close event handler:

```rust
// Save on window close.
let app_handle = app.handle().clone();
app.on_window_event(move |_window, event| {
    if let tauri::WindowEvent::CloseRequested { .. } = event {
        save_current_state(&app_handle);
    }
});
```

- [ ] **Step 7: Run tests and clippy**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: ALL PASS, no clippy errors

- [ ] **Step 8: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/lib.rs src-tauri/src/engine/state.rs
git commit -m "feat(persistence): add IPC commands and save hooks for game state"
```

---

### Task 4: Frontend types and IPC wrappers

Add TypeScript types and IPC functions for save/load.

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add `SavedState` type**

Add to `src/lib/types.ts`:

```typescript
export interface SavedState {
  streetId: string;
  x: number;
  y: number;
  facing: string;
  inventory: (ItemStack | null)[];
}

export interface ItemStack {
  itemId: string;
  count: number;
}
```

Check if `ItemStack` already exists in `types.ts`. If so, reuse it. If only `ItemStackFrame` exists (with extra fields like `name`, `description`), create the minimal `ItemStack` separately.

- [ ] **Step 2: Add IPC wrappers**

Add to `src/lib/ipc.ts`:

```typescript
import type { SavedState } from './types';

export async function getSavedState(): Promise<SavedState | null> {
  return invoke<SavedState | null>('get_saved_state');
}

export async function restoreSavedState(
  x: number,
  y: number,
  facing: string,
  inventory: (ItemStack | null)[]
): Promise<void> {
  return invoke('restore_saved_state', { x, y, facing, inventory });
}
```

Update the import line at the top of `ipc.ts` to include the new types.

- [ ] **Step 3: Verify frontend builds**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src/lib/types.ts src/lib/ipc.ts
git commit -m "feat(persistence): add frontend SavedState type and IPC wrappers"
```

---

### Task 5: Auto-resume in App.svelte

On mount, check for a saved state. If it exists, skip the street picker and auto-load the saved street with restored position/inventory.

**Files:**
- Modify: `src/App.svelte`

- [ ] **Step 1: Import the new IPC functions**

Update the import line in `App.svelte`:

```typescript
import { stopGame, loadStreet, getIdentity, streetTransitionReady, getRecipes, getSavedState, restoreSavedState, startGame } from './lib/ipc';
```

Note: `startGame` may already be imported elsewhere in the file. Check and add if needed. `loadStreet` is already imported.

- [ ] **Step 2: Add auto-resume logic in `onMount`**

After the identity check and recipe loading (around line 43), add:

```typescript
// Auto-resume from save file if available.
if (identityReady) {
  try {
    const saved = await getSavedState();
    if (saved) {
      const street = await loadStreet(saved.streetId);
      await restoreSavedState(saved.x, saved.y, saved.facing, saved.inventory);
      await startGame();
      // Recreate audio if needed
      if (!audioManager && cachedKit) {
        try {
          audioManager = new AudioManager(cachedKit, '/assets/audio/');
        } catch (e) {
          console.error('Failed to recreate audio:', e);
        }
      }
      currentStreet = street;
    }
  } catch (e) {
    console.error('Auto-resume failed, showing street picker:', e);
    // Fall through to street picker
  }
}
```

- [ ] **Step 3: Verify frontend builds**

Run: `npm run build`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src/App.svelte
git commit -m "feat(persistence): auto-resume from save file on app launch"
```

---

### Task 6: Final integration test and cleanup

Run full test suite, verify save/load works end-to-end, clean up.

**Files:**
- Modify: `src-tauri/src/engine/state.rs` (add restore_save test)

- [ ] **Step 1: Add restore_save test**

Add to the `save_tests` module:

```rust
#[test]
fn restore_save_clamps_position() {
    let item_defs = crate::item::types::ItemDefs::new();
    let entity_defs = crate::item::types::EntityDefs::new();
    let recipe_defs = crate::item::types::RecipeDefs::new();
    let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, recipe_defs);

    // Load a street with bounds l=-2000, r=2000, t=-1000, b=0
    let xml = include_str!("../../assets/streets/demo_meadow.xml");
    let street = crate::street::parser::parse_street(xml).unwrap();
    state.load_street(street, vec![], vec![]);

    // Try to restore a position outside bounds.
    let save = SaveState {
        street_id: "demo_meadow".to_string(),
        x: 99999.0,
        y: -99999.0,
        facing: Direction::Left,
        inventory: vec![],
    };
    state.restore_save(&save);

    // Position should be clamped to street bounds.
    let street = state.street.as_ref().unwrap();
    assert!(state.player.x <= street.right - 1.0);
    assert!(state.player.y >= street.top + 1.0);
    assert_eq!(state.facing, Direction::Left);
}

#[test]
fn restore_save_fills_inventory() {
    let item_defs = crate::item::types::ItemDefs::new();
    let entity_defs = crate::item::types::EntityDefs::new();
    let recipe_defs = crate::item::types::RecipeDefs::new();
    let mut state = GameState::new(1280.0, 720.0, item_defs, entity_defs, recipe_defs);
    assert_eq!(state.inventory.slots.len(), 16); // Default capacity

    let save = SaveState {
        street_id: "demo_meadow".to_string(),
        x: 0.0,
        y: 0.0,
        facing: Direction::Right,
        inventory: vec![
            Some(ItemStack { item_id: "cherry".to_string(), count: 5 }),
        ],
    };
    state.restore_save(&save);

    // Should have 16 slots (capacity preserved), first has cherry.
    assert_eq!(state.inventory.slots.len(), 16);
    assert_eq!(state.inventory.slots[0].as_ref().unwrap().item_id, "cherry");
    assert!(state.inventory.slots[1].is_none());
}
```

- [ ] **Step 2: Run full Rust test suite**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Expected: ALL PASS

- [ ] **Step 3: Run clippy**

Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: Clean

- [ ] **Step 4: Run frontend build**

Run: `npm run build`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add -A
git commit -m "test(persistence): add restore_save clamping and inventory tests"
```
