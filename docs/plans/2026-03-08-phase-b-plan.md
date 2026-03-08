# Phase B: Shared Streets — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Two or more players discover each other via Reticulum, sync state at 60Hz over Zenoh pub/sub, see each other's avatars, chat with text bubbles, and transition between two connected streets with a smooth swoop animation.

**Architecture:** Embed Reticulum Node + Zenoh Session/PubSubRouter as sans-I/O state machines driven by the existing 60Hz game loop. Non-blocking UDP/TCP for LAN discovery + optional WAN relay. Self-authoritative player state. All networking logic in Rust — frontend stays dumb.

**Tech Stack:** Rust (Tauri v2), harmony-identity, harmony-reticulum, harmony-zenoh, socket2/mio for non-blocking I/O, Svelte 5, PixiJS v8.

**Design doc:** `docs/plans/2026-03-08-phase-b-shared-streets-design.md`

**Important context:**
- Coordinate system: Y=0 at ground, negative Y = up. Screen conversion: `screenY = glitchY - street.top`
- Reticulum MTU: 500 bytes. Player state must fit comfortably.
- All harmony crates are sans-I/O state machines — you feed events, they return action vectors. No async/tokio needed.
- Existing game loop: 60Hz thread, reads InputState, calls `GameState::tick()`, emits `RenderFrame` via Tauri event.
- Run `cargo test` from `src-tauri/` for Rust tests. Run `npx vitest run` from repo root for frontend tests.
- Run `cd src-tauri && cargo clippy` for Rust lint. Run `npm run build` for frontend build.

---

### Task 1: Add Harmony Crate Dependencies

Add harmony-identity, harmony-reticulum, and harmony-zenoh as path dependencies. Add socket2 for non-blocking UDP/TCP. Verify the project compiles.

**Files:**
- Modify: `src-tauri/Cargo.toml`

**Step 1: Add dependencies to Cargo.toml**

Add these to `[dependencies]`:

```toml
harmony-crypto = { path = "../../harmony/crates/harmony-crypto" }
harmony-identity = { path = "../../harmony/crates/harmony-identity" }
harmony-reticulum = { path = "../../harmony/crates/harmony-reticulum" }
harmony-zenoh = { path = "../../harmony/crates/harmony-zenoh" }
socket2 = { version = "0.5", features = ["all"] }
rand = "0.8"
```

Note: `harmony-crypto` is needed transitively. `rand` provides `CryptoRngCore` for the Reticulum/Zenoh APIs.

**Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check 2>&1`
Expected: Compiles successfully. If path resolution fails, adjust relative paths — the harmony workspace should be at `../../harmony/` relative to `src-tauri/`.

**Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "deps: add harmony networking crates and socket2"
```

---

### Task 2: Second Demo Street with Signpost Connections

Create `demo_heights` street XML and add signpost elements to both streets for bidirectional connections. Update the street loader to serve both streets.

**Files:**
- Create: `assets/streets/demo_heights.xml`
- Modify: `assets/streets/demo_meadow.xml` (add signpost)
- Modify: `src-tauri/src/lib.rs:156-163` (load_street_xml match)
- Modify: `src-tauri/src/lib.rs:30-34` (list_streets)
- Test: `src-tauri/src/street/parser.rs` (existing tests)

**Step 1: Check if signpost parsing already works**

The types `Signpost` and `SignpostConnection` exist in `src-tauri/src/street/types.rs`. Check `src-tauri/src/street/parser.rs` for signpost parsing. If it already handles `<signpost>` elements, skip to Step 3. If not, add parsing in Step 2.

Read: `src-tauri/src/street/parser.rs` — search for "signpost" or "Signpost".

**Step 2: Add signpost parsing (if needed)**

If the parser doesn't handle signposts, add parsing for this XML structure:

```xml
<signpost id="sign_1" x="1950" y="0">
  <connect target_tsid="demo_heights" label="To the Heights" />
</signpost>
```

Add a case in the XML element matching to parse `<signpost>` elements and their `<connect>` children. Push parsed signposts into `street_data.signposts`.

**Step 3: Write a test for signpost parsing**

Add to the parser tests:

```rust
#[test]
fn parses_signpost_connections() {
    let xml = r#"
    <location tsid="test" label="test">
      <bounds left="-1000" right="1000" top="-500" bottom="0" />
      <layer name="middleground" z="0" w="2000" h="500">
        <signpost id="sign_1" x="950" y="0">
          <connect target_tsid="other_street" label="Go there" />
        </signpost>
      </layer>
    </location>
    "#;
    let street = parse_street(xml).unwrap();
    assert_eq!(street.signposts.len(), 1);
    assert_eq!(street.signposts[0].id, "sign_1");
    assert_eq!(street.signposts[0].x, 950.0);
    assert_eq!(street.signposts[0].connects.len(), 1);
    assert_eq!(street.signposts[0].connects[0].target_tsid, "other_street");
}
```

Run: `cd src-tauri && cargo test parses_signpost`
Expected: PASS

**Step 4: Create demo_heights.xml**

Create `assets/streets/demo_heights.xml` — a rocky hilltop scene:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<location tsid="demo_heights" label="Demo Heights">
  <bounds left="-2000" right="2000" top="-1000" bottom="0" />
  <gradient top="4A6670" bottom="7A8B6E" />
  <layer name="sky" z="-3" w="3600" h="1000" />
  <layer name="bg_1" z="-1" w="5000" h="1000">
    <deco id="mountain_1" x="-800" y="-600" w="400" h="500" r="0" h_flip="false" sprite_class="mountain" />
    <deco id="mountain_2" x="600" y="-500" w="350" h="400" r="0" h_flip="true" sprite_class="mountain" />
  </layer>
  <layer name="middleground" z="0" w="4000" h="1000">
    <!-- Main ground -->
    <platform_line id="ground" platform_item_perm="-1" platform_pc_perm="-1">
      <start x="-1800" y="0" />
      <end x="1800" y="0" />
    </platform_line>
    <!-- Rising slope left side -->
    <platform_line id="slope_left" platform_item_perm="-1" platform_pc_perm="-1">
      <start x="-1200" y="0" />
      <end x="-600" y="-200" />
    </platform_line>
    <!-- Plateau -->
    <platform_line id="plateau" platform_item_perm="-1" platform_pc_perm="-1">
      <start x="-600" y="-200" />
      <end x="200" y="-200" />
    </platform_line>
    <!-- Descending slope right side -->
    <platform_line id="slope_right" platform_item_perm="-1" platform_pc_perm="-1">
      <start x="200" y="-200" />
      <end x="800" y="0" />
    </platform_line>
    <!-- Floating one-way platform -->
    <platform_line id="float_1" platform_item_perm="0" platform_pc_perm="0">
      <start x="1000" y="-300" />
      <end x="1400" y="-280" />
    </platform_line>
    <!-- Decos -->
    <deco id="rock_1" x="-400" y="-200" w="80" h="60" r="0" h_flip="false" sprite_class="rock" />
    <deco id="rock_2" x="100" y="-200" w="60" h="45" r="15" h_flip="false" sprite_class="rock" />
    <!-- Signpost: left edge connects to demo_meadow -->
    <signpost id="sign_to_meadow" x="-1950" y="0">
      <connect target_tsid="demo_meadow" label="To the Meadow" />
    </signpost>
    <!-- Walls -->
    <wall id="wall_left" x="-2000" y="0" h="1000" />
    <wall id="wall_right" x="2000" y="0" h="1000" />
  </layer>
</location>
```

**Step 5: Add signpost to demo_meadow.xml**

Add a signpost near the right edge of demo_meadow, inside the `<layer name="middleground">` element:

```xml
<signpost id="sign_to_heights" x="1950" y="0">
  <connect target_tsid="demo_heights" label="To the Heights" />
</signpost>
```

**Step 6: Update the street loader**

In `src-tauri/src/lib.rs`, update `load_street_xml`:

```rust
fn load_street_xml(name: &str) -> Result<String, String> {
    match name {
        "demo_meadow" => Ok(include_str!("../../assets/streets/demo_meadow.xml").to_string()),
        "demo_heights" => Ok(include_str!("../../assets/streets/demo_heights.xml").to_string()),
        _ => Err(format!("Unknown street: {}", name)),
    }
}
```

Update `list_streets`:

```rust
fn list_streets() -> Vec<String> {
    vec!["demo_meadow".to_string(), "demo_heights".to_string()]
}
```

**Step 7: Verify both streets parse**

Run: `cd src-tauri && cargo test`
Expected: All existing tests pass. Add a quick parse test for demo_heights:

```rust
#[test]
fn parses_demo_heights() {
    let xml = include_str!("../../../assets/streets/demo_heights.xml");
    let street = parse_street(xml).unwrap();
    assert_eq!(street.name, "Demo Heights");
    assert!(!street.signposts.is_empty(), "demo_heights should have signposts");
}
```

Run: `cd src-tauri && cargo test parses_demo_heights`
Expected: PASS

**Step 8: Commit**

```bash
git add assets/streets/ src-tauri/src/lib.rs src-tauri/src/street/
git commit -m "feat: add demo_heights street with signpost connections"
```

---

### Task 3: Identity Management

Generate and persist a player identity (Ed25519/X25519 keypair) on first launch. Store display name alongside it. Expose via Tauri commands.

**Files:**
- Create: `src-tauri/src/identity/mod.rs`
- Create: `src-tauri/src/identity/persistence.rs`
- Modify: `src-tauri/src/lib.rs` (add module, commands, managed state)

**Step 1: Create identity module with persistence**

Create `src-tauri/src/identity/mod.rs`:

```rust
pub mod persistence;
```

Create `src-tauri/src/identity/persistence.rs`:

```rust
use harmony_identity::PrivateIdentity;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProfile {
    /// Hex-encoded private identity bytes (for now; could use OS keychain later)
    pub identity_hex: String,
    pub display_name: String,
}

/// Load or create a player profile in the given directory.
/// Creates the directory and a new identity if none exists.
pub fn load_or_create_profile(data_dir: &Path) -> Result<(PrivateIdentity, String), String> {
    let profile_path = data_dir.join("profile.json");

    if profile_path.exists() {
        let json = std::fs::read_to_string(&profile_path).map_err(|e| e.to_string())?;
        let profile: PlayerProfile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let id_bytes = hex::decode(&profile.identity_hex).map_err(|e| e.to_string())?;
        let identity = PrivateIdentity::from_bytes(&id_bytes).map_err(|e| e.to_string())?;
        Ok((identity, profile.display_name))
    } else {
        let mut rng = rand::thread_rng();
        let identity = PrivateIdentity::generate(&mut rng);
        let display_name = format!("Glitchen_{}", &hex::encode(&identity.public().address_hash)[..6]);

        std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        let profile = PlayerProfile {
            identity_hex: hex::encode(identity.to_bytes()),
            display_name: display_name.clone(),
        };
        let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
        std::fs::write(&profile_path, json).map_err(|e| e.to_string())?;

        Ok((identity, display_name))
    }
}
```

Note: Check `PrivateIdentity` API — it may use `from_bytes`/`to_bytes` or a different serialization method. Adapt accordingly. Add `hex = "0.4"` to Cargo.toml if not already present.

**Step 2: Write tests for identity persistence**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn creates_new_profile_when_none_exists() {
        let dir = TempDir::new().unwrap();
        let (identity, name) = load_or_create_profile(dir.path()).unwrap();
        assert!(name.starts_with("Glitchen_"));
        assert!(dir.path().join("profile.json").exists());
        // Address hash should be 16 bytes
        assert_eq!(identity.public().address_hash.len(), 16);
    }

    #[test]
    fn loads_existing_profile() {
        let dir = TempDir::new().unwrap();
        let (id1, name1) = load_or_create_profile(dir.path()).unwrap();
        let (id2, name2) = load_or_create_profile(dir.path()).unwrap();
        assert_eq!(name1, name2);
        assert_eq!(id1.public().address_hash, id2.public().address_hash);
    }
}
```

Add `tempfile = "3"` to `[dev-dependencies]` in Cargo.toml.

Run: `cd src-tauri && cargo test identity`
Expected: PASS

**Step 3: Add Tauri commands and managed state**

In `src-tauri/src/lib.rs`, add the module and identity-related state/commands:

```rust
pub mod identity;

use identity::persistence::load_or_create_profile;

struct PlayerIdentity {
    identity: Mutex<harmony_identity::PrivateIdentity>,
    display_name: Mutex<String>,
}

#[tauri::command]
fn get_identity(app: AppHandle) -> Result<serde_json::Value, String> {
    let pi = app.state::<PlayerIdentity>();
    let identity = pi.identity.lock().map_err(|e| e.to_string())?;
    let name = pi.display_name.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "displayName": *name,
        "addressHash": hex::encode(identity.public().address_hash),
    }))
}

#[tauri::command]
fn set_display_name(name: String, app: AppHandle) -> Result<(), String> {
    let pi = app.state::<PlayerIdentity>();
    let mut display_name = pi.display_name.lock().map_err(|e| e.to_string())?;
    *display_name = name;
    // TODO: persist to profile.json
    Ok(())
}
```

Register identity state in `run()` (load profile from Tauri's app data directory) and add commands to `invoke_handler`.

**Step 4: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: Compiles. Run: `cd src-tauri && cargo test`
Expected: All tests pass.

**Step 5: Commit**

```bash
git add src-tauri/src/identity/ src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: identity management with persistence and Tauri commands"
```

---

### Task 4: Network Wire Types

Define the compact message types that cross the wire: `PlayerNetState`, `ChatMessage`, `PresenceEvent`. All must serialize/deserialize and fit within Reticulum's 500-byte MTU.

**Files:**
- Create: `src-tauri/src/network/mod.rs`
- Create: `src-tauri/src/network/types.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod network;`)

**Step 1: Define wire types**

Create `src-tauri/src/network/mod.rs`:

```rust
pub mod types;
```

Create `src-tauri/src/network/types.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Compact player state for 60Hz network updates.
/// Uses f32 (not f64) to save wire bytes — sub-pixel precision is unnecessary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PlayerNetState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// 0 = left, 1 = right
    pub facing: u8,
    pub on_ground: bool,
}

/// Chat message — ephemeral, no history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    /// UTF-8 text, should be capped by sender to ~200 chars.
    pub text: String,
    /// Sender's address hash (16 bytes).
    pub sender: [u8; 16],
    /// Sender's display name at time of sending.
    pub sender_name: String,
}

/// Presence event — join/leave a street.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PresenceEvent {
    Joined {
        address_hash: [u8; 16],
        display_name: String,
    },
    Left {
        address_hash: [u8; 16],
    },
}

/// Wrapper enum for all network messages, so we can tag them on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetMessage {
    PlayerState(PlayerNetState),
    Chat(ChatMessage),
    Presence(PresenceEvent),
}
```

**Step 2: Write serialization round-trip and MTU tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const RETICULUM_MTU: usize = 500;
    // Header overhead: Reticulum header (19-35 bytes) + Zenoh envelope (33 bytes)
    const MAX_PAYLOAD: usize = RETICULUM_MTU - 35 - 33;

    #[test]
    fn player_net_state_round_trip() {
        let state = PlayerNetState {
            x: 123.456,
            y: -789.012,
            vx: 200.0,
            vy: -400.0,
            facing: 1,
            on_ground: true,
        };
        let msg = NetMessage::PlayerState(state);
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::PlayerState(s) => assert_eq!(s, state),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn player_state_fits_in_mtu() {
        let state = PlayerNetState {
            x: -99999.99,
            y: -99999.99,
            vx: 999.99,
            vy: 999.99,
            facing: 1,
            on_ground: true,
        };
        let msg = NetMessage::PlayerState(state);
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "PlayerState is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn chat_message_round_trip() {
        let msg = NetMessage::Chat(ChatMessage {
            text: "Hello world!".into(),
            sender: [0xAB; 16],
            sender_name: "Alice".into(),
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::Chat(c) => assert_eq!(c.text, "Hello world!"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn max_chat_fits_in_mtu() {
        let msg = NetMessage::Chat(ChatMessage {
            text: "x".repeat(200),
            sender: [0xFF; 16],
            sender_name: "A".repeat(30),
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "Max chat is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn presence_event_round_trip() {
        let msg = NetMessage::Presence(PresenceEvent::Joined {
            address_hash: [0x42; 16],
            display_name: "Bob".into(),
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::Presence(PresenceEvent::Joined { display_name, .. }) => {
                assert_eq!(display_name, "Bob");
            }
            _ => panic!("wrong variant"),
        }
    }
}
```

Run: `cd src-tauri && cargo test network::types`
Expected: All PASS

**Step 3: Consider binary serialization**

If JSON serialization is too large for MTU (check test results), switch to a compact binary format. Options:
- `bincode` — very compact, no schema overhead
- `postcard` — designed for embedded/constrained environments
- Manual byte packing — most compact but brittle

For Phase B, JSON is fine if it fits. Optimize later if needed.

**Step 4: Commit**

```bash
git add src-tauri/src/network/ src-tauri/src/lib.rs
git commit -m "feat: network wire types with serialization and MTU tests"
```

---

### Task 5: Remote Player Registry

Track remote players by address hash. Apply presence events (join/leave) and position updates. Produce `Vec<RemotePlayerFrame>` for the RenderFrame. Handle timeout of stale players.

**Files:**
- Create: `src-tauri/src/network/registry.rs`
- Modify: `src-tauri/src/network/mod.rs`
- Modify: `src-tauri/src/engine/state.rs` (add RemotePlayerFrame to RenderFrame)

**Step 1: Define RemotePlayerFrame**

Add to `src-tauri/src/engine/state.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemotePlayerFrame {
    pub address_hash: String,  // hex-encoded for JSON/IPC
    pub display_name: String,
    pub x: f64,
    pub y: f64,
    pub facing: String,  // "left" or "right"
    pub on_ground: bool,
}
```

Add to `RenderFrame`:

```rust
pub struct RenderFrame {
    pub player: PlayerFrame,
    pub remote_players: Vec<RemotePlayerFrame>,  // NEW
    pub camera: CameraFrame,
    pub street_id: String,
}
```

Update `GameState::tick()` to include `remote_players: vec![]` in the returned RenderFrame (empty for now — Task 8 wires it up).

**Step 2: Create the registry**

Create `src-tauri/src/network/registry.rs`:

```rust
use crate::engine::state::RemotePlayerFrame;
use crate::network::types::{PlayerNetState, PresenceEvent};
use std::collections::HashMap;

/// Timeout in seconds — if no update received, remove the player.
const STALE_TIMEOUT: f64 = 10.0;

#[derive(Debug, Clone)]
struct RemotePlayer {
    address_hash: [u8; 16],
    display_name: String,
    state: PlayerNetState,
    last_update: f64,  // seconds since epoch or game start
}

/// Tracks all remote players on the current street.
#[derive(Debug)]
pub struct RemotePlayerRegistry {
    players: HashMap<[u8; 16], RemotePlayer>,
}

impl RemotePlayerRegistry {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
        }
    }

    /// Apply a presence event.
    pub fn handle_presence(&mut self, event: &PresenceEvent) {
        match event {
            PresenceEvent::Joined { address_hash, display_name } => {
                self.players.insert(*address_hash, RemotePlayer {
                    address_hash: *address_hash,
                    display_name: display_name.clone(),
                    state: PlayerNetState {
                        x: 0.0, y: 0.0, vx: 0.0, vy: 0.0,
                        facing: 1, on_ground: true,
                    },
                    last_update: 0.0,
                });
            }
            PresenceEvent::Left { address_hash } => {
                self.players.remove(address_hash);
            }
        }
    }

    /// Apply a position update from a remote player.
    pub fn update_state(&mut self, address_hash: &[u8; 16], state: PlayerNetState, now: f64) {
        if let Some(player) = self.players.get_mut(address_hash) {
            player.state = state;
            player.last_update = now;
        }
        // Ignore updates from unknown players — they'll Joined first.
    }

    /// Remove players that haven't sent an update within the timeout.
    pub fn purge_stale(&mut self, now: f64) {
        self.players.retain(|_, p| {
            p.last_update == 0.0 || (now - p.last_update) < STALE_TIMEOUT
        });
    }

    /// Produce frames for rendering. Sorted by address hash for determinism.
    pub fn frames(&self) -> Vec<RemotePlayerFrame> {
        let mut frames: Vec<_> = self.players.values().map(|p| RemotePlayerFrame {
            address_hash: hex::encode(p.address_hash),
            display_name: p.display_name.clone(),
            x: p.state.x as f64,
            y: p.state.y as f64,
            facing: if p.state.facing == 0 { "left".into() } else { "right".into() },
            on_ground: p.state.on_ground,
        }).collect();
        frames.sort_by(|a, b| a.address_hash.cmp(&b.address_hash));
        frames
    }

    /// Clear all players (e.g., on street change).
    pub fn clear(&mut self) {
        self.players.clear();
    }

    pub fn count(&self) -> usize {
        self.players.len()
    }
}
```

Add `pub mod registry;` to `src-tauri/src/network/mod.rs`.

**Step 3: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_hash(id: u8) -> [u8; 16] {
        [id; 16]
    }

    #[test]
    fn join_and_leave() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(&PresenceEvent::Joined {
            address_hash: make_hash(1),
            display_name: "Alice".into(),
        });
        assert_eq!(reg.count(), 1);
        assert_eq!(reg.frames()[0].display_name, "Alice");

        reg.handle_presence(&PresenceEvent::Left {
            address_hash: make_hash(1),
        });
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn update_position() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(&PresenceEvent::Joined {
            address_hash: make_hash(1),
            display_name: "Alice".into(),
        });
        reg.update_state(&make_hash(1), PlayerNetState {
            x: 100.0, y: -50.0, vx: 200.0, vy: 0.0, facing: 1, on_ground: true,
        }, 1.0);
        let frames = reg.frames();
        assert_eq!(frames[0].x, 100.0);
        assert_eq!(frames[0].y, -50.0);
    }

    #[test]
    fn ignores_update_for_unknown_player() {
        let mut reg = RemotePlayerRegistry::new();
        reg.update_state(&make_hash(99), PlayerNetState {
            x: 100.0, y: 0.0, vx: 0.0, vy: 0.0, facing: 0, on_ground: true,
        }, 1.0);
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn purges_stale_players() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(&PresenceEvent::Joined {
            address_hash: make_hash(1),
            display_name: "Alice".into(),
        });
        reg.update_state(&make_hash(1), PlayerNetState {
            x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, facing: 1, on_ground: true,
        }, 1.0);
        // 11 seconds later, beyond STALE_TIMEOUT
        reg.purge_stale(12.0);
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn clear_removes_all() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(&PresenceEvent::Joined {
            address_hash: make_hash(1),
            display_name: "Alice".into(),
        });
        reg.handle_presence(&PresenceEvent::Joined {
            address_hash: make_hash(2),
            display_name: "Bob".into(),
        });
        reg.clear();
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn frames_sorted_deterministically() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(&PresenceEvent::Joined {
            address_hash: make_hash(3),
            display_name: "Charlie".into(),
        });
        reg.handle_presence(&PresenceEvent::Joined {
            address_hash: make_hash(1),
            display_name: "Alice".into(),
        });
        let frames = reg.frames();
        assert!(frames[0].address_hash < frames[1].address_hash);
    }
}
```

Run: `cd src-tauri && cargo test registry`
Expected: All PASS

**Step 4: Commit**

```bash
git add src-tauri/src/network/ src-tauri/src/engine/state.rs
git commit -m "feat: remote player registry with presence, updates, and stale purge"
```

---

### Task 6: NetworkState — Core Sans-I/O Network State Machine

Build the central `NetworkState` that wraps Reticulum `Node`, Zenoh `Session`s, and `PubSubRouter`. It exposes a `tick()` method that the game loop calls each frame. It handles:
- Announce broadcast and reception
- Session establishment when peers are discovered
- Pub/sub topic management per street
- Inbound message dispatch (player state, chat, presence)
- Outbound player state publishing

**This is the largest and most complex task.** It requires understanding the Reticulum and Zenoh APIs thoroughly. Read the harmony crate source and tests before coding.

**Files:**
- Create: `src-tauri/src/network/state.rs`
- Modify: `src-tauri/src/network/mod.rs`

**Step 1: Read the harmony crate APIs**

Before writing code, read these files to understand the exact method signatures and event/action types:
- `/Users/zeblith/work/zeblithic/harmony/crates/harmony-reticulum/src/node.rs` — `Node::new()`, `handle_event()`, `announce()`, `register_interface()`, `register_destination()`
- `/Users/zeblith/work/zeblithic/harmony/crates/harmony-reticulum/src/link.rs` — `Link::initiate()`, `Link::respond()`, `handle_proof()`
- `/Users/zeblith/work/zeblithic/harmony/crates/harmony-zenoh/src/session.rs` — `Session::new()`, `handle_event()`
- `/Users/zeblith/work/zeblithic/harmony/crates/harmony-zenoh/src/pubsub.rs` — `PubSubRouter::new()`, `subscribe()`, `publish()`, `handle_event()`

Check test files in each crate for usage examples.

**Step 2: Design the NetworkState struct**

```rust
use crate::network::registry::RemotePlayerRegistry;
use crate::network::types::{ChatMessage, NetMessage, PlayerNetState, PresenceEvent};
use harmony_identity::PrivateIdentity;
use harmony_reticulum::node::{Node, NodeEvent, NodeAction};
use harmony_zenoh::session::Session;
use harmony_zenoh::pubsub::PubSubRouter;
use std::collections::HashMap;
use std::net::UdpSocket;

/// Actions the game loop must execute after a network tick.
pub enum NetworkAction {
    /// A remote player's state was updated.
    RemotePlayerUpdate { address_hash: [u8; 16], state: PlayerNetState },
    /// A presence event occurred.
    PresenceChange(PresenceEvent),
    /// A chat message was received.
    ChatReceived(ChatMessage),
    /// Send raw bytes on a UDP socket.
    SendUdp { target: std::net::SocketAddr, data: Vec<u8> },
    /// Send raw bytes on a TCP stream.
    SendTcp { peer_id: [u8; 16], data: Vec<u8> },
}

pub struct NetworkState {
    identity: PrivateIdentity,
    display_name: String,
    node: Node,
    sessions: HashMap<[u8; 16], Session>,  // keyed by peer address hash
    routers: HashMap<[u8; 16], PubSubRouter>,  // one per session
    current_street: Option<String>,
    registry: RemotePlayerRegistry,
    // Socket handles are owned externally and passed into tick()
}
```

**Step 3: Implement the tick method**

The `tick` method processes inbound packets and returns actions:

```rust
impl NetworkState {
    pub fn new(identity: PrivateIdentity, display_name: String) -> Self { /* ... */ }

    /// Process inbound raw packets and timer events.
    /// Returns actions for the game loop to execute.
    pub fn tick(
        &mut self,
        inbound_packets: &[(String, Vec<u8>)],  // (interface_name, raw_bytes)
        now_ms: u64,
        rng: &mut impl rand::CryptoRngCore,
    ) -> Vec<NetworkAction> { /* ... */ }

    /// Publish local player state to all peers on the current street.
    pub fn publish_player_state(&mut self, state: PlayerNetState) -> Vec<NetworkAction> { /* ... */ }

    /// Send a chat message on the current street.
    pub fn send_chat(&mut self, text: String) -> Vec<NetworkAction> { /* ... */ }

    /// Switch to a new street — unsubscribe old topics, subscribe new ones.
    pub fn change_street(&mut self, street_name: &str) -> Vec<NetworkAction> { /* ... */ }

    /// Get current remote player frames for rendering.
    pub fn remote_frames(&self) -> Vec<crate::engine::state::RemotePlayerFrame> {
        self.registry.frames()
    }

    /// Number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.sessions.values().filter(|s| s.state() == SessionState::Active).count()
    }
}
```

**Step 4: Implement the internal event flow**

Inside `tick()`:

1. Feed each inbound packet to `self.node.handle_event(NodeEvent::InboundPacket { ... })`.
2. Feed a `NodeEvent::TimerTick { now }` for keepalive/timeout processing.
3. Process the returned `NodeAction`s:
   - `AnnounceReceived` → Check if peer is on same/adjacent street, initiate Link if so.
   - `DeliverLocally` → Route to the appropriate Session's `handle_event`.
   - `SendOnInterface` → Return as `NetworkAction::SendUdp`.
4. For each Session, process any pending events.
5. For each PubSubRouter, dispatch `Deliver` actions to the registry and chat handler.

**Step 5: Write tests**

Test the NetworkState in isolation (no real sockets):

```rust
#[test]
fn announces_on_first_tick() {
    let identity = test_identity();
    let mut net = NetworkState::new(identity, "TestPlayer".into());
    net.change_street("demo_meadow");
    let actions = net.tick(&[], 0, &mut rng());
    // Should contain at least one SendUdp action (the announce broadcast)
    assert!(actions.iter().any(|a| matches!(a, NetworkAction::SendUdp { .. })));
}

#[test]
fn processes_remote_player_state() {
    // Simulate receiving a PlayerNetState message from a peer
    // after session establishment
    // ...
}

#[test]
fn changes_street_resubscribes() {
    let identity = test_identity();
    let mut net = NetworkState::new(identity, "TestPlayer".into());
    net.change_street("demo_meadow");
    net.change_street("demo_heights");
    // Registry should be cleared
    assert_eq!(net.registry.count(), 0);
}
```

Run: `cd src-tauri && cargo test network::state`
Expected: All PASS

**Step 6: Commit**

```bash
git add src-tauri/src/network/
git commit -m "feat: NetworkState sans-I/O state machine with Reticulum + Zenoh"
```

**Important notes for the implementer:**
- This is the hardest task. Take time to read the harmony crate tests and understand the event/action patterns.
- Start with the simplest flow: announce → receive announce → establish link. Get that working before adding pub/sub.
- The Reticulum Node, Zenoh Session, and PubSubRouter are all driven the same way: feed events, get actions, execute actions. Layer them.
- If you get stuck on API mismatches, check the latest harmony crate source — the exploration above gives approximate signatures but they may have evolved.

---

### Task 7: Socket I/O Layer

Create non-blocking UDP and TCP socket management. The game loop calls this to read/write raw bytes without blocking.

**Files:**
- Create: `src-tauri/src/network/transport.rs`
- Modify: `src-tauri/src/network/mod.rs`

**Step 1: Create transport module**

```rust
use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::{SocketAddr, UdpSocket};

/// Default port for Reticulum LAN discovery.
pub const DEFAULT_PORT: u16 = 29170;

/// Non-blocking UDP transport for LAN discovery and data.
pub struct UdpTransport {
    socket: UdpSocket,
    recv_buf: Vec<u8>,
}

impl UdpTransport {
    /// Bind to the given port. Sets socket to non-blocking.
    /// Enables SO_REUSEADDR for multiple instances on same machine (dev/testing).
    pub fn bind(port: u16) -> io::Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        socket.set_nonblocking(true)?;
        socket.set_broadcast(true)?;
        socket.bind(&SocketAddr::from(([0, 0, 0, 0], port)).into())?;
        Ok(Self {
            socket: socket.into(),
            recv_buf: vec![0u8; 600], // > Reticulum MTU (500)
        })
    }

    /// Read all available packets (non-blocking). Returns (data, source_addr) pairs.
    pub fn recv_all(&mut self) -> Vec<(Vec<u8>, SocketAddr)> {
        let mut packets = Vec::new();
        loop {
            match self.socket.recv_from(&mut self.recv_buf) {
                Ok((len, addr)) => {
                    packets.push((self.recv_buf[..len].to_vec(), addr));
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        packets
    }

    /// Send data to a specific address.
    pub fn send_to(&self, data: &[u8], addr: SocketAddr) -> io::Result<usize> {
        self.socket.send_to(data, addr)
    }

    /// Broadcast data on the LAN (255.255.255.255:port).
    pub fn broadcast(&self, data: &[u8], port: u16) -> io::Result<usize> {
        self.socket.send_to(data, SocketAddr::from(([255, 255, 255, 255], port)))
    }
}

/// Optional TCP connection for WAN relay.
pub struct TcpRelay {
    // TODO: implement in a follow-up step.
    // For Phase B MVP, LAN UDP is sufficient.
    // TCP relay connects to a known harmony-node transport.
}
```

**Step 2: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_and_reads_without_blocking() {
        let transport = UdpTransport::bind(0).unwrap(); // port 0 = OS picks
        let packets = transport.recv_all();
        assert!(packets.is_empty()); // No data sent, no data received
    }

    #[test]
    fn send_and_receive_loopback() {
        let t1 = UdpTransport::bind(0).unwrap();
        let t2 = UdpTransport::bind(0).unwrap();

        let addr1 = t1.socket.local_addr().unwrap();
        t2.send_to(b"hello", addr1).unwrap();

        // Brief pause for OS to deliver — non-blocking read
        std::thread::sleep(std::time::Duration::from_millis(10));

        let mut t1 = t1; // rebind as mutable
        let packets = t1.recv_all();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].0, b"hello");
    }
}
```

Run: `cd src-tauri && cargo test transport`
Expected: All PASS

**Step 3: Commit**

```bash
git add src-tauri/src/network/transport.rs src-tauri/src/network/mod.rs
git commit -m "feat: non-blocking UDP transport for LAN discovery"
```

---

### Task 8: Game Loop Integration

Expand the game loop to drive `NetworkState` alongside physics. Publish local player state each tick. Process remote player updates into the `RenderFrame`.

**Files:**
- Modify: `src-tauri/src/lib.rs` (game_loop, managed state, commands)
- Modify: `src-tauri/src/engine/state.rs` (GameState accepts remote players)

**Step 1: Add NetworkState to Tauri managed state**

Add to `lib.rs`:

```rust
use network::state::NetworkState;
use network::transport::UdpTransport;

struct NetworkWrapper(Mutex<NetworkState>);
struct TransportWrapper(Mutex<UdpTransport>);
```

Initialize in `run()`:
```rust
// Load identity from profile
let data_dir = /* Tauri app data dir */;
let (identity, display_name) = load_or_create_profile(&data_dir)?;
let net_state = NetworkState::new(identity.clone(), display_name.clone());
let transport = UdpTransport::bind(network::transport::DEFAULT_PORT)?;

// ... .manage(NetworkWrapper(Mutex::new(net_state)))
// ... .manage(TransportWrapper(Mutex::new(transport)))
```

**Step 2: Expand the game loop**

Modify the `game_loop` function in `lib.rs`. Before the physics tick, drain network packets. After the physics tick, publish local state. Include remote players in the RenderFrame.

```rust
fn game_loop(app: AppHandle) {
    let tick_duration = Duration::from_secs_f64(1.0 / 60.0);
    let dt = 1.0 / 60.0;
    let mut rng = rand::thread_rng();

    loop {
        let tick_start = Instant::now();

        // Check if still running
        let running = app.state::<GameRunning>();
        let is_running = running.0.lock().unwrap_or_else(|e| e.into_inner());
        if !*is_running { break; }
        drop(is_running);

        // --- Network tick ---
        // 1. Read inbound packets (non-blocking)
        let inbound = {
            let transport = app.state::<TransportWrapper>();
            let mut t = transport.0.lock().unwrap_or_else(|e| e.into_inner());
            t.recv_all()
                .into_iter()
                .map(|(data, _addr)| ("udp".to_string(), data))
                .collect::<Vec<_>>()
        };

        // 2. Tick network state
        let now_ms = /* monotonic ms */;
        let net_actions = {
            let net = app.state::<NetworkWrapper>();
            let mut net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());
            net_state.tick(&inbound, now_ms, &mut rng)
        };

        // 3. Execute network actions (send packets)
        for action in net_actions {
            match action {
                NetworkAction::SendUdp { target, data } => {
                    let transport = app.state::<TransportWrapper>();
                    let t = transport.0.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = t.send_to(&data, target);
                }
                _ => {} // Handle other actions as needed
            }
        }

        // Read current input
        let input_wrapper = app.state::<InputStateWrapper>();
        let input = *input_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());

        // --- Physics tick + RenderFrame ---
        let frame = {
            let state_wrapper = app.state::<GameStateWrapper>();
            let mut state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
            let frame = state.tick(dt, &input);

            // Add remote players to frame
            if let Some(mut frame) = frame {
                let net = app.state::<NetworkWrapper>();
                let net_state = net.0.lock().unwrap_or_else(|e| e.into_inner());
                frame.remote_players = net_state.remote_frames();

                // Publish local player state
                drop(net_state); // drop before re-locking
                // ... publish via NetworkState
                Some(frame)
            } else {
                None
            }
        };

        if let Some(frame) = frame {
            let _ = app.emit("render_frame", &frame);
        }

        // Sleep for remainder of tick
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            std::thread::sleep(tick_duration - elapsed);
        }
    }
}
```

**Step 3: Add network-related Tauri commands**

```rust
#[tauri::command]
fn send_chat(message: String, app: AppHandle) -> Result<(), String> {
    let net = app.state::<NetworkWrapper>();
    let transport = app.state::<TransportWrapper>();
    let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
    let actions = net_state.send_chat(message);
    let t = transport.0.lock().map_err(|e| e.to_string())?;
    for action in actions {
        if let NetworkAction::SendUdp { target, data } = action {
            let _ = t.send_to(&data, target);
        }
    }
    Ok(())
}

#[tauri::command]
fn get_network_status(app: AppHandle) -> Result<serde_json::Value, String> {
    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "peerCount": net_state.peer_count(),
    }))
}
```

Register new commands in `invoke_handler`.

**Step 4: Verify compilation and existing tests**

Run: `cd src-tauri && cargo check`
Run: `cd src-tauri && cargo test`
Expected: All 48+ existing tests still pass. New integration works at compile level.

**Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/engine/state.rs
git commit -m "feat: integrate NetworkState into game loop with IPC commands"
```

---

### Task 9: Street Transition State Machine

Detect when the player approaches a signpost (pre-subscription zone). Track transition progress. Orchestrate the swoop: both streets visible, sliding animation, network presence events.

**Files:**
- Create: `src-tauri/src/engine/transition.rs`
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/engine/state.rs`

**Step 1: Define the transition state machine**

```rust
/// Distance from signpost to trigger pre-subscription (in game units).
const PRE_SUBSCRIBE_DISTANCE: f64 = 500.0;

/// Minimum swoop duration in seconds.
const MIN_SWOOP_SECS: f64 = 0.3;
/// Maximum swoop duration in seconds.
const MAX_SWOOP_SECS: f64 = 2.0;

#[derive(Debug, Clone, PartialEq)]
pub enum TransitionPhase {
    /// No transition in progress.
    None,
    /// Player is near a signpost — pre-subscribing to adjacent street.
    PreSubscribed {
        target_street: String,
        signpost_x: f64,
        direction: TransitionDirection,
    },
    /// Player crossed the threshold — swoop in progress.
    Swooping {
        from_street: String,
        to_street: String,
        direction: TransitionDirection,
        progress: f64,      // 0.0 → 1.0
        elapsed: f64,       // seconds into the swoop
        target_duration: f64, // adapts to load readiness
        street_ready: bool,   // is the new street loaded and ready?
    },
    /// Swoop complete — clean up old street.
    Complete {
        new_street: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionDirection {
    Left,  // Player moving left → new street slides in from left
    Right, // Player moving right → new street slides in from right
}

pub struct TransitionState {
    pub phase: TransitionPhase,
}
```

**Step 2: Implement tick logic**

```rust
impl TransitionState {
    pub fn new() -> Self {
        Self { phase: TransitionPhase::None }
    }

    /// Check signpost proximity each tick.
    pub fn check_signposts(
        &mut self,
        player_x: f64,
        signposts: &[Signpost],
        street_left: f64,
        street_right: f64,
    ) {
        if self.phase != TransitionPhase::None { return; }

        for signpost in signposts {
            if signpost.connects.is_empty() { continue; }
            let dist = (player_x - signpost.x).abs();
            if dist < PRE_SUBSCRIBE_DISTANCE {
                let direction = if signpost.x < (street_left + street_right) / 2.0 {
                    TransitionDirection::Left
                } else {
                    TransitionDirection::Right
                };
                self.phase = TransitionPhase::PreSubscribed {
                    target_street: signpost.connects[0].target_tsid.clone(),
                    signpost_x: signpost.x,
                    direction,
                };
                return;
            }
        }
    }

    /// Called when player crosses the signpost threshold.
    pub fn trigger_swoop(&mut self, from_street: String) {
        if let TransitionPhase::PreSubscribed { target_street, direction, .. } = &self.phase {
            self.phase = TransitionPhase::Swooping {
                from_street,
                to_street: target_street.clone(),
                direction: *direction,
                progress: 0.0,
                elapsed: 0.0,
                target_duration: MAX_SWOOP_SECS,  // starts max, shrinks when ready
                street_ready: false,
            };
        }
    }

    /// Notify that the destination street is loaded and ready.
    pub fn mark_street_ready(&mut self) {
        if let TransitionPhase::Swooping { street_ready, target_duration, elapsed, .. } = &mut self.phase {
            *street_ready = true;
            // Shrink duration to finish soon, but respect minimum
            let remaining = (*target_duration - *elapsed).max(MIN_SWOOP_SECS);
            *target_duration = *elapsed + remaining;
        }
    }

    /// Advance the swoop animation.
    pub fn tick(&mut self, dt: f64) {
        if let TransitionPhase::Swooping {
            to_street, elapsed, target_duration, progress, street_ready, ..
        } = &mut self.phase {
            *elapsed += dt;
            if *street_ready {
                *progress = (*elapsed / *target_duration).min(1.0);
            } else {
                // Slow progress — don't complete until ready
                *progress = (*elapsed / MAX_SWOOP_SECS).min(0.9);
            }
            if *progress >= 1.0 {
                self.phase = TransitionPhase::Complete {
                    new_street: to_street.clone(),
                };
            }
        }
    }

    /// Get swoop progress (0.0-1.0) for the renderer, or None if not swooping.
    pub fn swoop_progress(&self) -> Option<(f64, TransitionDirection)> {
        match &self.phase {
            TransitionPhase::Swooping { progress, direction, .. } => Some((*progress, *direction)),
            _ => None,
        }
    }

    /// Reset after the game has fully transitioned.
    pub fn reset(&mut self) {
        self.phase = TransitionPhase::None;
    }
}
```

**Step 3: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::types::{Signpost, SignpostConnection};

    fn make_signpost(x: f64, target: &str) -> Signpost {
        Signpost {
            id: "sign".into(),
            x,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: target.into(),
                target_label: "Go".into(),
            }],
        }
    }

    #[test]
    fn detects_pre_subscribe_zone() {
        let mut ts = TransitionState::new();
        let signposts = vec![make_signpost(1950.0, "other")];
        ts.check_signposts(1500.0, &signposts, -2000.0, 2000.0);
        assert!(matches!(ts.phase, TransitionPhase::PreSubscribed { .. }));
    }

    #[test]
    fn no_detection_when_far_from_signpost() {
        let mut ts = TransitionState::new();
        let signposts = vec![make_signpost(1950.0, "other")];
        ts.check_signposts(0.0, &signposts, -2000.0, 2000.0);
        assert_eq!(ts.phase, TransitionPhase::None);
    }

    #[test]
    fn swoop_completes_when_ready() {
        let mut ts = TransitionState::new();
        ts.phase = TransitionPhase::PreSubscribed {
            target_street: "other".into(),
            signpost_x: 1950.0,
            direction: TransitionDirection::Right,
        };
        ts.trigger_swoop("meadow".into());
        ts.mark_street_ready();

        // Tick until complete
        for _ in 0..60 { // ~1 second at 60fps
            ts.tick(1.0 / 60.0);
        }
        assert!(matches!(ts.phase, TransitionPhase::Complete { .. }));
    }

    #[test]
    fn swoop_stalls_without_ready() {
        let mut ts = TransitionState::new();
        ts.phase = TransitionPhase::PreSubscribed {
            target_street: "other".into(),
            signpost_x: 1950.0,
            direction: TransitionDirection::Right,
        };
        ts.trigger_swoop("meadow".into());
        // Don't mark ready

        for _ in 0..180 { // 3 seconds
            ts.tick(1.0 / 60.0);
        }
        // Progress should cap at 0.9, not complete
        if let TransitionPhase::Swooping { progress, .. } = &ts.phase {
            assert!(*progress <= 0.9);
        } else {
            panic!("should still be swooping");
        }
    }

    #[test]
    fn minimum_swoop_duration_respected() {
        let mut ts = TransitionState::new();
        ts.phase = TransitionPhase::PreSubscribed {
            target_street: "other".into(),
            signpost_x: 1950.0,
            direction: TransitionDirection::Right,
        };
        ts.trigger_swoop("meadow".into());
        ts.mark_street_ready();

        // After 0.1s (below minimum), should NOT be complete
        for _ in 0..6 { // 0.1s at 60fps
            ts.tick(1.0 / 60.0);
        }
        assert!(!matches!(ts.phase, TransitionPhase::Complete { .. }));
    }
}
```

Run: `cd src-tauri && cargo test transition`
Expected: All PASS

**Step 4: Integrate into GameState**

Add `TransitionState` to `GameState`. In `GameState::tick()`:
1. After physics, check signpost proximity via `transition.check_signposts()`
2. If player crosses street edge and transition is `PreSubscribed`, call `trigger_swoop()`
3. Call `transition.tick(dt)` each frame
4. Add transition progress to `RenderFrame`:
   ```rust
   pub struct RenderFrame {
       // ... existing fields ...
       pub transition: Option<TransitionInfo>,
   }

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct TransitionInfo {
       pub progress: f64,          // 0.0 → 1.0
       pub direction: String,     // "left" or "right"
       pub to_street: String,     // name of destination street
   }
   ```

**Step 5: Commit**

```bash
git add src-tauri/src/engine/
git commit -m "feat: street transition state machine with swoop animation"
```

---

### Task 10: Frontend Types and IPC Expansion

Add TypeScript types for remote players, chat, presence, and transition info. Add new IPC command wrappers.

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`

**Step 1: Add TypeScript types**

Add to `src/lib/types.ts`:

```typescript
export interface RemotePlayerFrame {
  addressHash: string;
  displayName: string;
  x: number;
  y: number;
  facing: Direction;
  onGround: boolean;
}

export interface TransitionInfo {
  progress: number;
  direction: 'left' | 'right';
  toStreet: string;
}

// Extend RenderFrame
export interface RenderFrame {
  player: PlayerFrame;
  remotePlayers: RemotePlayerFrame[];  // NEW
  camera: CameraFrame;
  streetId: string;
  transition: TransitionInfo | null;   // NEW
}

export interface NetworkStatus {
  peerCount: number;
}

export interface PlayerIdentity {
  displayName: string;
  addressHash: string;
}
```

**Step 2: Add IPC wrappers**

Add to `src/lib/ipc.ts`:

```typescript
export async function sendChat(message: string): Promise<void> {
  return invoke('send_chat', { message });
}

export async function getNetworkStatus(): Promise<NetworkStatus> {
  return invoke('get_network_status');
}

export async function getIdentity(): Promise<PlayerIdentity> {
  return invoke('get_identity');
}

export async function setDisplayName(name: string): Promise<void> {
  return invoke('set_display_name', { name });
}

export async function setRelayAddress(addr: string): Promise<void> {
  return invoke('set_relay_address', { addr });
}
```

**Step 3: Verify frontend builds**

Run: `npm run build`
Expected: Builds successfully. Type errors may appear in renderer.ts since RenderFrame changed — fix by adding default handling for new fields.

**Step 4: Commit**

```bash
git add src/lib/types.ts src/lib/ipc.ts
git commit -m "feat: frontend types and IPC for multiplayer and transitions"
```

---

### Task 11: Renderer — Remote Avatar Sprites

Render remote players as colored rectangles with display name labels. Manage sprite lifecycle: create on first appearance, update position each frame, remove when gone.

**Files:**
- Modify: `src/lib/engine/renderer.ts`

**Step 1: Add remote avatar rendering to updateFrame**

In `GameRenderer`, add a `Map<string, Container>` for remote player sprites. In `updateFrame`:

```typescript
// Remote player color — distinct from local avatar
private static REMOTE_COLOR = 0x4488FF;

private remoteSprites: Map<string, Container> = new Map();

updateFrame(frame: RenderFrame): void {
  // ... existing local player + camera + parallax code ...

  // Remote players
  const seen = new Set<string>();
  for (const remote of frame.remotePlayers) {
    seen.add(remote.addressHash);
    let sprite = this.remoteSprites.get(remote.addressHash);

    if (!sprite) {
      // Create new remote avatar
      sprite = new Container();
      const body = new Graphics();
      body.rect(-15, -60, 30, 60);
      body.fill(GameRenderer.REMOTE_COLOR);
      sprite.addChild(body);

      // Display name label
      const label = new Text({
        text: remote.displayName,
        style: { fontSize: 12, fill: 0xFFFFFF, align: 'center' },
      });
      label.anchor.set(0.5, 1);
      label.y = -65;
      sprite.addChild(label);

      this.worldContainer.addChild(sprite);
      this.remoteSprites.set(remote.addressHash, sprite);
    }

    // Update position (Glitch → screen coords)
    sprite.x = remote.x - this.street.left;
    sprite.y = remote.y - this.street.top;
    sprite.scale.x = remote.facing === 'right' ? 1 : -1;
  }

  // Remove departed players
  for (const [hash, sprite] of this.remoteSprites) {
    if (!seen.has(hash)) {
      this.worldContainer.removeChild(sprite);
      sprite.destroy();
      this.remoteSprites.delete(hash);
    }
  }
}
```

**Step 2: Clean up remote sprites in destroy()**

```typescript
destroy(): void {
  for (const [, sprite] of this.remoteSprites) {
    sprite.destroy();
  }
  this.remoteSprites.clear();
  this.app.destroy();
}
```

**Step 3: Handle empty remotePlayers gracefully**

Ensure the renderer handles `frame.remotePlayers` being undefined or empty (for backwards compatibility during development):

```typescript
const remotePlayers = frame.remotePlayers ?? [];
```

**Step 4: Verify frontend builds**

Run: `npm run build`
Expected: Builds without errors.

**Step 5: Commit**

```bash
git add src/lib/engine/renderer.ts
git commit -m "feat: render remote player avatars with name labels"
```

---

### Task 12: Renderer — Swoop Street Transition

Implement the swoop animation: old street slides out, new street slides in. Driven by `transition.progress` in the RenderFrame.

**Files:**
- Modify: `src/lib/engine/renderer.ts`
- Modify: `src/lib/components/GameCanvas.svelte`

**Step 1: Add transition rendering**

In `GameRenderer`, when `frame.transition` is non-null, offset the world container to create the slide effect:

```typescript
updateFrame(frame: RenderFrame): void {
  // ... existing code ...

  // Swoop transition
  if (frame.transition) {
    const { progress, direction } = frame.transition;
    const viewportWidth = this.app.canvas.width;
    const offset = direction === 'right'
      ? -progress * viewportWidth   // old street slides left
      : progress * viewportWidth;   // old street slides right
    this.worldContainer.x += offset;

    // Parallax layers follow the same offset
    for (const [, container] of this.layerContainers) {
      container.x += offset;
    }
  }
}
```

**Step 2: Handle new street scene building during transition**

The full swoop with two simultaneous world containers is complex. For Phase B, a simpler approach:
- During the swoop, the current street slides off-screen.
- When progress reaches 1.0, `GameCanvas` receives a signal to load the new street.
- The new street appears immediately (the swoop covered the loading seam).

In `GameCanvas.svelte`, watch for transition completion:

```typescript
$effect(() => {
  // When RenderFrame indicates transition complete, load new street
  // This is handled by the parent App.svelte via the onFrame callback
});
```

In `App.svelte`, the `onFrame` callback checks for `frame.transition?.progress >= 1.0` and triggers loading the new street.

**Step 3: Verify frontend builds**

Run: `npm run build`
Expected: Builds.

**Step 4: Commit**

```bash
git add src/lib/engine/renderer.ts src/lib/components/GameCanvas.svelte src/App.svelte
git commit -m "feat: swoop transition animation for street changes"
```

---

### Task 13: Chat System

End-to-end chat: Rust receives/publishes chat messages via pub/sub, frontend has a chat input that captures keyboard when focused, and chat bubbles appear above avatars.

**Files:**
- Create: `src/lib/components/ChatInput.svelte`
- Modify: `src/lib/engine/renderer.ts` (chat bubbles)
- Modify: `src/lib/components/GameCanvas.svelte` (chat event handling)
- Modify: `src/App.svelte` (include ChatInput)

**Step 1: Create ChatInput component**

```svelte
<script lang="ts">
  import { sendChat } from '../ipc';

  let { onFocusChange }: { onFocusChange: (focused: boolean) => void } = $props();

  let inputEl: HTMLInputElement;
  let text = $state('');
  let focused = $state(false);

  function handleGlobalKeyDown(e: KeyboardEvent) {
    if (!focused && (e.key === 'Enter' || e.key === '/')) {
      e.preventDefault();
      focused = true;
      onFocusChange(true);
      // Focus the input after state update
      requestAnimationFrame(() => inputEl?.focus());
    }
  }

  function handleSubmit() {
    if (text.trim()) {
      sendChat(text.trim()).catch(console.error);
      text = '';
    }
    blur();
  }

  function blur() {
    focused = false;
    onFocusChange(false);
    inputEl?.blur();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      text = '';
      blur();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      handleSubmit();
    }
    e.stopPropagation(); // Prevent game input while typing
  }
</script>

<svelte:window onkeydown={focused ? undefined : handleGlobalKeyDown} />

{#if focused}
  <div class="chat-input">
    <input
      bind:this={inputEl}
      bind:value={text}
      onkeydown={handleKeyDown}
      placeholder="Type a message..."
      maxlength="200"
    />
  </div>
{/if}

<style>
  .chat-input {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 100;
  }
  input {
    width: 400px;
    padding: 8px 12px;
    font-size: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-radius: 20px;
    background: rgba(0, 0, 0, 0.6);
    color: white;
    outline: none;
  }
</style>
```

**Step 2: Add chat bubbles to renderer**

In `GameRenderer`, track active chat bubbles with a decay timer:

```typescript
interface ChatBubble {
  text: Text;
  targetHash: string;
  age: number; // seconds
}

private chatBubbles: ChatBubble[] = [];
private static CHAT_DURATION = 5.0; // seconds

addChatBubble(addressHash: string, message: string): void {
  const text = new Text({
    text: message,
    style: {
      fontSize: 12,
      fill: 0xFFFFFF,
      backgroundColor: 0x000000,
      padding: 4,
      wordWrap: true,
      wordWrapWidth: 200,
    },
  });
  text.anchor.set(0.5, 1);
  this.worldContainer.addChild(text);
  this.chatBubbles.push({ text, targetHash: addressHash, age: 0 });
}

updateChatBubbles(dt: number, remotePlayers: RemotePlayerFrame[]): void {
  this.chatBubbles = this.chatBubbles.filter(bubble => {
    bubble.age += dt;
    if (bubble.age >= GameRenderer.CHAT_DURATION) {
      this.worldContainer.removeChild(bubble.text);
      bubble.text.destroy();
      return false;
    }
    // Position above the player's avatar
    const player = remotePlayers.find(p => p.addressHash === bubble.targetHash);
    if (player && this.street) {
      bubble.text.x = player.x - this.street.left;
      bubble.text.y = player.y - this.street.top - 75;
    }
    // Fade out in last second
    bubble.text.alpha = Math.min(1, (GameRenderer.CHAT_DURATION - bubble.age));
    return true;
  });
}
```

Call `updateChatBubbles(1/60, frame.remotePlayers)` in `updateFrame`.

**Step 3: Wire chat events through GameCanvas**

The backend emits chat events via Tauri IPC. Listen for a `chat_message` event in GameCanvas and call `renderer.addChatBubble()`.

**Step 4: Integrate ChatInput into App.svelte**

Add `<ChatInput onFocusChange={handleChatFocus} />` to the game view. When chat is focused, stop sending keyboard input to the game (set a flag that prevents `sendInput` calls).

**Step 5: Verify frontend builds**

Run: `npm run build`
Expected: Builds.

**Step 6: Commit**

```bash
git add src/lib/components/ChatInput.svelte src/lib/engine/renderer.ts src/lib/components/GameCanvas.svelte src/App.svelte
git commit -m "feat: chat input and floating text bubbles"
```

---

### Task 14: Frontend UI — Identity Setup, Network Status, Relay Config

Add the remaining UI elements: first-launch identity/name setup, network status indicator, and WAN relay configuration.

**Files:**
- Create: `src/lib/components/IdentitySetup.svelte`
- Create: `src/lib/components/NetworkStatus.svelte`
- Modify: `src/lib/components/StreetPicker.svelte` (add relay config)
- Modify: `src/App.svelte` (add identity setup flow, network status)

**Step 1: Create IdentitySetup component**

Simple modal shown on first launch (when no display name is set). Text input for display name, "Start" button.

```svelte
<script lang="ts">
  import { setDisplayName } from '../ipc';

  let { onComplete }: { onComplete: () => void } = $props();
  let name = $state('');

  async function handleSubmit() {
    const trimmed = name.trim();
    if (trimmed.length < 1) return;
    await setDisplayName(trimmed);
    onComplete();
  }
</script>

<div class="identity-setup" role="dialog" aria-label="Choose your display name">
  <h2>Welcome to Ur</h2>
  <p>Choose a name for your Glitchen:</p>
  <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
    <input
      bind:value={name}
      placeholder="Enter display name"
      maxlength="30"
      autofocus
    />
    <button type="submit" disabled={name.trim().length < 1}>
      Enter the World
    </button>
  </form>
</div>
```

**Step 2: Create NetworkStatus component**

Small overlay showing peer count. Positioned in bottom-right corner.

```svelte
<script lang="ts">
  import { getNetworkStatus } from '../ipc';

  let peerCount = $state(0);

  // Poll network status every 2 seconds
  const interval = setInterval(async () => {
    try {
      const status = await getNetworkStatus();
      peerCount = status.peerCount;
    } catch { /* ignore */ }
  }, 2000);

  import { onDestroy } from 'svelte';
  onDestroy(() => clearInterval(interval));
</script>

<div class="network-status" aria-live="polite">
  {peerCount} peer{peerCount === 1 ? '' : 's'} connected
</div>

<style>
  .network-status {
    position: fixed;
    bottom: 8px;
    right: 8px;
    padding: 4px 8px;
    font-size: 12px;
    color: rgba(255, 255, 255, 0.7);
    background: rgba(0, 0, 0, 0.3);
    border-radius: 4px;
    z-index: 50;
    pointer-events: none;
  }
</style>
```

**Step 3: Add relay config to StreetPicker**

Add a collapsible "Network Settings" section to `StreetPicker.svelte` with a single text input for relay address.

**Step 4: Wire up in App.svelte**

Add identity setup flow: check if identity exists on mount, show IdentitySetup if needed. Add NetworkStatus to the game view.

**Step 5: Verify frontend builds**

Run: `npm run build`
Expected: Builds.

**Step 6: Commit**

```bash
git add src/lib/components/ src/App.svelte
git commit -m "feat: identity setup, network status, and relay config UI"
```

---

### Task 15: End-to-End Verification

Manual testing with two instances. Verify discovery, avatar rendering, chat, and street transitions all work together.

**Step 1: Build the app**

Run: `npm run tauri build`
Expected: Builds successfully.

**Step 2: Test LAN discovery**

1. Launch two instances on the same machine (or two machines on the same LAN).
2. Both select `demo_meadow`.
3. Verify: each player sees the other's avatar (blue rectangle with name label).
4. Walk around — remote avatar should track at 60Hz.

**Step 3: Test chat**

1. Press Enter to open chat input.
2. Type a message and send.
3. Verify: text bubble appears above your avatar on the other player's screen.
4. Bubble fades after ~5 seconds.

**Step 4: Test street transitions**

1. Walk toward the right edge of `demo_meadow`.
2. Verify: pre-subscription activates near the signpost.
3. Cross the edge — swoop animation plays.
4. Arrive on `demo_heights`.
5. Other player on `demo_meadow` should see you disappear.
6. Walk back to `demo_meadow` via the left edge of `demo_heights`.

**Step 5: Test WAN relay (optional)**

1. Run a `harmony-node` in transport mode on a reachable host.
2. Configure relay address in both clients.
3. Verify: peers on different networks can discover each other.

**Step 6: Run all automated tests**

Run: `cd src-tauri && cargo test`
Run: `cd src-tauri && cargo clippy`
Run: `npx vitest run`
Run: `npm run build`
Expected: All pass with zero warnings.

**Step 7: Final commit**

Fix any issues found during manual testing. Commit and push.

```bash
git commit -m "fix: address issues found during end-to-end testing"
```
