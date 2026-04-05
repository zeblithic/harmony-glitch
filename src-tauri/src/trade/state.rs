use crate::item::inventory::Inventory;
use crate::item::types::ItemDefs;
use crate::trade::types::*;

/// Timeout after 60 seconds of inactivity.
const TRADE_TIMEOUT_SECS: f64 = 60.0;

/// Maximum distinct item types per offer (MTU constraint).
const MAX_OFFER_ITEMS: usize = 8;

/// Manages at most one active trade session at a time.
#[derive(Debug)]
pub struct TradeManager {
    /// Currently active trade (negotiating, locking, or executing).
    active_trade: Option<TradeSession>,
    /// Pending inbound request waiting for the player to accept or decline.
    pending_request: Option<TradeSession>,
    /// Our address hash (set once at startup).
    our_hash: [u8; 16],
}

impl TradeManager {
    pub fn new(our_hash: [u8; 16]) -> Self {
        Self {
            active_trade: None,
            pending_request: None,
            our_hash,
        }
    }

    // ── Queries ──────────────────────────────────────────────────────────

    pub fn has_active_trade(&self) -> bool {
        self.active_trade.is_some()
    }

    pub fn is_executing(&self) -> bool {
        self.active_trade
            .as_ref()
            .is_some_and(|t| t.phase == TradePhase::Executing)
    }

    pub fn pending_request(&self) -> Option<&TradeSession> {
        self.pending_request.as_ref()
    }

    pub fn is_trading_with(&self, peer_hash: &[u8; 16]) -> bool {
        self.active_trade
            .as_ref()
            .is_some_and(|t| &t.peer_hash == peer_hash)
            || self
                .pending_request
                .as_ref()
                .is_some_and(|t| &t.peer_hash == peer_hash)
    }

    /// Cancel only the trade/request involving a specific peer.
    /// Returns a Cancel message if the active trade was with that peer.
    pub fn cancel_trade_with_peer(&mut self, peer_hash: &[u8; 16]) -> Option<TradeMessage> {
        if self
            .active_trade
            .as_ref()
            .is_some_and(|s| &s.peer_hash == peer_hash)
        {
            return self.active_trade.take().map(|s| TradeMessage::Cancel {
                trade_id: s.trade_id,
                sender: self.our_hash,
            });
        }
        if self
            .pending_request
            .as_ref()
            .is_some_and(|s| &s.peer_hash == peer_hash)
        {
            self.pending_request = None;
        }
        None
    }

    /// Build a TradeFrame for the frontend, enriching items with names/icons.
    pub fn trade_frame(&self, item_defs: &ItemDefs) -> Option<TradeFrame> {
        let session = self.active_trade.as_ref()?;
        Some(TradeFrame {
            trade_id: session.trade_id,
            phase: match session.phase {
                TradePhase::PendingResponse => "pending".into(),
                TradePhase::Negotiating => "negotiating".into(),
                TradePhase::LockedLocal => "lockedLocal".into(),
                TradePhase::LockedRemote => "lockedRemote".into(),
                TradePhase::Executing => "executing".into(),
                TradePhase::Completed => "completed".into(),
                TradePhase::Cancelled => "cancelled".into(),
            },
            peer_name: session.peer_name.clone(),
            local_offer: offer_to_frame(&session.local_offer, item_defs),
            remote_offer: offer_to_frame(&session.remote_offer, item_defs),
            local_locked: session.local_terms_hash.is_some(),
            remote_locked: session.remote_terms_hash.is_some(),
        })
    }

    // ── Initiator flow ──────────────────────────────────────────────────

    pub fn initiate_trade(
        &mut self,
        trade_id: TradeId,
        peer_hash: [u8; 16],
        peer_name: String,
        now: f64,
    ) -> Result<TradeMessage, String> {
        if self.active_trade.is_some() {
            return Err("Already in a trade".into());
        }
        let session = TradeSession {
            trade_id,
            phase: TradePhase::PendingResponse,
            role: TradeRole::Initiator,
            our_hash: self.our_hash,
            peer_hash,
            peer_name,
            local_offer: TradeOffer::empty(),
            remote_offer: TradeOffer::empty(),
            local_terms_hash: None,
            remote_terms_hash: None,
            last_activity: now,
        };
        self.active_trade = Some(session);
        Ok(TradeMessage::Request {
            trade_id,
            initiator: self.our_hash,
            recipient: peer_hash,
        })
    }

    // ── Responder flow ──────────────────────────────────────────────────

    pub fn receive_request(
        &mut self,
        trade_id: TradeId,
        initiator: [u8; 16],
        initiator_name: String,
        now: f64,
    ) -> Result<(), String> {
        if self.active_trade.is_some() {
            return Err("Already in a trade".into());
        }
        // Replace any existing pending request.
        self.pending_request = Some(TradeSession {
            trade_id,
            phase: TradePhase::PendingResponse,
            role: TradeRole::Responder,
            our_hash: self.our_hash,
            peer_hash: initiator,
            peer_name: initiator_name,
            local_offer: TradeOffer::empty(),
            remote_offer: TradeOffer::empty(),
            local_terms_hash: None,
            remote_terms_hash: None,
            last_activity: now,
        });
        Ok(())
    }

    pub fn accept_trade(&mut self, now: f64) -> Result<TradeMessage, String> {
        let session = self
            .pending_request
            .take()
            .ok_or("No pending trade request")?;
        if self.active_trade.is_some() {
            return Err("Already in a trade".into());
        }
        let trade_id = session.trade_id;
        self.active_trade = Some(TradeSession {
            phase: TradePhase::Negotiating,
            last_activity: now,
            ..session
        });
        Ok(TradeMessage::Accept {
            trade_id,
            responder: self.our_hash,
        })
    }

    pub fn decline_trade(&mut self) -> Result<TradeMessage, String> {
        let session = self
            .pending_request
            .take()
            .ok_or("No pending trade request")?;
        Ok(TradeMessage::Decline {
            trade_id: session.trade_id,
            responder: self.our_hash,
        })
    }

    /// Called when the peer accepts our trade request.
    pub fn receive_accept(
        &mut self,
        trade_id: TradeId,
        now: f64,
    ) -> Result<(), String> {
        let session = self
            .active_trade
            .as_mut()
            .ok_or("No active trade")?;
        if session.trade_id != trade_id {
            return Err("Trade ID mismatch".into());
        }
        if session.phase != TradePhase::PendingResponse {
            return Err("Trade not in pending state".into());
        }
        session.phase = TradePhase::Negotiating;
        session.last_activity = now;
        Ok(())
    }

    /// Called when the peer declines our trade request.
    pub fn receive_decline(&mut self, trade_id: TradeId) -> Result<(), String> {
        let session = self
            .active_trade
            .as_ref()
            .ok_or("No active trade")?;
        if session.trade_id != trade_id {
            return Err("Trade ID mismatch".into());
        }
        if session.phase != TradePhase::PendingResponse {
            return Err("Trade not in pending state".into());
        }
        self.active_trade = None;
        Ok(())
    }

    // ── Negotiation ─────────────────────────────────────────────────────

    pub fn update_offer(
        &mut self,
        offer: TradeOffer,
        now: f64,
    ) -> Result<TradeMessage, String> {
        let session = self
            .active_trade
            .as_mut()
            .ok_or("No active trade")?;
        if session.phase != TradePhase::Negotiating
            && session.phase != TradePhase::LockedRemote
            && session.phase != TradePhase::LockedLocal
        {
            return Err("Cannot update offer in current phase".into());
        }
        if offer.items.len() > MAX_OFFER_ITEMS {
            return Err(format!(
                "Too many item types (max {})",
                MAX_OFFER_ITEMS
            ));
        }
        // Changing our offer invalidates both locks — the remote peer's
        // lock was computed against the old terms and is now stale.
        session.local_terms_hash = None;
        session.remote_terms_hash = None;
        if session.phase == TradePhase::LockedLocal
            || session.phase == TradePhase::LockedRemote
        {
            session.phase = TradePhase::Negotiating;
        }
        session.local_offer = offer.clone();
        session.last_activity = now;
        Ok(TradeMessage::Update {
            trade_id: session.trade_id,
            sender: self.our_hash,
            offer,
        })
    }

    pub fn receive_remote_update(
        &mut self,
        trade_id: TradeId,
        offer: TradeOffer,
        now: f64,
    ) -> Result<(), String> {
        let session = self
            .active_trade
            .as_mut()
            .ok_or("No active trade")?;
        if session.trade_id != trade_id {
            return Err("Trade ID mismatch".into());
        }
        match session.phase {
            TradePhase::Negotiating
            | TradePhase::LockedLocal
            | TradePhase::LockedRemote => {}
            _ => return Ok(()), // ignore stale/reordered updates
        }
        // Remote updating their offer invalidates both locks.
        session.remote_terms_hash = None;
        session.local_terms_hash = None;
        if session.phase == TradePhase::LockedLocal
            || session.phase == TradePhase::LockedRemote
        {
            session.phase = TradePhase::Negotiating;
        }
        session.remote_offer = offer;
        session.last_activity = now;
        Ok(())
    }

    // ── Locking ─────────────────────────────────────────────────────────

    /// Lock our side of the trade. Pre-validates that we can fulfill
    /// the offer before locking, minimizing the execution failure window.
    pub fn lock_trade(
        &mut self,
        inventory: &Inventory,
        currants: u64,
        now: f64,
    ) -> Result<TradeMessage, String> {
        let session = self
            .active_trade
            .as_ref()
            .ok_or("No active trade")?;
        match session.phase {
            TradePhase::Negotiating | TradePhase::LockedRemote => {}
            _ => return Err("Cannot lock in current phase".into()),
        }
        // Pre-validate: refuse to lock if we can't fulfill the offer.
        for item in &session.local_offer.items {
            let have = inventory.count_item(&item.item_id);
            if have < item.count {
                return Err(format!(
                    "Cannot lock: insufficient {} (need {}, have {})",
                    item.item_id, item.count, have
                ));
            }
        }
        if currants < session.local_offer.currants {
            return Err(format!(
                "Cannot lock: insufficient currants (need {}, have {})",
                session.local_offer.currants, currants
            ));
        }
        let session = self.active_trade.as_mut().unwrap();
        let terms_hash = compute_terms_hash(
            &session.local_offer,
            &session.remote_offer,
            &session.our_hash,
            &session.peer_hash,
        );
        session.local_terms_hash = Some(terms_hash);
        session.last_activity = now;

        // Check if both are now locked with matching hashes.
        if let Some(remote_hash) = session.remote_terms_hash {
            if remote_hash == terms_hash {
                session.phase = TradePhase::Executing;
            } else {
                // Hash mismatch — peer locked with different terms.
                // Stay in LockedLocal; peer needs to re-lock after seeing our offer.
                session.phase = TradePhase::LockedLocal;
            }
        } else {
            session.phase = TradePhase::LockedLocal;
        }

        Ok(TradeMessage::Lock {
            trade_id: session.trade_id,
            sender: self.our_hash,
            terms_hash,
        })
    }

    /// Unlock our side of the trade.
    pub fn unlock_trade(&mut self, now: f64) -> Result<TradeMessage, String> {
        let session = self
            .active_trade
            .as_mut()
            .ok_or("No active trade")?;
        if session.phase != TradePhase::LockedLocal {
            return Err("Not locked".into());
        }
        session.local_terms_hash = None;
        session.phase = if session.remote_terms_hash.is_some() {
            TradePhase::LockedRemote
        } else {
            TradePhase::Negotiating
        };
        session.last_activity = now;
        Ok(TradeMessage::Unlock {
            trade_id: session.trade_id,
            sender: self.our_hash,
        })
    }

    /// Remote peer locked their side.
    /// Returns true if both are now locked with matching hashes (ready to execute).
    pub fn receive_remote_lock(
        &mut self,
        trade_id: TradeId,
        terms_hash: [u8; 16],
        now: f64,
    ) -> Result<bool, String> {
        let session = self
            .active_trade
            .as_mut()
            .ok_or("No active trade")?;
        if session.trade_id != trade_id {
            return Err("Trade ID mismatch".into());
        }
        match session.phase {
            TradePhase::Negotiating
            | TradePhase::LockedLocal
            | TradePhase::LockedRemote => {}
            _ => return Ok(false), // ignore lock in wrong phase (e.g., UDP reorder before Accept)
        }
        session.remote_terms_hash = Some(terms_hash);
        session.last_activity = now;

        if let Some(local_hash) = session.local_terms_hash {
            if local_hash == terms_hash {
                session.phase = TradePhase::Executing;
                return Ok(true);
            }
        }
        // We haven't locked yet, or hashes don't match.
        if session.local_terms_hash.is_none() {
            session.phase = TradePhase::LockedRemote;
        }
        Ok(false)
    }

    /// Remote peer unlocked their side.
    pub fn receive_remote_unlock(
        &mut self,
        trade_id: TradeId,
        now: f64,
    ) -> Result<(), String> {
        let session = self
            .active_trade
            .as_mut()
            .ok_or("No active trade")?;
        if session.trade_id != trade_id {
            return Err("Trade ID mismatch".into());
        }
        session.remote_terms_hash = None;
        if session.phase == TradePhase::LockedRemote {
            session.phase = TradePhase::Negotiating;
        } else if session.phase == TradePhase::Executing {
            // Peer unlocked after we both locked — go back to our-side-locked.
            session.phase = TradePhase::LockedLocal;
        }
        session.last_activity = now;
        Ok(())
    }

    // ── Execution ───────────────────────────────────────────────────────

    /// Execute the trade: validate inventory + currants, then mutate.
    /// Returns the Complete message to send on success.
    pub fn execute_trade(
        &mut self,
        inventory: &mut Inventory,
        currants: &mut u64,
        item_defs: &ItemDefs,
    ) -> Result<TradeMessage, String> {
        let session = self
            .active_trade
            .as_ref()
            .ok_or("No active trade")?;
        if session.phase != TradePhase::Executing {
            return Err("Trade not ready for execution".into());
        }

        // 1. Validate we have all offered items.
        for item in &session.local_offer.items {
            let have = inventory.count_item(&item.item_id);
            if have < item.count {
                return Err(format!(
                    "Insufficient {}: need {}, have {}",
                    item.item_id, item.count, have
                ));
            }
        }
        // 2. Validate we have enough currants.
        if *currants < session.local_offer.currants {
            return Err(format!(
                "Insufficient currants: need {}, have {}",
                session.local_offer.currants, *currants
            ));
        }
        // 3. Validate room for incoming items by simulating the full
        // remove-then-add sequence on a clone. This correctly handles
        // cross-item slot contention (multiple item types competing for
        // the same empty slots).
        {
            let mut sim = inventory.clone();
            for item in &session.local_offer.items {
                sim.remove_item(&item.item_id, item.count);
            }
            for item in &session.remote_offer.items {
                let overflow = sim.add(&item.item_id, item.count, item_defs);
                if overflow > 0 {
                    return Err(format!("No room for {}", item.item_id));
                }
            }
        }

        // 4. Execute atomically: remove offered, add received.
        for item in &session.local_offer.items {
            inventory.remove_item(&item.item_id, item.count);
        }
        *currants -= session.local_offer.currants;

        for item in &session.remote_offer.items {
            let overflow = inventory.add(&item.item_id, item.count, item_defs);
            if overflow > 0 {
                eprintln!(
                    "[trade] BUG: overflow of {} adding {} (room check passed)",
                    overflow, item.item_id
                );
            }
        }
        *currants += session.remote_offer.currants;

        let trade_id = session.trade_id;
        // Mark completed.
        self.active_trade.as_mut().unwrap().phase = TradePhase::Completed;
        let msg = TradeMessage::Complete {
            trade_id,
            sender: self.our_hash,
        };
        self.active_trade = None;
        Ok(msg)
    }

    // ── Cancellation ────────────────────────────────────────────────────

    /// Cancel the active trade. Returns Cancel message to send (if there was a trade).
    pub fn cancel_trade(&mut self) -> Option<TradeMessage> {
        if let Some(session) = self.active_trade.take() {
            Some(TradeMessage::Cancel {
                trade_id: session.trade_id,
                sender: self.our_hash,
            })
        } else {
            self.pending_request = None;
            None
        }
    }

    /// Remote peer cancelled the trade.
    pub fn receive_cancel(&mut self, trade_id: TradeId) -> Result<(), String> {
        if let Some(ref session) = self.active_trade {
            if session.trade_id == trade_id {
                self.active_trade = None;
                return Ok(());
            }
        }
        if let Some(ref session) = self.pending_request {
            if session.trade_id == trade_id {
                self.pending_request = None;
                return Ok(());
            }
        }
        Err("No matching trade to cancel".into())
    }

    /// Remote peer completed the trade (courtesy message — we already executed).
    pub fn receive_complete(&mut self, trade_id: TradeId) -> Result<(), String> {
        // The active trade should already be cleared by execute_trade.
        // If we see a Complete for a trade_id we don't recognize, ignore it.
        if let Some(ref session) = self.active_trade {
            if session.trade_id == trade_id {
                self.active_trade = None;
            }
        }
        Ok(())
    }

    // ── Tick ────────────────────────────────────────────────────────────

    /// Check for timeout. Returns Cancel message if the trade timed out.
    pub fn tick(&mut self, now: f64) -> Option<TradeMessage> {
        if let Some(ref session) = self.active_trade {
            if now - session.last_activity >= TRADE_TIMEOUT_SECS {
                return self.cancel_trade();
            }
        }
        if let Some(ref session) = self.pending_request {
            if now - session.last_activity >= TRADE_TIMEOUT_SECS {
                self.pending_request = None;
            }
        }
        None
    }
}

fn offer_to_frame(offer: &TradeOffer, item_defs: &ItemDefs) -> TradeOfferFrame {
    TradeOfferFrame {
        items: offer
            .items
            .iter()
            .map(|item| {
                let def = item_defs.get(&item.item_id);
                TradeItemFrame {
                    item_id: item.item_id.clone(),
                    name: def
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| item.item_id.clone()),
                    icon: def
                        .map(|d| d.icon.clone())
                        .unwrap_or_default(),
                    count: item.count,
                }
            })
            .collect(),
        currants: offer.currants,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::{ItemDef, ItemStack};
    use std::collections::HashMap;

    const ALICE: [u8; 16] = [0x01; 16];
    const BOB: [u8; 16] = [0x02; 16];

    fn make_item_defs() -> ItemDefs {
        let mut defs = HashMap::new();
        defs.insert(
            "cherry".to_string(),
            ItemDef {
                id: "cherry".into(),
                name: "Cherry".into(),
                description: "A tasty cherry".into(),
                category: "food".into(),
                stack_limit: 5,
                icon: "cherry_icon".into(),
                base_cost: Some(2),
            },
        );
        defs.insert(
            "grain".to_string(),
            ItemDef {
                id: "grain".into(),
                name: "Grain".into(),
                description: "A handful of grain".into(),
                category: "material".into(),
                stack_limit: 10,
                icon: "grain_icon".into(),
                base_cost: Some(1),
            },
        );
        defs
    }

    fn make_inventory(items: &[(&str, u32)]) -> Inventory {
        let mut inv = Inventory {
            slots: vec![None; 16],
            capacity: 16,
        };
        for (item_id, count) in items {
            inv.add(item_id, *count, &make_item_defs());
        }
        inv
    }

    // ── Happy path ──────────────────────────────────────────────────────

    #[test]
    fn happy_path_initiate_accept_negotiate_lock_execute() {
        let defs = make_item_defs();
        let mut alice_mgr = TradeManager::new(ALICE);
        let mut bob_mgr = TradeManager::new(BOB);

        // Alice initiates trade.
        let request = alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        assert!(alice_mgr.has_active_trade());

        // Bob receives request.
        if let TradeMessage::Request {
            trade_id,
            initiator,
            ..
        } = &request
        {
            bob_mgr
                .receive_request(*trade_id, *initiator, "Alice".into(), 0.0)
                .unwrap();
        }
        assert!(bob_mgr.pending_request().is_some());

        // Bob accepts.
        let accept = bob_mgr.accept_trade(1.0).unwrap();
        assert!(bob_mgr.has_active_trade());

        // Alice receives accept.
        if let TradeMessage::Accept { trade_id, .. } = &accept {
            alice_mgr.receive_accept(*trade_id, 1.0).unwrap();
        }

        // Alice offers 5 cherries.
        let alice_offer = TradeOffer {
            items: vec![ItemStack {
                item_id: "cherry".into(),
                count: 5,
            }],
            currants: 0,
        };
        let update = alice_mgr.update_offer(alice_offer, 2.0).unwrap();
        if let TradeMessage::Update {
            trade_id, offer, ..
        } = &update
        {
            bob_mgr
                .receive_remote_update(*trade_id, offer.clone(), 2.0)
                .unwrap();
        }

        // Bob offers 3 grain.
        let bob_offer = TradeOffer {
            items: vec![ItemStack {
                item_id: "grain".into(),
                count: 3,
            }],
            currants: 0,
        };
        let update = bob_mgr.update_offer(bob_offer, 3.0).unwrap();
        if let TradeMessage::Update {
            trade_id, offer, ..
        } = &update
        {
            alice_mgr
                .receive_remote_update(*trade_id, offer.clone(), 3.0)
                .unwrap();
        }

        // Alice locks (pre-validates she has the cherries).
        let alice_inv = make_inventory(&[("cherry", 5)]);
        let lock_msg = alice_mgr.lock_trade(&alice_inv, 100, 4.0).unwrap();
        if let TradeMessage::Lock {
            trade_id,
            terms_hash,
            ..
        } = &lock_msg
        {
            let both_locked = bob_mgr
                .receive_remote_lock(*trade_id, *terms_hash, 4.0)
                .unwrap();
            assert!(!both_locked);
        }

        // Bob locks (pre-validates he has the grain).
        let bob_inv = make_inventory(&[("grain", 3)]);
        let lock_msg = bob_mgr.lock_trade(&bob_inv, 50, 5.0).unwrap();
        if let TradeMessage::Lock {
            trade_id,
            terms_hash,
            ..
        } = &lock_msg
        {
            let both_locked = alice_mgr
                .receive_remote_lock(*trade_id, *terms_hash, 5.0)
                .unwrap();
            assert!(both_locked);
        }

        // Both execute.
        let mut alice_inv = make_inventory(&[("cherry", 5)]);
        let mut alice_currants: u64 = 100;
        let complete_msg = alice_mgr
            .execute_trade(&mut alice_inv, &mut alice_currants, &defs)
            .unwrap();
        assert!(!alice_mgr.has_active_trade());
        assert_eq!(alice_inv.count_item("cherry"), 0);
        assert_eq!(alice_inv.count_item("grain"), 3);
        assert_eq!(alice_currants, 100);

        // Bob also executes independently (both execute when both locked).
        let mut bob_inv = make_inventory(&[("grain", 3)]);
        let mut bob_currants: u64 = 50;
        let _bob_complete = bob_mgr
            .execute_trade(&mut bob_inv, &mut bob_currants, &defs)
            .unwrap();
        assert_eq!(bob_inv.count_item("grain"), 0);
        assert_eq!(bob_inv.count_item("cherry"), 5);

        // Alice's Complete message is a courtesy — Bob already executed.
        if let TradeMessage::Complete { trade_id, .. } = &complete_msg {
            bob_mgr.receive_complete(*trade_id).unwrap();
        }
    }

    // ── Decline ─────────────────────────────────────────────────────────

    #[test]
    fn decline_clears_pending_request() {
        let mut bob_mgr = TradeManager::new(BOB);
        bob_mgr
            .receive_request(1, ALICE, "Alice".into(), 0.0)
            .unwrap();
        let decline = bob_mgr.decline_trade().unwrap();
        assert!(bob_mgr.pending_request().is_none());
        assert!(matches!(decline, TradeMessage::Decline { .. }));
    }

    #[test]
    fn initiator_receives_decline() {
        let mut alice_mgr = TradeManager::new(ALICE);
        alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        alice_mgr.receive_decline(1).unwrap();
        assert!(!alice_mgr.has_active_trade());
    }

    // ── Cancel ──────────────────────────────────────────────────────────

    #[test]
    fn cancel_clears_active_trade() {
        let mut alice_mgr = TradeManager::new(ALICE);
        alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        let cancel = alice_mgr.cancel_trade();
        assert!(cancel.is_some());
        assert!(!alice_mgr.has_active_trade());
    }

    #[test]
    fn receive_cancel_clears_trade() {
        let mut alice_mgr = TradeManager::new(ALICE);
        let mut bob_mgr = TradeManager::new(BOB);

        alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        bob_mgr
            .receive_request(1, ALICE, "Alice".into(), 0.0)
            .unwrap();
        bob_mgr.accept_trade(1.0).unwrap();

        bob_mgr.receive_cancel(1).unwrap();
        assert!(!bob_mgr.has_active_trade());
    }

    // ── Timeout ─────────────────────────────────────────────────────────

    #[test]
    fn timeout_cancels_trade() {
        let mut alice_mgr = TradeManager::new(ALICE);
        alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        let cancel = alice_mgr.tick(61.0);
        assert!(cancel.is_some());
        assert!(!alice_mgr.has_active_trade());
    }

    #[test]
    fn no_timeout_before_deadline() {
        let mut alice_mgr = TradeManager::new(ALICE);
        alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        let cancel = alice_mgr.tick(59.0);
        assert!(cancel.is_none());
        assert!(alice_mgr.has_active_trade());
    }

    // ── Double-initiate ─────────────────────────────────────────────────

    #[test]
    fn double_initiate_rejected() {
        let mut alice_mgr = TradeManager::new(ALICE);
        alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        let err = alice_mgr
            .initiate_trade(2, BOB, "Bob".into(), 0.0)
            .unwrap_err();
        assert_eq!(err, "Already in a trade");
    }

    // ── Out-of-order ────────────────────────────────────────────────────

    #[test]
    fn lock_before_negotiating_rejected() {
        let mut alice_mgr = TradeManager::new(ALICE);
        alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        // Still in PendingResponse, can't lock.
        let empty = make_inventory(&[]);
        let err = alice_mgr.lock_trade(&empty, 0, 1.0).unwrap_err();
        assert_eq!(err, "Cannot lock in current phase");
    }

    // ── Trade ID mismatch ───────────────────────────────────────────────

    #[test]
    fn trade_id_mismatch_rejected() {
        let mut alice_mgr = TradeManager::new(ALICE);
        alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        let err = alice_mgr.receive_accept(999, 1.0).unwrap_err();
        assert_eq!(err, "Trade ID mismatch");
    }

    // ── Terms hash mismatch ─────────────────────────────────────────────

    #[test]
    fn terms_hash_mismatch_does_not_execute() {
        let mut alice_mgr = TradeManager::new(ALICE);
        let mut bob_mgr = TradeManager::new(BOB);

        // Set up and enter negotiation.
        alice_mgr
            .initiate_trade(1, BOB, "Bob".into(), 0.0)
            .unwrap();
        bob_mgr
            .receive_request(1, ALICE, "Alice".into(), 0.0)
            .unwrap();
        bob_mgr.accept_trade(1.0).unwrap();
        alice_mgr.receive_accept(1, 1.0).unwrap();

        // Alice offers cherries.
        alice_mgr
            .update_offer(
                TradeOffer {
                    items: vec![ItemStack {
                        item_id: "cherry".into(),
                        count: 5,
                    }],
                    currants: 0,
                },
                2.0,
            )
            .unwrap();

        // Alice locks.
        let alice_inv = make_inventory(&[("cherry", 5)]);
        alice_mgr.lock_trade(&alice_inv, 100, 3.0).unwrap();

        // Bob sends a lock with a WRONG terms hash (simulating offer mismatch).
        let both_locked = alice_mgr
            .receive_remote_lock(1, [0xFF; 16], 4.0)
            .unwrap();
        assert!(!both_locked, "Should not execute with mismatched hash");
    }

    // ── Execution validation ────────────────────────────────────────────

    #[test]
    fn execute_insufficient_items_fails() {
        let defs = make_item_defs();
        let mut mgr = TradeManager::new(ALICE);

        // Set up a trade in Executing phase manually.
        mgr.active_trade = Some(TradeSession {
            trade_id: 1,
            phase: TradePhase::Executing,
            role: TradeRole::Initiator,
            our_hash: ALICE,
            peer_hash: BOB,
            peer_name: "Bob".into(),
            local_offer: TradeOffer {
                items: vec![ItemStack {
                    item_id: "cherry".into(),
                    count: 10,
                }],
                currants: 0,
            },
            remote_offer: TradeOffer::empty(),
            local_terms_hash: Some([0; 16]),
            remote_terms_hash: Some([0; 16]),
            last_activity: 0.0,
        });

        let mut inv = make_inventory(&[("cherry", 3)]); // only 3, need 10
        let mut currants: u64 = 100;
        let err = mgr
            .execute_trade(&mut inv, &mut currants, &defs)
            .unwrap_err();
        assert!(err.contains("Insufficient cherry"));
        // Inventory unchanged.
        assert_eq!(inv.count_item("cherry"), 3);
    }

    #[test]
    fn execute_insufficient_currants_fails() {
        let defs = make_item_defs();
        let mut mgr = TradeManager::new(ALICE);

        mgr.active_trade = Some(TradeSession {
            trade_id: 1,
            phase: TradePhase::Executing,
            role: TradeRole::Initiator,
            our_hash: ALICE,
            peer_hash: BOB,
            peer_name: "Bob".into(),
            local_offer: TradeOffer {
                items: Vec::new(),
                currants: 200,
            },
            remote_offer: TradeOffer::empty(),
            local_terms_hash: Some([0; 16]),
            remote_terms_hash: Some([0; 16]),
            last_activity: 0.0,
        });

        let mut inv = make_inventory(&[]);
        let mut currants: u64 = 50; // only 50, need 200
        let err = mgr
            .execute_trade(&mut inv, &mut currants, &defs)
            .unwrap_err();
        assert!(err.contains("Insufficient currants"));
        assert_eq!(currants, 50); // unchanged
    }

    #[test]
    fn currants_only_trade() {
        let defs = make_item_defs();
        let mut mgr = TradeManager::new(ALICE);

        mgr.active_trade = Some(TradeSession {
            trade_id: 1,
            phase: TradePhase::Executing,
            role: TradeRole::Initiator,
            our_hash: ALICE,
            peer_hash: BOB,
            peer_name: "Bob".into(),
            local_offer: TradeOffer {
                items: Vec::new(),
                currants: 50,
            },
            remote_offer: TradeOffer {
                items: Vec::new(),
                currants: 30,
            },
            local_terms_hash: Some([0; 16]),
            remote_terms_hash: Some([0; 16]),
            last_activity: 0.0,
        });

        let mut inv = make_inventory(&[]);
        let mut currants: u64 = 100;
        mgr.execute_trade(&mut inv, &mut currants, &defs)
            .unwrap();
        assert_eq!(currants, 80); // 100 - 50 + 30
    }

    #[test]
    fn unlock_returns_to_negotiating() {
        let mut mgr = TradeManager::new(ALICE);
        let mut bob = TradeManager::new(BOB);

        mgr.initiate_trade(1, BOB, "Bob".into(), 0.0).unwrap();
        bob.receive_request(1, ALICE, "Alice".into(), 0.0)
            .unwrap();
        bob.accept_trade(1.0).unwrap();
        mgr.receive_accept(1, 1.0).unwrap();

        mgr.update_offer(TradeOffer::empty(), 2.0).unwrap();
        let empty = make_inventory(&[]);
        mgr.lock_trade(&empty, 0, 3.0).unwrap();
        assert_eq!(
            mgr.active_trade.as_ref().unwrap().phase,
            TradePhase::LockedLocal
        );

        mgr.unlock_trade(4.0).unwrap();
        assert_eq!(
            mgr.active_trade.as_ref().unwrap().phase,
            TradePhase::Negotiating
        );
    }

    #[test]
    fn is_trading_with_checks_both_active_and_pending() {
        let mut mgr = TradeManager::new(ALICE);
        assert!(!mgr.is_trading_with(&BOB));

        mgr.receive_request(1, BOB, "Bob".into(), 0.0).unwrap();
        assert!(mgr.is_trading_with(&BOB));
        assert!(!mgr.is_trading_with(&[0x03; 16]));
    }

    #[test]
    fn trade_frame_populated() {
        let defs = make_item_defs();
        let mut mgr = TradeManager::new(ALICE);

        mgr.active_trade = Some(TradeSession {
            trade_id: 42,
            phase: TradePhase::Negotiating,
            role: TradeRole::Initiator,
            our_hash: ALICE,
            peer_hash: BOB,
            peer_name: "Bob".into(),
            local_offer: TradeOffer {
                items: vec![ItemStack {
                    item_id: "cherry".into(),
                    count: 3,
                }],
                currants: 10,
            },
            remote_offer: TradeOffer::empty(),
            local_terms_hash: None,
            remote_terms_hash: None,
            last_activity: 0.0,
        });

        let frame = mgr.trade_frame(&defs).unwrap();
        assert_eq!(frame.trade_id, 42);
        assert_eq!(frame.phase, "negotiating");
        assert_eq!(frame.peer_name, "Bob");
        assert_eq!(frame.local_offer.items.len(), 1);
        assert_eq!(frame.local_offer.items[0].name, "Cherry");
        assert_eq!(frame.local_offer.currants, 10);
        assert!(!frame.local_locked);
        assert!(!frame.remote_locked);
    }
}
