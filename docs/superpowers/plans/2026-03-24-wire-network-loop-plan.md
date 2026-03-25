# Wire Network Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the peer connection pipeline so two harmony-glitch instances on a LAN discover each other via Reticulum announces, establish encrypted Links, activate Zenoh Sessions with PubSubRouters, and sync player state at 60Hz.

**Architecture:** All protocol logic lives in `NetworkState` (sans-I/O state machine). The game loop drives it via `tick()` — raw packets in, `NetworkAction`s out. Link handshake (3-step ECDH) → Session handshake (Ed25519 proof exchange) → PubSubRouter (topic-based pub/sub). Lower address hash initiates.

**Tech Stack:** Rust (Tauri v2), harmony-identity, harmony-reticulum (Link, Node, Packet), harmony-zenoh (Session, PubSubRouter), serde_json for wire format.

**Spec:** `docs/superpowers/specs/2026-03-24-wire-network-loop-design.md`

**Test command:** `cd src-tauri && cargo test -p harmony-glitch`
**Lint command:** `cd src-tauri && cargo clippy`

---

## File Structure

| File | Responsibility | Change |
|------|---------------|--------|
| `src-tauri/src/network/registry.rs` | Remote player tracking + stale purge | Modify: `purge_stale()` returns purged address hashes |
| `src-tauri/src/network/state.rs` | Central network state machine | Modify: implement `handle_local_delivery()`, `publish_to_all_peers()`, `activate_peer()`, extend `handle_announce_received()` and `tick_peer_session()` |

No new files. No frontend changes.

---

### Task 1: Registry returns purged address hashes

Extend `purge_stale()` to return the address hashes it removed, so `NetworkState` can clean up corresponding `PeerState` entries.

**Files:**
- Modify: `src-tauri/src/network/registry.rs:99-102`
- Test: `src-tauri/src/network/registry.rs` (existing test module)

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/network/registry.rs` in the `mod tests` block:

```rust
#[test]
fn purge_stale_returns_removed_hashes() {
    let mut reg = RemotePlayerRegistry::new();
    reg.handle_presence(
        &PresenceEvent::Joined {
            address_hash: make_hash(1),
            display_name: "Alice".into(),
        },
        1.0,
    );
    reg.handle_presence(
        &PresenceEvent::Joined {
            address_hash: make_hash(2),
            display_name: "Bob".into(),
        },
        5.0,
    );

    // At t=12, Alice (joined at 1.0) is stale, Bob (joined at 5.0) is not.
    let purged = reg.purge_stale(12.0);
    assert_eq!(purged.len(), 1);
    assert_eq!(purged[0], make_hash(1));
    assert_eq!(reg.count(), 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch purge_stale_returns_removed_hashes -- --nocapture`
Expected: FAIL — `purge_stale` returns `()`, not `Vec<[u8; 16]>`

- [ ] **Step 3: Change `purge_stale` to return removed hashes**

In `src-tauri/src/network/registry.rs`, replace the `purge_stale` method:

```rust
/// Remove players whose `last_update` is more than `STALE_TIMEOUT`
/// seconds behind `now`. Returns the address hashes of removed players
/// so the caller can clean up associated peer state.
pub fn purge_stale(&mut self, now: f64) -> Vec<[u8; 16]> {
    let mut purged = Vec::new();
    self.players.retain(|hash, player| {
        if (now - player.last_update) >= STALE_TIMEOUT {
            purged.push(*hash);
            false
        } else {
            true
        }
    });
    purged
}
```

- [ ] **Step 4: Update `NetworkState::tick()` to use the return value**

In `src-tauri/src/network/state.rs`, in the `tick()` method, change:

```rust
// OLD:
self.registry.purge_stale(now_secs);
```

to:

```rust
// Purge stale players and clean up their PeerState entries.
let purged = self.registry.purge_stale(now_secs);
for addr in purged {
    self.peers.remove(&addr);
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Expected: ALL PASS (existing tests still pass, new test passes)

- [ ] **Step 6: Run clippy**

Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: No errors

- [ ] **Step 7: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/network/registry.rs src-tauri/src/network/state.rs
git commit -m "feat(network): purge_stale returns removed address hashes for PeerState cleanup"
```

---

### Task 2: Link initiation on announce receipt

When a new peer is discovered on the same street and our address hash is lower, initiate a Reticulum Link handshake. This is the first half of the connection lifecycle.

**Files:**
- Modify: `src-tauri/src/network/state.rs:440-518` (`handle_announce_received`)
- Test: `src-tauri/src/network/state.rs` (test module)

**Important context:**
- `Link::initiate(rng, dest_identity, dest_name) -> Result<(Link, Packet), ReticulumError>` — creates a Link in `Pending` state and produces the link request packet.
- The request packet must be serialized via `packet.to_bytes()` and then fed back into the `Node` via `node.handle_event(NodeEvent::InboundPacket)` on the OTHER side's `NetworkState`.
- But for the SENDING side, we just need to emit the packet as `NetworkAction::SendPacket`. The Node doesn't need to route our own outbound link requests — they go directly on the wire.
- Tiebreaker: our `public_identity.address_hash < peer.address_hash` means we initiate.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/network/state.rs` in the `mod tests` block:

```rust
#[test]
fn announce_triggers_link_initiation_for_lower_hash() {
    use harmony_reticulum::ValidatedAnnounce;

    let id_a = make_identity();
    let id_b = make_identity();

    // Determine which has the lower address hash.
    let pub_a = id_a.public_identity().clone();
    let pub_b = id_b.public_identity().clone();
    let (lower_id, higher_pub) = if pub_a.address_hash < pub_b.address_hash {
        (id_a, pub_b)
    } else {
        (id_b, pub_a)
    };

    // Create NetworkState for the lower-hash side.
    let mut state = NetworkState::new(lower_id, "Lower".to_string());
    let mut rng = OsRng;
    state.change_street("meadow", 1.0, &mut rng).unwrap();

    // Simulate an announce from the higher-hash peer on the same street.
    let app_data = encode_app_data("Higher", Some("meadow"));
    let dest_name =
        DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();
    let announce = ValidatedAnnounce {
        identity: higher_pub.clone(),
        dest_name: dest_name.clone(),
        app_data,
        hops: 0,
    };

    let mut actions = Vec::new();
    state.handle_announce_received(&announce, 2, 2.0, &mut rng, &mut actions);

    // Should have recorded the peer.
    assert!(state.peers.contains_key(&higher_pub.address_hash));

    // The peer should have a Link in Pending state.
    let peer = state.peers.get(&higher_pub.address_hash).unwrap();
    assert!(peer.link.is_some());
    let link = peer.link.as_ref().unwrap();
    assert_eq!(link.state(), harmony_reticulum::LinkState::Pending);

    // Should have emitted at least one SendPacket (the link request).
    let send_count = actions
        .iter()
        .filter(|a| matches!(a, NetworkAction::SendPacket { .. }))
        .count();
    assert!(send_count > 0, "Expected link request SendPacket");
}

#[test]
fn announce_does_not_initiate_link_for_higher_hash() {
    use harmony_reticulum::ValidatedAnnounce;

    let id_a = make_identity();
    let id_b = make_identity();

    let pub_a = id_a.public_identity().clone();
    let pub_b = id_b.public_identity().clone();
    let (higher_id, lower_pub) = if pub_a.address_hash > pub_b.address_hash {
        (id_a, pub_b)
    } else {
        (id_b, pub_a)
    };

    // Create NetworkState for the higher-hash side.
    let mut state = NetworkState::new(higher_id, "Higher".to_string());
    let mut rng = OsRng;
    state.change_street("meadow", 1.0, &mut rng).unwrap();

    // Simulate announce from the lower-hash peer.
    let app_data = encode_app_data("Lower", Some("meadow"));
    let dest_name =
        DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();
    let announce = ValidatedAnnounce {
        identity: lower_pub.clone(),
        dest_name: dest_name.clone(),
        app_data,
        hops: 0,
    };

    let mut actions = Vec::new();
    state.handle_announce_received(&announce, 2, 2.0, &mut rng, &mut actions);

    // Should have recorded the peer but NOT initiated a link.
    assert!(state.peers.contains_key(&lower_pub.address_hash));
    let peer = state.peers.get(&lower_pub.address_hash).unwrap();
    assert!(peer.link.is_none(), "Higher hash should not initiate link");
}
```

Note: `handle_announce_received` and `encode_app_data` are currently private. The tests are in the same module (`mod tests` inside `state.rs`), so they have access. `ValidatedAnnounce` needs to be imported from `harmony_reticulum`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test -p harmony-glitch announce_triggers_link -- --nocapture`
Expected: FAIL — the first test fails because `peer.link` is `None` (link initiation not implemented yet)

- [ ] **Step 3: Implement link initiation in `handle_announce_received`**

In `src-tauri/src/network/state.rs`, replace the TODO comment block at the end of `handle_announce_received` (lines ~513-517) with:

```rust
// Tiebreaker: lower address hash initiates the Link.
// The higher-hash side waits for the incoming link request.
if self.public_identity.address_hash < addr {
    if let Some(ref dest_name) = self.dest_name {
        match Link::initiate(rng, &announce.identity, dest_name) {
            Ok((link, request_packet)) => {
                // Serialize the link request and emit for sending.
                if let Ok(raw) = request_packet.to_bytes() {
                    out.push(NetworkAction::SendPacket {
                        interface_name: INTERFACE_NAME.to_string(),
                        data: raw,
                    });
                }
                // Store the pending link.
                if let Some(peer) = self.peers.get_mut(&addr) {
                    peer.link = Some(link);
                }
            }
            Err(_) => {
                // Link initiation failed — peer stays in discovered state.
                // They'll be cleaned up by stale purge if no link forms.
            }
        }
    }
}
```

Also add `use harmony_reticulum::Packet;` at the top if not already imported (it should be via the existing `use harmony_reticulum::...` line — check and add `Packet` to the import list if needed).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -p harmony-glitch announce_triggers_link -- --nocapture`
Run: `cd src-tauri && cargo test -p harmony-glitch announce_does_not_initiate -- --nocapture`
Expected: BOTH PASS

- [ ] **Step 5: Run the full test suite**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Expected: ALL PASS

- [ ] **Step 6: Run clippy**

Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: No errors

- [ ] **Step 7: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/network/state.rs
git commit -m "feat(network): initiate Link on announce receipt (lower hash tiebreaker)"
```

---

### Task 3: Handle local delivery — link request and proof routing

Implement `handle_local_delivery()` to process three packet types: link requests (we're responder), link proofs (we're initiator), and link RTT (responder completes handshake). After link activation, begin Session handshake.

**Files:**
- Modify: `src-tauri/src/network/state.rs:520-532` (`handle_local_delivery`)
- Test: `src-tauri/src/network/state.rs` (test module)

**Important context:**
- The `Node` produces `NodeAction::DeliverLocally { destination_hash, packet, interface_name }`.
- For link requests: `packet.header.flags.packet_type == PacketType::LinkRequest`. Call `Link::respond(private_identity, dest_name, &packet)` → returns `(Link, proof_packet)`.
- For link proofs (context = `LinkProof` or packet to a pending link): `link.complete_handshake(rng, &packet, rtt_secs)` → returns RTT packet. After this, `link.state() == Active`.
- For link RTT: responder's link activates via `link.handle_rtt(&packet)`. Check if this method exists.
- After link becomes Active on either side: call `activate_peer()` (new helper, Task 4).
- Link data packets to active links: handled in Task 5.
- Packet classification: use `packet.header.flags.packet_type` and `packet.header.context` to distinguish request/proof/RTT/data.

**Critical API note:** We need to look up peers by `destination_hash` which equals the `link_id` for link-addressed packets. For link requests, the destination_hash is our announcing destination hash. For proofs/RTT/data, the destination_hash is the link_id.

- [ ] **Step 1: Write the failing test — full link handshake between two NetworkStates**

Add to `src-tauri/src/network/state.rs` in `mod tests`:

```rust
/// Helper: create two NetworkStates on the same street, with deterministic
/// ordering (A = lower hash, B = higher hash).
fn make_pair_on_street(street: &str) -> (NetworkState, NetworkState) {
    let mut rng = OsRng;
    let id_a = make_identity();
    let id_b = make_identity();

    let pub_a = id_a.public_identity().clone();
    let pub_b = id_b.public_identity().clone();

    let (lower_id, higher_id) = if pub_a.address_hash < pub_b.address_hash {
        (id_a, id_b)
    } else {
        (id_b, id_a)
    };

    let mut state_a = NetworkState::new(lower_id, "Alice".to_string());
    let mut state_b = NetworkState::new(higher_id, "Bob".to_string());

    state_a.change_street(street, 1.0, &mut rng).unwrap();
    state_b.change_street(street, 1.0, &mut rng).unwrap();

    (state_a, state_b)
}

/// Helper: extract raw packet bytes from SendPacket actions.
fn extract_packets(actions: &[NetworkAction]) -> Vec<Vec<u8>> {
    actions
        .iter()
        .filter_map(|a| match a {
            NetworkAction::SendPacket { data, .. } => Some(data.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn full_link_handshake_between_two_states() {
    use harmony_reticulum::ValidatedAnnounce;

    let (mut state_a, mut state_b) = make_pair_on_street("meadow");
    let mut rng = OsRng;

    let pub_a = state_a.public_identity.clone();
    let pub_b = state_b.public_identity.clone();

    // Step 1: B's announce arrives at A. A initiates link (A has lower hash).
    let app_data_b = encode_app_data("Bob", Some("meadow"));
    let dest_name = DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();
    let announce_b = ValidatedAnnounce {
        identity: pub_b.clone(),
        dest_name: dest_name.clone(),
        app_data: app_data_b,
        hops: 0,
    };
    let mut actions_a = Vec::new();
    state_a.handle_announce_received(&announce_b, 2, 2.0, &mut rng, &mut actions_a);

    // A should have a pending link and emitted a link request packet.
    let peer_a = state_a.peers.get(&pub_b.address_hash).unwrap();
    assert!(peer_a.link.is_some());
    let packets_from_a = extract_packets(&actions_a);
    assert!(!packets_from_a.is_empty(), "A should emit link request packet");

    // Step 2: Feed A's link request to B (also need B to know about A first).
    let app_data_a = encode_app_data("Alice", Some("meadow"));
    let announce_a = ValidatedAnnounce {
        identity: pub_a.clone(),
        dest_name: dest_name.clone(),
        app_data: app_data_a,
        hops: 0,
    };
    let mut actions_b_announce = Vec::new();
    state_b.handle_announce_received(&announce_a, 2, 2.0, &mut rng, &mut actions_b_announce);
    // B should NOT have initiated a link (higher hash).
    let peer_b_before = state_b.peers.get(&pub_a.address_hash).unwrap();
    assert!(peer_b_before.link.is_none());

    // Now feed A's link request packet to B via tick.
    let inbound_for_b: Vec<(String, Vec<u8>)> = packets_from_a
        .iter()
        .map(|p| ("udp0".to_string(), p.clone()))
        .collect();
    let actions_b = state_b.tick(&inbound_for_b, 3.0, &mut rng);

    // B should have responded — should have a link now and emitted a proof packet.
    let peer_b = state_b.peers.get(&pub_a.address_hash).unwrap();
    assert!(peer_b.link.is_some(), "B should have created a responder link");
    let packets_from_b = extract_packets(&actions_b);
    assert!(!packets_from_b.is_empty(), "B should emit link proof packet");

    // Step 3: Feed B's proof to A.
    let inbound_for_a: Vec<(String, Vec<u8>)> = packets_from_b
        .iter()
        .map(|p| ("udp0".to_string(), p.clone()))
        .collect();
    let actions_a2 = state_a.tick(&inbound_for_a, 4.0, &mut rng);

    // A's link should now be Active (proof verified, RTT sent).
    let peer_a2 = state_a.peers.get(&pub_b.address_hash).unwrap();
    assert!(peer_a2.link.is_some());
    let link_a = peer_a2.link.as_ref().unwrap();
    assert_eq!(
        link_a.state(),
        harmony_reticulum::LinkState::Active,
        "A's link should be Active after receiving proof"
    );

    // A should have emitted RTT packet.
    let packets_from_a2 = extract_packets(&actions_a2);
    assert!(!packets_from_a2.is_empty(), "A should emit RTT packet");

    // Step 4: Feed A's RTT to B.
    let inbound_for_b2: Vec<(String, Vec<u8>)> = packets_from_a2
        .iter()
        .map(|p| ("udp0".to_string(), p.clone()))
        .collect();
    let _actions_b2 = state_b.tick(&inbound_for_b2, 5.0, &mut rng);

    // B's link should now be Active.
    let peer_b2 = state_b.peers.get(&pub_a.address_hash).unwrap();
    assert!(peer_b2.link.is_some());
    let link_b = peer_b2.link.as_ref().unwrap();
    assert_eq!(
        link_b.state(),
        harmony_reticulum::LinkState::Active,
        "B's link should be Active after receiving RTT"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch full_link_handshake -- --nocapture`
Expected: FAIL — `handle_local_delivery` is a no-op, so B never processes the link request

- [ ] **Step 3: Implement `handle_local_delivery`**

Replace the body of `handle_local_delivery` in `src-tauri/src/network/state.rs`:

```rust
fn handle_local_delivery(
    &mut self,
    packet: &harmony_reticulum::Packet,
    now_secs: u64,
    rng: &mut impl CryptoRngCore,
    out: &mut Vec<NetworkAction>,
) {
    use harmony_reticulum::{LinkState, PacketContext, PacketType};

    let dest_hash = packet.header.destination_hash;

    match packet.header.flags.packet_type {
        PacketType::Proof => {
            // We're the initiator — this is the link proof from the responder.
            // Find our pending link whose link_id matches the destination_hash.
            let peer_addr = self
                .peers
                .iter()
                .find_map(|(addr, peer)| {
                    peer.link
                        .as_ref()
                        .filter(|l| *l.link_id() == dest_hash && l.state() == LinkState::Pending)
                        .map(|_| *addr)
                });

            if let Some(addr) = peer_addr {
                let peer = match self.peers.get_mut(&addr) {
                    Some(p) => p,
                    None => return,
                };
                let link = match peer.link.as_mut() {
                    Some(l) => l,
                    None => return,
                };

                let now_f64 = now_secs as f64;
                match link.complete_handshake(rng, packet, now_f64) {
                    Ok(rtt_packet) => {
                        if let Ok(raw) = rtt_packet.to_bytes() {
                            out.push(NetworkAction::SendPacket {
                                interface_name: INTERFACE_NAME.to_string(),
                                data: raw,
                            });
                        }
                        // Link is now Active — begin Session handshake.
                        let now_ms = (now_secs as f64 * 1000.0) as u64;
                        self.activate_peer_session(&addr, now_ms, rng, out);
                    }
                    Err(_) => {
                        peer.link = None;
                    }
                }
            }
        }

        PacketType::LinkRequest => {
            // We're the responder. The destination_hash is our announcing
            // destination hash. Create a responding Link.
            let identity = match PrivateIdentity::from_private_bytes(
                self.identity_bytes.as_ref(),
            ) {
                Ok(id) => id,
                Err(_) => return,
            };
            let dest_name = match &self.dest_name {
                Some(dn) => dn.clone(),
                None => return,
            };

            match Link::respond(&identity, &dest_name, packet) {
                Ok((link, proof_packet)) => {
                    // Serialize and send the proof.
                    if let Ok(raw) = proof_packet.to_bytes() {
                        out.push(NetworkAction::SendPacket {
                            interface_name: INTERFACE_NAME.to_string(),
                            data: raw,
                        });
                    }

                    // Find the peer by scanning for the link's remote identity.
                    // The link request comes from a peer we may already know
                    // (via announce) or may not — find by identity match.
                    let link_remote = link.remote_identity().map(|i| i.address_hash);
                    let peer_addr = link_remote.and_then(|addr| {
                        if self.peers.contains_key(&addr) {
                            Some(addr)
                        } else {
                            None
                        }
                    });

                    if let Some(addr) = peer_addr {
                        if let Some(peer) = self.peers.get_mut(&addr) {
                            peer.link = Some(link);
                        }
                    }
                    // If we don't know this peer yet (no announce received),
                    // we drop the link. They'll re-announce and we'll
                    // discover them properly.
                }
                Err(_) => {}
            }
        }

        PacketType::Data => {
            // Data packets addressed to a link_id. Could be:
            // - Link RTT (context = Lrrtt, for our Handshake link as responder)
            // - Session/PubSub data (for active links)
            // Find the peer whose link matches this destination_hash (link_id).
            let peer_addr = self
                .peers
                .iter()
                .find_map(|(addr, peer)| {
                    peer.link
                        .as_ref()
                        .filter(|l| *l.link_id() == dest_hash)
                        .map(|_| *addr)
                });

            if let Some(addr) = peer_addr {
                let peer = match self.peers.get_mut(&addr) {
                    Some(p) => p,
                    None => return,
                };

                let link = match peer.link.as_mut() {
                    Some(l) => l,
                    None => return,
                };

                match link.state() {
                    LinkState::Handshake => {
                        // We're the responder — this is the RTT from the initiator.
                        // Use link.activate() which decrypts internally and
                        // transitions from Handshake → Active.
                        match link.activate(packet) {
                            Ok(_rtt_secs) => {
                                // Link is now Active — begin Session handshake.
                                let now_ms = (now_secs as f64 * 1000.0) as u64;
                                self.activate_peer_session(&addr, now_ms, rng, out);
                            }
                            Err(_) => {
                                // Bad RTT — drop the link.
                                peer.link = None;
                            }
                        }
                    }

                    LinkState::Active => {
                        // Session/PubSub data — handled in Task 5.
                    }

                    _ => {}
                }
            }
        }

        _ => {
            // Other packet types (Announce, Proof) are handled by the Node
            // before DeliverLocally. Ignore here.
        }
    }
}
```

**Note:** The responder uses `link.activate(rtt_packet)` to process the RTT — this decrypts internally, extracts the RTT value, and transitions from `Handshake` → `Active`. The initiator uses `link.complete_handshake(rng, proof_packet, rtt_secs)` which processes the proof and transitions from `Pending` → `Active`. Proof packets use `PacketType::Proof` (not `Data`), so they're handled in their own match arm.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -p harmony-glitch full_link_handshake -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run the full test suite and clippy**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: ALL PASS, no clippy errors

- [ ] **Step 6: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/network/state.rs
git commit -m "feat(network): implement handle_local_delivery for link handshake routing"
```

---

### Task 4: Session activation after Link becomes Active

When a Link transitions to `Active`, create a Zenoh `Session` and begin the Session handshake (exchange Ed25519 proofs). After both sides verify, set up the `PubSubRouter`.

**Files:**
- Modify: `src-tauri/src/network/state.rs` (add `activate_peer()` helper, extend `handle_local_delivery`)
- Test: `src-tauri/src/network/state.rs` (test module)

**Important context:**
- `Session::new(local_identity: PrivateIdentity, remote_identity: Identity, config: SessionConfig, now_ms: u64) -> (Session, Vec<SessionAction>)` — consumes `PrivateIdentity` by value. Must reconstruct from `self.identity_bytes`.
- Returns `SendHandshake { proof }` action. This proof must be encrypted via `link.encrypt(rng, &proof)` and sent as a link data packet.
- On receiving peer's handshake: `session.handle_event(SessionEvent::HandshakeReceived { proof })` → `SessionAction::SessionOpened`.
- After `SessionOpened`: create `PubSubRouter::new()`, declare publishers (state + chat topics), subscribe to peer's topics.
- Topic format: `harmony/glitch/street/{street}/player/{addr_hex}/state` etc.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/network/state.rs` in `mod tests`:

```rust
#[test]
fn session_activates_after_link_handshake() {
    use harmony_reticulum::ValidatedAnnounce;

    let (mut state_a, mut state_b) = make_pair_on_street("meadow");
    let mut rng = OsRng;

    let pub_a = state_a.public_identity.clone();
    let pub_b = state_b.public_identity.clone();

    // Exchange announces.
    let dest_name = DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();
    let announce_b = ValidatedAnnounce {
        identity: pub_b.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Bob", Some("meadow")),
        hops: 0,
    };
    let announce_a = ValidatedAnnounce {
        identity: pub_a.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Alice", Some("meadow")),
        hops: 0,
    };

    // A discovers B → initiates link.
    let mut actions_a = Vec::new();
    state_a.handle_announce_received(&announce_b, 2, 2.0, &mut rng, &mut actions_a);
    // B discovers A (no link initiation).
    let mut actions_b_ann = Vec::new();
    state_b.handle_announce_received(&announce_a, 2, 2.0, &mut rng, &mut actions_b_ann);

    // Shuttle packets until handshake completes.
    // Round 1: A→B (link request)
    let pkts = extract_packets(&actions_a);
    let inbound_b: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
    let actions_b = state_b.tick(&inbound_b, 3.0, &mut rng);

    // Round 2: B→A (link proof + session handshake)
    let pkts = extract_packets(&actions_b);
    let inbound_a: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
    let actions_a2 = state_a.tick(&inbound_a, 4.0, &mut rng);

    // Round 3: A→B (RTT + session handshake)
    let pkts = extract_packets(&actions_a2);
    let inbound_b2: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
    let actions_b2 = state_b.tick(&inbound_b2, 5.0, &mut rng);

    // Round 4: B→A (session handshake response, if any)
    let pkts = extract_packets(&actions_b2);
    if !pkts.is_empty() {
        let inbound_a3: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
        let _actions_a3 = state_a.tick(&inbound_a3, 6.0, &mut rng);
    }

    // Both sides should have active sessions.
    let peer_a = state_a.peers.get(&pub_b.address_hash).unwrap();
    assert!(
        peer_a.session.is_some(),
        "A should have a Session after handshake"
    );
    if let Some(ref session) = peer_a.session {
        assert_eq!(
            session.state(),
            SessionState::Active,
            "A's session should be Active"
        );
    }

    let peer_b = state_b.peers.get(&pub_a.address_hash).unwrap();
    assert!(
        peer_b.session.is_some(),
        "B should have a Session after handshake"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch session_activates_after -- --nocapture`
Expected: FAIL — no session activation code exists

- [ ] **Step 3: Implement `activate_peer()` and integrate with `handle_local_delivery`**

Add the `activate_peer` helper and a `send_via_link` helper to `NetworkState`:

```rust
/// Build a link data packet (encrypted payload, addressed to link_id).
fn send_via_link(
    link: &Link,
    rng: &mut impl CryptoRngCore,
    plaintext: &[u8],
    context: harmony_reticulum::PacketContext,
    out: &mut Vec<NetworkAction>,
) {
    use harmony_reticulum::packet::*;

    let encrypted = match link.encrypt(rng, plaintext) {
        Ok(e) => e,
        Err(_) => return,
    };

    let packet = Packet {
        header: PacketHeader {
            flags: PacketFlags {
                ifac: false,
                header_type: HeaderType::Type1,
                context_flag: true,
                propagation: PropagationType::Broadcast,
                destination_type: DestinationType::Link,
                packet_type: PacketType::Data,
            },
            hops: 0,
            transport_id: None,
            destination_hash: *link.link_id(),
            context,
        },
        data: encrypted.into(),
    };

    if let Ok(raw) = packet.to_bytes() {
        out.push(NetworkAction::SendPacket {
            interface_name: INTERFACE_NAME.to_string(),
            data: raw,
        });
    }
}

/// Activate a peer's Session after their Link becomes Active.
/// Creates a Zenoh Session, sends the handshake proof through the Link.
fn activate_peer_session(
    &mut self,
    addr: &[u8; 16],
    now_ms: u64,
    rng: &mut impl CryptoRngCore,
    out: &mut Vec<NetworkAction>,
) {
    let identity = match PrivateIdentity::from_private_bytes(self.identity_bytes.as_ref()) {
        Ok(id) => id,
        Err(_) => return,
    };

    let peer = match self.peers.get_mut(addr) {
        Some(p) => p,
        None => return,
    };

    let link = match peer.link.as_ref() {
        Some(l) => l,
        None => return,
    };

    let remote_identity = peer.identity.clone();
    let (session, session_actions) = Session::new(
        identity,
        remote_identity,
        harmony_zenoh::SessionConfig::default(),
        now_ms,
    );

    // Send the handshake proof through the link.
    for action in &session_actions {
        if let SessionAction::SendHandshake { proof } = action {
            Self::send_via_link(
                link,
                rng,
                proof,
                harmony_reticulum::PacketContext::None,
                out,
            );
        }
    }

    peer.session = Some(session);
}
```

Then update `handle_local_delivery` to call `activate_peer_session` when a link becomes Active (after completing handshake on either side).

In the `LinkState::Pending` arm (initiator receives proof), after the `complete_handshake` succeeds, add:

```rust
// Link is now Active — begin Session handshake.
let now_ms = (now_secs as f64 * 1000.0) as u64;
self.activate_peer_session(&addr, now_ms, rng, out);
```

In the `LinkState::Handshake` arm (responder receives RTT), after successful decrypt, add the same call.

Also, in the `LinkState::Active` arm, add Session data handling:

```rust
LinkState::Active => {
    // Decrypt link data and feed to Session.
    let plaintext = match link.decrypt(&packet.data) {
        Ok(p) => p,
        Err(_) => return,
    };

    if let Some(ref mut session) = peer.session {
        if session.state() == SessionState::Init {
            // Session waiting for peer's handshake proof.
            match session.handle_event(SessionEvent::HandshakeReceived {
                proof: plaintext,
            }) {
                Ok(actions) => {
                    for action in actions {
                        if let SessionAction::SessionOpened = action {
                            // Session is now Active — set up PubSubRouter in Task 5.
                        }
                    }
                }
                Err(_) => {
                    // Handshake failed — close session.
                    peer.session = None;
                }
            }
        } else {
            // Active session — PubSub data routing in Task 5.
        }
    } else {
        // No session yet — might be the peer's session handshake
        // arriving before we created our session. This can happen if
        // the responder activates their link + session before processing
        // the RTT. The data is lost, but the peer will re-send.
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test -p harmony-glitch session_activates_after -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full test suite and clippy**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/network/state.rs
git commit -m "feat(network): activate Zenoh Session after Link handshake completes"
```

---

### Task 5: PubSubRouter setup and publish_to_all_peers

After a Session opens, create a `PubSubRouter`, declare publishers for our topics, subscribe to the peer's topics. Implement `publish_to_all_peers()` to route messages through the router → session → link → node pipeline.

**Files:**
- Modify: `src-tauri/src/network/state.rs` (extend `activate_peer_session`, implement `publish_to_all_peers`, extend `handle_local_delivery` for PubSub data)
- Test: `src-tauri/src/network/state.rs` (test module)

**Important context:**
- `PubSubRouter::new()` creates an empty router.
- `router.subscribe(key_expr, &mut session) -> Result<(SubscriptionId, Vec<PubSubAction>), ZenohError>` — subscribe to a key expression. Returns actions including `SendSubscriberDeclare` which needs to be sent to the peer.
- `router.declare_publisher(key_expr, &mut session) -> Result<(PublisherId, Vec<PubSubAction>), ZenohError>` — declare a publisher. Returns actions including `Session` actions for resource declaration.
- `router.publish(pub_id, payload, &session) -> Result<Vec<PubSubAction>, ZenohError>` — publish a message. Returns `PubSubAction::SendMessage { expr_id, payload }`.
- `PubSubAction::Session(SessionAction)` — some pub/sub actions wrap session-level actions that also need to be sent.
- Topic format: `harmony/glitch/street/{street}/player/{addr_hex}/state` and `/chat`.
- `PubSubAction::SendMessage { expr_id, payload }` needs to be framed (prepend expr_id as 2-byte big-endian + payload), encrypted via link, and sent as a link data packet.
- `PubSubAction::Deliver { subscription_id, key_expr, payload }` is the inbound delivery for local subscribers.

- [ ] **Step 1: Write the failing test**

Add to `src-tauri/src/network/state.rs` in `mod tests`:

```rust
#[test]
fn publish_player_state_round_trip() {
    use harmony_reticulum::ValidatedAnnounce;

    let (mut state_a, mut state_b) = make_pair_on_street("meadow");
    let mut rng = OsRng;

    let pub_a = state_a.public_identity.clone();
    let pub_b = state_b.public_identity.clone();
    let dest_name = DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();

    // Exchange announces.
    let announce_b = ValidatedAnnounce {
        identity: pub_b.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Bob", Some("meadow")),
        hops: 0,
    };
    let announce_a = ValidatedAnnounce {
        identity: pub_a.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Alice", Some("meadow")),
        hops: 0,
    };
    let mut out = Vec::new();
    state_a.handle_announce_received(&announce_b, 2, 2.0, &mut rng, &mut out);
    let mut out_b = Vec::new();
    state_b.handle_announce_received(&announce_a, 2, 2.0, &mut rng, &mut out_b);

    // Shuttle packets until both have active sessions with routers.
    // This may take several rounds due to Link + Session handshakes.
    for round in 0..10 {
        let t = 3.0 + round as f64;
        let pkts_a = extract_packets(&out);
        if !pkts_a.is_empty() {
            let inbound: Vec<_> = pkts_a.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
            out = Vec::new();
            let actions = state_b.tick(&inbound, t, &mut rng);
            out = actions;
        } else {
            out = Vec::new();
        }

        let pkts_b = extract_packets(&out);
        if !pkts_b.is_empty() {
            let inbound: Vec<_> = pkts_b.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
            out = Vec::new();
            let actions = state_a.tick(&inbound, t + 0.5, &mut rng);
            out = actions;
        } else {
            // Check if both are ready.
            let a_ready = state_a
                .peers
                .get(&pub_b.address_hash)
                .and_then(|p| p.router.as_ref())
                .is_some();
            let b_ready = state_b
                .peers
                .get(&pub_a.address_hash)
                .and_then(|p| p.router.as_ref())
                .is_some();
            if a_ready && b_ready {
                break;
            }
        }
    }

    // Verify both have routers.
    let peer_a = state_a.peers.get(&pub_b.address_hash).unwrap();
    assert!(peer_a.router.is_some(), "A should have a PubSubRouter");
    assert!(!peer_a.publisher_ids.is_empty(), "A should have publisher IDs");

    // Now A publishes player state.
    let net_state = PlayerNetState {
        x: 100.0,
        y: -50.0,
        vx: 10.0,
        vy: 0.0,
        facing: 1,
        on_ground: true,
    };
    let publish_actions = state_a.publish_player_state(&net_state);

    // Feed A's published packets to B.
    let pkts = extract_packets(&publish_actions);
    assert!(!pkts.is_empty(), "publish_player_state should produce packets");

    let inbound: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
    let _actions = state_b.tick(&inbound, 20.0, &mut rng);

    // B's registry should now have A's position.
    let frames = state_b.remote_frames();
    assert!(!frames.is_empty(), "B should see A as a remote player");
    let frame = &frames[0];
    assert!((frame.x - 100.0).abs() < 1.0, "B should see A's x position");
    assert!((frame.y - (-50.0)).abs() < 1.0, "B should see A's y position");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch publish_player_state_round_trip -- --nocapture`
Expected: FAIL — `publish_to_all_peers()` still returns empty Vec

- [ ] **Step 3: Implement PubSubRouter setup in `activate_peer_session`**

After `SessionAction::SessionOpened` is received (in `handle_local_delivery`'s Active arm), set up the PubSubRouter:

```rust
// After SessionOpened:
fn setup_pubsub_router(
    &mut self,
    addr: &[u8; 16],
    rng: &mut impl CryptoRngCore,
    out: &mut Vec<NetworkAction>,
) {
    let our_addr_hex = hex::encode(self.public_identity.address_hash);
    let peer_addr_hex = hex::encode(addr);
    let street = match &self.current_street {
        Some(s) => s.clone(),
        None => return,
    };

    let peer = match self.peers.get_mut(addr) {
        Some(p) => p,
        None => return,
    };

    let session = match peer.session.as_mut() {
        Some(s) => s,
        None => return,
    };

    let link = match peer.link.as_ref() {
        Some(l) => l,
        None => return,
    };

    let mut router = PubSubRouter::new();
    let mut publisher_ids = Vec::new();
    let mut subscription_ids = Vec::new();

    // Declare publishers for our state and chat topics.
    let our_state_topic = format!(
        "harmony/glitch/street/{}/player/{}/state",
        street, our_addr_hex
    );
    let our_chat_topic = format!(
        "harmony/glitch/street/{}/player/{}/chat",
        street, our_addr_hex
    );

    // declare_publisher takes (String, &mut Session).
    if let Ok((pub_id, actions)) = router.declare_publisher(our_state_topic, session) {
        publisher_ids.push(pub_id);
        Self::process_pubsub_actions(&actions, link, rng, session, out);
    }
    if let Ok((pub_id, actions)) = router.declare_publisher(our_chat_topic, session) {
        publisher_ids.push(pub_id);
        Self::process_pubsub_actions(&actions, link, rng, session, out);
    }

    // Subscribe to peer's state and chat topics.
    let peer_state_topic = format!(
        "harmony/glitch/street/{}/player/{}/state",
        street, peer_addr_hex
    );
    let peer_chat_topic = format!(
        "harmony/glitch/street/{}/player/{}/chat",
        street, peer_addr_hex
    );

    // subscribe takes (&mut self, &str) — no session parameter.
    if let Ok((sub_id, actions)) = router.subscribe(&peer_state_topic) {
        subscription_ids.push(sub_id);
        Self::process_pubsub_actions(&actions, link, rng, session, out);
    }
    if let Ok((sub_id, actions)) = router.subscribe(&peer_chat_topic) {
        subscription_ids.push(sub_id);
        Self::process_pubsub_actions(&actions, link, rng, session, out);
    }

    peer.router = Some(router);
    peer.publisher_ids = publisher_ids;
    peer.subscription_ids = subscription_ids;
}

/// Process PubSubActions: send SessionActions through the link,
/// handle SendMessage, etc.
fn process_pubsub_actions(
    actions: &[harmony_zenoh::PubSubAction],
    link: &Link,
    rng: &mut impl CryptoRngCore,
    _session: &Session,
    out: &mut Vec<NetworkAction>,
) {
    use harmony_zenoh::PubSubAction;

    for action in actions {
        match action {
            PubSubAction::Session(session_action) => {
                // Session-level actions (resource declare/undeclare) need to
                // be serialized and sent through the link.
                Self::send_session_action(link, rng, session_action, out);
            }
            PubSubAction::SendMessage { expr_id, payload } => {
                // Frame: [expr_id: 2 bytes BE][payload]
                let mut frame = Vec::with_capacity(2 + payload.len());
                frame.extend_from_slice(&(*expr_id as u16).to_be_bytes());
                frame.extend_from_slice(payload);
                Self::send_via_link(
                    link,
                    rng,
                    &frame,
                    harmony_reticulum::PacketContext::None,
                    out,
                );
            }
            PubSubAction::SendSubscriberDeclare { key_expr } => {
                // Encode as a simple tagged message through the link.
                let msg = format!("SUB:{}", key_expr);
                Self::send_via_link(
                    link,
                    rng,
                    msg.as_bytes(),
                    harmony_reticulum::PacketContext::None,
                    out,
                );
            }
            PubSubAction::SendSubscriberUndeclare { key_expr } => {
                let msg = format!("UNSUB:{}", key_expr);
                Self::send_via_link(
                    link,
                    rng,
                    msg.as_bytes(),
                    harmony_reticulum::PacketContext::None,
                    out,
                );
            }
            PubSubAction::Deliver { .. } => {
                // Local delivery — handled separately.
            }
        }
    }
}

fn send_session_action(
    link: &Link,
    rng: &mut impl CryptoRngCore,
    action: &SessionAction,
    out: &mut Vec<NetworkAction>,
) {
    // Serialize session actions as tagged messages.
    let data = match action {
        SessionAction::SendResourceDeclare { expr_id, key_expr } => {
            format!("RESDECL:{}:{}", expr_id, key_expr).into_bytes()
        }
        SessionAction::SendResourceUndeclare { expr_id } => {
            format!("RESUNDECL:{}", expr_id).into_bytes()
        }
        SessionAction::SendKeepalive => b"KEEPALIVE".to_vec(),
        SessionAction::SendClose => b"CLOSE".to_vec(),
        SessionAction::SendCloseAck => b"CLOSEACK".to_vec(),
        SessionAction::SendHandshake { proof } => {
            let mut data = b"HANDSHAKE:".to_vec();
            data.extend_from_slice(proof);
            data
        }
        _ => return, // Non-send actions.
    };
    Self::send_via_link(link, rng, &data, harmony_reticulum::PacketContext::None, out);
}
```

- [ ] **Step 4: Implement `publish_to_all_peers`**

Replace the stub in `src-tauri/src/network/state.rs`:

```rust
fn publish_to_all_peers(&mut self, payload: &[u8]) -> Vec<NetworkAction> {
    let mut out = Vec::new();
    let mut rng = rand::rngs::OsRng;

    let peer_addrs: Vec<[u8; 16]> = self.peers.keys().copied().collect();
    for addr in peer_addrs {
        let peer = match self.peers.get(&addr) {
            Some(p) => p,
            None => continue,
        };

        let (router, session, link) = match (&peer.router, &peer.session, &peer.link) {
            (Some(r), Some(s), Some(l)) => (r, s, l),
            _ => continue,
        };

        if session.state() != SessionState::Active {
            continue;
        }

        // Use the first publisher_id (state topic).
        // For chat, we'd use the second publisher_id.
        let pub_id = match peer.publisher_ids.first() {
            Some(id) => *id,
            None => continue,
        };

        match router.publish(pub_id, payload.to_vec(), session) {
            Ok(actions) => {
                for action in &actions {
                    if let harmony_zenoh::PubSubAction::SendMessage { expr_id, payload } = action {
                        // Frame: [expr_id: 2 bytes BE][payload]
                        let mut frame = Vec::with_capacity(2 + payload.len());
                        frame.extend_from_slice(&(*expr_id as u16).to_be_bytes());
                        frame.extend_from_slice(payload);
                        Self::send_via_link(
                            link,
                            &mut rng,
                            &frame,
                            harmony_reticulum::PacketContext::None,
                            &mut out,
                        );
                    }
                }
            }
            Err(_) => continue,
        }
    }
    out
}
```

**Note:** `publish_to_all_peers` currently takes `&mut self` but `router.publish()` takes `&self`. The borrow checker may complain about borrowing `self` mutably while also borrowing peer fields. If so, restructure to collect peer data (link_id, pub_id, session ref) first, then publish. The implementer should handle this based on what the borrow checker requires.

- [ ] **Step 5: Handle inbound PubSub data in `handle_local_delivery`**

In the `LinkState::Active` arm, after decrypting, check for framed PubSub messages:

```rust
// In the Active arm of handle_local_delivery, after decrypting:
// Check if this is a framed PubSub message (2-byte expr_id + payload)
// or a tagged Session message (starts with known prefix).
if plaintext.starts_with(b"SUB:") {
    // Subscriber declaration from peer.
    let key_expr = String::from_utf8_lossy(&plaintext[4..]).to_string();
    if let Some(ref mut router) = peer.router {
        if let Some(ref session) = peer.session {
            let _ = router.handle_event(
                harmony_zenoh::PubSubEvent::SubscriberDeclared { key_expr },
                session,
            );
        }
    }
} else if plaintext.starts_with(b"RESDECL:") {
    // Resource declaration from peer.
    let rest = &plaintext[8..];
    if let Some(colon_pos) = rest.iter().position(|&b| b == b':') {
        let expr_id_str = String::from_utf8_lossy(&rest[..colon_pos]);
        let key_expr = String::from_utf8_lossy(&rest[colon_pos + 1..]).to_string();
        if let Ok(expr_id) = expr_id_str.parse::<u64>() {
            if let Some(ref mut session) = peer.session {
                let _ = session.handle_event(SessionEvent::ResourceDeclared {
                    expr_id,
                    key_expr,
                });
            }
        }
    }
} else if plaintext.starts_with(b"HANDSHAKE:") {
    // Session handshake from peer (after session already created).
    let proof = plaintext[10..].to_vec();
    if let Some(ref mut session) = peer.session {
        match session.handle_event(SessionEvent::HandshakeReceived { proof }) {
            Ok(actions) => {
                for action in actions {
                    if let SessionAction::SessionOpened = action {
                        // Session is now Active — set up router.
                        drop(peer); // Release mutable borrow.
                        self.setup_pubsub_router(&addr, rng, out);
                        return;
                    }
                }
            }
            Err(_) => {
                peer.session = None;
            }
        }
    }
} else if plaintext.len() >= 2 {
    // Framed PubSub message: [expr_id: 2 bytes BE][payload]
    let expr_id = u16::from_be_bytes([plaintext[0], plaintext[1]]) as u64;
    let payload = plaintext[2..].to_vec();

    if let Some(ref mut router) = peer.router {
        if let Some(ref session) = peer.session {
            match router.handle_event(
                harmony_zenoh::PubSubEvent::MessageReceived { expr_id, payload },
                session,
            ) {
                Ok(actions) => {
                    for action in actions {
                        if let harmony_zenoh::PubSubAction::Deliver {
                            key_expr, payload, ..
                        } = action
                        {
                            // Deserialize and route to registry.
                            self.handle_pubsub_delivery(&addr, &key_expr, &payload, now_secs as f64, out);
                        }
                    }
                }
                Err(_) => {}
            }
        }
    }
}
```

Add the `handle_pubsub_delivery` helper:

```rust
fn handle_pubsub_delivery(
    &mut self,
    sender_addr: &[u8; 16],
    _key_expr: &str,
    payload: &[u8],
    now_secs: f64,
    out: &mut Vec<NetworkAction>,
) {
    match serde_json::from_slice::<NetMessage>(payload) {
        Ok(NetMessage::PlayerState(state)) => {
            self.registry.update_state(sender_addr, state, now_secs);
        }
        Ok(NetMessage::Chat(chat)) => {
            out.push(NetworkAction::ChatReceived(chat));
        }
        Ok(NetMessage::Presence(event)) => {
            self.registry.handle_presence(&event, now_secs);
            out.push(NetworkAction::PresenceChange(event));
        }
        Err(_) => {}
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cd src-tauri && cargo test -p harmony-glitch publish_player_state_round_trip -- --nocapture`
Expected: PASS

- [ ] **Step 7: Run full test suite and clippy**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: ALL PASS

- [ ] **Step 8: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/network/state.rs
git commit -m "feat(network): implement PubSubRouter setup and publish_to_all_peers"
```

---

### Task 6: Wire Session actions through tick_peer_session

Extend `tick_peer_session` to serialize and send Session-level actions (keepalives, close, resource changes) through the Link. Currently these are matched but not sent.

**Files:**
- Modify: `src-tauri/src/network/state.rs:538-599` (`tick_peer_session`)
- Test: `src-tauri/src/network/state.rs` (test module)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn tick_peer_session_sends_keepalive() {
    use harmony_reticulum::ValidatedAnnounce;

    let (mut state_a, mut state_b) = make_pair_on_street("meadow");
    let mut rng = OsRng;

    let pub_a = state_a.public_identity.clone();
    let pub_b = state_b.public_identity.clone();
    let dest_name = DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();

    // Exchange announces and complete full handshake (same shuttle as Task 5 test).
    let announce_b = ValidatedAnnounce {
        identity: pub_b.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Bob", Some("meadow")),
        hops: 0,
    };
    let announce_a = ValidatedAnnounce {
        identity: pub_a.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Alice", Some("meadow")),
        hops: 0,
    };
    let mut out = Vec::new();
    state_a.handle_announce_received(&announce_b, 2, 2.0, &mut rng, &mut out);
    let mut out_b = Vec::new();
    state_b.handle_announce_received(&announce_a, 2, 2.0, &mut rng, &mut out_b);

    // Complete handshake.
    for round in 0..10 {
        let t = 3.0 + round as f64;
        let pkts = extract_packets(&out);
        if !pkts.is_empty() {
            let inbound: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
            let actions = state_b.tick(&inbound, t, &mut rng);
            out = actions;
        }
        let pkts = extract_packets(&out);
        if !pkts.is_empty() {
            let inbound: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
            let actions = state_a.tick(&inbound, t + 0.5, &mut rng);
            out = actions;
        }
        let a_has_session = state_a.peers.get(&pub_b.address_hash)
            .and_then(|p| p.session.as_ref())
            .map_or(false, |s| s.state() == SessionState::Active);
        if a_has_session { break; }
    }

    // Verify A has an active session.
    let peer = state_a.peers.get(&pub_b.address_hash).unwrap();
    assert!(peer.session.is_some());

    // Advance time past the keepalive interval (30s default).
    // The session's TimerTick should produce SendKeepalive.
    let keepalive_actions = state_a.tick(&[], 35.0, &mut rng);

    // Should have emitted at least one SendPacket (the keepalive).
    let send_count = keepalive_actions
        .iter()
        .filter(|a| matches!(a, NetworkAction::SendPacket { .. }))
        .count();
    assert!(send_count > 0, "Should send keepalive after interval");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd src-tauri && cargo test -p harmony-glitch tick_peer_session_sends_keepalive -- --nocapture`
Expected: FAIL — session actions aren't sent through the link yet

- [ ] **Step 3: Update `tick_peer_session` to send session actions**

In `tick_peer_session`, replace the arms that match `SendKeepalive`, `SendClose`, `SendCloseAck`, `SendHandshake`, `SendResourceDeclare`, and `SendResourceUndeclare` with actual sends:

```rust
SessionAction::SendKeepalive
| SessionAction::SendClose
| SessionAction::SendCloseAck
| SessionAction::SendHandshake { .. }
| SessionAction::SendResourceDeclare { .. }
| SessionAction::SendResourceUndeclare { .. } => {
    if let Some(ref link) = peer.link {
        let mut rng = rand::rngs::OsRng;
        Self::send_session_action(link, &mut rng, &action, out);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test -p harmony-glitch tick_peer_session_sends_keepalive -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full suite and clippy**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/network/state.rs
git commit -m "feat(network): wire Session actions (keepalive, close) through Link in tick"
```

---

### Task 7: Chat routing test

Verify chat messages flow end-to-end between two NetworkStates.

**Files:**
- Test: `src-tauri/src/network/state.rs` (test module)
- Modify: `src-tauri/src/network/state.rs` (if `send_chat` needs adjustment for pub_id selection)

- [ ] **Step 1: Write the chat round-trip test**

```rust
#[test]
fn chat_message_round_trip() {
    use harmony_reticulum::ValidatedAnnounce;

    let (mut state_a, mut state_b) = make_pair_on_street("meadow");
    let mut rng = OsRng;

    let pub_a = state_a.public_identity.clone();
    let pub_b = state_b.public_identity.clone();
    let dest_name = DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();

    // Exchange announces and complete full handshake (reuse shuttle pattern).
    let announce_b = ValidatedAnnounce {
        identity: pub_b.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Bob", Some("meadow")),
        hops: 0,
    };
    let announce_a = ValidatedAnnounce {
        identity: pub_a.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Alice", Some("meadow")),
        hops: 0,
    };
    let mut out = Vec::new();
    state_a.handle_announce_received(&announce_b, 2, 2.0, &mut rng, &mut out);
    let mut out_b = Vec::new();
    state_b.handle_announce_received(&announce_a, 2, 2.0, &mut rng, &mut out_b);

    for round in 0..10 {
        let t = 3.0 + round as f64;
        let pkts = extract_packets(&out);
        if !pkts.is_empty() {
            let inbound: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
            let actions = state_b.tick(&inbound, t, &mut rng);
            out = actions;
        }
        let pkts = extract_packets(&out);
        if !pkts.is_empty() {
            let inbound: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
            let actions = state_a.tick(&inbound, t + 0.5, &mut rng);
            out = actions;
        }
        let b_has_router = state_b.peers.get(&pub_a.address_hash)
            .and_then(|p| p.router.as_ref())
            .is_some();
        let a_has_router = state_a.peers.get(&pub_b.address_hash)
            .and_then(|p| p.router.as_ref())
            .is_some();
        if a_has_router && b_has_router { break; }
    }

    // A sends a chat message.
    let chat_actions = state_a.send_chat("Hello Bob!".to_string());

    // First action should be the local echo (ChatReceived).
    let local_echo = chat_actions.iter().find(|a| matches!(a, NetworkAction::ChatReceived(_)));
    assert!(local_echo.is_some(), "Sender should get local echo");

    // Feed the network packets to B.
    let pkts = extract_packets(&chat_actions);
    if !pkts.is_empty() {
        let inbound: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
        let b_actions = state_b.tick(&inbound, 20.0, &mut rng);

        // B should receive a ChatReceived action.
        let chat_received = b_actions
            .iter()
            .find(|a| matches!(a, NetworkAction::ChatReceived(_)));
        assert!(chat_received.is_some(), "B should receive the chat message");

        if let Some(NetworkAction::ChatReceived(msg)) = chat_received {
            assert_eq!(msg.text, "Hello Bob!");
            assert_eq!(msg.sender_name, "Alice");
        }
    }
}
```

**Note:** `send_chat` currently calls `publish_to_all_peers` which uses the first publisher_id (state topic). Chat needs to use the second publisher_id (chat topic). If this is the case, `send_chat` needs to be updated to pass the topic/pub_id. The implementer should check and fix this — either by adding a `publish_chat_to_all_peers` method or by making `publish_to_all_peers` take a topic discriminator.

- [ ] **Step 2: Run test**

Run: `cd src-tauri && cargo test -p harmony-glitch chat_message_round_trip -- --nocapture`
Expected: PASS (or fix chat topic routing if needed)

- [ ] **Step 3: Fix any issues, run full suite**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/network/state.rs
git commit -m "test(network): add chat message round-trip test"
```

---

### Task 8: Street change teardown test

Verify that `change_street()` properly tears down all peer connections and the peer can reconnect on the new street.

**Files:**
- Test: `src-tauri/src/network/state.rs` (test module)

- [ ] **Step 1: Write the test**

```rust
#[test]
fn street_change_tears_down_peers() {
    use harmony_reticulum::ValidatedAnnounce;

    let (mut state_a, mut state_b) = make_pair_on_street("meadow");
    let mut rng = OsRng;

    let pub_a = state_a.public_identity.clone();
    let pub_b = state_b.public_identity.clone();
    let dest_name = DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();

    // Complete handshake (abbreviated — same shuttle pattern).
    let announce_b = ValidatedAnnounce {
        identity: pub_b.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Bob", Some("meadow")),
        hops: 0,
    };
    let announce_a = ValidatedAnnounce {
        identity: pub_a.clone(),
        dest_name: dest_name.clone(),
        app_data: encode_app_data("Alice", Some("meadow")),
        hops: 0,
    };
    let mut out = Vec::new();
    state_a.handle_announce_received(&announce_b, 2, 2.0, &mut rng, &mut out);
    let mut out_b = Vec::new();
    state_b.handle_announce_received(&announce_a, 2, 2.0, &mut rng, &mut out_b);

    for round in 0..10 {
        let t = 3.0 + round as f64;
        let pkts = extract_packets(&out);
        if !pkts.is_empty() {
            let inbound: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
            let actions = state_b.tick(&inbound, t, &mut rng);
            out = actions;
        }
        let pkts = extract_packets(&out);
        if !pkts.is_empty() {
            let inbound: Vec<_> = pkts.iter().map(|p| ("udp0".to_string(), p.clone())).collect();
            let actions = state_a.tick(&inbound, t + 0.5, &mut rng);
            out = actions;
        }
    }

    // Verify A has a peer.
    assert!(
        state_a.peers.contains_key(&pub_b.address_hash),
        "A should have B as a peer"
    );

    // A changes street.
    state_a.change_street("heights", 20.0, &mut rng).unwrap();

    // All peers should be gone.
    assert!(state_a.peers.is_empty(), "Street change should clear all peers");
    assert_eq!(state_a.registry.count(), 0, "Registry should be empty");
    assert_eq!(state_a.current_street(), Some("heights"));
}
```

- [ ] **Step 2: Run test**

Run: `cd src-tauri && cargo test -p harmony-glitch street_change_tears_down -- --nocapture`
Expected: PASS (should already work since `change_street` calls `self.peers.clear()`)

- [ ] **Step 3: Run full suite**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture`
Expected: ALL PASS

- [ ] **Step 4: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/network/state.rs
git commit -m "test(network): add street change teardown test"
```

---

### Task 9: Final integration — full test suite and cleanup

Run the complete test suite, fix any remaining issues, run clippy, and do a final review pass.

**Files:**
- Modify: `src-tauri/src/network/state.rs` (cleanup only)

- [ ] **Step 1: Run the full Rust test suite**

Run: `cd src-tauri && cargo test -p harmony-glitch -- --nocapture 2>&1`
Expected: ALL PASS

- [ ] **Step 2: Run clippy**

Run: `cd src-tauri && cargo clippy -p harmony-glitch 2>&1`
Expected: No warnings. Fix any that appear.

- [ ] **Step 3: Run the frontend build (ensures types still align)**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build 2>&1`
Expected: PASS (no frontend changes, but verify nothing broke)

- [ ] **Step 4: Remove stale TODO comments**

Search for TODO comments referencing "Task 7" or "Task 8" in state.rs that are now implemented. Remove them.

Run: `cd src-tauri && grep -n "TODO" src/network/state.rs`
Remove any that refer to completed work.

- [ ] **Step 5: Final commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add -A
git commit -m "chore(network): remove stale TODOs and cleanup after wire network loop implementation"
```
