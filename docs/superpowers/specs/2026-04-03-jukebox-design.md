# Mama Gogo's Zeblithic Jukebox — Roxy Music Integration

**Issue:** glitch-1s4
**Date:** 2026-04-03
**Status:** Approved

## Overview

In-world jukebox entities that play music tracks with distance-based audio falloff. Players walk up to a jukebox, interact to open a track picker, and control playback. Music files are CAS-stored, loaded via a `MusicSource` abstraction that resolves track IDs to audio URLs — local files now, Roxy streaming later.

The jukebox is a street entity (not an inventory item). It participates in the existing entity and interaction systems. A new Music audio channel gives players independent volume control over jukebox output.

## Architecture

Follows the existing Rust-owns-logic / PixiJS-renders / Svelte-does-UI split:

- **Rust:** Jukebox entity detection, playback state management, distance computation, AudioEvent emission
- **Frontend AudioManager:** Music channel, Howl lifecycle for active jukeboxes, distance-based volume
- **Svelte JukeboxPanel:** Track list, playback controls, now-playing display
- **MusicSource:** Abstraction layer for resolving track IDs to audio URLs

### Data Flow Per Tick

1. Rust computes player distance to each jukebox within `audio_radius`
2. Emits `JukeboxUpdate` AudioEvent per audible jukebox (entity ID, track ID, playing state, distance factor, elapsed time)
3. Frontend AudioManager creates/updates Howl instances, sets volume to `distanceFactor * musicChannelVolume`
4. When a jukebox leaves range, frontend fades out and cleans up its Howl

## Entity System Extensions

### EntityDef (new optional fields)

```rust
pub struct EntityDef {
    // ... existing fields ...
    pub playlist: Option<Vec<String>>,  // track IDs from catalog
    pub audio_radius: Option<f64>,       // pixels — music audible within this range
}
```

An entity is a jukebox if `playlist.is_some()`. The interaction system checks this and routes to jukebox behavior instead of harvest behavior.

`audio_radius` is the distance at which music becomes audible. `interact_radius` (existing field) is the distance at which the player can open the track picker. `audio_radius` should be larger than `interact_radius` — you hear the jukebox before you can interact with it.

### Entity Definition Example

```json
{
    "jukebox_tavern": {
        "name": "Tavern Jukebox",
        "verb": "Listen",
        "yields": [],
        "cooldownSecs": 0,
        "maxHarvests": 0,
        "respawnSecs": 0,
        "spriteClass": "jukebox",
        "interactRadius": 100,
        "playlist": ["glitch-theme", "meadow-waltz", "ur-lament"],
        "audioRadius": 400
    }
}
```

### JukeboxState

Per-instance runtime state, stored in `HashMap<String, JukeboxState>` in `GameState`:

```rust
pub struct JukeboxState {
    pub current_track_index: usize,
    pub playing: bool,
    pub elapsed_secs: f64,
}
```

Rust ticks `elapsed_secs` forward each frame when `playing` is true. When `elapsed_secs >= track_duration`, it advances to the next track (wrapping to index 0).

## Audio Events

### JukeboxUpdate

New variant on the existing `AudioEvent` enum:

```rust
AudioEvent::JukeboxUpdate {
    entity_id: String,
    track_id: String,
    playing: bool,
    distance_factor: f64,
    elapsed_secs: f64,
}
```

Emitted **every tick** for each jukebox within `audio_radius` of the player. Unlike one-shot AudioEvents (Jump, ItemPickup, etc.), this is continuous because the distance factor changes as the player moves.

### Distance Falloff

```
distance = abs(player_x - jukebox_x)    // 1D horizontal for side-scroller
if distance >= audio_radius: no event emitted
if distance <= 0: factor = 1.0
else: factor = 1.0 - (distance / audio_radius)
```

Linear interpolation. The player hears the jukebox at full volume when standing on it, fading to silence at `audio_radius`.

### Nearest Jukebox Only

If the player is within `audio_radius` of multiple jukeboxes, only the nearest one emits a `JukeboxUpdate`. Multi-source mixing is out of scope.

### Player Commands (Tauri IPC)

```rust
jukebox_play(entity_id: String)
jukebox_pause(entity_id: String)
jukebox_select_track(entity_id: String, track_index: usize)
```

These mutate `JukeboxState` in Rust. The next tick's `JukeboxUpdate` reflects the change. Rust validates that the player is within `interact_radius` before accepting commands.

## Track Catalog

### Catalog File

`assets/music/catalog.json` — committed to git:

```json
{
  "tracks": {
    "glitch-theme": {
      "title": "Glitch Theme",
      "artist": "Tiny Speck",
      "durationSecs": 180,
      "file": "glitch-theme.mp3"
    },
    "meadow-waltz": {
      "title": "Meadow Waltz",
      "artist": "Tiny Speck",
      "durationSecs": 210,
      "file": "meadow-waltz.mp3"
    }
  }
}
```

### Rust Types

```rust
pub struct TrackCatalog {
    pub tracks: HashMap<String, TrackDef>,
}

pub struct TrackDef {
    pub title: String,
    pub artist: String,
    pub duration_secs: f64,
    pub file: String,
}
```

Loaded at startup. Rust validates that every track ID in every jukebox playlist exists in the catalog. Missing tracks log a warning and are skipped from the playlist.

### CAS Storage

Music files live in `assets/music/tracks/`, managed by `cas-tool` with `manifests/music.json`:

```json
"ingest-music": "cargo run --manifest-path src-tauri/Cargo.toml --features cas-tool --bin cas-tool -- ingest --input assets/music/tracks --manifest manifests/music.json",
"restore-music": "cargo run --manifest-path src-tauri/Cargo.toml --features cas-tool --bin cas-tool -- restore --manifest manifests/music.json --output assets/music/tracks"
```

## MusicSource Abstraction

Frontend interface for resolving track IDs to playable URLs:

```typescript
interface MusicSource {
  resolveTrackUrl(trackId: string, filename: string): string;
}

class LocalMusicSource implements MusicSource {
  resolveTrackUrl(_trackId: string, filename: string): string {
    return `/assets/music/tracks/${filename}`;
  }
}
```

`AudioManager` receives a `MusicSource` at construction. When creating a Howl for a jukebox track, it calls `resolveTrackUrl()`. Swapping to Roxy later means injecting a `RoxyMusicSource` that returns streaming URLs.

## Music Channel

### AudioManager

New independent Music channel alongside SFX and Ambient:

- `musicVolume: number` (default 0.5)
- `musicMuted: boolean` (default false)
- `activeJukeboxes: Map<string, Howl>` — keyed by entity_id

Channel type extends from `'sfx' | 'ambient'` to `'sfx' | 'ambient' | 'music'`. Same `setVolume`/`getVolume`/`setMuted`/`isMuted` pattern.

`AudioPreferences` gains `musicVolume` and `musicMuted`, persisted to localStorage. Backward-compatible — missing keys fall back to defaults.

### Jukebox Audio Lifecycle

- `JukeboxUpdate` arrives with unknown `entity_id` → create Howl via `MusicSource`, seek to `elapsed_secs`, play
- `JukeboxUpdate` arrives with known `entity_id` but different `track_id` → swap Howl (fade out old, fade in new)
- Each tick: set Howl volume to `distanceFactor * effectiveMusicVolume()`
- `playing: false` → pause Howl
- No `JukeboxUpdate` for a known jukebox for 500ms → fade out, remove from `activeJukeboxes`
- `dispose()` cleans up all active jukebox Howls

### VolumeSettings

Third channel slider added to VolumeSettings panel, identical pattern to SFX and Ambient:

```
SFX              [75%]
Ambient          [50%]
Music            [50%]
```

## Jukebox UI Panel

`JukeboxPanel.svelte` — opens when player interacts with a jukebox, closes on interact again or walking out of `interact_radius`.

### Data

Initial state fetched via `get_jukebox_state(entity_id)` Tauri IPC command. After opening, the panel updates reactively from `JukeboxUpdate` AudioEvents (current track, elapsed time, playing state):

```typescript
interface JukeboxInfo {
  entityId: string;
  name: string;
  playlist: TrackInfo[];
  currentTrackIndex: number;
  playing: boolean;
  elapsedSecs: number;
}

interface TrackInfo {
  id: string;
  title: string;
  artist: string;
  durationSecs: number;
}
```

### Layout

```
+-- Tavern Jukebox ----------- x -+
|                                  |
|  > Glitch Theme        3:00     |  <- now playing (highlighted)
|    Meadow Waltz        3:30     |
|    Ur's Lament         4:15     |
|                                  |
|    <<      >>/||       >>       |  <- prev / play-pause / next
|    ---*------------- 1:42       |  <- progress (read-only)
+---------------------------------+
```

- Click a track to select it (`jukebox_select_track`)
- Play/pause toggles (`jukebox_play`/`jukebox_pause`)
- Prev/next advance playlist index
- Progress bar is read-only — reflects `elapsed_secs` from `JukeboxUpdate` events
- Panel updates reactively from JukeboxUpdate events

### Accessibility

- `<dialog>` with `aria-label="Jukebox: {name}"`
- Track list: `<ul role="listbox">` with `aria-activedescendant` for current track
- Controls: native `<button>` with `aria-label` ("Play", "Pause", "Previous track", "Next track")
- Focus trapped within dialog while open, returns to game on close
- Enter and Space activate all buttons (native behavior)

## Testing

### Rust Tests

- JukeboxState advances track when elapsed exceeds duration
- JukeboxState wraps playlist index at end of playlist
- JukeboxUpdate event serialization round-trip
- Distance factor: at entity = 1.0, at audio_radius = 0.0, halfway = 0.5, beyond = no event
- Jukebox interaction routes to jukebox behavior (not harvest)
- Catalog loading: valid catalog parses correctly
- Missing track ID in playlist logs warning and is skipped
- jukebox_play/jukebox_pause/jukebox_select_track mutate state correctly
- Player commands rejected when out of interact_radius

### Frontend Tests (Vitest)

- AudioManager processes JukeboxUpdate events (creates Howl, adjusts volume)
- AudioManager music channel volume/mute independent from SFX/ambient
- AudioPreferences round-trip with music fields (backward-compatible)
- LocalMusicSource.resolveTrackUrl() returns correct path
- JukeboxPanel renders track list and highlights current track
- JukeboxPanel play/pause/skip buttons trigger correct IPC calls
- VolumeSettings renders three channels

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Track file missing (not restored from CAS) | Howl load error, skip track, advance playlist |
| Unknown track ID in playlist | Logged at catalog load, skipped |
| Empty playlist on jukebox entity | Interaction shows "No tracks available" |
| Catalog JSON missing | Log warning, all jukeboxes have empty playlists |
| Player commands from out of range | Rust validates proximity, rejects silently |
| Multiple jukeboxes in range | Only nearest plays |

## Out of Scope

- Roxy streaming integration (follow-up bead)
- Seekable progress bar (read-only for now)
- Multiple simultaneous jukeboxes mixing (nearest only)
- Music blocks (inventory-carried items, different from world jukeboxes)
- Shuffle/repeat modes
- 2D distance calculation (Y-axis) — 1D horizontal for side-scroller
- Cryptographic licensing verification (Roxy bead)

## Follow-Up Beads

- **Roxy Streaming** — Swap `LocalMusicSource` for `RoxyMusicSource`, fetch tracks from Harmony network via harmony-roxy. Add cryptographic licensing ("free in Glitch, paid elsewhere").
- **Music Blocks** — Inventory-carried music items that play globally (no spatial component). Different interaction model from world jukeboxes.
- **Multi-Source Mixing** — When multiple jukeboxes are in range, mix their audio based on relative distance.
