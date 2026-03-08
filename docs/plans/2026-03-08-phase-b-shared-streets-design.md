# Phase B: Shared Streets — Design Document

## Overview

Phase B brings multiplayer to harmony-glitch: "I can see other players in the same street." Two or more peers discover each other via Reticulum announces, establish encrypted Zenoh sessions, and sync player state at 60Hz over pub/sub topics scoped by street. Players can walk between two connected streets with a smooth swoop transition, seeing other players arrive and depart in real time.

**Goal:** Two players on a LAN (or connected via a WAN relay) see each other's avatars, chat with text bubbles, and transition between streets — all running on Harmony's decentralized network stack with zero central servers.

**Guiding principle:** Reticulum for discovery and transport, Zenoh for game data fan-out. Both embedded as sans-I/O state machines driven by the game loop. No async runtime, no external processes.

## Architecture: Embedded Network Stack

The Rust game loop expands from a physics-only tick to a physics+network tick. Each 60Hz iteration:

1. **Drain inbound network events** — Non-blocking reads from UDP/TCP sockets, feed raw packets to Reticulum `Node`, which produces `NodeAction`s (deliver locally, forward, etc.)
2. **Process Zenoh events** — Delivered packets become Zenoh `Session` events, which produce `PubSubRouter` deliveries (remote player updates, chat messages, presence changes)
3. **Tick game state** — Physics, collision, camera (as today), plus apply remote player state from step 2
4. **Publish outbound** — Push local player's state through `PubSubRouter` → `Session` → `Node` → socket send
5. **Emit RenderFrame** — Now includes both local player and all remote players visible on this street

```
┌─────────────────────────────────────────────┐
│                Game Loop (60Hz)              │
│                                             │
│  1. recv_nonblocking() → raw packets        │
│  2. Node.handle(packets) → NodeActions      │
│  3. Session.handle(delivered) → PubSub msgs │
│  4. GameState.tick(input, remote_players)    │
│  5. PubSub.publish(local_player_state)      │
│  6. Node.handle(outbound) → send packets    │
│  7. emit RenderFrame (local + remotes)      │
└─────────────────────────────────────────────┘
```

**Socket I/O:** Non-blocking UDP for LAN discovery (Reticulum announces use broadcast). Optional TCP for WAN relay connections. All reads are non-blocking — if no data, the loop moves on. The tick stays deterministic and never blocks on network.

**New Rust module:** `src-tauri/src/network/` — owns the Reticulum Node, Zenoh Session, PubSubRouter, and socket management. Exposes a `NetworkState` struct that the game loop calls `tick()` on, mirroring how `GameState.tick()` works today.

## Peer Discovery & Session Establishment

### Discovery via Reticulum Announces

Each harmony-glitch instance creates a Reticulum `Destination` with app name `harmony/glitch`. On startup, it broadcasts an `Announce` containing:
- Its public identity (Ed25519 + X25519 keys)
- App data: player display name + current street name

Other instances receive the announce via the `Node`'s path learning. They now know: "a player named Alice is on demo_meadow, reachable in N hops."

### Session Establishment

When two players are on the same street (or adjacent streets, for pre-subscription), they establish a Zenoh `Session` over a Reticulum `Link`:

1. Player A sees Player B's announce (same or adjacent street)
2. Player A initiates a `Link` to Player B (3-step ECDH handshake, Fernet-encrypted channel)
3. Once the Link is Active, a Zenoh `Session` is established over it (identity-based handshake, resource mapping)
4. Both sides declare pub/sub interests via the `PubSubRouter`

### Interfaces

- **LAN:** UDP socket bound to a configurable port (default `29170`). Reticulum broadcasts announces on the local subnet. Zero config for same-network play.
- **WAN relay:** Optional TCP connection to a known Reticulum transport node (configured by IP:port in settings). Announces propagate through the relay, enabling internet play. The relay is just a standard `harmony-node` running in transport mode — no special game logic.

### Re-announce & Departure

- **Re-announce cadence:** Periodic re-announces (every ~5 minutes) so peers detect street changes and new arrivals. On street transition, an immediate re-announce with the new street name.
- **Departure:** When a player quits or transitions away, their Session closes gracefully (Zenoh close handshake). Peers detect stale sessions via keepalive timeout (90s default) as a fallback.

## Pub/Sub Topic Schema & Game State Sync

### Topic Hierarchy

All game data flows through Zenoh key expressions scoped by street:

```
harmony/glitch/street/{street_name}/player/{address_hash}/state   → PlayerNetState (60Hz)
harmony/glitch/street/{street_name}/player/{address_hash}/chat    → ChatMessage (on send)
harmony/glitch/street/{street_name}/presence                      → PresenceEvent (join/leave)
```

### Player State (60Hz)

Each tick, the local player publishes a compact `PlayerNetState`:

```rust
struct PlayerNetState {
    x: f32,          // f32 not f64 — saves bytes, sub-pixel precision unnecessary over wire
    y: f32,
    vx: f32,         // velocity for dead-reckoning if a packet drops
    vy: f32,
    facing: u8,      // 0=left, 1=right
    on_ground: bool,
}
// ~17 bytes payload — fits easily in Reticulum's 500-byte MTU with headers
// Street context comes from the Zenoh topic key expression, not the payload.
```

At 60Hz with snap rendering (no interpolation), every received update directly sets the remote avatar's position. If a packet drops, `vx`/`vy` allow one frame of dead-reckoning before the next update arrives.

### Chat Messages

```rust
struct ChatMessage {
    text: String,        // UTF-8, capped at ~200 chars to fit MTU
    sender: [u8; 16],    // sender's address hash (for rendering bubble above correct avatar)
    sender_name: String, // display name at time of sending
}
```

Published on the chat topic. All subscribers on the street receive it. Ephemeral — no history. Sender identity is included in the payload (rather than derived from the topic path) so the renderer can position chat bubbles without a separate lookup.

### Presence Events

```rust
enum PresenceEvent {
    Joined { address_hash: [u8; 16], display_name: String },
    Left { address_hash: [u8; 16] },
}
```

Published when a player's Session becomes active on a street (Joined) or when it closes/times out (Left). Drives remote avatar creation/destruction.

### Write-side Filtering

The `PubSubRouter` only sends data to peers who have declared interest (subscribed to the topic). If nobody is on `demo_meadow`, publishing there is a silent no-op. This is built into the router — no extra logic needed.

## Authority Model

**Self-authoritative:** Each player is the authority on their own position. You publish your position, others trust it. No server validation for Phase B — there's no economy, no items, no stakes.

Future phases will route updates through a WASM sidecar or Queryable that signs updates with the binary hash, and the Harmony trust layer will handle reputation-based anti-cheat (cheaters get their reputation slashed until there's nothing left to interact with).

## Street Transitions & Pre-subscription

### Signpost Data

Streets connect via signposts parsed from the XML. Each signpost has a target street name and a spawn position on the destination. The XML parser is extended to handle `<signpost>` elements with connection attributes.

For Phase B, we create a second demo street (`demo_heights`) with bidirectional signpost connections to `demo_meadow`.

### Pre-subscription Zone

When the player enters a configurable distance from a signpost (~500px), the game enters a "pre-transition" state:

1. **Pre-subscribe** — Subscribe to the adjacent street's presence and player state topics. Start receiving remote player updates from the next street (data is flowing before you arrive).
2. **Pre-load** — Load the destination street's XML data and build the PixiJS scene graph offscreen (second `Container` not yet added to the stage).

### The Swoop

When the player crosses the signpost threshold (walks past the edge), the swoop begins:

```
Before:    [====MEADOW====]        player at right edge
During:    [==MEADOW==][==HEIGHTS==]   both visible, sliding
After:              [====HEIGHTS====]  player at left edge
```

- The current street slides out in the direction of travel; the new street slides in from the opposite side.
- The avatar stays centered on screen while the world moves around them.
- **Duration adapts to readiness:** If pre-load is complete (fast connection / compiled-in XML), the swoop takes ~0.5s. If still loading, the swoop stretches — the player keeps walking but the new street slides in more slowly, matching load progress.
- **Minimum swoop duration:** ~0.3s (never feels instant/jarring).
- **Maximum:** ~2s (cap it so slow connections don't feel broken — show a subtle loading indicator if exceeded).

### Network Transition Sequence

During the swoop:
1. Publish `PresenceEvent::Left` on the old street
2. Re-announce with the new street name
3. Unsubscribe from old street topics (except presence — keep listening briefly for graceful cleanup)
4. Publish `PresenceEvent::Joined` on the new street
5. Start publishing player state on the new street's topic

Other players on the old street see you leave; players on the new street see you arrive.

## Renderer Changes

### RenderFrame Expansion

```rust
struct RenderFrame {
    player: PlayerFrame,                       // local player (as today)
    remote_players: Vec<RemotePlayerFrame>,     // new
    camera: CameraFrame,
    street_id: String,
}

struct RemotePlayerFrame {
    address_hash: [u8; 16],    // stable identity for sprite reuse
    display_name: String,
    x: f64,
    y: f64,
    facing: String,            // "left" or "right"
    on_ground: bool,
}
```

Rust owns the remote player registry — it receives `PlayerNetState` from the network, converts to `RemotePlayerFrame`, and includes all visible remote players in each `RenderFrame`. The renderer stays dumb.

### Remote Avatar Sprites

The renderer maintains a `Map<string, Graphics>` of remote avatar sprites keyed by address hash (hex string). Each frame:

- **New remote player** → Create a colored rectangle (different color from local avatar) with a text label for display name above it.
- **Existing remote player** → Update position, facing.
- **Gone remote player** → Remove sprite from scene graph and map.

### Chat Bubbles

When a chat message arrives, the renderer creates a temporary `PIXI.Text` element above the sender's avatar sprite. It fades out after ~5 seconds.

### Swoop Rendering

During a street transition, the renderer has two world containers active simultaneously:
- Old street's container slides out (x position animates off-screen)
- New street's container slides in from the opposite side
- Once the swoop completes, the old container is destroyed

Driven by a transition progress field (0.0→1.0) on `RenderFrame`, so the renderer just follows instructions.

## Frontend UI Changes

### New UI Elements

- **Player identity setup** — On first launch, generate a `PrivateIdentity` and persist it (Tauri app data directory). Prompt for a display name. Happens once, before the street picker.
- **Network status indicator** — Small text overlay in the corner: peer count + current street.
- **Chat input** — Text input at the bottom of the screen. Enter (or `/`) to focus, type message, Enter to send, Escape to cancel. When focused, keyboard input goes to chat instead of movement.
- **WAN relay config** — Settings panel (accessible from street picker) with a single text field: relay address (`host:port`). Empty = LAN only. Stored in Tauri app data alongside identity.

### New Tauri IPC Commands

```
send_chat(message: String)         → publish chat message
get_network_status()               → { peer_count, street_name }
set_relay_address(addr: String)    → configure WAN relay
get_identity()                     → { display_name, address_hash }
set_display_name(name: String)     → update display name
```

No changes to existing commands — `load_street`, `start_game`, `send_input`, `stop_game` all work as today. The network layer is additive.

## Second Demo Street

`demo_heights` — a hilltop scene connecting to `demo_meadow`:

- **Theme:** Rocky hilltop, visually distinct (darker ground color, different deco shapes).
- **Size:** ~6000px wide, ~1000px tall (similar to demo_meadow).
- **Platforms:** Mix of flat and sloped, including taller hills to differentiate.
- **Signposts:** Left edge connects to `demo_meadow` (right edge), and vice versa.
- **Layers:** Same parallax structure but different widths so parallax rates differ.

Hand-authored XML, compiled in via `include_str!`. `load_street_xml` gets a second arm. `list_streets` returns both.

## Testing Strategy

### Rust Unit Tests

- **Network module** — `NetworkState` tick produces correct actions: announce on startup, publish player state, handle inbound presence events, pre-subscribe on zone entry, unsubscribe on street leave. Sans-I/O: feed events, assert actions.
- **Signpost parser** — XML signpost elements parse correctly. Bidirectional links. Unknown targets handled gracefully.
- **Remote player registry** — Players appear on `Joined`, disappear on `Left` or timeout. State updates apply. Street filtering works.
- **Street transition state machine** — Pre-subscription triggers at correct distance. Swoop progress tracks load readiness. Presence events fire in correct order. Old street cleanup after swoop.
- **PlayerNetState serialization** — Round-trip encode/decode. Fits within MTU. f32 precision adequate.

### Integration Tests (Manual)

- Two instances on same LAN discover each other, see avatars
- Walk between two streets, see presence transitions
- Chat message appears above remote avatar
- WAN relay: two instances on different networks connect via relay
- Graceful disconnect: remote avatar disappears after peer quits

### Frontend Tests (vitest)

- Remote avatar sprite lifecycle: created on new player, removed on departure
- Chat bubble appears and fades
- Chat input focus steals keyboard from movement, unfocus returns it

## What Phase B Does NOT Include

- Items, NPCs, inventory, crafting
- Avatar sprite animation (still colored rectangles)
- Sound
- Peer validation / anti-cheat
- Persistent world state
- Player list UI
- Chat history

## Dependencies

### Rust Crates (from harmony workspace)

- `harmony-identity` — Ed25519/X25519 keypairs, signing, ECDH
- `harmony-reticulum` — Packet format, Node, Link, Announce, Destination
- `harmony-zenoh` — Session, PubSubRouter, HarmonyEnvelope
- `harmony-crypto` — ChaCha20-Poly1305, HKDF, Fernet (via identity/zenoh)

### New Dependencies for harmony-glitch

- `socket2` or `mio` — Non-blocking UDP/TCP socket I/O
- `harmony-identity`, `harmony-reticulum`, `harmony-zenoh` — Git dependencies from harmony workspace
