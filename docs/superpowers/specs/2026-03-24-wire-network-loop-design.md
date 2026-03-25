# Wire Network Loop — Peer Connection Lifecycle

## Overview

Complete the peer connection pipeline in `NetworkState` so that two harmony-glitch instances on the same LAN discover each other via Reticulum announces, establish encrypted Links, activate Zenoh Sessions with PubSubRouters, and sync player state at 60Hz over pub/sub topics scoped by street. The frontend already renders `RemotePlayerFrame[]` from `RenderFrame` — no frontend changes needed.

**Goal:** Two players on a LAN see each other's avatars move in real time, with chat messages routed between them, all running on Harmony's decentralized network stack with zero central servers.

**Guiding principle:** All protocol logic is sans-I/O state machines driven by the existing 60Hz game loop. No async runtime, no embedded I/O. The caller (game loop) provides raw packets in, gets raw packets out.

## Scope

### In scope

- Link initiation on announce receipt (lower address hash initiates, deterministic tiebreaker)
- Link handshake completion (3-step ECDH: request → proof → RTT)
- Zenoh Session activation over active Link
- PubSubRouter setup (declare publishers + subscribers for player state, chat, presence topics)
- `publish_to_all_peers()` — route through PubSubRouter → Session → Link → Node → UDP
- `handle_local_delivery()` — route inbound link data through Link → Session → PubSubRouter → registry
- Peer teardown on stale timeout (existing 10s purge + Link/Session/Router cleanup)
- Unit tests for the full peer lifecycle

### Out of scope (future beads)

- Frontend/renderer changes (already handles `remotePlayers`)
- WAN relay / TCP transport
- Pre-subscription for adjacent streets
- Explicit keepalive messages (60Hz publishing is the implicit heartbeat)

## Current State

The game loop already:
1. Drains UDP packets non-blocking → feeds to `NetworkState.tick()`
2. Executes outbound `NetworkAction`s (broadcast via `UdpTransport`)
3. Augments `RenderFrame` with `NetworkState.remote_frames()`
4. Calls `NetworkState.publish_player_state()` (currently returns empty Vec)

What's stubbed:
- `handle_announce_received()` records peers but never initiates Links
- `handle_local_delivery()` is a no-op
- `PeerState.session` and `PeerState.router` are always `None`
- `publish_to_all_peers()` returns `Vec::new()`

## Architecture

### Data Flow

**Outbound (local player → remote peers):**

```
GameState.tick() → RenderFrame → PlayerNetState (f32 positions, 17 bytes)
  → NetworkState.publish_player_state(&state)
  → PubSubRouter.publish(state_topic, json_payload)
  → PubSubAction::SendMessage { key_expr, payload }
  → Session framing (message type + expr_id + payload)
  → Link.encrypt(session_frame) → Fernet ciphertext
  → Node builds Reticulum data packet (Type1 header + encrypted payload)
  → NetworkAction::SendPacket { interface_name, data }
  → UdpTransport.broadcast() or .send_to()
```

**Inbound (remote peers → local player):**

```
UDP recv → raw bytes
  → Node.handle_event(NodeEvent::InboundPacket)
  → NodeAction::DeliverLocally { packet, link_id }
  → NetworkState.handle_local_delivery()
  → Match link_id to PeerState → Link.decrypt(packet.data)
  → Session.handle_event(SessionEvent::Message { data })
  → SessionAction::Deliver { data } (after stripping session framing)
  → PubSubRouter.handle_event(PubSubEvent::InboundMessage { key_expr, payload })
  → PubSubEvent::DataReceived { subscription_id, key_expr, payload }
  → Deserialize NetMessage (PlayerState | Chat | Presence)
  → Update RemotePlayerRegistry or emit NetworkAction::ChatReceived
```

### Peer Lifecycle State Machine

```
                    Announce received
                    (same street)
                          │
                          ▼
              ┌───────────────────────┐
              │ Our hash < their hash?│
              └───────┬───────┬───────┘
                 yes  │       │  no
                      ▼       ▼
              Initiate Link   Wait for
              (send request)  incoming request
                      │       │
                      ▼       ▼
              Link Pending    Link Handshake
              (await proof)   (send proof, await RTT)
                      │       │
                      ▼       ▼
              Link Active     Link Active
                      │       │
                      ▼       ▼
              Create Session (both sides)
              → handle_event(Open)
              → SessionState::Open
                      │
                      ▼
              Create PubSubRouter
              → declare_publisher(state_topic)
              → declare_publisher(chat_topic)
              → subscribe(peer's state_topic)
              → subscribe(peer's chat_topic)
              → subscribe(presence_topic)
                      │
                      ▼
              Peer fully active
              (publishing + receiving at 60Hz)
                      │
                      ▼ (10s no updates OR street change OR quit)
              Tear down Router → Session → Link
              → emit PresenceEvent::Left
              → remove from registry
```

### Link Handshake Protocol

The Reticulum Link uses a 3-step ECDH handshake:

1. **Initiator → Responder:** Link request packet containing ephemeral X25519 + Ed25519 public keys. Sent as a Reticulum `LinkRequest` packet type to the peer's destination hash.

2. **Responder → Initiator:** Link proof packet. Responder performs ECDH with initiator's ephemeral key, derives session key via HKDF-SHA256, signs the proof with Ed25519. Creates the Link via `Link::accept_request()`.

3. **Initiator → Responder:** RTT packet. Initiator verifies proof signature, performs ECDH, derives matching session key, sends RTT acknowledgment. Both sides now have `LinkState::Active` with a shared Fernet encryption key.

After step 3, both sides have an authenticated, encrypted channel. All subsequent data is Fernet-encrypted (AES-256-CBC + HMAC-SHA256).

### Tiebreaker Rule

When a peer announce is received for a player on the same street, the side with the **lexicographically lower address hash** initiates the Link. This avoids duplicate links: if both sides see each other's announces simultaneously, only one initiates.

Address hashes are `[u8; 16]` — comparison is byte-by-byte from index 0.

### Topic Schema

All game data flows through Zenoh key expressions scoped by street:

```
harmony/glitch/street/{street_name}/player/{address_hash}/state   → PlayerNetState (60Hz)
harmony/glitch/street/{street_name}/player/{address_hash}/chat    → ChatMessage (on send)
harmony/glitch/street/{street_name}/presence                      → PresenceEvent (join/leave)
```

Street name and address hash are hex-encoded strings in the key expression.

### Wire Message Format

Messages are JSON-serialized `NetMessage` variants (already defined in `network/types.rs`):

```rust
enum NetMessage {
    PlayerState(PlayerNetState),  // ~17 bytes payload
    Chat(ChatMessage),            // ~200 bytes max
    Presence(PresenceEvent),      // ~50 bytes
}
```

All fit within Reticulum's 500-byte MTU after headers (35 bytes Type2) + Fernet overhead (~57 bytes) + Session framing (~10 bytes).

### Stale Peer Cleanup

The existing `RemotePlayerRegistry` purges players not updated for 10 seconds. This spec extends cleanup to also tear down the corresponding `PeerState`:

1. Registry detects stale player → returns address hash
2. `NetworkState` looks up `PeerState` for that address hash
3. If router exists: undeclare all publishers and subscribers
4. If session exists: initiate close
5. Drop the `PeerState` entry
6. Emit `PresenceEvent::Left`

The `change_street()` method already clears all peers — it will be extended to perform the same cleanup sequence.

## Key Implementation Changes

### `NetworkState::handle_announce_received()`

After recording the peer (existing logic), add:
- Check if peer is on the same street as us
- If our `address_hash < peer.address_hash`: call `Link::initiate()` with the peer's identity and destination
- Store the Link (Pending state) in `PeerState`
- Route the link request packet through the Node → emit as `NetworkAction::SendPacket`

### `NetworkState::handle_local_delivery()`

Currently a no-op. Implement packet classification:

1. **Link request (we're responder):** Extract destination hash from packet, find matching peer. Call `Link::accept_request()`, store link in PeerState, emit proof packet.
2. **Link proof (we're initiator):** Find peer with matching pending link. Call `link.complete_handshake()`, emit RTT packet. On success, activate Session + PubSubRouter.
3. **Link data (active link):** Find peer by link_id. Call `link.decrypt()`, feed decrypted data to `session.handle_event(Message)`, process session actions through PubSubRouter.

### `NetworkState::publish_to_all_peers()`

Replace the empty stub:
1. Iterate all peers with active routers
2. For each peer: `router.publish(topic, payload)` → `PubSubAction::SendMessage`
3. Convert: Session frame the message → `link.encrypt()` → build Reticulum data packet via Node
4. Collect all resulting `NetworkAction::SendPacket` actions

### `NetworkState::tick_peer_session()`

Already called in the tick loop. Extend to:
1. Process any pending Session actions
2. Feed PubSubRouter events from Session deliveries
3. Convert `PubSubEvent::DataReceived` → deserialize `NetMessage` → update registry or emit chat action
4. Return whether the peer should be closed (session closed or errored)

### `NetworkState::activate_peer()`

New helper called when a Link becomes Active:
1. Create `Session::new()` with peer identity
2. Call `session.handle_event(SessionEvent::Open)` → get `SessionAction`s
3. Create `PubSubRouter::new()`
4. Declare publishers for our state + chat topics
5. Subscribe to the peer's state + chat + presence topics
6. Store session and router in `PeerState`

## Testing Strategy

### Unit Tests (Rust, sans-I/O)

All tests create two `NetworkState` instances and shuttle packets between them by feeding one's outbound actions as the other's inbound packets.

1. **Link lifecycle** — Feed announce from B to A (A has lower hash). Verify A emits link request. Feed request to B. Verify B emits proof. Feed proof to A. Verify A emits RTT. Feed RTT to B. Verify both have `LinkState::Active`.

2. **Session + Router activation** — After link lifecycle completes, verify both peers have `session.state() == Open` and router has declared publishers/subscribers.

3. **Publish round-trip** — After activation, call `A.publish_player_state(state)`. Feed A's outbound packets to B. Call `B.tick()`. Verify `B.remote_frames()` contains A's position.

4. **Chat routing** — Call `A.send_chat("hello")`. Feed packets to B. Verify B's tick produces `NetworkAction::ChatReceived` with the message.

5. **Tiebreaker** — Create A (lower hash) and B (higher hash). Feed A's announce to B and B's announce to A. Verify only A initiates a link request, not B.

6. **Stale purge cleanup** — Activate peers. Advance time past 10s without updates. Verify peer is purged from registry AND PeerState is cleaned up.

7. **Street change teardown** — Activate peers on "meadow". Call `A.change_street("heights")`. Verify all peers torn down, new announce emitted with "heights".

8. **Responder-initiated flow** — Feed announce from A to B (B has lower hash). Verify B initiates, A responds. Full handshake completes.

### What We Don't Test Here

- UDP transport (already tested, and integration testing requires two processes)
- Frontend rendering of remote players (already implemented, separate bead for visual testing)
- WAN relay connectivity (out of scope)

## Files Modified

| File | Change |
|------|--------|
| `src-tauri/src/network/state.rs` | Implement `handle_local_delivery()`, `publish_to_all_peers()`, `activate_peer()`, extend `handle_announce_received()` and `tick_peer_session()` |
| `src-tauri/src/network/types.rs` | Add `NetMessage::Presence` variant if not present; add any helper serialization |
| `src-tauri/src/network/registry.rs` | Extend stale purge to return purged address hashes for cleanup |

No new files. No frontend changes. No Makefile changes.

## Dependencies

All already in `Cargo.toml`:
- `harmony-identity` — `PrivateIdentity`, `Identity`, ECDH
- `harmony-reticulum` — `Node`, `Link`, `Packet`, `NodeAction`, `NodeEvent`
- `harmony-zenoh` — `Session`, `PubSubRouter`, `SessionEvent`, `PubSubEvent`
- `harmony-crypto` — Fernet (via Link), HKDF (via Link)
- `serde_json` — `NetMessage` serialization (already used)

## Future Work (separate beads)

- **WAN relay transport** — TCP connection to relay node for internet play
- **Pre-subscription** — Subscribe to adjacent street topics before swoop
- **Explicit keepalives** — If 10s stale timeout proves insufficient
- **Presence events on wire** — Currently presence is derived from announce/stale; explicit Joined/Left messages would be faster
