# Custom Sound Kit Loading and Selection UI

**Issue:** glitch-c79
**Date:** 2026-04-02
**Status:** Approved

## Overview

Allow players to load custom sound kits from a local directory and select between them in the audio settings panel. Kits are full replacements (not overlays). CAS bundle support is stubbed for future integration with `glitch-irt`.

## Kit Directory Structure

Custom kits live in `<app_data_dir>/sound-kits/<kit-id>/`:

```
sound-kits/
  retro-kit/
    kit.json
    sfx/
      jump.mp3
      land.mp3
      pick-up.mp3
      ...
    ambient/
      meadow.mp3
      ...
```

**Kit ID** = directory name (alphanumeric, hyphens, underscores). The reserved ID `"default"` refers to the bundled kit at `/assets/audio/`.

### kit.json Schema

Same as the existing `SoundKit` interface, plus an optional `cas` field:

```json
{
  "name": "Retro Kit",
  "version": 1,
  "cas": null,
  "sfxVolume": 1.0,
  "ambientVolume": 0.5,
  "events": {
    "itemPickup": { "default": "sfx/pick-up.mp3" },
    "craftSuccess": { "default": "sfx/craft-success.mp3" },
    "actionFailed": { "default": "sfx/fail.mp3" },
    "jump": { "default": "sfx/jump.mp3" },
    "land": { "default": "sfx/land.mp3" },
    "transitionStart": { "default": "sfx/transition-start.mp3" },
    "transitionComplete": { "default": "sfx/transition-complete.mp3" },
    "entityInteract": {
      "default": "sfx/interact.mp3",
      "variants": { "fruit_tree": "sfx/harvest-tree.mp3" }
    }
  },
  "ambient": {
    "default": "ambient/meadow.mp3",
    "variants": { "LADEMO001": "ambient/meadow.mp3" }
  }
}
```

The `cas` field is `null` for locally-bundled kits. It will eventually hold a CAS bundle reference for network-loaded kits (see `glitch-irt`). Ignored at runtime for now.

## Rust Backend

### New Tauri Commands

**`list_sound_kits()`** returns `Vec<SoundKitMeta>`:
- Always includes `{ id: "default", name: "Default" }` first
- Scans `<app_data_dir>/sound-kits/` for subdirectories containing a valid `kit.json`
- Reads only `name` and `version` from each manifest
- Skips directories with missing/invalid `kit.json` (logs warning)
- Creates `sound-kits/` directory if absent

**`read_sound_kit(kitId)`** returns `SoundKit`:
- `"default"`: returns the bundled kit (compile-time `include_str!` of `assets/audio/default-kit.json`)
- Custom kits: reads `<app_data_dir>/sound-kits/<kit-id>/kit.json`
- Validates kit ID has no path traversal characters (`..`, `/`, `\`)
- Errors if kit not found or manifest invalid

### New Data Type

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SoundKitMeta {
    id: String,
    name: String,
}
```

### Custom URI Scheme Protocol

Registered as `soundkit://` via `register_uri_scheme_protocol`:

```
soundkit://localhost/<kit-id>/<relative-path>
```

- Extracts kit ID and relative file path from URL
- Validates: no `..` segments, kit ID is alphanumeric + hyphens/underscores
- Resolves to `<app_data_dir>/sound-kits/<kit-id>/<relative-path>`
- Returns file bytes with MIME type based on extension (`audio/mpeg` for .mp3, `audio/ogg` for .ogg, `audio/wav` for .wav)
- Returns 404 for missing files, 403 for path traversal attempts

## Frontend Changes

### audio.ts

- Add optional `cas: string | null` field to `SoundKit` interface
- `loadSoundKit(kitId: string)`: calls `read_sound_kit` IPC for custom kits, fetches `/assets/audio/default-kit.json` for `"default"`
- New `kitBasePath(kitId: string): string`: returns `/assets/audio/` for `"default"`, `soundkit://localhost/<kitId>/` for custom kits
- No changes to `AudioManager` — callers dispose and recreate with the new kit

### ipc.ts

- `listSoundKits()` invokes `list_sound_kits`
- `readSoundKit(kitId)` invokes `read_sound_kit`

### types.ts

- Add `SoundKitMeta`: `{ id: string; name: string }`

### App.svelte

- Fetch kit list on mount via `listSoundKits()`
- New state: `soundKits`, `selectedKitId`
- Load selected kit ID from `localStorage` key `"selected-sound-kit"` (fall back to `"default"`)
- New `switchKit(kitId)` function: saves to localStorage, loads manifest, disposes old AudioManager, creates new one
- Pass `soundKits`, `selectedKitId`, `onKitChange` to VolumeSettings

### VolumeSettings.svelte

- Renamed panel header: "Volume" becomes "Audio Settings"
- New props: `soundKits: SoundKitMeta[]`, `selectedKitId: string`, `onKitChange: (kitId: string) => void`
- "Sound Kit" section above volume sliders with a `<select>` dropdown
- Helper text below dropdown: "Place custom kits in your sound-kits folder. CAS bundle support coming soon."
- Panel `aria-label` updated to "Audio Settings"

### Persistence

- Selected kit ID: `localStorage` key `"selected-sound-kit"` (separate from `audio-prefs`)
- Volume preferences remain kit-independent

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Audio file missing in kit | Howler `onloaderror` logs warning; sound doesn't play |
| `read_sound_kit` fails (corrupt JSON, deleted kit) | Show error in selector, fall back to default kit, reset localStorage |
| Kit directory missing/empty | `list_sound_kits` creates directory, returns only default |
| Saved kit ID not in list on startup | Fall back to `"default"`, clear stale localStorage |
| Path traversal in kit ID or file path | Rust rejects with 403 (protocol) or error (command) |
| Hot-swap during gameplay | Brief silence gap (acceptable Phase A); ambient restarts on next `streetChanged` event (~16ms) |

## Testing

### Rust Tests

- `list_sound_kits` with empty directory returns only default
- `list_sound_kits` discovers valid kit, skips invalid directory
- `read_sound_kit("default")` returns bundled kit
- `read_sound_kit` rejects path traversal
- `SoundKitMeta` serialization

### Frontend Tests (audio.test.ts)

- `kitBasePath` returns correct URL for default vs custom
- `loadSoundKit` calls IPC for custom kits, fetches for default

### Frontend Tests (VolumeSettings.test.ts)

- Kit selector renders with available kits
- Changing selection calls `onKitChange`
- Panel header reads "Audio Settings"
- Handles single-kit list (only default)

### Manual Verification

- Switching kit disposes old AudioManager, creates new one
- Fallback to default on invalid saved kit
- localStorage persists selection across sessions

## Out of Scope

- In-app kit import via file picker (separate issue)
- CAS bundle resolution at runtime (deferred to `glitch-irt`)
- Kit overlay/merge semantics (may revisit later)
