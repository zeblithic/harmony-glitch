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
use std::sync::Arc;

use harmony_reticulum::{
    DestinationName, DestinationType, HeaderType, InterfaceMode, Link, LinkState, Node, NodeAction,
    NodeEvent, Packet, PacketContext, PacketFlags, PacketHeader, PacketType, PropagationType,
};
use harmony_zenoh::{
    ExprId, PubSubAction, PubSubEvent, PubSubRouter, PublisherId, Session, SessionAction,
    SessionConfig, SessionEvent, SessionState, SubscriptionId,
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

/// Frame tag for control messages (UTF-8 text follows).
/// Eliminates ambiguity with binary PubSub data frames.
const FRAME_TAG_CONTROL: u8 = 0x01;

/// Frame tag for binary PubSub data ([expr_id: u16 BE][payload] follows).
const FRAME_TAG_DATA: u8 = 0x02;

/// Which topic to publish on — avoids positional index bugs.
#[derive(Clone, Copy)]
enum PubTopic {
    State,
    Chat,
}

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
    /// Publisher ID for our player state topic (None if declaration failed).
    pub state_publisher_id: Option<PublisherId>,
    /// Publisher ID for our chat topic (None if declaration failed).
    pub chat_publisher_id: Option<PublisherId>,
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
    /// Links from incoming LinkRequests that haven't been matched to a peer yet.
    /// The responder can't determine the initiator's identity from the request
    /// (it only contains ephemeral keys), so we buffer the link here and match
    /// it to a peer during Session handshake when the real identity is revealed.
    /// Keyed by link_id.
    unmatched_links: HashMap<[u8; 16], Link>,
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
            unmatched_links: HashMap::new(),
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
    ) -> Result<Vec<NetworkAction>, String> {
        self.display_name = name;

        // Re-register destination with fresh app_data containing the new name.
        let mut actions = Vec::new();
        let now_secs_u64 = now_secs as u64;

        if let Some(ref old_hash) = self.dest_hash {
            self.node.unregister_announcing_destination(old_hash);
        }

        let identity = PrivateIdentity::from_private_bytes(self.identity_bytes.as_ref())
            .map_err(|e| format!("identity reconstruction failed: {e:?}"))?;

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
                ..
            } = action
            {
                actions.push(NetworkAction::SendPacket {
                    interface_name: interface_name.to_string(),
                    data: raw,
                });
            }
        }

        Ok(actions)
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
        let tick_actions = self
            .node
            .handle_event(NodeEvent::TimerTick { now: now_secs_u64 });
        self.process_node_actions(tick_actions, now_secs_u64, now_secs, rng, &mut actions);

        // Tick all active sessions and process their actions.
        let now_ms = (now_secs * 1000.0) as u64;
        let peer_keys: Vec<[u8; 16]> = self.peers.keys().copied().collect();
        let mut closed_peers = Vec::new();
        for addr in peer_keys {
            if self.tick_peer_session(&addr, now_ms, now_secs, rng, &mut actions) {
                closed_peers.push(addr);
            }
        }
        // Remove peers whose sessions have closed/gone stale.
        for addr in closed_peers {
            self.unregister_peer_link(&addr);
            self.peers.remove(&addr);
        }

        // Purge stale players and clean up their PeerState entries.
        // Send a graceful CLOSE to peers with active links so the remote
        // side tears down promptly instead of waiting for its own stale timeout.
        let purged = self.registry.purge_stale(now_secs);
        for addr in purged {
            if let Some(peer) = self.peers.get(&addr) {
                if let Some(link) = peer.link.as_ref() {
                    if link.state() == LinkState::Active {
                        Self::send_control(link, rng, b"CLOSE", &mut actions);
                    }
                }
            }
            self.unregister_peer_link(&addr);
            self.peers.remove(&addr);
            let event = PresenceEvent::Left { address_hash: addr };
            actions.push(NetworkAction::PresenceChange(event));
        }

        // Sweep unmatched_links: only remove Closed links. Active/Handshake
        // links are kept regardless of peer count — a LinkRequest can arrive
        // before the initiator's announce (race), so we must not purge valid
        // links just because no peer entry exists yet. Truly orphaned links
        // are cleaned up by change_street() which clears everything.
        let remove_ids: Vec<[u8; 16]> = self
            .unmatched_links
            .iter()
            .filter(|(_, l)| l.state() == LinkState::Closed)
            .map(|(id, _)| *id)
            .collect();
        for link_id in remove_ids {
            self.node.unregister_destination(&link_id);
            self.unmatched_links.remove(&link_id);
        }

        actions
    }

    /// Publish our player state to all active peers.
    pub fn publish_player_state(
        &mut self,
        state: &PlayerNetState,
        rng: &mut impl CryptoRngCore,
    ) -> Vec<NetworkAction> {
        let msg = NetMessage::PlayerState(*state);
        let payload = match serde_json::to_vec(&msg) {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };
        self.publish_to_all_peers(&payload, PubTopic::State, rng)
    }

    /// Send a chat message to all active peers.
    /// Text is truncated to 200 UTF-8 bytes to stay within the Reticulum
    /// 500-byte MTU (worst case: 500 - 35 header - 33 Zenoh = 432 payload).
    /// Also emits a local `ChatReceived` so the sender sees their own bubble.
    pub fn send_chat(&mut self, text: String, rng: &mut impl CryptoRngCore) -> Vec<NetworkAction> {
        let truncated = truncate_to_bytes(&text, 200);
        let chat = ChatMessage {
            text: truncated,
            sender: self.public_identity.address_hash,
            sender_name: self.display_name.clone(),
        };

        // Echo locally so the sender sees their own speech bubble.
        let mut actions = vec![NetworkAction::ChatReceived(chat.clone())];

        let msg = NetMessage::Chat(chat);
        if let Ok(payload) = serde_json::to_vec(&msg) {
            actions.extend(self.publish_to_all_peers(&payload, PubTopic::Chat, rng));
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
    ) -> Result<Vec<NetworkAction>, String> {
        let mut actions = Vec::new();
        let now_secs_u64 = now_secs as u64;

        // Send graceful CLOSE to all peers with active links, then unregister.
        let peer_addrs: Vec<[u8; 16]> = self.peers.keys().copied().collect();
        for addr in &peer_addrs {
            if let Some(peer) = self.peers.get(addr) {
                if let Some(link) = peer.link.as_ref() {
                    if link.state() == LinkState::Active {
                        Self::send_control(link, rng, b"CLOSE", &mut actions);
                    }
                }
            }
        }
        for addr in &peer_addrs {
            self.unregister_peer_link(addr);
        }
        // Also unregister any unmatched links.
        let unmatched_ids: Vec<[u8; 16]> = self.unmatched_links.keys().copied().collect();
        for link_id in unmatched_ids {
            self.node.unregister_destination(&link_id);
        }

        // Emit PresenceChange(Left) for all peers so the frontend removes them.
        for addr in &peer_addrs {
            actions.push(NetworkAction::PresenceChange(PresenceEvent::Left {
                address_hash: *addr,
            }));
        }

        // Clear all remote players and peer connections.
        self.registry.clear();
        self.peers.clear();
        self.unmatched_links.clear();

        // Unregister old destination.
        if let Some(ref old_hash) = self.dest_hash {
            self.node.unregister_announcing_destination(old_hash);
        }

        // Create fresh identity from saved bytes for re-registration.
        let identity = PrivateIdentity::from_private_bytes(self.identity_bytes.as_ref())
            .map_err(|e| format!("identity reconstruction failed: {e:?}"))?;

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
                ..
            } = action
            {
                actions.push(NetworkAction::SendPacket {
                    interface_name: interface_name.to_string(),
                    data: raw,
                });
            }
        }

        Ok(actions)
    }

    /// Get render frames for all tracked remote players.
    pub fn remote_frames(&self) -> Vec<RemotePlayerFrame> {
        self.registry.frames()
    }

    /// Number of discovered players on the same street.
    /// Uses registry count (announce-based discovery) until session-layer
    /// peer counting is wired up (Task 8).
    pub fn peer_count(&self) -> usize {
        self.registry.count()
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
                    ..
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
                            ..
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
                    self.handle_local_delivery(&packet, now_secs_f64, rng, out);
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
        rng: &mut impl CryptoRngCore,
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
            self.unregister_peer_link(&addr);
            if self.peers.remove(&addr).is_some() {
                let event = PresenceEvent::Left { address_hash: addr };
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
            // A re-announce proves the peer is alive — refresh their
            // liveness so they aren't evicted by purge_stale while idle.
            self.registry.refresh_liveness(&addr, now_secs_f64);
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
            state_publisher_id: None,
            chat_publisher_id: None,
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
                        // Register the link_id so proof/data packets
                        // addressed to it get DeliverLocally by the node.
                        self.node.register_destination(*link.link_id());

                        // Store the pending link.
                        if let Some(peer) = self.peers.get_mut(&addr) {
                            peer.link = Some(link);
                        }
                    }
                    Err(_) => {
                        // Roll back peer insertion so re-announce retries cleanly.
                        self.peers.remove(&addr);
                        self.registry.handle_presence(
                            &PresenceEvent::Left { address_hash: addr },
                            now_secs_f64,
                        );
                        // Match the Joined we already emitted above.
                        out.push(NetworkAction::PresenceChange(PresenceEvent::Left {
                            address_hash: addr,
                        }));
                    }
                }
            }
        }
    }

    fn handle_local_delivery(
        &mut self,
        packet: &harmony_reticulum::Packet,
        now_secs: f64,
        rng: &mut impl CryptoRngCore,
        out: &mut Vec<NetworkAction>,
    ) {
        let dest_hash = packet.header.destination_hash;

        match packet.header.flags.packet_type {
            // ── LinkRequest: we are the responder ────────────────────
            PacketType::LinkRequest => {
                let identity =
                    match PrivateIdentity::from_private_bytes(self.identity_bytes.as_ref()) {
                        Ok(id) => id,
                        Err(_) => return,
                    };
                let dest_name = match &self.dest_name {
                    Some(dn) => dn,
                    None => return,
                };

                let (link, proof_packet) = match Link::respond(&identity, dest_name, packet) {
                    Ok(pair) => pair,
                    Err(_) => return,
                };

                // Register the link_id as a local destination so the RTT
                // data packet (addressed to link_id) gets DeliverLocally.
                self.node.register_destination(*link.link_id());

                // Buffer the link — we can't determine the initiator's identity
                // from the request (it only contains ephemeral keys). The link
                // will be matched to the correct peer during Session handshake,
                // which reveals the real Ed25519 identity.
                let link_id = *link.link_id();
                self.unmatched_links.insert(link_id, link);

                // Emit the proof packet for sending.
                if let Ok(raw) = proof_packet.to_bytes() {
                    out.push(NetworkAction::SendPacket {
                        interface_name: INTERFACE_NAME.to_string(),
                        data: raw,
                    });
                }
            }

            // ── Proof: we are the initiator ──────────────────────────
            PacketType::Proof => {
                // dest_hash is the link_id. Find the peer with a matching
                // pending link. Extract the address first to avoid holding
                // a mutable borrow when calling activate_peer_session.
                let peer_addr = self
                    .peers
                    .iter()
                    .find(|(_, p)| {
                        p.link.as_ref().is_some_and(|l| {
                            *l.link_id() == dest_hash && l.state() == LinkState::Pending
                        })
                    })
                    .map(|(addr, _)| *addr);

                let peer_addr = match peer_addr {
                    Some(a) => a,
                    None => return,
                };

                let peer = self.peers.get_mut(&peer_addr).unwrap();
                let link = peer.link.as_mut().unwrap();

                // Use a reasonable RTT estimate — we don't have real timing
                // yet, so use 0.1s as a placeholder.
                // TODO: measure actual RTT from (now_secs - request_sent_time)
                // once timestamps are threaded through the link state.
                let rtt_secs = 0.1;

                let rtt_packet = match link.complete_handshake(rng, packet, rtt_secs) {
                    Ok(pkt) => pkt,
                    Err(_) => return,
                };

                // Emit the RTT packet for sending.
                if let Ok(raw) = rtt_packet.to_bytes() {
                    out.push(NetworkAction::SendPacket {
                        interface_name: INTERFACE_NAME.to_string(),
                        data: raw,
                    });
                }

                // Link is now Active — activate the Zenoh Session.
                let now_ms = (now_secs * 1000.0) as u64;
                self.activate_peer_session(&peer_addr, now_ms, rng, out);
            }

            // ── Data ─────────────────────────────────────────────────
            PacketType::Data => {
                // RTT packet from initiator: context=Lrrtt, link in Handshake state.
                // The link may be in unmatched_links (responder side) or on a peer.
                if packet.header.context == PacketContext::Lrrtt {
                    // Check unmatched_links first (responder side).
                    if let Some(link) = self.unmatched_links.get_mut(&dest_hash) {
                        if link.state() == LinkState::Handshake {
                            if link.activate(packet).is_err() {
                                // RTT verification failed — drop the link.
                                self.node.unregister_destination(&dest_hash);
                                self.unmatched_links.remove(&dest_hash);
                                return;
                            }
                            // Link is now Active but not yet assigned to a peer.
                            // It will be matched during Session handshake when
                            // the remote identity is revealed via
                            // activate_peer_session → Session::new → proof exchange.
                            // For now, start the Session with the link still in
                            // unmatched_links. We need to move it to a peer first.
                            // Since we can't determine peer identity yet, we'll
                            // handle the Session handshake proof to discover it.
                            //
                            // We don't know the remote identity yet — try to find
                            // a linkless peer. If there's exactly one, it's the one.
                            // If multiple, we defer until identity is revealed.
                            let linkless_peers: Vec<[u8; 16]> = self
                                .peers
                                .iter()
                                .filter(|(_, p)| p.link.is_none())
                                .map(|(addr, _)| *addr)
                                .collect();

                            if linkless_peers.len() == 1 {
                                // Exactly one candidate — assign link to this peer.
                                let peer_addr = linkless_peers[0];
                                let link = self.unmatched_links.remove(&dest_hash).unwrap();
                                if let Some(peer) = self.peers.get_mut(&peer_addr) {
                                    peer.link = Some(link);
                                }
                                let now_ms = (now_secs * 1000.0) as u64;
                                self.activate_peer_session(&peer_addr, now_ms, rng, out);
                            }
                            // Multiple linkless peers — can't determine which one
                            // sent this link request from the RTT alone. The link
                            // stays in unmatched_links until the initiator's
                            // Session handshake proof arrives (which contains an
                            // Ed25519 signature we can verify against each candidate).
                            //
                            // This resolves in the same tick: the initiator emits
                            // both the RTT and session proof in the same output
                            // batch, so the proof packet is processed immediately
                            // after this RTT → try_match_unmatched_link verifies
                            // the signature and assigns the link to the correct peer.
                            //
                            // Edge case: if the session proof packet is lost (UDP),
                            // the initiator's Session timer will retransmit the
                            // handshake, resolving on the next arrival.
                            return;
                        }
                    }

                    // Also check peers (initiator side won't hit this for RTT,
                    // but handle it for completeness).
                    let peer_addr = self
                        .peers
                        .iter()
                        .find(|(_, p)| {
                            p.link.as_ref().is_some_and(|l| {
                                *l.link_id() == dest_hash && l.state() == LinkState::Handshake
                            })
                        })
                        .map(|(addr, _)| *addr);

                    if let Some(peer_addr) = peer_addr {
                        let peer = self.peers.get_mut(&peer_addr).unwrap();
                        let link = peer.link.as_mut().unwrap();
                        let _rtt = link.activate(packet);
                        let now_ms = (now_secs * 1000.0) as u64;
                        self.activate_peer_session(&peer_addr, now_ms, rng, out);
                    }
                    return;
                }

                // Active link data — decrypt and route to Session/PubSub.
                // Check peers first, then unmatched_links for the multi-peer case.
                let peer_addr = self
                    .peers
                    .iter()
                    .find(|(_, p)| {
                        p.link
                            .as_ref()
                            .is_some_and(|l| *l.link_id() == dest_hash && l.state() == LinkState::Active)
                    })
                    .map(|(addr, _)| *addr);

                // If not on a peer, check unmatched_links (multi-peer case:
                // link activated via RTT but couldn't determine which peer).
                if peer_addr.is_none() {
                    if let Some(link) = self.unmatched_links.get(&dest_hash) {
                        if link.state() == LinkState::Active {
                            // Only session handshake proofs (PacketContext::Channel)
                            // can be used for identity-based peer matching. Other
                            // packets (KEEPALIVE, RESDECL, data frames) on unmatched
                            // links are discarded — they can't be processed without
                            // a Session, and the link will be matched when the proof
                            // arrives (same tick or via retransmit).
                            if packet.header.context == PacketContext::Channel {
                                if let Ok(plaintext) = link.decrypt(&packet.data) {
                                    self.try_match_unmatched_link(
                                        &dest_hash, &plaintext, now_secs, rng, out,
                                    );
                                }
                            }
                            return;
                        }
                    }
                    return;
                }

                let peer_addr = peer_addr.unwrap();

                let peer = self.peers.get_mut(&peer_addr).unwrap();
                let link = match peer.link.as_ref() {
                    Some(l) => l,
                    None => return,
                };

                // Decrypt the link data.
                let plaintext = match link.decrypt(&packet.data) {
                    Ok(pt) => pt,
                    Err(_) => return,
                };

                // Session handshake proofs are sent with PacketContext::Channel.
                // Use this as a discriminator so late/retransmitted proofs
                // arriving after the session goes Active are silently ignored
                // instead of falling through to the PubSub handler.
                let session_state = peer.session.as_ref().map(|s| s.state());
                if packet.header.context == PacketContext::Channel {
                    // This is a session handshake proof (initial or retransmit).
                    if session_state == Some(SessionState::Init) {
                        let mut opened = false;
                        if let Some(ref mut session) = peer.session {
                            if let Ok(actions) =
                                session.handle_event(SessionEvent::HandshakeReceived {
                                    proof: plaintext,
                                })
                            {
                                for action in &actions {
                                    if matches!(action, SessionAction::SessionOpened) {
                                        opened = true;
                                    }
                                }
                            }
                        }
                        if opened {
                            self.setup_pubsub_router(&peer_addr, now_secs, rng, out);
                        }
                    }
                    // If session is already Active, this is a late retransmit — ignore.
                    return;
                }

                // Session is Active — handle PubSub data and control messages.
                if session_state == Some(SessionState::Active) {
                    self.handle_inbound_pubsub(&peer_addr, &plaintext, now_secs, rng, out);
                }
            }

            // Announce packets are handled via AnnounceReceived, not DeliverLocally.
            PacketType::Announce => {}
        }
    }

    // ── Internal: Session activation ──────────────────────────────────────

    /// Unregister a peer's link_id from the Node so orphan destinations
    /// don't continue to match/deliver packets after teardown.
    /// Try to match an unmatched link to a peer using the Session handshake
    /// proof. The proof is an Ed25519 signature of "harmony-session-v1" ||
    /// our_address_hash, signed by the remote peer. We verify against each
    /// linkless peer's identity to find the match.
    fn try_match_unmatched_link(
        &mut self,
        link_id: &[u8; 16],
        proof_bytes: &[u8],
        now_secs: f64,
        rng: &mut impl CryptoRngCore,
        out: &mut Vec<NetworkAction>,
    ) {
        // Ed25519 signature length.
        const SIG_LEN: usize = 64;

        // Session handshake proofs are exactly 64 bytes (Ed25519 signature).
        if proof_bytes.len() != SIG_LEN {
            return;
        }
        let signature: &[u8; SIG_LEN] = proof_bytes.try_into().unwrap();

        // Construct the expected signed message.
        let mut expected_msg = b"harmony-session-v1".to_vec();
        expected_msg.extend_from_slice(&self.public_identity.address_hash);

        // Try each linkless peer's identity.
        let matched_addr = self
            .peers
            .iter()
            .filter(|(_, p)| p.link.is_none())
            .find(|(_, p)| p.identity.verify(&expected_msg, signature).is_ok())
            .map(|(addr, _)| *addr);

        if let Some(peer_addr) = matched_addr {
            // Match found — move link from unmatched_links to this peer.
            if let Some(link) = self.unmatched_links.remove(link_id) {
                if let Some(peer) = self.peers.get_mut(&peer_addr) {
                    peer.link = Some(link);
                }
                // Activate Session for this peer.
                let now_ms = (now_secs * 1000.0) as u64;
                self.activate_peer_session(&peer_addr, now_ms, rng, out);

                // Now feed the handshake proof to the new Session.
                if let Some(peer) = self.peers.get_mut(&peer_addr) {
                    if let Some(ref mut session) = peer.session {
                        if session.state() == SessionState::Init {
                            let mut opened = false;
                            if let Ok(actions) = session.handle_event(
                                SessionEvent::HandshakeReceived {
                                    proof: proof_bytes.to_vec(),
                                },
                            ) {
                                for action in &actions {
                                    if matches!(action, SessionAction::SessionOpened) {
                                        opened = true;
                                    }
                                }
                            }
                            if opened {
                                self.setup_pubsub_router(&peer_addr, now_secs, rng, out);
                            }
                        }
                    }
                }
            }
        }
    }

    fn unregister_peer_link(&mut self, addr: &[u8; 16]) {
        if let Some(peer) = self.peers.get(addr) {
            if let Some(ref link) = peer.link {
                self.node.unregister_destination(link.link_id());
            }
        }
    }

    /// Send a control message (UTF-8 text) through a link with FRAME_TAG_CONTROL prefix.
    fn send_control(
        link: &Link,
        rng: &mut impl CryptoRngCore,
        text: &[u8],
        out: &mut Vec<NetworkAction>,
    ) {
        let mut tagged = Vec::with_capacity(1 + text.len());
        tagged.push(FRAME_TAG_CONTROL);
        tagged.extend_from_slice(text);
        Self::send_via_link(link, rng, &tagged, PacketContext::None, out);
    }

    /// Send a binary PubSub data frame through a link with FRAME_TAG_DATA prefix.
    fn send_data_frame(
        link: &Link,
        rng: &mut impl CryptoRngCore,
        expr_id: u16,
        payload: &[u8],
        out: &mut Vec<NetworkAction>,
    ) {
        let mut tagged = Vec::with_capacity(1 + 2 + payload.len());
        tagged.push(FRAME_TAG_DATA);
        tagged.extend_from_slice(&expr_id.to_be_bytes());
        tagged.extend_from_slice(payload);
        Self::send_via_link(link, rng, &tagged, PacketContext::None, out);
    }

    /// Build an encrypted link data packet and push it as a `NetworkAction::SendPacket`.
    ///
    /// Low-level — prefer `send_control` or `send_data_frame` for tagged messages.
    /// Used directly only for session handshake proofs (PacketContext::Channel).
    fn send_via_link(
        link: &Link,
        rng: &mut impl CryptoRngCore,
        plaintext: &[u8],
        context: PacketContext,
        out: &mut Vec<NetworkAction>,
    ) {
        let ciphertext = match link.encrypt(rng, plaintext) {
            Ok(ct) => ct,
            Err(_) => return,
        };

        let packet = Packet {
            header: PacketHeader {
                flags: PacketFlags {
                    ifac: false,
                    header_type: HeaderType::Type1,
                    context_flag: context != PacketContext::None,
                    propagation: PropagationType::Broadcast,
                    destination_type: DestinationType::Link,
                    packet_type: PacketType::Data,
                },
                hops: 0,
                transport_id: None,
                destination_hash: *link.link_id(),
                context,
            },
            data: Arc::from(ciphertext.as_slice()),
        };

        if let Ok(raw) = packet.to_bytes() {
            out.push(NetworkAction::SendPacket {
                interface_name: INTERFACE_NAME.to_string(),
                data: raw,
            });
        }
    }

    /// Activate a Zenoh Session for a peer whose Link just became Active.
    ///
    /// Reconstructs a `PrivateIdentity` from saved bytes, creates a Session,
    /// and sends the handshake proof encrypted through the peer's Link.
    fn activate_peer_session(
        &mut self,
        addr: &[u8; 16],
        now_ms: u64,
        rng: &mut impl CryptoRngCore,
        out: &mut Vec<NetworkAction>,
    ) {
        // Don't overwrite a session that is already underway (guards against
        // retransmit races between the RTT path and try_match_unmatched_link).
        if self
            .peers
            .get(addr)
            .and_then(|p| p.session.as_ref())
            .is_some()
        {
            return;
        }

        // Reconstruct the PrivateIdentity (Session::new consumes it by value).
        let private_identity =
            match PrivateIdentity::from_private_bytes(self.identity_bytes.as_ref()) {
                Ok(id) => id,
                Err(_) => return,
            };

        // Gather peer data needed for session creation.
        let peer = match self.peers.get(addr) {
            Some(p) => p,
            None => return,
        };

        let peer_identity = peer.identity.clone();
        let link = match peer.link.as_ref() {
            Some(l) if l.state() == LinkState::Active => l,
            _ => return,
        };

        // Create the Session — this produces a SendHandshake action with our proof.
        let (session, session_actions) = Session::new(
            private_identity,
            peer_identity,
            SessionConfig::default(),
            now_ms,
        );

        // Encrypt and send each session action through the link.
        for action in &session_actions {
            if let SessionAction::SendHandshake { proof } = action {
                Self::send_via_link(link, rng, proof, PacketContext::Channel, out);
            }
        }

        // Store the session in Init state.
        if let Some(peer) = self.peers.get_mut(addr) {
            peer.session = Some(session);
        }
    }

    // ── Internal: PubSub setup ─────────────────────────────────────────

    /// Set up a PubSubRouter for a peer whose Session just became Active.
    ///
    /// Declares publishers for our topics, subscribes to the peer's topics,
    /// and sends all resulting actions through the link.
    fn setup_pubsub_router(
        &mut self,
        addr: &[u8; 16],
        now_secs: f64,
        rng: &mut impl CryptoRngCore,
        out: &mut Vec<NetworkAction>,
    ) {
        // Don't reinitialise a router that is already running (guards against
        // retransmitted session proofs causing duplicate SessionOpened actions).
        if self
            .peers
            .get(addr)
            .and_then(|p| p.router.as_ref())
            .is_some()
        {
            return;
        }

        let street = match &self.current_street {
            Some(s) => s.clone(),
            None => return,
        };

        // Refresh liveness so the peer isn't evicted before their first
        // keepalive (STALE_TIMEOUT=10s < keepalive interval=30s).
        // After the street guard — don't refresh if we have no street context.
        self.registry.refresh_liveness(addr, now_secs);

        let our_addr_hex = hex::encode(self.public_identity.address_hash);
        let peer_addr_hex = hex::encode(addr);

        let mut router = PubSubRouter::new();

        // We need the session mutably for declare_publisher.
        let peer = match self.peers.get_mut(addr) {
            Some(p) => p,
            None => return,
        };

        let session = match peer.session.as_mut() {
            Some(s) if s.state() == SessionState::Active => s,
            _ => return,
        };

        // Declare publishers for our topics.
        let state_topic =
            format!("harmony/glitch/street/{street}/player/{our_addr_hex}/state");
        let chat_topic =
            format!("harmony/glitch/street/{street}/player/{our_addr_hex}/chat");

        let mut state_publisher_id = None;
        let mut chat_publisher_id = None;
        let mut all_pubsub_actions: Vec<PubSubAction> = Vec::new();

        if let Ok((pub_id, actions)) = router.declare_publisher(state_topic, session) {
            state_publisher_id = Some(pub_id);
            all_pubsub_actions.extend(actions);
        }
        if let Ok((pub_id, actions)) = router.declare_publisher(chat_topic, session) {
            chat_publisher_id = Some(pub_id);
            all_pubsub_actions.extend(actions);
        }

        // Subscribe to the peer's topics.
        let peer_state_topic =
            format!("harmony/glitch/street/{street}/player/{peer_addr_hex}/state");
        let peer_chat_topic =
            format!("harmony/glitch/street/{street}/player/{peer_addr_hex}/chat");

        let mut subscription_ids = Vec::new();

        if let Ok((sub_id, actions)) = router.subscribe(&peer_state_topic) {
            subscription_ids.push(sub_id);
            all_pubsub_actions.extend(actions);
        }
        if let Ok((sub_id, actions)) = router.subscribe(&peer_chat_topic) {
            subscription_ids.push(sub_id);
            all_pubsub_actions.extend(actions);
        }

        // Fail fast: if no publishers were declared, don't announce
        // subscriptions either — we'd create dangling SUBs on the remote
        // side that can never be satisfied (no router stored locally).
        if state_publisher_id.is_none() && chat_publisher_id.is_none() {
            return;
        }

        // Process all PubSubActions — send them through the link.
        let link = match peer.link.as_ref() {
            Some(l) if l.state() == LinkState::Active => l,
            _ => return,
        };

        for action in &all_pubsub_actions {
            match action {
                PubSubAction::Session(SessionAction::SendResourceDeclare {
                    expr_id,
                    key_expr,
                }) => {
                    let msg = format!("RESDECL:{expr_id}:{key_expr}");
                    Self::send_control(link, rng, msg.as_bytes(), out);
                }
                PubSubAction::SendSubscriberDeclare { key_expr } => {
                    let msg = format!("SUB:{key_expr}");
                    Self::send_control(link, rng, msg.as_bytes(), out);
                }
                _ => {}
            }
        }

        // Store the router and IDs in the peer state.
        peer.router = Some(router);
        peer.state_publisher_id = state_publisher_id;
        peer.chat_publisher_id = chat_publisher_id;
        peer.subscription_ids = subscription_ids;
    }

    // ── Internal: Inbound PubSub routing ────────────────────────────────

    /// Handle an inbound decrypted message from a peer with an Active session.
    ///
    /// Dispatches control messages (SUB:, RESDECL:) and PubSub data frames
    /// to the appropriate handler. Returns an address to remove from peers
    /// if a CLOSE or Presence(Left) triggers teardown.
    fn handle_inbound_pubsub(
        &mut self,
        addr: &[u8; 16],
        plaintext: &[u8],
        now_secs_f64: f64,
        rng: &mut impl CryptoRngCore,
        out: &mut Vec<NetworkAction>,
    ) {
        // Dispatch based on frame tag byte.
        if plaintext.is_empty() {
            return;
        }
        let tag = plaintext[0];
        let body = &plaintext[1..];

        // Data frames (tag 0x02): [expr_id: 2 bytes BE][payload].
        if tag == FRAME_TAG_DATA {
            self.handle_inbound_data_frame(addr, body, now_secs_f64, rng, out);
            return;
        }

        // Control messages (tag 0x01): UTF-8 text.
        if tag != FRAME_TAG_CONTROL {
            // Unknown tag — discard. No fallback to raw UTF-8 parsing,
            // which would reintroduce the framing ambiguity.
            return;
        }
        if let Ok(text) = std::str::from_utf8(body) {
            if text == "KEEPALIVE" {
                let peer = match self.peers.get_mut(addr) {
                    Some(p) => p,
                    None => return,
                };
                if let Some(session) = peer.session.as_mut() {
                    let _ = session.handle_event(SessionEvent::KeepaliveReceived);
                }
                // Refresh registry liveness so idle-but-alive peers aren't
                // evicted by purge_stale (STALE_TIMEOUT=10s < keepalive=30s).
                // NOTE: purge_stale() in tick() runs AFTER inbound packet
                // processing, so this refresh is visible to the same-tick purge.
                self.registry.refresh_liveness(addr, now_secs_f64);
                return;
            }
            if text == "CLOSE" {
                let peer = match self.peers.get_mut(addr) {
                    Some(p) => p,
                    None => return,
                };
                let actions = peer
                    .session
                    .as_mut()
                    .and_then(|s| s.handle_event(SessionEvent::CloseReceived).ok())
                    .unwrap_or_default();
                let link = peer
                    .link
                    .as_ref()
                    .filter(|l| l.state() == LinkState::Active);
                for action in &actions {
                    match action {
                        SessionAction::SendCloseAck => {
                            if let Some(link) = link {
                                Self::send_control(link, rng, b"CLOSEACK", out);
                            }
                        }
                        SessionAction::SessionClosed | SessionAction::PeerStale => {
                            // Session transitioned to Closed — schedule teardown.
                        }
                        _ => {}
                    }
                }
                // The remote explicitly asked us to close — tear down the peer.
                // Emit PresenceEvent::Left so the frontend knows they departed.
                self.registry.handle_presence(
                    &PresenceEvent::Left { address_hash: *addr },
                    now_secs_f64,
                );
                out.push(NetworkAction::PresenceChange(PresenceEvent::Left {
                    address_hash: *addr,
                }));
                self.unregister_peer_link(addr);
                self.peers.remove(addr);
                return;
            }
            if text == "CLOSEACK" {
                let peer = match self.peers.get_mut(addr) {
                    Some(p) => p,
                    None => return,
                };
                let mut should_close = false;
                if let Some(session) = peer.session.as_mut() {
                    if let Ok(actions) = session.handle_event(SessionEvent::CloseAckReceived) {
                        for action in &actions {
                            if matches!(
                                action,
                                SessionAction::SessionClosed | SessionAction::PeerStale
                            ) {
                                should_close = true;
                            }
                        }
                    }
                }
                if should_close {
                    self.registry.handle_presence(
                        &PresenceEvent::Left { address_hash: *addr },
                        now_secs_f64,
                    );
                    out.push(NetworkAction::PresenceChange(PresenceEvent::Left {
                        address_hash: *addr,
                    }));
                    self.unregister_peer_link(addr);
                    self.peers.remove(addr);
                }
                return;
            }
            if let Some(key_expr) = text.strip_prefix("SUB:") {
                // Peer declared subscriber interest — feed to our router and
                // process any response actions (e.g. resource declarations the
                // peer needs to resolve our ExprIds).
                let peer = match self.peers.get_mut(addr) {
                    Some(p) => p,
                    None => return,
                };
                if let (Some(router), Some(session)) =
                    (peer.router.as_mut(), peer.session.as_ref())
                {
                    if let Ok(actions) = router.handle_event(
                        PubSubEvent::SubscriberDeclared {
                            key_expr: key_expr.to_string(),
                        },
                        session,
                    ) {
                        let link =
                            peer.link.as_ref().filter(|l| l.state() == LinkState::Active);
                        if let Some(link) = link {
                            for action in &actions {
                                match action {
                                    PubSubAction::Session(
                                        SessionAction::SendResourceDeclare {
                                            expr_id,
                                            key_expr,
                                        },
                                    ) => {
                                        let msg = format!("RESDECL:{expr_id}:{key_expr}");
                                        Self::send_control(
                                            link,
                                            rng,
                                            msg.as_bytes(),
                                            out,
                                        );
                                    }
                                    PubSubAction::SendSubscriberDeclare { key_expr } => {
                                        let msg = format!("SUB:{key_expr}");
                                        Self::send_control(
                                            link,
                                            rng,
                                            msg.as_bytes(),
                                            out,
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                return;
            }
            if let Some(rest) = text.strip_prefix("RESDECL:") {
                // Peer declared a resource — feed to our session.
                // Format: "RESDECL:{expr_id}:{key_expr}" — split at first colon.
                // expr_id is always numeric, so key_expr (after the colon) may
                // safely contain colons itself (e.g. "harmony:v2/...").
                if let Some(colon_pos) = rest.find(':') {
                    if let Ok(expr_id) = rest[..colon_pos].parse::<ExprId>() {
                        let key_expr = &rest[colon_pos + 1..];
                        let peer = match self.peers.get_mut(addr) {
                            Some(p) => p,
                            None => return,
                        };
                        if let Some(session) = peer.session.as_mut() {
                            if let Ok(actions) = session.handle_event(
                                SessionEvent::ResourceDeclared {
                                    expr_id,
                                    key_expr: key_expr.to_string(),
                                },
                            ) {
                                let link = peer
                                    .link
                                    .as_ref()
                                    .filter(|l| l.state() == LinkState::Active);
                                if let Some(link) = link {
                                    for action in &actions {
                                        if let SessionAction::SendResourceDeclare {
                                            expr_id,
                                            key_expr,
                                        } = action
                                        {
                                            let msg =
                                                format!("RESDECL:{expr_id}:{key_expr}");
                                            Self::send_control(
                                                link,
                                                rng,
                                                msg.as_bytes(),
                                                out,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                return;
            }
            if let Some(rest) = text.strip_prefix("RESUNDECL:") {
                // Peer undeclared a resource — feed to our session.
                if let Ok(expr_id) = rest.parse::<ExprId>() {
                    let peer = match self.peers.get_mut(addr) {
                        Some(p) => p,
                        None => return,
                    };
                    if let Some(session) = peer.session.as_mut() {
                        if let Ok(actions) =
                            session.handle_event(SessionEvent::ResourceUndeclared { expr_id })
                        {
                            let link = peer
                                .link
                                .as_ref()
                                .filter(|l| l.state() == LinkState::Active);
                            if let Some(link) = link {
                                for action in &actions {
                                    if let SessionAction::SendResourceUndeclare { expr_id } =
                                        action
                                    {
                                        let msg = format!("RESUNDECL:{expr_id}");
                                        Self::send_control(link, rng, msg.as_bytes(), out);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Handle a binary PubSub data frame: [expr_id: 2 bytes BE][payload].
    fn handle_inbound_data_frame(
        &mut self,
        addr: &[u8; 16],
        body: &[u8],
        now_secs_f64: f64,
        rng: &mut impl CryptoRngCore,
        out: &mut Vec<NetworkAction>,
    ) {
        if body.len() < 2 {
            return;
        }
        let expr_id = u16::from_be_bytes([body[0], body[1]]) as ExprId;
        let payload = body[2..].to_vec();

        let peer = match self.peers.get_mut(addr) {
            Some(p) => p,
            None => return,
        };

        // Feed to router, get delivery actions.
        let deliver_actions = {
            let (router, session) = match (peer.router.as_mut(), peer.session.as_ref()) {
                (Some(r), Some(s)) => (r, s),
                _ => return,
            };
            match router.handle_event(
                PubSubEvent::MessageReceived {
                    expr_id,
                    payload,
                },
                session,
            ) {
                Ok(actions) => actions,
                Err(_) => return,
            }
        };

        // Process delivery actions.
        let link = match peer.link.as_ref() {
            Some(l) if l.state() == LinkState::Active => l,
            _ => return,
        };

        let mut peer_to_remove: Option<[u8; 16]> = None;
        for action in deliver_actions {
            match action {
                PubSubAction::Deliver {
                    key_expr: _, payload, ..
                } => {
                    // Deserialize the NetMessage and route it.
                    if let Ok(msg) = serde_json::from_slice::<NetMessage>(&payload) {
                        match msg {
                            NetMessage::PlayerState(state) => {
                                self.registry.update_state(addr, state, now_secs_f64);
                                out.push(NetworkAction::RemotePlayerUpdate {
                                    address_hash: *addr,
                                    state,
                                });
                            }
                            NetMessage::Chat(chat) => {
                                out.push(NetworkAction::ChatReceived(chat));
                            }
                            NetMessage::Presence(event) => {
                                // Validate: a peer may only announce their own
                                // presence changes. Ignore forged events.
                                let event_hash = match &event {
                                    PresenceEvent::Left { address_hash } => *address_hash,
                                    PresenceEvent::Joined { address_hash, .. } => *address_hash,
                                };
                                if &event_hash != addr {
                                    continue;
                                }
                                if let PresenceEvent::Left { .. } = &event {
                                    peer_to_remove = Some(*addr);
                                }
                                self.registry.handle_presence(&event, now_secs_f64);
                                out.push(NetworkAction::PresenceChange(event));
                            }
                        }
                    }
                }
                PubSubAction::SendSubscriberDeclare { key_expr } => {
                    let msg = format!("SUB:{key_expr}");
                    Self::send_control(link, rng, msg.as_bytes(), out);
                }
                PubSubAction::Session(SessionAction::SendResourceDeclare {
                    expr_id,
                    key_expr,
                }) => {
                    let msg = format!("RESDECL:{expr_id}:{key_expr}");
                    Self::send_control(link, rng, msg.as_bytes(), out);
                }
                _ => {}
            }
        }

        // Deferred peer teardown for Presence(Left) — couldn't do it inside
        // the loop because self.peers was borrowed via `peer`/`link`.
        if let Some(leave_addr) = peer_to_remove {
            self.unregister_peer_link(&leave_addr);
            self.peers.remove(&leave_addr);
        }
    }

    // ── Internal: Session ticking ────────────────────────────────────────

    /// Tick a single peer's session. Returns `true` if the peer should be
    /// removed (session closed or peer went stale).
    fn tick_peer_session(
        &mut self,
        addr: &[u8; 16],
        now_ms: u64,
        now_secs_f64: f64,
        rng: &mut impl CryptoRngCore,
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

        // Collect the link after releasing the session borrow.
        // `session_actions` is now owned; the mutable borrow on session is gone.
        // We can reborrow peer immutably to get the link.
        let peer = match self.peers.get(addr) {
            Some(p) => p,
            None => return false,
        };
        let link = peer.link.as_ref().filter(|l| l.state() == LinkState::Active);

        let mut should_remove = false;
        for action in session_actions {
            if should_remove {
                break; // Don't send keepalives/data to an evicted peer.
            }
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
                SessionAction::SendKeepalive => {
                    if let Some(link) = link {
                        Self::send_control(link, rng, b"KEEPALIVE", out);
                    }
                }
                SessionAction::SendClose => {
                    if let Some(link) = link {
                        Self::send_control(link, rng, b"CLOSE", out);
                    }
                }
                SessionAction::SendCloseAck => {
                    if let Some(link) = link {
                        Self::send_control(link, rng, b"CLOSEACK", out);
                    }
                }
                SessionAction::SendHandshake { proof } => {
                    // Send raw proof bytes with PacketContext::Channel,
                    // matching activate_peer_session's initial handshake send.
                    // The receiver passes raw decrypted bytes directly to
                    // SessionEvent::HandshakeReceived — no prefix stripping.
                    if let Some(link) = link {
                        Self::send_via_link(link, rng, &proof, PacketContext::Channel, out);
                    }
                }
                SessionAction::SendResourceDeclare { expr_id, key_expr } => {
                    if let Some(link) = link {
                        let msg = format!("RESDECL:{expr_id}:{key_expr}");
                        Self::send_control(link, rng, msg.as_bytes(), out);
                    }
                }
                SessionAction::SendResourceUndeclare { expr_id } => {
                    if let Some(link) = link {
                        let msg = format!("RESUNDECL:{expr_id}");
                        Self::send_control(link, rng, msg.as_bytes(), out);
                    }
                }
                SessionAction::SessionOpened
                | SessionAction::ResourceAdded { .. }
                | SessionAction::ResourceRemoved { .. } => {}
            }
        }
        should_remove
    }

    // ── Internal: Publishing ─────────────────────────────────────────────

    /// Publish a payload to all peers through a specific topic's publisher.
    ///
    /// `topic` selects which publisher to use. Each publisher is stored by
    /// name on PeerState (not positional), so declaration failures can't
    /// cause topic misrouting.
    /// For each peer with active router/session/link: calls
    /// `router.publish(pub_id, payload, &session)`, frames `SendMessage`
    /// actions as `[expr_id: 2 bytes BE][payload]`, encrypts via link,
    /// and emits `NetworkAction::SendPacket`.
    fn publish_to_all_peers(
        &mut self,
        payload: &[u8],
        topic: PubTopic,
        rng: &mut impl CryptoRngCore,
    ) -> Vec<NetworkAction> {
        let mut out = Vec::new();
        let peer_addrs: Vec<[u8; 16]> = self.peers.keys().copied().collect();

        for addr in peer_addrs {
            let peer = match self.peers.get(&addr) {
                Some(p) => p,
                None => continue,
            };

            // Need router, session, and active link.
            let (router, session, link) = match (
                peer.router.as_ref(),
                peer.session.as_ref(),
                peer.link.as_ref(),
            ) {
                (Some(r), Some(s), Some(l)) if s.state() == SessionState::Active && l.state() == LinkState::Active => {
                    (r, s, l)
                }
                _ => continue,
            };

            let pub_id = match topic {
                PubTopic::State => match peer.state_publisher_id {
                    Some(id) => id,
                    None => continue,
                },
                PubTopic::Chat => match peer.chat_publisher_id {
                    Some(id) => id,
                    None => continue,
                },
            };

            let actions = match router.publish(pub_id, payload.to_vec(), session) {
                Ok(a) => a,
                Err(_) => continue,
            };

            for action in actions {
                match action {
                    PubSubAction::SendMessage {
                        expr_id,
                        payload: msg_payload,
                    } => {
                        let expr_id_u16 = match u16::try_from(expr_id) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        Self::send_data_frame(link, rng, expr_id_u16, &msg_payload, &mut out);
                    }
                    PubSubAction::Session(SessionAction::SendResourceDeclare {
                        expr_id,
                        key_expr,
                    }) => {
                        let msg = format!("RESDECL:{expr_id}:{key_expr}");
                        Self::send_control(link, rng, msg.as_bytes(), &mut out);
                    }
                    PubSubAction::SendSubscriberDeclare { key_expr } => {
                        let msg = format!("SUB:{key_expr}");
                        Self::send_control(link, rng, msg.as_bytes(), &mut out);
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Truncate a string to at most `max_bytes` UTF-8 bytes, splitting on
/// character boundaries so the result is always valid UTF-8.
fn truncate_to_bytes(s: &str, max_bytes: usize) -> String {
    let mut result = String::new();
    for ch in s.chars() {
        if result.len() + ch.len_utf8() > max_bytes {
            break;
        }
        result.push(ch);
    }
    result
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
    // Strip NUL bytes from decoded values to be robust against peers
    // running pre-fix clients or sending non-sanitized app_data.
    if let Some(sep_pos) = data.iter().position(|&b| b == APP_DATA_SEPARATOR) {
        let name = String::from_utf8_lossy(&data[..sep_pos]).replace('\0', "");
        let street = String::from_utf8_lossy(&data[sep_pos + 1..]).replace('\0', "");
        (name, Some(street))
    } else {
        let name = String::from_utf8_lossy(data).replace('\0', "");
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
        state.change_street("heights", 100.0, &mut rng).unwrap();
        assert_eq!(state.registry.count(), 0);
    }

    #[test]
    fn change_street_updates_current() {
        let mut state = make_state();
        let mut rng = OsRng;

        assert!(state.current_street().is_none());

        state.change_street("meadow", 100.0, &mut rng).unwrap();
        assert_eq!(state.current_street(), Some("meadow"));

        state.change_street("heights", 200.0, &mut rng).unwrap();
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
        state.change_street("heights", 100.0, &mut rng).unwrap();

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
                state_publisher_id: None,
                chat_publisher_id: None,
                subscription_ids: Vec::new(),
            },
        );
        assert_eq!(state.peers.len(), 1);

        state.change_street("heights", 100.0, &mut rng).unwrap();
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
        let actions = state
            .set_display_name("NewName".to_string(), 10.0, &mut rng)
            .unwrap();
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

    #[test]
    fn announce_triggers_link_initiation_for_lower_hash() {
        use harmony_reticulum::ValidatedAnnounce;

        let id_a = make_identity();
        let id_b = make_identity();

        let pub_a = id_a.public_identity().clone();
        let pub_b = id_b.public_identity().clone();
        let (lower_id, higher_pub) = if pub_a.address_hash < pub_b.address_hash {
            (id_a, pub_b)
        } else {
            (id_b, pub_a)
        };

        let mut state = NetworkState::new(lower_id, "Lower".to_string());
        let mut rng = OsRng;
        state.change_street("meadow", 1.0, &mut rng).unwrap();

        let app_data = encode_app_data("Higher", Some("meadow"));
        let dest_name =
            DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();
        let destination_hash = dest_name.destination_hash(&higher_pub.address_hash);
        let announce = ValidatedAnnounce {
            identity: higher_pub.clone(),
            destination_name: dest_name.clone(),
            destination_hash,
            random_hash: [0u8; 10],
            ratchet: None,
            app_data,
        };

        let mut actions = Vec::new();
        state.handle_announce_received(&announce, 2, 2.0, &mut rng, &mut actions);

        assert!(state.peers.contains_key(&higher_pub.address_hash));
        let peer = state.peers.get(&higher_pub.address_hash).unwrap();
        assert!(peer.link.is_some());
        let link = peer.link.as_ref().unwrap();
        assert_eq!(link.state(), harmony_reticulum::LinkState::Pending);

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

        let mut state = NetworkState::new(higher_id, "Higher".to_string());
        let mut rng = OsRng;
        state.change_street("meadow", 1.0, &mut rng).unwrap();

        let app_data = encode_app_data("Lower", Some("meadow"));
        let dest_name =
            DestinationName::from_name("harmony", &["glitch", "player"]).unwrap();
        let destination_hash = dest_name.destination_hash(&lower_pub.address_hash);
        let announce = ValidatedAnnounce {
            identity: lower_pub.clone(),
            destination_name: dest_name.clone(),
            destination_hash,
            random_hash: [0u8; 10],
            ratchet: None,
            app_data,
        };

        let mut actions = Vec::new();
        state.handle_announce_received(&announce, 2, 2.0, &mut rng, &mut actions);

        assert!(state.peers.contains_key(&lower_pub.address_hash));
        let peer = state.peers.get(&lower_pub.address_hash).unwrap();
        assert!(peer.link.is_none(), "Higher hash should not initiate link");
    }

    // ── Helpers for handshake test ───────────────────────────────────

    /// Create two NetworkStates on the same street, with the lower-hash one first.
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

        let mut state_low = NetworkState::new(lower_id, "Lower".to_string());
        let mut state_high = NetworkState::new(higher_id, "Higher".to_string());

        state_low.change_street(street, 1.0, &mut rng).unwrap();
        state_high.change_street(street, 1.0, &mut rng).unwrap();

        (state_low, state_high)
    }

    /// Extract raw packet bytes from SendPacket actions.
    fn extract_packets(actions: &[NetworkAction]) -> Vec<Vec<u8>> {
        actions
            .iter()
            .filter_map(|a| match a {
                NetworkAction::SendPacket { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect()
    }

    /// Build a ValidatedAnnounce for a given NetworkState, suitable for feeding
    /// to another state's handle_announce_received.
    fn build_announce_for(state: &NetworkState) -> harmony_reticulum::ValidatedAnnounce {
        let dest_name =
            DestinationName::from_name(APP_NAME, DEST_ASPECTS).unwrap();
        let destination_hash =
            dest_name.destination_hash(&state.public_identity.address_hash);
        let app_data = encode_app_data(
            &state.display_name,
            state.current_street.as_deref(),
        );

        harmony_reticulum::ValidatedAnnounce {
            identity: state.public_identity.clone(),
            destination_name: dest_name,
            destination_hash,
            random_hash: [0u8; 10],
            ratchet: None,
            app_data,
        }
    }

    #[test]
    fn full_link_handshake_between_two_states() {
        let mut rng = OsRng;

        // 1. Create two NetworkStates: A (lower hash), B (higher hash), both on "meadow".
        let (mut state_a, mut state_b) = make_pair_on_street("meadow");

        // Verify the tiebreaker ordering.
        assert!(
            state_a.public_identity.address_hash < state_b.public_identity.address_hash,
            "state_a should have the lower hash"
        );

        // 2. Feed B's announce to A → A discovers B, initiates link, emits request packet.
        let announce_b = build_announce_for(&state_b);
        let mut actions_a = Vec::new();
        state_a.handle_announce_received(&announce_b, 2, 2.0, &mut rng, &mut actions_a);

        // A should have a peer for B with a Pending link.
        let peer_b_in_a = state_a
            .peers
            .get(&state_b.public_identity.address_hash)
            .expect("A should have B as peer");
        assert!(peer_b_in_a.link.is_some(), "A should have initiated a link to B");
        assert_eq!(
            peer_b_in_a.link.as_ref().unwrap().state(),
            LinkState::Pending
        );

        // A should have emitted a link request SendPacket.
        let request_packets = extract_packets(&actions_a);
        assert!(
            !request_packets.is_empty(),
            "A should emit a link request packet"
        );

        // 3. Feed A's announce to B (so B knows about A, but doesn't initiate — higher hash).
        let announce_a = build_announce_for(&state_a);
        let mut actions_b = Vec::new();
        state_b.handle_announce_received(&announce_a, 2, 2.0, &mut rng, &mut actions_b);

        let peer_a_in_b = state_b
            .peers
            .get(&state_a.public_identity.address_hash)
            .expect("B should have A as peer");
        assert!(
            peer_a_in_b.link.is_none(),
            "B (higher hash) should NOT initiate a link"
        );

        // 4. Feed A's request packet to B via tick → B responds with proof.
        let request_raw = &request_packets[0];
        let actions_b = state_b.tick(
            &[(INTERFACE_NAME.to_string(), request_raw.clone())],
            3.0,
            &mut rng,
        );

        // B's link should be in unmatched_links (Handshake state) — the responder
        // can't determine the initiator's identity from the request, so the link
        // is buffered until the Session handshake reveals identity.
        assert!(
            !state_b.unmatched_links.is_empty(),
            "B should have an unmatched link after receiving request"
        );
        let unmatched_link = state_b.unmatched_links.values().next().unwrap();
        assert_eq!(
            unmatched_link.state(),
            LinkState::Handshake,
            "B's unmatched link should be in Handshake state after responding"
        );

        // B should have emitted a proof packet.
        let proof_packets = extract_packets(&actions_b);
        assert!(
            !proof_packets.is_empty(),
            "B should emit a proof packet"
        );

        // 5. Feed B's proof to A via tick → A completes handshake, emits RTT.
        let proof_raw = &proof_packets[0];
        let actions_a = state_a.tick(
            &[(INTERFACE_NAME.to_string(), proof_raw.clone())],
            4.0,
            &mut rng,
        );

        // A's link should now be Active.
        let peer_b_in_a = state_a
            .peers
            .get(&state_b.public_identity.address_hash)
            .expect("A should still have B as peer");
        assert_eq!(
            peer_b_in_a.link.as_ref().unwrap().state(),
            LinkState::Active,
            "A's link should be Active after receiving proof"
        );

        // A should have emitted an RTT packet.
        let rtt_packets = extract_packets(&actions_a);
        assert!(
            !rtt_packets.is_empty(),
            "A should emit an RTT packet"
        );

        // 6. Feed A's RTT to B via tick → B activates link.
        let rtt_raw = &rtt_packets[0];
        let _actions_b = state_b.tick(
            &[(INTERFACE_NAME.to_string(), rtt_raw.clone())],
            5.0,
            &mut rng,
        );

        // 7. Both links should be Active. B's link should have moved from
        // unmatched_links to the peer entry after RTT resolution.
        assert!(
            state_b.unmatched_links.is_empty(),
            "unmatched_links should be empty after RTT resolves"
        );
        let peer_a_in_b = state_b
            .peers
            .get(&state_a.public_identity.address_hash)
            .expect("B should still have A as peer");
        assert_eq!(
            peer_a_in_b.link.as_ref().unwrap().state(),
            LinkState::Active,
            "B's link should be Active after receiving RTT"
        );

        let peer_b_in_a = state_a
            .peers
            .get(&state_b.public_identity.address_hash)
            .expect("A should still have B as peer");
        assert_eq!(
            peer_b_in_a.link.as_ref().unwrap().state(),
            LinkState::Active,
            "A's link should still be Active"
        );
    }

    #[test]
    fn session_activates_after_link_handshake() {
        let mut rng = OsRng;

        // 1. Create two NetworkStates on the same street.
        let (mut state_a, mut state_b) = make_pair_on_street("meadow");

        assert!(
            state_a.public_identity.address_hash < state_b.public_identity.address_hash,
            "state_a should have the lower hash"
        );

        let addr_a = state_a.public_identity.address_hash;
        let addr_b = state_b.public_identity.address_hash;

        // 2. Exchange announces so both know about each other.
        let announce_b = build_announce_for(&state_b);
        let mut actions_a = Vec::new();
        state_a.handle_announce_received(&announce_b, 2, 2.0, &mut rng, &mut actions_a);

        let announce_a = build_announce_for(&state_a);
        let mut actions_b = Vec::new();
        state_b.handle_announce_received(&announce_a, 2, 2.0, &mut rng, &mut actions_b);

        // 3. A emitted a link request (lower hash initiates).
        let request_packets = extract_packets(&actions_a);
        assert!(!request_packets.is_empty(), "A should emit a link request");

        // 4. Feed A's request to B → B responds with proof.
        let actions_b = state_b.tick(
            &[(INTERFACE_NAME.to_string(), request_packets[0].clone())],
            3.0,
            &mut rng,
        );
        let proof_packets = extract_packets(&actions_b);
        assert!(!proof_packets.is_empty(), "B should emit a proof packet");

        // 5. Feed B's proof to A → A completes handshake, emits RTT + session handshake.
        let actions_a = state_a.tick(
            &[(INTERFACE_NAME.to_string(), proof_packets[0].clone())],
            4.0,
            &mut rng,
        );
        let a_packets = extract_packets(&actions_a);
        // At least 2 packets: RTT + session handshake proof.
        assert!(
            a_packets.len() >= 2,
            "A should emit RTT + session handshake, got {} packets",
            a_packets.len()
        );

        // A should now have a session in Init state.
        let peer_b_in_a = state_a.peers.get(&addr_b).unwrap();
        assert!(
            peer_b_in_a.session.is_some(),
            "A should have created a session"
        );
        assert_eq!(
            peer_b_in_a.session.as_ref().unwrap().state(),
            SessionState::Init,
            "A's session should be in Init state"
        );

        // 6. Shuttle all of A's packets to B (RTT activates B's link, session handshake
        //    may arrive in the same or subsequent tick).
        let inbound_for_b: Vec<(String, Vec<u8>)> = a_packets
            .iter()
            .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
            .collect();
        let actions_b = state_b.tick(&inbound_for_b, 5.0, &mut rng);

        // B's link should now be Active and it should have created a session.
        let peer_a_in_b = state_b.peers.get(&addr_a).unwrap();
        assert_eq!(
            peer_a_in_b.link.as_ref().unwrap().state(),
            LinkState::Active,
            "B's link should be Active"
        );
        assert!(
            peer_a_in_b.session.is_some(),
            "B should have created a session"
        );

        // 7. Continue shuttling packets until both sessions are Active (up to 10 rounds).
        let mut b_packets = extract_packets(&actions_b);
        let mut round = 0;
        loop {
            round += 1;
            if round > 10 {
                panic!("Sessions did not both become Active within 10 rounds");
            }

            // Check if both sessions are Active.
            let a_session_active = state_a
                .peers
                .get(&addr_b)
                .and_then(|p| p.session.as_ref())
                .is_some_and(|s| s.state() == SessionState::Active);
            let b_session_active = state_b
                .peers
                .get(&addr_a)
                .and_then(|p| p.session.as_ref())
                .is_some_and(|s| s.state() == SessionState::Active);

            if a_session_active && b_session_active {
                break;
            }

            // Shuttle B's packets to A.
            if !b_packets.is_empty() {
                let inbound_for_a: Vec<(String, Vec<u8>)> = b_packets
                    .iter()
                    .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
                    .collect();
                let time_a = 5.0 + (round as f64) * 0.5;
                let actions_a = state_a.tick(&inbound_for_a, time_a, &mut rng);
                let a_new_packets = extract_packets(&actions_a);

                // Shuttle A's packets to B.
                if !a_new_packets.is_empty() {
                    let inbound_for_b: Vec<(String, Vec<u8>)> = a_new_packets
                        .iter()
                        .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
                        .collect();
                    let time_b = 5.0 + (round as f64) * 0.5 + 0.1;
                    let actions_b = state_b.tick(&inbound_for_b, time_b, &mut rng);
                    b_packets = extract_packets(&actions_b);
                } else {
                    b_packets = Vec::new();
                }
            } else {
                // No more packets to shuttle but sessions aren't both Active yet.
                // Run empty ticks to let timer events process.
                let time = 5.0 + (round as f64) * 0.5;
                let actions_a = state_a.tick(&[], time, &mut rng);
                let a_new = extract_packets(&actions_a);
                if !a_new.is_empty() {
                    let inbound: Vec<(String, Vec<u8>)> = a_new
                        .iter()
                        .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
                        .collect();
                    let actions_b = state_b.tick(&inbound, time + 0.1, &mut rng);
                    b_packets = extract_packets(&actions_b);
                }
            }
        }

        // 8. Assert both peers have active sessions.
        let peer_b_in_a = state_a.peers.get(&addr_b).unwrap();
        assert_eq!(
            peer_b_in_a.session.as_ref().unwrap().state(),
            SessionState::Active,
            "A's session should be Active"
        );

        let peer_a_in_b = state_b.peers.get(&addr_a).unwrap();
        assert_eq!(
            peer_a_in_b.session.as_ref().unwrap().state(),
            SessionState::Active,
            "B's session should be Active"
        );
    }

    /// Drive two NetworkStates through the full handshake + session + pubsub
    /// setup, then shuttle all pending packets between them until both have
    /// routers AND all SUB/RESDECL control messages have been exchanged.
    /// Returns the pair and their address hashes.
    fn drive_to_pubsub_ready(
        street: &str,
    ) -> (NetworkState, NetworkState, [u8; 16], [u8; 16]) {
        let mut rng = OsRng;

        let (mut state_a, mut state_b) = make_pair_on_street(street);
        let addr_a = state_a.public_identity.address_hash;
        let addr_b = state_b.public_identity.address_hash;

        // Exchange announces.
        let announce_b = build_announce_for(&state_b);
        let mut actions_a = Vec::new();
        state_a.handle_announce_received(&announce_b, 2, 2.0, &mut rng, &mut actions_a);

        let announce_a = build_announce_for(&state_a);
        let mut actions_b = Vec::new();
        state_b.handle_announce_received(&announce_a, 2, 2.0, &mut rng, &mut actions_b);

        // Shuttle A's link request to B.
        let request_packets = extract_packets(&actions_a);
        assert!(!request_packets.is_empty(), "A should emit a link request");

        let actions_b = state_b.tick(
            &[(INTERFACE_NAME.to_string(), request_packets[0].clone())],
            3.0,
            &mut rng,
        );
        let proof_packets = extract_packets(&actions_b);

        // Shuttle B's proof to A.
        let actions_a = state_a.tick(
            &[(INTERFACE_NAME.to_string(), proof_packets[0].clone())],
            4.0,
            &mut rng,
        );
        let a_packets = extract_packets(&actions_a);

        // Shuttle A's RTT + session handshake to B.
        let inbound_for_b: Vec<(String, Vec<u8>)> = a_packets
            .iter()
            .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
            .collect();
        let actions_b = state_b.tick(&inbound_for_b, 5.0, &mut rng);
        let mut b_packets = extract_packets(&actions_b);

        // Continue shuttling until both have Active sessions, routers,
        // and all control packets (SUB/RESDECL) have been exchanged.
        // We keep going a few rounds after routers are ready to ensure
        // the subscriber declarations reach the other side.
        let mut settled_rounds = 0;
        for round in 1..=20 {
            let a_has_router = state_a
                .peers
                .get(&addr_b)
                .and_then(|p| p.router.as_ref())
                .is_some();
            let b_has_router = state_b
                .peers
                .get(&addr_a)
                .and_then(|p| p.router.as_ref())
                .is_some();

            if a_has_router && b_has_router {
                // Keep shuttling until no more packets in flight.
                if b_packets.is_empty() {
                    settled_rounds += 1;
                    if settled_rounds >= 2 {
                        break;
                    }
                } else {
                    settled_rounds = 0;
                }
            }

            if round == 20 {
                panic!(
                    "PubSub not ready after 20 rounds (A router: {a_has_router}, B router: {b_has_router})"
                );
            }

            // Use tiny time increments (0.01s) to stay well within the
            // 10s stale timeout window (join was at t=2.0).
            if !b_packets.is_empty() {
                let inbound_for_a: Vec<(String, Vec<u8>)> = b_packets
                    .iter()
                    .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
                    .collect();
                let time_a = 5.0 + (round as f64) * 0.01;
                let actions_a = state_a.tick(&inbound_for_a, time_a, &mut rng);
                let a_new_packets = extract_packets(&actions_a);

                if !a_new_packets.is_empty() {
                    let inbound_for_b: Vec<(String, Vec<u8>)> = a_new_packets
                        .iter()
                        .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
                        .collect();
                    let time_b = time_a + 0.005;
                    let actions_b = state_b.tick(&inbound_for_b, time_b, &mut rng);
                    b_packets = extract_packets(&actions_b);
                } else {
                    b_packets = Vec::new();
                }
            } else {
                let time = 5.0 + (round as f64) * 0.01;
                let actions_a = state_a.tick(&[], time, &mut rng);
                let a_new = extract_packets(&actions_a);
                if !a_new.is_empty() {
                    let inbound: Vec<(String, Vec<u8>)> = a_new
                        .iter()
                        .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
                        .collect();
                    let actions_b = state_b.tick(&inbound, time + 0.005, &mut rng);
                    b_packets = extract_packets(&actions_b);
                } else {
                    b_packets = Vec::new();
                }
            }
        }

        (state_a, state_b, addr_a, addr_b)
    }

    #[test]
    fn publish_player_state_round_trip() {
        let mut rng = OsRng;

        // 1. Drive both states through full handshake + session + router setup.
        let (mut state_a, mut state_b, addr_a, addr_b) =
            drive_to_pubsub_ready("meadow");

        // Verify routers are set up.
        assert!(
            state_a.peers.get(&addr_b).unwrap().router.is_some(),
            "A should have a PubSubRouter for B"
        );
        assert!(
            state_b.peers.get(&addr_a).unwrap().router.is_some(),
            "B should have a PubSubRouter for A"
        );

        // 2. A publishes player state.
        let player_state = PlayerNetState {
            x: 100.0,
            y: -50.0,
            vx: 10.0,
            vy: -5.0,
            facing: 1,
            on_ground: true,
        };

        let publish_actions =
            state_a.publish_player_state(&player_state, &mut rng);

        // 3. Extract packets from A and feed them to B.
        let a_packets = extract_packets(&publish_actions);
        assert!(
            !a_packets.is_empty(),
            "A should emit data packets when publishing player state"
        );

        // Use a timestamp close to setup time (not 20s later, which would
        // trigger stale purge on the registry — stale timeout is 10s).
        let inbound_for_b: Vec<(String, Vec<u8>)> = a_packets
            .iter()
            .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
            .collect();
        let _actions_b = state_b.tick(&inbound_for_b, 8.0, &mut rng);

        // 4. B should have A's position in its remote frames.
        let frames = state_b.remote_frames();
        assert!(
            !frames.is_empty(),
            "B should have at least one remote player frame"
        );

        let a_hex = hex::encode(addr_a);
        let a_frame = frames
            .iter()
            .find(|f| f.address_hash == a_hex)
            .expect("B should have A's remote player frame");

        assert!(
            (a_frame.x - 100.0).abs() < f64::EPSILON,
            "A's x should be 100.0, got {}",
            a_frame.x
        );
        assert!(
            (a_frame.y - -50.0).abs() < f64::EPSILON,
            "A's y should be -50.0, got {}",
            a_frame.y
        );
    }

    #[test]
    fn chat_message_network_round_trip() {
        let mut rng = OsRng;

        // 1. Drive both states to pubsub-ready.
        let (mut state_a, mut state_b, _addr_a, _addr_b) = drive_to_pubsub_ready("meadow");

        // 2. A sends a chat message — first action should be local echo.
        let chat_actions = state_a.send_chat("Hello Bob!".to_string(), &mut rng);

        // First action is the local echo ChatReceived.
        assert!(
            !chat_actions.is_empty(),
            "send_chat should return at least one action (local echo)"
        );
        match &chat_actions[0] {
            NetworkAction::ChatReceived(msg) => {
                assert_eq!(msg.text, "Hello Bob!", "local echo text mismatch");
                assert_eq!(msg.sender_name, "Lower", "local echo sender_name mismatch");
            }
            other => panic!("expected ChatReceived as first action, got {other:?}"),
        }

        // 3. Feed the SendPacket actions from A into B.
        let a_packets = extract_packets(&chat_actions);
        assert!(
            !a_packets.is_empty(),
            "send_chat should emit at least one SendPacket to peers"
        );

        let inbound_for_b: Vec<(String, Vec<u8>)> = a_packets
            .iter()
            .map(|p| (INTERFACE_NAME.to_string(), p.clone()))
            .collect();
        let b_actions = state_b.tick(&inbound_for_b, 8.0, &mut rng);

        // 4. B should receive a ChatReceived with the correct text and sender name.
        let chat_received: Vec<_> = b_actions
            .iter()
            .filter_map(|a| match a {
                NetworkAction::ChatReceived(msg) => Some(msg),
                _ => None,
            })
            .collect();

        assert!(
            !chat_received.is_empty(),
            "B should receive a ChatReceived action"
        );
        assert_eq!(
            chat_received[0].text, "Hello Bob!",
            "received chat text mismatch"
        );
        assert_eq!(
            chat_received[0].sender_name, "Lower",
            "received chat sender_name mismatch"
        );
    }

    #[test]
    fn street_change_tears_down_peers() {
        let mut rng = OsRng;

        // 1. Drive both states to pubsub-ready so A has B as an active peer.
        let (mut state_a, _state_b, _addr_a, addr_b) = drive_to_pubsub_ready("meadow");

        // 2. Verify A has B as a peer before the street change.
        assert!(
            state_a.peers.contains_key(&addr_b),
            "A should have B as a peer after pubsub handshake"
        );

        // 3. A changes street.
        state_a.change_street("heights", 10.0, &mut rng).unwrap();

        // 4. Assert teardown: peers cleared, registry empty, current street updated.
        assert!(
            state_a.peers.is_empty(),
            "peers should be cleared after change_street"
        );
        assert_eq!(
            state_a.registry.count(),
            0,
            "registry should be empty after change_street"
        );
        assert_eq!(
            state_a.current_street(),
            Some("heights"),
            "current_street should be updated to the new street"
        );
    }

    #[test]
    fn tick_peer_session_sends_keepalive() {
        let mut rng = OsRng;

        // 1. Drive both states through full handshake + session + router setup.
        // drive_to_pubsub_ready returns (state_a, state_b, addr_a, addr_b).
        let (mut state_a, _state_b, _addr_a, addr_b) = drive_to_pubsub_ready("meadow");

        // Verify A has an active session with B.
        let session_state = state_a
            .peers
            .get(&addr_b)
            .and_then(|p| p.session.as_ref())
            .map(|s| s.state());
        assert_eq!(
            session_state,
            Some(SessionState::Active),
            "A should have an active session with B before keepalive test"
        );

        // 2. Advance time past the default keepalive interval (30_000 ms = 30 s).
        //    drive_to_pubsub_ready ends around t=5.0s. Tick at t=35.0s (35_000 ms)
        //    so that now_ms - last_sent_ms >= 30_000.
        let actions = state_a.tick(&[], 35.0, &mut rng);

        // 3. Assert that at least one SendPacket action was emitted (the keepalive).
        let send_packets: Vec<_> = actions
            .iter()
            .filter(|a| matches!(a, NetworkAction::SendPacket { .. }))
            .collect();
        assert!(
            !send_packets.is_empty(),
            "tick() at t=35s should emit at least one SendPacket (keepalive) but got: {:?}",
            actions
        );
    }
}
