//! Sans-I/O network state machine — the beating heart of multiplayer.
//!
//! [`NetworkState`] wraps a Reticulum [`Node`], manages peer connections
//! (each consisting of a [`Link`], Zenoh [`Session`], and [`PubSubRouter`]),
//! and dispatches inbound/outbound messages. The game loop drives it via
//! `tick()` (passing raw packets received from non-blocking sockets) and
//! reads back [`NetworkAction`]s that describe what to send on the wire.
//!
//! **No I/O happens here.** The caller owns sockets and executes actions.

use std::collections::HashMap;

use harmony_identity::{Identity, PrivateIdentity};
use harmony_reticulum::{DestinationName, InterfaceMode, Link, Node, NodeAction, NodeEvent};
use harmony_zenoh::{
    PubSubRouter, PublisherId, Session, SessionAction, SessionEvent, SessionState, SubscriptionId,
};
use rand_core::CryptoRngCore;
use zeroize::Zeroizing;

use crate::engine::state::RemotePlayerFrame;
use crate::network::registry::RemotePlayerRegistry;
use crate::network::types::{ChatMessage, NetMessage, PlayerNetState, PresenceEvent};

// ── Constants ────────────────────────────────────────────────────────────

/// The Reticulum interface name used for all network traffic.
const INTERFACE_NAME: &str = "udp0";

/// App name for Reticulum destination naming.
const APP_NAME: &str = "harmony";

/// Aspect for Reticulum destination naming.
const DEST_ASPECTS: &[&str] = &["glitch", "player"];

/// Announce interval in seconds (5 minutes).
/// Re-announce every 30s so evicted peers reappear promptly.
/// This is intentionally shorter than Reticulum's default (300s) to
/// stay consistent with STALE_TIMEOUT (10s) in the registry — a peer
/// evicted for silence will re-announce within 30s rather than 5 minutes.
const ANNOUNCE_INTERVAL_SECS: u64 = 30;

/// Separator between display name and street in announce app_data.
const APP_DATA_SEPARATOR: u8 = 0x00;

// ── Types ────────────────────────────────────────────────────────────────

/// Actions the game loop must execute after each `tick()`.
#[derive(Debug)]
pub enum NetworkAction {
    /// Send raw bytes on the named interface (maps to Reticulum SendOnInterface).
    SendPacket {
        interface_name: String,
        data: Vec<u8>,
    },
    /// A remote player joined or left.
    PresenceChange(PresenceEvent),
    /// A chat message arrived from a remote player.
    ChatReceived(ChatMessage),
    /// A remote player's position/velocity was updated.
    RemotePlayerUpdate {
        address_hash: [u8; 16],
        state: PlayerNetState,
    },
}

/// Tracks a single peer's connection lifecycle.
///
/// Peers progress through: discovered (announce received) → linking
/// (Link handshake in progress) → active (Session + PubSubRouter ready).
pub struct PeerState {
    /// The peer's public identity (from their announce).
    pub identity: Identity,
    /// The peer's display name (from announce app_data).
    pub display_name: String,
    /// The peer's street (from announce app_data).
    pub street: String,
    /// Reticulum link (present during and after handshake).
    pub link: Option<Link>,
    /// Zenoh session (present once link is active).
    pub session: Option<Session>,
    /// Pub/sub router (present once session is active).
    pub router: Option<PubSubRouter>,
    /// Publisher IDs for our topics on this peer's router.
    pub publisher_ids: Vec<PublisherId>,
    /// Subscription IDs for cleanup on disconnect.
    pub subscription_ids: Vec<SubscriptionId>,
}

/// The central network state machine.
///
/// Sans-I/O: receives raw packets and returns actions. Does not own sockets.
pub struct NetworkState {
    /// Reticulum node (packet routing, announces, links).
    node: Node,
    /// Our private identity (kept as raw bytes since PrivateIdentity is not Clone).
    /// Wrapped in `Zeroizing` so key material is zeroed on drop.
    identity_bytes: Zeroizing<[u8; 64]>,
    /// Our public identity.
    public_identity: Identity,
    /// Our display name.
    display_name: String,
    /// The street we're currently on (None = lobby/offline).
    current_street: Option<String>,
    /// Our Reticulum announcing destination hash.
    dest_hash: Option<[u8; 16]>,
    /// Our Reticulum destination name.
    dest_name: Option<DestinationName>,
    /// Remote player registry (drives rendering).
    registry: RemotePlayerRegistry,
    /// Active and pending peer connections, keyed by address_hash.
    peers: HashMap<[u8; 16], PeerState>,
}

impl NetworkState {
    /// Create a new network state machine.
    ///
    /// Registers a Reticulum interface and announcing destination. The
    /// identity is consumed (passed to the Node) but we keep a byte copy
    /// for creating Sessions later.
    pub fn new(identity: PrivateIdentity, display_name: String) -> Self {
        // Save identity bytes before we hand it off to the node.
        let identity_bytes = Zeroizing::new(identity.to_private_bytes());
        let public_identity = identity.public_identity().clone();

        let mut node = Node::new();
        node.register_interface(INTERFACE_NAME.to_string(), InterfaceMode::Full, None);

        let dest_name =
            DestinationName::from_name(APP_NAME, DEST_ASPECTS).expect("valid destination name");
        let app_data = encode_app_data(&display_name, None);
        let dest_hash = node.register_announcing_destination(
            identity,
            dest_name.clone(),
            app_data,
            Some(ANNOUNCE_INTERVAL_SECS),
            0,
        );

        Self {
            node,
            identity_bytes,
            public_identity,
            display_name,
            current_street: None,
            dest_hash: Some(dest_hash),
            dest_name: Some(dest_name),
            registry: RemotePlayerRegistry::new(),
            peers: HashMap::new(),
        }
    }

    /// Update the display name and re-register the announcing destination
    /// so the next announce broadcasts the new name immediately.
    ///
    /// Returns actions for the caller to execute (the immediate re-announce).
    pub fn set_display_name(
        &mut self,
        name: String,
        now_secs: f64,
        rng: &mut impl CryptoRngCore,
    ) -> Vec<NetworkAction> {
        self.display_name = name;

        // Re-register destination with fresh app_data containing the new name.
        let mut actions = Vec::new();
        let now_secs_u64 = now_secs as u64;

        if let Some(ref old_hash) = self.dest_hash {
            self.node.unregister_announcing_destination(old_hash);
        }

        let identity = PrivateIdentity::from_private_bytes(self.identity_bytes.as_ref())
            .expect("identity bytes are valid");

        let dest_name =
            DestinationName::from_name(APP_NAME, DEST_ASPECTS).expect("valid destination name");
        let app_data = encode_app_data(&self.display_name, self.current_street.as_deref());

        let dest_hash = self.node.register_announcing_destination(
            identity,
            dest_name.clone(),
            app_data,
            Some(ANNOUNCE_INTERVAL_SECS),
            now_secs_u64,
        );

        self.dest_hash = Some(dest_hash);
        self.dest_name = Some(dest_name);

        // Trigger immediate announce with the new name.
        let announce_actions = self.node.announce(&dest_hash, rng, now_secs_u64);
        for action in announce_actions {
            if let NodeAction::SendOnInterface {
                interface_name,
                raw,
            } = action
            {
                actions.push(NetworkAction::SendPacket {
                    interface_name: interface_name.to_string(),
                    data: raw,
                });
            }
        }

        actions
    }

    /// Process inbound packets and timer ticks. Returns actions for the caller.
    ///
    /// Called by the game loop each frame. `inbound_packets` are raw bytes
    /// received from non-blocking sockets since the last tick.
    /// `now_secs` is monotonic time in seconds.
    pub fn tick(
        &mut self,
        inbound_packets: &[(String, Vec<u8>)],
        now_secs: f64,
        rng: &mut impl CryptoRngCore,
    ) -> Vec<NetworkAction> {
        let mut actions = Vec::new();
        let now_secs_u64 = now_secs as u64;

        // Feed inbound packets to the node.
        for (iface, raw) in inbound_packets {
            let node_actions = self.node.handle_event(NodeEvent::InboundPacket {
                interface_name: iface.clone(),
                raw: raw.clone(),
                now: now_secs_u64,
            });
            self.process_node_actions(node_actions, now_secs_u64, now_secs, rng, &mut actions);
        }

        // Timer tick for path expiry, scheduled announces, etc.
        let tick_actions = self.node.handle_event(NodeEvent::TimerTick { now: now_secs_u64 });
        self.process_node_actions(tick_actions, now_secs_u64, now_secs, rng, &mut actions);

        // Tick all active sessions and process their actions.
        let now_ms = (now_secs * 1000.0) as u64;
        let peer_keys: Vec<[u8; 16]> = self.peers.keys().copied().collect();
        let mut closed_peers = Vec::new();
        for addr in peer_keys {
            if self.tick_peer_session(&addr, now_ms, now_secs, &mut actions) {
                closed_peers.push(addr);
            }
        }
        // Remove peers whose sessions have closed/gone stale.
        for addr in closed_peers {
            self.peers.remove(&addr);
        }

        // Purge stale players from the registry.
        self.registry.purge_stale(now_secs);

        actions
    }

    /// Publish our player state to all active peers.
    pub fn publish_player_state(&mut self, state: &PlayerNetState) -> Vec<NetworkAction> {
        let msg = NetMessage::PlayerState(*state);
        let payload = match serde_json::to_vec(&msg) {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };
        self.publish_to_all_peers(&payload)
    }

    /// Send a chat message to all active peers.
    /// Text is truncated to 200 chars to stay within the Reticulum 500-byte MTU.
    /// Also emits a local `ChatReceived` so the sender sees their own bubble.
    pub fn send_chat(&mut self, text: String) -> Vec<NetworkAction> {
        let truncated: String = text.chars().take(200).collect();
        let chat = ChatMessage {
            text: truncated,
            sender: self.public_identity.address_hash,
            sender_name: self.display_name.clone(),
        };

        // Echo locally so the sender sees their own speech bubble.
        let mut actions = vec![NetworkAction::ChatReceived(chat.clone())];

        let msg = NetMessage::Chat(chat);
        if let Ok(payload) = serde_json::to_vec(&msg) {
            actions.extend(self.publish_to_all_peers(&payload));
        }
        actions
    }

    /// Change the street we're on.
    ///
    /// Clears the remote player registry, tears down existing peer
    /// connections for the old street, re-registers the announcing
    /// destination with updated app_data, and triggers a new announce.
    pub fn change_street(
        &mut self,
        street_name: &str,
        now_secs: f64,
        rng: &mut impl CryptoRngCore,
    ) -> Vec<NetworkAction> {
        let mut actions = Vec::new();
        let now_secs_u64 = now_secs as u64;

        // Clear all remote players and peer connections.
        self.registry.clear();
        self.peers.clear();

        // Unregister old destination.
        if let Some(ref old_hash) = self.dest_hash {
            self.node.unregister_announcing_destination(old_hash);
        }

        // Create fresh identity from saved bytes for re-registration.
        let identity = PrivateIdentity::from_private_bytes(self.identity_bytes.as_ref())
            .expect("identity bytes are valid");

        let dest_name =
            DestinationName::from_name(APP_NAME, DEST_ASPECTS).expect("valid destination name");
        let app_data = encode_app_data(&self.display_name, Some(street_name));

        let dest_hash = self.node.register_announcing_destination(
            identity,
            dest_name.clone(),
            app_data,
            Some(ANNOUNCE_INTERVAL_SECS),
            now_secs_u64,
        );

        self.current_street = Some(street_name.to_string());
        self.dest_hash = Some(dest_hash);
        self.dest_name = Some(dest_name);

        // Trigger an immediate announce for the new street.
        let announce_actions = self.node.announce(&dest_hash, rng, now_secs_u64);
        for action in announce_actions {
            if let NodeAction::SendOnInterface {
                interface_name,
                raw,
            } = action
            {
                actions.push(NetworkAction::SendPacket {
                    interface_name: interface_name.to_string(),
                    data: raw,
                });
            }
        }

        actions
    }

    /// Get render frames for all tracked remote players.
    pub fn remote_frames(&self) -> Vec<RemotePlayerFrame> {
        self.registry.frames()
    }

    /// Number of active peer connections (sessions in Active state).
    pub fn peer_count(&self) -> usize {
        self.peers
            .values()
            .filter(|p| {
                p.session
                    .as_ref()
                    .is_some_and(|s| s.state() == SessionState::Active)
            })
            .count()
    }

    /// The current street name, if any.
    pub fn current_street(&self) -> Option<&str> {
        self.current_street.as_deref()
    }

    // ── Internal: Node action processing ─────────────────────────────────

    fn process_node_actions(
        &mut self,
        node_actions: Vec<NodeAction>,
        now_secs: u64,
        now_secs_f64: f64,
        rng: &mut impl CryptoRngCore,
        out: &mut Vec<NetworkAction>,
    ) {
        for action in node_actions {
            match action {
                NodeAction::SendOnInterface {
                    interface_name,
                    raw,
                } => {
                    out.push(NetworkAction::SendPacket {
                        interface_name: interface_name.to_string(),
                        data: raw,
                    });
                }

                NodeAction::AnnounceReceived {
                    validated_announce, ..
                } => {
                    self.handle_announce_received(
                        &validated_announce,
                        now_secs,
                        now_secs_f64,
                        rng,
                        out,
                    );
                }

                NodeAction::AnnounceNeeded { dest_hash } => {
                    let announce_actions = self.node.announce(&dest_hash, rng, now_secs);
                    for a in announce_actions {
                        if let NodeAction::SendOnInterface {
                            interface_name,
                            raw,
                        } = a
                        {
                            out.push(NetworkAction::SendPacket {
                                interface_name: interface_name.to_string(),
                                data: raw,
                            });
                        }
                    }
                }

                NodeAction::DeliverLocally { packet, .. } => {
                    // Link-related packets delivered locally.
                    // For now we handle link proofs for pending links.
                    self.handle_local_delivery(&packet, now_secs, rng, out);
                }

                // Diagnostic/transport actions we don't need to surface.
                NodeAction::PacketDropped { .. }
                | NodeAction::PathsExpired { .. }
                | NodeAction::AnnounceRebroadcast { .. }
                | NodeAction::PacketRelayed { .. }
                | NodeAction::ProofRelayed { .. }
                | NodeAction::ReverseTableExpired { .. }
                | NodeAction::LinkRequestForwarded { .. }
                | NodeAction::LinkProofRouted { .. }
                | NodeAction::LinkDataRouted { .. }
                | NodeAction::LinkTableExpired { .. } => {}
            }
        }
    }

    fn handle_announce_received(
        &mut self,
        announce: &harmony_reticulum::ValidatedAnnounce,
        _now_secs: u64,
        now_secs_f64: f64,
        _rng: &mut impl CryptoRngCore,
        out: &mut Vec<NetworkAction>,
    ) {
        let addr = announce.identity.address_hash;

        // Ignore our own announces.
        if addr == self.public_identity.address_hash {
            return;
        }

        // Decode app_data to get display name and street.
        let (display_name, street) = decode_app_data(&announce.app_data);

        // Check if the peer is on the same street as us.
        let same_street = match (&self.current_street, &street) {
            (Some(ours), Some(theirs)) => ours == theirs,
            _ => false,
        };

        if !same_street {
            // If we had this peer before and they changed streets, treat as leave.
            if self.peers.remove(&addr).is_some() {
                let event = PresenceEvent::Left {
                    address_hash: addr,
                };
                self.registry.handle_presence(&event, now_secs_f64);
                out.push(NetworkAction::PresenceChange(event));
            }
            return;
        }

        // Don't re-initiate if we already have this peer.
        if self.peers.contains_key(&addr) {
            // Update their display name / street in case it changed.
            if let Some(peer) = self.peers.get_mut(&addr) {
                peer.display_name = display_name.clone();
                if let Some(s) = &street {
                    peer.street = s.clone();
                }
            }
            // Propagate name change to the registry so render frames use
            // the updated name (not just PeerState).
            self.registry.update_display_name(&addr, display_name);
            return;
        }

        // New peer on our street — record them.
        let peer = PeerState {
            identity: announce.identity.clone(),
            display_name: display_name.clone(),
            street: street.unwrap_or_default(),
            link: None,
            session: None,
            router: None,
            publisher_ids: Vec::new(),
            subscription_ids: Vec::new(),
        };
        self.peers.insert(addr, peer);

        // Emit presence join.
        let event = PresenceEvent::Joined {
            address_hash: addr,
            display_name,
        };
        self.registry.handle_presence(&event, now_secs_f64);
        out.push(NetworkAction::PresenceChange(event));

        // TODO: In Task 8 (game loop integration), initiate a Link to this
        // peer to establish an encrypted channel, then layer a Zenoh Session
        // on top. For now we record the peer and emit presence — the full
        // link/session handshake requires routing link packets through the
        // node which needs the socket layer (Task 7) to be in place.
    }

    fn handle_local_delivery(
        &mut self,
        _packet: &harmony_reticulum::Packet,
        _now_secs: u64,
        _rng: &mut impl CryptoRngCore,
        _out: &mut Vec<NetworkAction>,
    ) {
        // Link proof handling and session data routing will be implemented
        // when the socket layer (Task 7) and game loop (Task 8) are in place.
        // The Link/Session handshake involves multiple round-trips that need
        // actual packet exchange, which this tick-driven skeleton supports
        // but the test harness doesn't exercise yet.
    }

    // ── Internal: Session ticking ────────────────────────────────────────

    /// Tick a single peer's session. Returns `true` if the peer should be
    /// removed (session closed or peer went stale).
    fn tick_peer_session(
        &mut self,
        addr: &[u8; 16],
        now_ms: u64,
        now_secs_f64: f64,
        out: &mut Vec<NetworkAction>,
    ) -> bool {
        let peer = match self.peers.get_mut(addr) {
            Some(p) => p,
            None => return false,
        };

        let session = match peer.session.as_mut() {
            Some(s) => s,
            None => return false,
        };

        if session.state() == SessionState::Closed {
            // Session already closed on a prior tick — clean up the zombie entry.
            let event = PresenceEvent::Left {
                address_hash: *addr,
            };
            self.registry.handle_presence(&event, now_secs_f64);
            out.push(NetworkAction::PresenceChange(event));
            return true;
        }

        // Tick the session timer.
        let session_actions = match session.handle_event(SessionEvent::TimerTick { now_ms }) {
            Ok(a) => a,
            Err(_) => return false,
        };

        let mut should_remove = false;
        for action in session_actions {
            match action {
                SessionAction::PeerStale | SessionAction::SessionClosed => {
                    // Peer went stale or session closed — emit presence leave
                    // and mark for removal so they can rejoin via fresh announce.
                    let event = PresenceEvent::Left {
                        address_hash: *addr,
                    };
                    self.registry.handle_presence(&event, now_secs_f64);
                    out.push(NetworkAction::PresenceChange(event));
                    should_remove = true;
                }
                SessionAction::SendKeepalive
                | SessionAction::SendClose
                | SessionAction::SendCloseAck
                | SessionAction::SendHandshake { .. }
                | SessionAction::SendResourceDeclare { .. }
                | SessionAction::SendResourceUndeclare { .. } => {
                    // These need to be wrapped in a Reticulum packet and sent
                    // through the node. Will be wired up in Task 8.
                }
                SessionAction::SessionOpened
                | SessionAction::ResourceAdded { .. }
                | SessionAction::ResourceRemoved { .. } => {}
            }
        }
        should_remove
    }

    // ── Internal: Publishing ─────────────────────────────────────────────

    /// Publish a payload to all peers with active sessions and routers.
    ///
    /// Currently a no-op stub — PubSubRouter.publish() produces SendMessage
    /// actions that need to be wrapped in Reticulum data packets and routed
    /// through the Node. This requires the socket layer (Task 7) and game
    /// loop integration (Task 8) to be in place. Once those are done, this
    /// method will iterate peers, call router.publish(), and convert
    /// SendMessage actions into NetworkAction::SendPacket.
    fn publish_to_all_peers(&mut self, _payload: &[u8]) -> Vec<NetworkAction> {
        // TODO: Wire up in Task 8 when link/session data routing is complete.
        Vec::new()
    }
}

// ── App data encoding ────────────────────────────────────────────────────

/// Encode display name and optional street name into announce app_data.
///
/// Format: `display_name\0street_name` (NUL-separated).
/// If no street, just `display_name`.
///
/// NUL bytes are stripped from inputs to prevent delimiter injection
/// by untrusted peers crafting names like `"Alice\0LADEMO001"`.
fn encode_app_data(display_name: &str, street: Option<&str>) -> Vec<u8> {
    let safe_name = display_name.replace('\0', "");
    let mut data = safe_name.as_bytes().to_vec();
    if let Some(street) = street {
        data.push(APP_DATA_SEPARATOR);
        data.extend_from_slice(street.replace('\0', "").as_bytes());
    }
    data
}

/// Decode announce app_data into (display_name, optional street).
fn decode_app_data(data: &[u8]) -> (String, Option<String>) {
    if let Some(sep_pos) = data.iter().position(|&b| b == APP_DATA_SEPARATOR) {
        let name = String::from_utf8_lossy(&data[..sep_pos]).to_string();
        let street = String::from_utf8_lossy(&data[sep_pos + 1..]).to_string();
        (name, Some(street))
    } else {
        let name = String::from_utf8_lossy(data).to_string();
        (name, None)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    fn make_identity() -> PrivateIdentity {
        PrivateIdentity::generate(&mut OsRng)
    }

    fn make_state() -> NetworkState {
        let identity = make_identity();
        NetworkState::new(identity, "TestPlayer".to_string())
    }

    #[test]
    fn new_creates_node_with_interface() {
        let state = make_state();
        // Node should have one interface registered.
        assert_eq!(state.node.interface_count(), 1);
        // Should have one announcing destination registered.
        assert_eq!(state.node.announcing_destination_count(), 1);
        // Should have a dest_hash set.
        assert!(state.dest_hash.is_some());
        assert!(state.dest_name.is_some());
    }

    #[test]
    fn change_street_clears_registry() {
        let mut state = make_state();
        let mut rng = OsRng;

        // Add a fake remote player to the registry.
        state.registry.handle_presence(
            &PresenceEvent::Joined {
                address_hash: [0xAA; 16],
                display_name: "Peer".into(),
            },
            1.0,
        );
        assert_eq!(state.registry.count(), 1);

        // Change street should clear registry.
        state.change_street("heights", 100.0, &mut rng);
        assert_eq!(state.registry.count(), 0);
    }

    #[test]
    fn change_street_updates_current() {
        let mut state = make_state();
        let mut rng = OsRng;

        assert!(state.current_street().is_none());

        state.change_street("meadow", 100.0, &mut rng);
        assert_eq!(state.current_street(), Some("meadow"));

        state.change_street("heights", 200.0, &mut rng);
        assert_eq!(state.current_street(), Some("heights"));
    }

    #[test]
    fn tick_with_no_packets_produces_announce_actions() {
        let mut state = make_state();
        let mut rng = OsRng;

        // First tick at t=0 should trigger AnnounceNeeded (next_announce_at was
        // set to 0 in register_announcing_destination).
        let actions = state.tick(&[], 0.0, &mut rng);

        // Should produce at least one SendPacket (the announce broadcast).
        let send_count = actions
            .iter()
            .filter(|a| matches!(a, NetworkAction::SendPacket { .. }))
            .count();
        assert!(
            send_count > 0,
            "Expected announce SendPacket actions, got {send_count}"
        );
    }

    #[test]
    fn peer_count_starts_at_zero() {
        let state = make_state();
        assert_eq!(state.peer_count(), 0);
    }

    #[test]
    fn remote_frames_empty_initially() {
        let state = make_state();
        assert!(state.remote_frames().is_empty());
    }

    #[test]
    fn app_data_round_trip_with_street() {
        let encoded = encode_app_data("Alice", Some("meadow"));
        let (name, street) = decode_app_data(&encoded);
        assert_eq!(name, "Alice");
        assert_eq!(street.as_deref(), Some("meadow"));
    }

    #[test]
    fn app_data_round_trip_without_street() {
        let encoded = encode_app_data("Bob", None);
        let (name, street) = decode_app_data(&encoded);
        assert_eq!(name, "Bob");
        assert!(street.is_none());
    }

    #[test]
    fn change_street_re_registers_destination() {
        let mut state = make_state();
        let mut rng = OsRng;

        let old_hash = state.dest_hash.unwrap();
        state.change_street("heights", 100.0, &mut rng);

        // Destination hash should remain the same (same identity + same dest name).
        // But the announcing destination should still be registered.
        assert_eq!(state.node.announcing_destination_count(), 1);
        assert_eq!(state.dest_hash.unwrap(), old_hash);
    }

    #[test]
    fn change_street_clears_peers() {
        let mut state = make_state();
        let mut rng = OsRng;

        // Insert a fake peer.
        state.peers.insert(
            [0xBB; 16],
            PeerState {
                identity: state.public_identity.clone(),
                display_name: "FakePeer".into(),
                street: "meadow".into(),
                link: None,
                session: None,
                router: None,
                publisher_ids: Vec::new(),
                subscription_ids: Vec::new(),
            },
        );
        assert_eq!(state.peers.len(), 1);

        state.change_street("heights", 100.0, &mut rng);
        assert!(state.peers.is_empty());
    }

    #[test]
    fn display_name_preserved() {
        let state = make_state();
        assert_eq!(state.display_name, "TestPlayer");
    }

    #[test]
    fn set_display_name_triggers_re_announce() {
        let mut state = make_state();
        let mut rng = OsRng;

        // Rename should produce SendPacket actions (the immediate re-announce).
        let actions = state.set_display_name("NewName".to_string(), 10.0, &mut rng);
        assert_eq!(state.display_name, "NewName");

        let send_count = actions
            .iter()
            .filter(|a| matches!(a, NetworkAction::SendPacket { .. }))
            .count();
        assert!(
            send_count > 0,
            "Expected re-announce SendPacket actions after name change"
        );

        // Destination should still be registered.
        assert_eq!(state.node.announcing_destination_count(), 1);
    }
}
