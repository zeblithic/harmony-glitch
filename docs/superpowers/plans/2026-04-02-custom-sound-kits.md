# Custom Sound Kit Loading Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow players to load custom sound kits from a local directory and select between them via a dropdown in the audio settings panel.

**Architecture:** Rust backend discovers kits in `<app_data_dir>/sound-kits/`, serves custom kit audio files via a `soundkit://` URI scheme protocol. Frontend reads kit manifests via IPC, builds AudioManager instances with the correct base path, and persists selection to localStorage. VolumeSettings panel gains a kit selector dropdown.

**Tech Stack:** Rust (Tauri v2 commands + URI scheme protocol), TypeScript, Svelte 5, Howler.js, Vitest

**Spec:** `docs/superpowers/specs/2026-04-02-custom-sound-kits-design.md`

---

## File Map

| Action | File | Responsibility |
|--------|------|---------------|
| Modify | `src-tauri/src/lib.rs` | New commands (`list_sound_kits`, `read_sound_kit`), URI protocol, sound-kits dir init |
| Modify | `src/lib/types.ts` | Add `SoundKitMeta` type |
| Modify | `src/lib/ipc.ts` | Add `listSoundKits()`, `readSoundKit()` IPC wrappers |
| Modify | `src/lib/engine/audio.ts` | Add `cas` to `SoundKit`, new `kitBasePath()`, update `loadSoundKit()` |
| Modify | `src/lib/engine/audio.test.ts` | Tests for `kitBasePath` and updated `loadSoundKit` |
| Modify | `src/lib/components/VolumeSettings.svelte` | Kit selector dropdown, rename header |
| Modify | `src/lib/components/VolumeSettings.test.ts` | Tests for kit selector UI |
| Modify | `src/App.svelte` | Kit list state, `switchKit()`, wire props to VolumeSettings |

---

### Task 1: Rust — SoundKitMeta type and list_sound_kits command

**Files:**
- Modify: `src-tauri/src/lib.rs:1-8` (add use statements)
- Modify: `src-tauri/src/lib.rs:44-46` (add SoundKitsDir wrapper)
- Modify: `src-tauri/src/lib.rs:53-58` (add commands after list_streets)
- Modify: `src-tauri/src/lib.rs:577-608` (setup: create sound-kits dir, manage it)
- Modify: `src-tauri/src/lib.rs:615-630` (register new commands)

- [ ] **Step 1: Add SoundKitMeta type and SoundKitsDir wrapper**

After the existing `PlayerIdentityWrapper` struct (around line 45), add:

```rust
/// Path to the sound-kits directory, created on startup.
struct SoundKitsDir(std::path::PathBuf);

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SoundKitMeta {
    id: String,
    name: String,
}
```

- [ ] **Step 2: Add list_sound_kits command**

After the `list_streets` command (around line 58), add:

```rust
#[tauri::command]
fn list_sound_kits(app: AppHandle) -> Result<Vec<SoundKitMeta>, String> {
    let kits_dir = app.state::<SoundKitsDir>();
    let mut kits = vec![SoundKitMeta {
        id: "default".to_string(),
        name: "Default".to_string(),
    }];

    let entries = match std::fs::read_dir(&kits_dir.0) {
        Ok(e) => e,
        Err(_) => return Ok(kits), // Directory missing/unreadable — just return default
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let kit_json = path.join("kit.json");
        if !kit_json.exists() {
            continue;
        }
        let content = match std::fs::read_to_string(&kit_json) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[sound-kits] Failed to read {}: {e}",
                    kit_json.display()
                );
                continue;
            }
        };
        // Parse just enough to get the name
        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "[sound-kits] Invalid JSON in {}: {e}",
                    kit_json.display()
                );
                continue;
            }
        };
        let name = parsed
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unnamed")
            .to_string();
        let id = entry
            .file_name()
            .to_string_lossy()
            .to_string();

        kits.push(SoundKitMeta { id, name });
    }

    Ok(kits)
}
```

- [ ] **Step 3: Create sound-kits directory in setup and manage SoundKitsDir**

In the `.setup(|app| { ... })` closure, after the `data_dir` line (line 578), add:

```rust
            let kits_dir = data_dir.join("sound-kits");
            if let Err(e) = std::fs::create_dir_all(&kits_dir) {
                eprintln!("[sound-kits] Failed to create {}: {e}", kits_dir.display());
            }
            app.manage(SoundKitsDir(kits_dir));
```

- [ ] **Step 4: Register list_sound_kits in invoke_handler**

In the `tauri::generate_handler!` macro (around line 615), add `list_sound_kits` after `list_streets`:

```rust
        .invoke_handler(tauri::generate_handler![
            list_streets,
            list_sound_kits,
            // ... rest unchanged
        ])
```

- [ ] **Step 5: Run Rust tests to verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles with no errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(audio): add list_sound_kits command and SoundKitMeta type

Scans <app_data>/sound-kits/ for custom kit directories.
Always includes the built-in Default kit first."
```

---

### Task 2: Rust — read_sound_kit command with path traversal validation

**Files:**
- Modify: `src-tauri/src/lib.rs` (add read_sound_kit after list_sound_kits, register in handler)

- [ ] **Step 1: Add kit ID validation helper**

Above the `list_sound_kits` function, add:

```rust
/// Validate a sound kit ID: alphanumeric, hyphens, and underscores only.
/// Rejects path traversal attempts.
fn validate_kit_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("Kit ID must not be empty".to_string());
    }
    if id.contains('.') || id.contains('/') || id.contains('\\') {
        return Err(format!("Invalid kit ID: {id}"));
    }
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(format!("Invalid kit ID: {id}"));
    }
    Ok(())
}
```

- [ ] **Step 2: Add read_sound_kit command**

After `list_sound_kits`, add:

```rust
#[tauri::command]
fn read_sound_kit(kit_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    if kit_id == "default" {
        let kit: serde_json::Value =
            serde_json::from_str(include_str!("../../assets/audio/default-kit.json"))
                .map_err(|e| format!("Failed to parse bundled kit: {e}"))?;
        return Ok(kit);
    }

    validate_kit_id(&kit_id)?;

    let kits_dir = app.state::<SoundKitsDir>();
    let kit_path = kits_dir.0.join(&kit_id).join("kit.json");

    let content =
        std::fs::read_to_string(&kit_path).map_err(|e| format!("Kit '{kit_id}' not found: {e}"))?;
    let kit: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid kit manifest: {e}"))?;

    Ok(kit)
}
```

- [ ] **Step 3: Register read_sound_kit in invoke_handler**

Add `read_sound_kit` after `list_sound_kits` in `tauri::generate_handler!`.

- [ ] **Step 4: Write Rust tests for validate_kit_id**

At the bottom of `src-tauri/src/lib.rs`, inside an existing or new `#[cfg(test)] mod tests { ... }` block, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_kit_id_accepts_valid() {
        assert!(validate_kit_id("retro-kit").is_ok());
        assert!(validate_kit_id("my_kit_2").is_ok());
        assert!(validate_kit_id("Default").is_ok());
    }

    #[test]
    fn validate_kit_id_rejects_path_traversal() {
        assert!(validate_kit_id("..").is_err());
        assert!(validate_kit_id("../etc").is_err());
        assert!(validate_kit_id("foo/bar").is_err());
        assert!(validate_kit_id("foo\\bar").is_err());
    }

    #[test]
    fn validate_kit_id_rejects_empty() {
        assert!(validate_kit_id("").is_err());
    }

    #[test]
    fn validate_kit_id_rejects_dots() {
        assert!(validate_kit_id("my.kit").is_err());
    }

    #[test]
    fn read_default_kit_parses() {
        let json: serde_json::Value =
            serde_json::from_str(include_str!("../../assets/audio/default-kit.json"))
                .expect("bundled default-kit.json must be valid JSON");
        assert_eq!(json["name"], "Default");
        assert!(json["events"]["jump"]["default"].is_string());
    }
}
```

- [ ] **Step 5: Run Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass including the new `validate_kit_id_*` and `read_default_kit_parses` tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(audio): add read_sound_kit command with path traversal validation

Returns bundled kit for 'default', reads from sound-kits dir for custom IDs.
Kit IDs are validated: alphanumeric, hyphens, underscores only."
```

---

### Task 3: Rust — Register soundkit:// URI scheme protocol

**Files:**
- Modify: `src-tauri/src/lib.rs` (add protocol registration on Builder)

- [ ] **Step 1: Add soundkit:// protocol handler**

On the `tauri::Builder::default()` chain, before `.manage(MonotonicEpoch(...))`, add the protocol registration. The `UriSchemeContext` provides `app_handle()` which gives us `app.path().app_data_dir()` — no need for pre-computed paths:

```rust
pub fn run() {
    tauri::Builder::default()
        .register_uri_scheme_protocol("soundkit", |ctx, request| {
            let app = ctx.app_handle();
            let data_dir = match app.path().app_data_dir() {
                Ok(d) => d,
                Err(_) => {
                    return http::Response::builder()
                        .status(500)
                        .body(Vec::new().into())
                        .unwrap();
                }
            };
            let kits_dir = data_dir.join("sound-kits");

            // Parse path from URL: /kit-id/relative/path/to/file.mp3
            let uri_path = request.uri().path();
            let trimmed = uri_path.trim_start_matches('/');
            let (kit_id, file_path) = match trimmed.split_once('/') {
                Some((k, f)) => (k, f),
                None => {
                    return http::Response::builder()
                        .status(400)
                        .body(b"Invalid path".to_vec().into())
                        .unwrap();
                }
            };

            // Validate kit ID
            if validate_kit_id(kit_id).is_err() {
                return http::Response::builder()
                    .status(403)
                    .body(b"Invalid kit ID".to_vec().into())
                    .unwrap();
            }

            // Reject path traversal in file path
            if file_path.contains("..") {
                return http::Response::builder()
                    .status(403)
                    .body(b"Path traversal rejected".to_vec().into())
                    .unwrap();
            }

            let full_path = kits_dir.join(kit_id).join(file_path);

            // Verify the resolved path is still within the kit directory
            let canonical_kits = match kits_dir.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    return http::Response::builder()
                        .status(404)
                        .body(b"Kits directory not found".to_vec().into())
                        .unwrap();
                }
            };
            let canonical_file = match full_path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    return http::Response::builder()
                        .status(404)
                        .body(b"File not found".to_vec().into())
                        .unwrap();
                }
            };
            if !canonical_file.starts_with(&canonical_kits) {
                return http::Response::builder()
                    .status(403)
                    .body(b"Access denied".to_vec().into())
                    .unwrap();
            }

            let bytes = match std::fs::read(&full_path) {
                Ok(b) => b,
                Err(_) => {
                    return http::Response::builder()
                        .status(404)
                        .body(b"File not found".to_vec().into())
                        .unwrap();
                }
            };

            let mime = match full_path.extension().and_then(|e| e.to_str()) {
                Some("mp3") => "audio/mpeg",
                Some("ogg") => "audio/ogg",
                Some("wav") => "audio/wav",
                Some("flac") => "audio/flac",
                _ => "application/octet-stream",
            };

            http::Response::builder()
                .status(200)
                .header("Content-Type", mime)
                .header("Access-Control-Allow-Origin", "*")
                .body(bytes.into())
                .unwrap()
        })
        .manage(MonotonicEpoch(Instant::now()))
        // ... rest of builder chain unchanged
```

- [ ] **Step 2: Add `use tauri::http` at top of file**

At the top of `lib.rs`, after the existing `use` statements (around line 21):

```rust
use tauri::http;
```

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(audio): register soundkit:// URI protocol for custom kit audio

Serves audio files from <app_data>/sound-kits/<kit-id>/ via soundkit:// URLs.
Validates kit IDs, rejects path traversal, sets correct MIME types."
```

---

### Task 4: Frontend — Add SoundKitMeta type and IPC wrappers

**Files:**
- Modify: `src/lib/types.ts:262` (add SoundKitMeta after AudioEvent)
- Modify: `src/lib/ipc.ts:78` (add listSoundKits, readSoundKit)

- [ ] **Step 1: Add SoundKitMeta to types.ts**

At the end of `src/lib/types.ts` (after the `AudioEvent` type, line 261), add:

```typescript
export interface SoundKitMeta {
  id: string;
  name: string;
}
```

- [ ] **Step 2: Add IPC wrappers to ipc.ts**

At the end of `src/lib/ipc.ts`, add:

```typescript
export async function listSoundKits(): Promise<SoundKitMeta[]> {
  return invoke<SoundKitMeta[]>('list_sound_kits');
}

export async function readSoundKit(kitId: string): Promise<SoundKit> {
  return invoke<SoundKit>('read_sound_kit', { kitId });
}
```

- [ ] **Step 3: Add missing imports to ipc.ts**

Update the import on line 3 to include the new types:

```typescript
import type { StreetData, InputState, RenderFrame, NetworkStatus, PlayerIdentity, ChatEvent, RecipeDef, SavedState, SoundKitMeta } from './types';
import type { SoundKit } from './engine/audio';
```

- [ ] **Step 4: Verify TypeScript compilation**

Run: `npx tsc --noEmit`
Expected: No type errors.

- [ ] **Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/ipc.ts
git commit -m "feat(audio): add SoundKitMeta type and IPC wrappers for kit commands"
```

---

### Task 5: Frontend — Update audio.ts with kitBasePath and updated loadSoundKit

**Files:**
- Modify: `src/lib/engine/audio.ts:9-16` (add `cas` to SoundKit)
- Modify: `src/lib/engine/audio.ts:270-276` (update loadSoundKit, add kitBasePath)
- Test: `src/lib/engine/audio.test.ts`

- [ ] **Step 1: Write failing tests for kitBasePath**

At the end of `src/lib/engine/audio.test.ts`, add a new describe block:

```typescript
describe('kitBasePath', () => {
  it('returns /assets/audio/ for default kit', () => {
    expect(kitBasePath('default')).toBe('/assets/audio/');
  });

  it('returns soundkit:// URL for custom kit', () => {
    expect(kitBasePath('retro-kit')).toBe('soundkit://localhost/retro-kit/');
  });
});
```

Update the imports at line 22-23 to also import `kitBasePath`:

```typescript
import { AudioManager, kitBasePath } from './audio';
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/lib/engine/audio.test.ts`
Expected: FAIL — `kitBasePath` is not exported from `./audio`.

- [ ] **Step 3: Add `cas` field to SoundKit and implement kitBasePath**

In `src/lib/engine/audio.ts`, update the `SoundKit` interface (lines 9-16):

```typescript
export interface SoundKit {
  name: string;
  version: number;
  cas?: string | null;
  sfxVolume: number;
  ambientVolume: number;
  events: Record<string, SoundEntry>;
  ambient: SoundEntry;
}
```

Replace the `loadSoundKit` function (lines 270-276) with:

```typescript
export function kitBasePath(kitId: string): string {
  if (kitId === 'default') return '/assets/audio/';
  return `soundkit://localhost/${kitId}/`;
}

export async function loadSoundKit(kitId: string): Promise<SoundKit> {
  if (kitId === 'default') {
    const response = await fetch('/assets/audio/default-kit.json');
    if (!response.ok) {
      throw new Error(`Failed to load default sound kit: ${response.status}`);
    }
    return response.json();
  }
  const { readSoundKit } = await import('../ipc');
  return readSoundKit(kitId);
}
```

Note: The dynamic import of `readSoundKit` avoids a circular dependency and keeps the default kit path working in test environments without Tauri IPC.

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/lib/engine/audio.test.ts`
Expected: All tests pass including the new `kitBasePath` tests.

- [ ] **Step 5: Commit**

```bash
git add src/lib/engine/audio.ts src/lib/engine/audio.test.ts
git commit -m "feat(audio): add kitBasePath helper and update loadSoundKit for custom kits

kitBasePath returns /assets/audio/ for default, soundkit:// URL for custom kits.
loadSoundKit now accepts a kitId and uses IPC for custom kits."
```

---

### Task 6: Frontend — Kit selector in VolumeSettings

**Files:**
- Modify: `src/lib/components/VolumeSettings.svelte`
- Test: `src/lib/components/VolumeSettings.test.ts`

- [ ] **Step 1: Write failing tests for kit selector**

At the end of the existing `describe('VolumeSettings', ...)` block in `VolumeSettings.test.ts`, add:

```typescript
  it('renders kit selector with available kits', () => {
    const kits = [
      { id: 'default', name: 'Default' },
      { id: 'retro', name: 'Retro Kit' },
    ];
    render(VolumeSettings, {
      props: {
        audioManager: makeAudioManager(),
        visible: true,
        soundKits: kits,
        selectedKitId: 'default',
      },
    });
    const select = screen.getByLabelText('Sound Kit');
    expect(select).toBeDefined();
    expect(select.querySelectorAll('option')).toHaveLength(2);
  });

  it('calls onKitChange when kit selection changes', async () => {
    const kits = [
      { id: 'default', name: 'Default' },
      { id: 'retro', name: 'Retro Kit' },
    ];
    const onKitChange = vi.fn();
    render(VolumeSettings, {
      props: {
        audioManager: makeAudioManager(),
        visible: true,
        soundKits: kits,
        selectedKitId: 'default',
        onKitChange,
      },
    });
    const select = screen.getByLabelText('Sound Kit') as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'retro' } });
    expect(onKitChange).toHaveBeenCalledWith('retro');
  });

  it('renders panel header as Audio Settings', () => {
    render(VolumeSettings, {
      props: {
        audioManager: makeAudioManager(),
        visible: true,
        soundKits: [{ id: 'default', name: 'Default' }],
        selectedKitId: 'default',
      },
    });
    const dialog = screen.getByRole('dialog');
    expect(dialog.textContent).toContain('Audio Settings');
  });

  it('handles empty soundKits gracefully', () => {
    expect(() => {
      render(VolumeSettings, {
        props: {
          audioManager: makeAudioManager(),
          visible: true,
          soundKits: [],
          selectedKitId: 'default',
        },
      });
    }).not.toThrow();
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npx vitest run src/lib/components/VolumeSettings.test.ts`
Expected: FAIL — `soundKits` prop not recognized, no "Sound Kit" label found.

- [ ] **Step 3: Update VolumeSettings.svelte**

Update the props block (lines 4-12):

```typescript
  let {
    audioManager,
    visible = false,
    soundKits = [],
    selectedKitId = 'default',
    onClose,
    onKitChange,
  }: {
    audioManager: AudioManager | null;
    visible: boolean;
    soundKits?: SoundKitMeta[];
    selectedKitId?: string;
    onClose?: () => void;
    onKitChange?: (kitId: string) => void;
  } = $props();
```

Add the import at the top of the script block (after the AudioManager import):

```typescript
  import type { SoundKitMeta } from '../types';
```

Change the panel header `<h2>` from `Volume` to `Audio Settings` (line 85):

```svelte
      <h2>Audio Settings</h2>
```

Update the close button `aria-label` (line 86):

```svelte
      <button type="button" class="close-btn" aria-label="Close audio settings" onclick={() => onClose?.()}>
```

Update the dialog `aria-label` (line 79):

```svelte
    aria-label="Audio Settings"
```

Add the kit selector section between `</div>` (panel-header closing, line 88) and `<div class="channels">` (line 90):

```svelte
    {#if soundKits.length > 0}
      <div class="kit-selector">
        <label for="kit-select">Sound Kit</label>
        <select
          id="kit-select"
          value={selectedKitId}
          onchange={(e) => onKitChange?.((e.target as HTMLSelectElement).value)}
        >
          {#each soundKits as kit (kit.id)}
            <option value={kit.id}>{kit.name}</option>
          {/each}
        </select>
        <p class="kit-hint">
          Place custom kits in your sound-kits folder. CAS bundle support coming soon.
        </p>
      </div>
    {/if}
```

Add CSS for the kit selector (inside the `<style>` block):

```css
  .kit-selector {
    margin-bottom: 14px;
    padding-bottom: 14px;
    border-bottom: 1px solid #333;
  }

  .kit-selector label {
    display: block;
    font-size: 0.75rem;
    color: #ccc;
    margin-bottom: 6px;
  }

  .kit-selector select {
    width: 100%;
    padding: 6px 8px;
    font-size: 0.8rem;
    background: rgba(40, 40, 70, 0.6);
    border: 1px solid #555;
    border-radius: 4px;
    color: #e0e0e0;
    cursor: pointer;
  }

  .kit-selector select:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: -2px;
  }

  .kit-hint {
    font-size: 0.6rem;
    color: #666;
    margin: 6px 0 0;
    line-height: 1.3;
  }
```

- [ ] **Step 4: Fix existing test that checks for "Volume" in dialog text**

In `VolumeSettings.test.ts`, update the test on line 36-43. The dialog text now contains "Audio Settings" instead of "Volume":

```typescript
  it('renders dialog with Audio Settings label when visible', () => {
    render(VolumeSettings, {
      props: { audioManager: makeAudioManager(), visible: true },
    });
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeDefined();
    expect(dialog.textContent).toContain('Audio Settings');
  });
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `npx vitest run src/lib/components/VolumeSettings.test.ts`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/VolumeSettings.svelte src/lib/components/VolumeSettings.test.ts
git commit -m "feat(audio): add kit selector to Audio Settings panel

Dropdown shows available kits, fires onKitChange callback.
Includes CAS stub hint text. Panel renamed from Volume to Audio Settings."
```

---

### Task 7: Frontend — Wire kit selection into App.svelte

**Files:**
- Modify: `src/App.svelte`

- [ ] **Step 1: Add kit state and imports**

Update the import from `audio.ts` (line 15):

```typescript
  import { AudioManager, loadSoundKit, kitBasePath, type SoundKit } from './lib/engine/audio';
```

Add import for `listSoundKits` from ipc (line 12):

```typescript
  import { stopGame, loadStreet, getIdentity, streetTransitionReady, getRecipes, getSavedState, listSoundKits } from './lib/ipc';
```

Add import for the type (after the existing type import on line 13):

```typescript
  import type { StreetData, RenderFrame, RecipeDef, SoundKitMeta } from './lib/types';
```

After the existing `let cachedKit` line (line 18), add:

```typescript
  let soundKits = $state<SoundKitMeta[]>([]);
  let selectedKitId = $state('default');
```

- [ ] **Step 2: Load kit list and restore selection on mount**

In the `onMount` callback, after the recipe loading block (around line 49) and before the audio initialization block, add:

```typescript
    // Load available sound kits
    try {
      soundKits = await listSoundKits();
    } catch (e) {
      console.error('Failed to list sound kits:', e);
      soundKits = [{ id: 'default', name: 'Default' }];
    }

    // Restore saved kit selection
    try {
      const savedKit = localStorage.getItem('selected-sound-kit');
      if (savedKit && soundKits.some((k) => k.id === savedKit)) {
        selectedKitId = savedKit;
      }
    } catch { /* localStorage unavailable */ }
```

Update the audio initialization block (around line 53-58) to use the selected kit:

```typescript
    // Initialize audio eagerly so handleStreetLoaded stays synchronous
    try {
      cachedKit = await loadSoundKit(selectedKitId);
      audioManager = new AudioManager(cachedKit, kitBasePath(selectedKitId));
    } catch (e) {
      console.error('Failed to initialize audio:', e);
      // Fall back to default if custom kit failed
      if (selectedKitId !== 'default') {
        selectedKitId = 'default';
        try {
          localStorage.setItem('selected-sound-kit', 'default');
        } catch { /* localStorage unavailable */ }
        try {
          cachedKit = await loadSoundKit('default');
          audioManager = new AudioManager(cachedKit, kitBasePath('default'));
        } catch (e2) {
          console.error('Fallback to default kit also failed:', e2);
        }
      }
    }
```

- [ ] **Step 3: Add switchKit function**

After the `handleFrame` function (around line 138), add:

```typescript
  async function switchKit(kitId: string) {
    selectedKitId = kitId;
    try {
      localStorage.setItem('selected-sound-kit', kitId);
    } catch { /* localStorage unavailable */ }

    try {
      const kit = await loadSoundKit(kitId);
      audioManager?.dispose();
      cachedKit = kit;
      audioManager = new AudioManager(kit, kitBasePath(kitId));
    } catch (e) {
      console.error(`Failed to load kit '${kitId}':`, e);
      // Fall back to default
      if (kitId !== 'default') {
        selectedKitId = 'default';
        try {
          localStorage.setItem('selected-sound-kit', 'default');
        } catch { /* localStorage unavailable */ }
        try {
          const fallback = await loadSoundKit('default');
          audioManager?.dispose();
          cachedKit = fallback;
          audioManager = new AudioManager(fallback, kitBasePath('default'));
        } catch (e2) {
          console.error('Fallback to default kit also failed:', e2);
        }
      }
    }
  }
```

- [ ] **Step 4: Update handleStreetLoaded to use selected kit**

In `handleStreetLoaded` (around line 84-94), update the AudioManager recreation:

```typescript
  function handleStreetLoaded(street: StreetData) {
    if (!audioManager && cachedKit) {
      try {
        audioManager = new AudioManager(cachedKit, kitBasePath(selectedKitId));
      } catch (e) {
        console.error('Failed to recreate audio:', e);
      }
    }
    currentStreet = street;
  }
```

- [ ] **Step 5: Wire new props to VolumeSettings**

Update the `<VolumeSettings>` component (around line 173-177):

```svelte
    <VolumeSettings
      {audioManager}
      visible={volumeOpen}
      {soundKits}
      {selectedKitId}
      onClose={() => { volumeOpen = false; }}
      onKitChange={switchKit}
    />
```

- [ ] **Step 6: Verify TypeScript compilation**

Run: `npx tsc --noEmit`
Expected: No type errors.

- [ ] **Step 7: Commit**

```bash
git add src/App.svelte
git commit -m "feat(audio): wire kit selection into App with switchKit and fallback

Loads kit list on mount, restores from localStorage, disposes and recreates
AudioManager on kit change. Falls back to default on failure."
```

---

### Task 8: Run full test suite and verify

**Files:** None (verification only)

- [ ] **Step 1: Run all frontend tests**

Run: `npx vitest run`
Expected: All tests pass.

- [ ] **Step 2: Run Rust tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass.

- [ ] **Step 3: Verify Rust compilation with clippy**

Run: `cd src-tauri && cargo clippy`
Expected: No warnings or errors.

- [ ] **Step 4: Final commit if any fixes were needed**

Only if fixes were required. Otherwise skip.

---

### Task 9: Update beads issue status and create PR

**Files:** None

- [ ] **Step 1: Mark issue as in-progress**

Run: `bd progress glitch-c79`

- [ ] **Step 2: Create PR**

Push branch and create PR with summary of all changes.
