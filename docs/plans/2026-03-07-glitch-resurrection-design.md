# Glitch Resurrection — Design Document

## Overview

Glitch was a browser-based MMO developed by Tiny Speck (later Slack Technologies), shut down in December 2012. In 2013, the entire game's art, audio, and substantial source code were released into the public domain under CC0 1.0. This project resurrects Glitch as a standalone desktop application (`harmony-glitch`) running on the Harmony decentralized network stack, eliminating the centralized server costs that killed the original.

**Goal:** A living, evolving recreation of Glitch that honors the spirit of the original while freeing it from the technical limitations of Flash and centralized infrastructure.

**Guiding principle:** Slow and steady. Each phase is proven before the next begins. Glitch is a piece of art and culture worthy of every ounce of consideration.

## Architecture: Tauri-Heavy

The game uses a Tauri v2 + Svelte 5 desktop app with a clear separation:

- **Rust backend** owns all game logic: street loading, physics, collision, game state, tick loop
- **PixiJS** is a thin, dumb renderer — it draws what Rust tells it to draw
- **Svelte 5** handles UI chrome: menus, street picker, debug overlays, HUD
- **Tauri IPC** bridges the two: commands (frontend → Rust) and events (Rust → frontend)

Every line of game logic in Rust is reusable: the same physics/collision code runs on peers for multiplayer validation in Phase B, and on harmony-compute WASM nodes for NPC AI in Phase C.

```
+-------------------------------------+
|          Tauri Shell                |
+--------------+----------------------+
|  Svelte 5    |  PixiJS Canvas       |
|  (UI chrome) |  (game rendering)    |
+--------------+----------------------+
|          Tauri IPC Bridge           |
+-------------------------------------+
|  Rust Backend                       |
|  - Street loader (XML -> JSON)      |
|  - Physics engine (AABB + slopes)   |
|  - Asset manager                    |
|  - Game loop tick (60Hz)            |
+-------------------------------------+
```

## Rendering: PixiJS

PixiJS handles the 2D rendering pipeline. It is purpose-built for sprite-based games with parallax scrolling and skeletal animation.

**Scene graph:**

```
Stage
+-- ParallaxContainer (layers move at different rates by depth)
|   +-- Layer[0] sky (depth 0.0, fixed)
|   +-- Layer[1] far bg (depth 0.3, slow scroll)
|   +-- Layer[2] mid bg (depth 0.6)
|   +-- Layer[3] near bg (depth 0.9)
+-- WorldContainer (moves 1:1 with camera)
|   +-- DecoSprites (behind, z < 0)
|   +-- PlatformSprites
|   +-- AvatarSprite
|   +-- DecoSprites (front, z > 0)
+-- UIContainer (fixed to screen)
```

**Parallax formula:** When camera moves by `dx`, each layer moves by `dx * layer.depth`.

**The renderer is dumb.** It receives `RenderFrame` from Rust and draws it. No game logic, no physics, no state in JS. Swappable.

## Asset Pipeline

The CC0 archive contains `.fla` (Flash source), `.swf` (compiled Flash), location XML, and audio files.

**Strategy: pre-processed offline, informed by community work.**

1. Audit existing community extractions (Children of Ur, Eleven Giants, Odd Giants) for usable PNGs and JSON
2. For gaps, build offline conversion tools (Node.js scripts using SWF parsing libraries)
3. Output: PNG texture atlases + JSON animation metadata + street geometry
4. Content-address the converted assets for future distribution via Harmony's content layer
5. Bundle 1-2 demo streets directly with the app for Phase A

**Fidelity approach:** Structurally faithful, visually close. Geometry and collision are accurate. Visual details may be approximate in Phase A and refined later. The game is free to improve beyond its Flash-era origins.

## Street Data Model

Streets are parsed from the `glitch-locations` XML archive into Rust structs.

**Core types:**

```rust
struct StreetData {
    name: String,
    width: f64,
    height: f64,
    layers: Vec<Layer>,        // Parallax backgrounds (back to front)
    platforms: Vec<Platform>,  // Walkable/collidable surfaces
    decos: Vec<Deco>,          // Decorative objects
    walls: Vec<Wall>,          // Street boundaries
    spawners: Vec<Spawner>,    // Item/NPC spawn points (Phase C)
}

struct Platform {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    start: PlatformEndpoint,   // Left edge (x, y) -- platforms can slope
    end: PlatformEndpoint,     // Right edge (x, y)
    is_one_way: bool,          // Can jump through from below?
}
```

**Sloped platforms:** Glitch platforms slope (rolling hills). Player Y is linearly interpolated between `start.y` and `end.y` based on X position.

**XML parsing:** `quick-xml` crate, SAX-style. Unknown elements logged and skipped (forward-compatible). Parser is standalone, no Tauri dependency, fully unit-testable.

**Street loader is generic:** Any valid street XML works. Two demo streets bundled for Phase A.

## Game Loop & IPC Protocol

**Fixed 60Hz tick in Rust:**

```
Input arrives (Tauri command)
  -> GameState.tick(input, dt)
       -> Apply input to velocity
       -> Apply gravity
       -> Move player
       -> Resolve collisions (AABB + slope)
       -> Update camera
       -> Return RenderFrame
  -> Emit render_frame event to frontend
```

**RenderFrame DTO (crosses IPC each tick):**

```rust
struct RenderFrame {
    player: PlayerFrame,   // position, facing, animation_state
    camera: CameraFrame,   // x, y viewport offset
    street_id: String,
}
```

**What does NOT cross IPC per frame:** street geometry, asset data, platform positions (all sent once on load).

**IPC commands (frontend -> Rust):**

| Command | Purpose |
|---------|---------|
| `load_street(name)` | Parse XML, return StreetData |
| `start_game()` | Begin the tick loop |
| `send_input(input)` | Key state changed |
| `list_streets()` | Available street names |

**IPC events (Rust -> frontend):**

| Event | Purpose |
|-------|---------|
| `render_frame` | Emitted every tick with RenderFrame |
| `street_loaded` | Street data ready for PixiJS |

## Input Handling

**Key state model, not events.** Frontend tracks which keys are held and sends `InputState` to Rust on change:

```typescript
interface InputState {
    left: boolean;
    right: boolean;
    jump: boolean;
}
```

Rust latches state, reads every tick. No stuck keys, no event queue overflow, minimal IPC.

## Camera

Player-centered, clamped to street bounds:

```
Camera.x = clamp(player.x - viewport_width/2, 0, street_width - viewport_width)
Camera.y = clamp(player.y - viewport_height/2, 0, street_height - viewport_height)
```

Smoothing, dead zones, and vertical bias deferred to post-Phase A.

## Physics Constants (Phase A, tunable)

| Constant | Value | Notes |
|----------|-------|-------|
| Tick rate | 60 Hz | Fixed timestep |
| Gravity | 980 px/s^2 | ~1g at game scale |
| Walk speed | 200 px/s | Glitch's leisurely pace |
| Jump velocity | -400 px/s | Comfortable hop |
| Terminal velocity | 600 px/s | Cap falling speed |

Starting values. Tuned to match Glitch's floaty, whimsical feel.

## Project Structure

```
harmony-glitch/
  src-tauri/
    src/
      main.rs                 # Tauri entry point
      lib.rs                  # Tauri command registrations
      street/
        mod.rs
        loader.rs             # XML -> StreetData parser
        types.rs              # Platform, Deco, Layer structs
      physics/
        mod.rs
        aabb.rs               # Collision detection
        movement.rs           # Gravity, velocity, jump, walk
      engine/
        mod.rs
        state.rs              # GameState struct
        tick.rs               # Game loop: input -> physics -> RenderFrame
      avatar/
        mod.rs
        types.rs              # Avatar state
    Cargo.toml
    tauri.conf.json
  src/
    main.ts
    App.svelte
    app.css
    lib/
      components/
        GameCanvas.svelte     # PixiJS canvas wrapper
        StreetPicker.svelte   # Street selection UI
        DebugOverlay.svelte   # Collision viz, FPS, position
        HUD.svelte            # Minimal heads-up display
      engine/
        renderer.ts           # PixiJS setup
        street-renderer.ts    # Parallax layers, platforms, decos
        avatar-renderer.ts    # Avatar sprite animation
        camera.ts             # Camera follow + parallax math
      types.ts                # TypeScript types matching Rust DTOs
      ipc.ts                  # Tauri invoke wrappers
  assets/
    streets/
      groddle_meadow/
      alakol/
    avatar/
      default/
  tools/
    convert-streets/          # Offline asset conversion
  docs/
    plans/
```

Rust game logic modules (street, physics, engine, avatar) have no Tauri imports and are fully unit-testable.

## Phase Roadmap

### Phase A: Walking Simulator

"I can see Ur and walk around in it."

- App launches, shows street picker
- Selecting a street loads and renders parallax backgrounds
- Avatar stands on platforms, obeys gravity, walks, jumps
- Sloped platforms work
- Camera follows player, clamped to street bounds
- Debug overlay shows collision geometry, FPS, position
- Street loader is generic (any valid XML)
- 2 demo streets bundled
- All Rust game logic has unit tests
- Renderer contains zero game logic

Not in Phase A: multiplayer, items, NPCs, sound, game logic, street transitions, avatar customization.

### Phase B: Shared Streets

"I can see other players in the same street."

- Two peers connect via Zenoh pub/sub
- Both see each other's avatars in real-time
- Street transitions (walk off edge -> adjacent street)
- Player presence (who's here?)
- Basic chat (text bubbles above avatars)
- State sync via Harmony content layer

### Phase C: Playable Micro-World

"I can interact with the world."

- GameServerJS subset running in embedded QuickJS/Boa WASM runtime
- Pick up, use, drop items
- Basic NPC interactions
- Crafting recipes
- Inventory UI (Svelte)
- Game state persists via content-addressed snapshots

### Phase D: Living World

"The world sustains itself."

- Ephemeral street instancing (Zenoh topics spin up/down organically)
- Persistent world state via CAS (BLAKE3 snapshots)
- Peer validation (deterministic WASM)
- Trust-based reputation (Harmony trust layer)
- Full economy subset

Each phase builds on the last. Nothing is thrown away.

## Legal Status

- All game assets (art, audio, code) released under CC0 1.0 by Tiny Speck in 2013
- Original "GLITCH" gaming trademark (USPTO #4074831) cancelled via Section 8 in 2018 (non-use)
- Fastly's coding platform "Glitch" operates in a different trademark class and is being deprecated (2025-2026)
- The name is legally available for interactive entertainment use

## Technical Dependencies

- **Tauri v2** — desktop app shell
- **Svelte 5** — UI framework (runes: $state, $derived, $props, $effect)
- **PixiJS** — 2D WebGL rendering engine
- **quick-xml** — Rust XML parser
- **serde / serde_json** — Rust serialization for IPC DTOs
- **Harmony core crates** (Phase B+) — harmony-crypto, harmony-identity, harmony-zenoh, harmony-content
