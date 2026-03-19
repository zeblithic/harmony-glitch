# Audio System Design

## Goal

Add SFX and ambient audio so player actions have sound feedback and streets
have atmosphere. Rust emits semantic `AudioEvent`s in RenderFrame; the frontend
maps them to audio files via a swappable sound kit manifest and plays them
through Howler.js.

## Non-Goals

- Background music / playlists (future: Mama Gogo's Zeblithic Jukebox via Roxy)
- Footstep audio synced to walk animation (separate future task)
- Custom sound kit loading UI (future)
- Volume settings UI (future — use sensible defaults now)
- Spatial / positional audio (future)
- Seasonal sound packs (future)

## Architecture

Three layers:

1. **Rust (AudioEvent emitter)** — GameState produces `Vec<AudioEvent>` each
   tick, included in RenderFrame. Pure data, no I/O. Events are semantic:
   "what happened," not "what to play."

2. **TypeScript (AudioManager)** — A standalone class (no PixiJS or Svelte
   dependency) that loads a sound kit manifest, receives AudioEvents from
   RenderFrame, resolves events to audio files, and plays them via Howler.js.
   Manages ambient loops with crossfading on street transitions.

3. **Sound Kit (JSON manifest + audio files)** — `assets/audio/default-kit.json`
   maps event types to relative file paths. Ships with ~15 curated Glitch
   sounds (CC0 from `tinyspeck/glitch-sounds/`).

Data flow:

```
GameState.tick() → RenderFrame { audio_events: [...] }
    → App.svelte handleFrame()
        → AudioManager.processEvents(events)
            → Howler play/fade/stop
```

The AudioManager is instantiated once in App.svelte alongside the renderer. It
is completely independent of PixiJS — this separation means a future Roxy
integration only needs to extend AudioManager's loading logic; the
event/playback interface stays the same.

### Future Roxy Integration

SFX and ambient are **bundled assets** shipped with the binary. Music will be
**streamed content** fetched from Roxy, licensed per-play, decrypted on-the-fly
via Mama Gogo's Zeblithic Jukebox (an in-game item). The AudioManager's
separation of "event resolution" from "audio loading" supports this: a Roxy
music source would provide a different loader, not a different playback path.

Custom sound kits enable community creativity — players can make Glitch sound
however they want, share kits, and create seasonal packs. The kit manifest
format is the contract for this.

## AudioEvent Enum (Rust)

New file: `src-tauri/src/engine/audio.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
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
```

- Tagged enum with `tag = "type"` — serializes as
  `{ "type": "itemPickup", "itemId": "cherry" }` for clean TypeScript
  discriminated union.
- Payload fields (`item_id`, `entity_type`, `recipe_id`) carry enough context
  for variant-specific sounds.
- `ActionFailed` has no payload — visual feedback already distinguishes the
  reason; audio just signals "nope."

### RenderFrame Addition

```rust
pub audio_events: Vec<AudioEvent>,
```

Built fresh each tick (not accumulated across ticks).

### Emission Points

| Event | Where emitted | Trigger condition |
|-------|--------------|-------------------|
| `Jump` | `tick()` physics block | `on_ground` was true, now false, vy < 0 |
| `Land` | `tick()` physics block | `on_ground` was false, now true |
| `ItemPickup` | `tick()` interaction result | Successful ground item pickup or entity harvest |
| `EntityInteract` | `tick()` interaction result | Player interacts with entity |
| `ActionFailed` | `tick()` interaction result | Inventory full, cooldown, depletion |
| `CraftSuccess` | `craft_recipe()` method | Successful craft |
| `TransitionStart` | `tick()` transition block | Swoop phase begins |
| `TransitionComplete` | `tick()` transition block | Swoop phase ends |
| `StreetChanged` | `load_street()` method | New street loaded |

Note: `CraftSuccess` is emitted from the `craft_recipe` IPC command handler,
not from `tick()`. The event needs to be stored on GameState and drained into
the next tick's RenderFrame, since `craft_recipe` executes outside the tick
loop.

## Sound Kit Manifest

File: `assets/audio/default-kit.json`

```json
{
  "name": "Default",
  "version": 1,
  "sfxVolume": 1.0,
  "ambientVolume": 0.5,
  "events": {
    "itemPickup": {
      "default": "sfx/pick-up.mp3",
      "variants": {
        "cherry": "sfx/pick-up.mp3"
      }
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
        "fruit_tree": "sfx/harvest-tree.mp3"
      }
    }
  },
  "ambient": {
    "default": "ambient/outdoors.mp3",
    "variants": {
      "demo_meadow": "ambient/meadow.mp3",
      "demo_heights": "ambient/heights.mp3"
    }
  }
}
```

### Manifest Structure

- **`default` + `variants` pattern** — every event type has a fallback sound.
  Variants key off the event payload (`item_id`, `entity_type`, `street_id`).
  A custom kit only needs `default` entries; variants are optional specificity.
- **Paths relative to `assets/audio/`** — keeps the manifest portable.
- **`sfxVolume` / `ambientVolume`** — kit-suggested defaults. A future settings
  UI would read these as initial values, then store user overrides separately.
- **`version: 1`** — schema versioning for forward compatibility.

### Default Kit Audio Sources

Curate ~15 files from `tinyspeck/glitch-sounds/` (CC0). Exact file selection
is best-effort from naming — swap later if sounds don't fit. Organize as:

```
assets/audio/
  default-kit.json
  sfx/
    pick-up.mp3
    craft-success.mp3
    fail.mp3
    jump.mp3
    land.mp3
    transition-start.mp3
    transition-complete.mp3
    interact.mp3
    harvest-tree.mp3
  ambient/
    outdoors.mp3
    meadow.mp3
    heights.mp3
```

## AudioManager (TypeScript)

New file: `src/lib/engine/audio.ts`

```typescript
class AudioManager {
  private kit: SoundKit;
  private sounds: Map<string, Howl>;
  private currentAmbient: Howl | null;
  private sfxVolume: number;
  private ambientVolume: number;

  constructor(kit: SoundKit) { ... }

  processEvents(events: AudioEvent[]): void { ... }

  setVolume(channel: 'sfx' | 'ambient', volume: number): void { ... }

  dispose(): void { ... }
}
```

### Behavior

- **Construction** — parses manifest, pre-loads all referenced audio files via
  Howler. Howler handles lazy decoding and browser audio context setup.
- **`processEvents()`** — called once per frame from `handleFrame()` in
  App.svelte. Iterates events, resolves each to a sound file (check variant
  first, fall back to default), plays via Howler.
- **Ambient management** — on `TransitionStart`, begin fade-out of current
  ambient (~1s). On `StreetChanged`, load new ambient. On
  `TransitionComplete`, fade in new ambient (~1s). For direct street loads
  (no transition), start ambient immediately.
- **Volume** — SFX and ambient have independent volume multipliers. Default:
  `sfxVolume: 1.0`, `ambientVolume: 0.5`. Exposed via `setVolume()` for
  future settings UI.
- **`dispose()`** — stops all sounds, unloads Howls. Called on game stop.

### Integration in App.svelte

```typescript
// On game init:
const kit = await loadSoundKit();
const audioManager = new AudioManager(kit);

// In handleFrame:
if (frame.audioEvents?.length) {
  audioManager.processEvents(frame.audioEvents);
}

// On game stop:
audioManager.dispose();
```

### TypeScript Types

```typescript
interface AudioEvent {
  type: 'itemPickup' | 'craftSuccess' | 'actionFailed' | 'jump' | 'land'
    | 'transitionStart' | 'transitionComplete' | 'entityInteract'
    | 'streetChanged';
  itemId?: string;
  recipeId?: string;
  entityType?: string;
  streetId?: string;
}

interface SoundKit {
  name: string;
  version: number;
  sfxVolume: number;
  ambientVolume: number;
  events: Record<string, SoundEntry>;
  ambient: SoundEntry;
}

interface SoundEntry {
  default: string;
  variants?: Record<string, string>;
}
```

## Testing Strategy

### Rust unit tests (audio.rs)

- `AudioEvent` serialization — verify tagged enum JSON output matches expected
  format for each variant
- Round-trip: serialize → deserialize

### Rust unit tests (state.rs)

- Jump event: `on_ground` true → false with vy < 0 produces `Jump`
- Land event: `on_ground` false → true produces `Land`
- No duplicate: staying on ground doesn't re-emit `Land`
- Interaction events: successful harvest produces `ItemPickup` + `EntityInteract`
- Failed interaction: inventory full produces `ActionFailed`
- Craft success: `craft_recipe("bread")` produces `CraftSuccess`
- `audio_events` is empty on ticks with no events
- `audio_events` does not accumulate across ticks

### Rust unit tests (loader / lib.rs)

- Parse bundled `default-kit.json` validates as correct JSON
- All audio file paths referenced in manifest exist on disk (compile-time
  `include_bytes!` or runtime check)

### Frontend tests (vitest, mocked Howler)

- Event→sound mapping: `itemPickup` with `item_id: "cherry"` resolves to
  variant if present, falls back to default
- Ambient crossfade: `TransitionStart` triggers fade-out, `StreetChanged` +
  `TransitionComplete` triggers fade-in of new ambient
- Volume: `setVolume('sfx', 0.5)` adjusts SFX but not ambient
- Dispose: stops all active sounds

### Integration

Manual: `npm run tauri dev` — harvest entities (hear SFX), craft items, jump
and land, transition between streets (hear ambient crossfade), try interaction
with full inventory (hear fail sound).

## Files Modified

### Rust (new)
- `src-tauri/src/engine/audio.rs` — `AudioEvent` enum with serde

### Rust (modified)
- `src-tauri/src/engine/mod.rs` — add `pub mod audio;`
- `src-tauri/src/engine/state.rs` — add `audio_events` to GameState and
  RenderFrame, emit events in tick/craft_recipe/load_street
- `src-tauri/src/item/interaction.rs` — return audio event info from
  execute_interaction (or emit in state.rs based on result)

### Frontend (new)
- `src/lib/engine/audio.ts` — AudioManager class, SoundKit types, kit loading
- `src/lib/engine/audio.test.ts` — AudioManager tests with mocked Howler

### Frontend (modified)
- `src/lib/types.ts` — AudioEvent type
- `src/App.svelte` — instantiate AudioManager, pipe audioEvents, dispose
- `package.json` — add howler dependency

### Data (new)
- `assets/audio/default-kit.json` — sound kit manifest
- `assets/audio/sfx/*.mp3` — ~10 SFX files from Glitch library
- `assets/audio/ambient/*.mp3` — ~2-3 ambient loops from Glitch library

## Future Work (beads to file)

- **Custom sound kit loading** — user-provided kit directory, kit selection UI
- **Sound kit sharing** — community kits, marketplace
- **Seasonal sound packs** — Christmas, Halloween themed audio
- **Mama Gogo's Zeblithic Jukebox** — Roxy-integrated music streaming as an
  in-game jukebox/sound-block item. Original Glitch music assets hosted on
  Roxy for free. Roxy enables cryptographic licensing: "free in Glitch,
  $0.01 elsewhere"
- **Volume settings UI** — per-channel sliders, mute toggles
- **Footstep audio** — animation-synced, surface-dependent footstep sounds
- **Peer audio events** — friend joined/left sounds (Phase B multiplayer)
