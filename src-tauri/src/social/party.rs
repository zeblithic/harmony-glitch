use serde::{Deserialize, Serialize};

/// Maximum number of members in a party (including leader).
pub const MAX_PARTY_SIZE: usize = 5;

/// Seconds before a pending party invite expires.
pub const PARTY_INVITE_TIMEOUT: f64 = 90.0;

/// Seconds of grace period after a member disconnects before they are removed.
pub const PARTY_GRACE_PERIOD: f64 = 30.0;

/// Role of a player within a party.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyRole {
    Leader,
    Member,
}

/// A single member of an active party.
#[derive(Debug, Clone, PartialEq)]
pub struct PartyMember {
    pub address_hash: [u8; 16],
    pub display_name: String,
    /// Monotonic seconds since app epoch when this member joined.
    pub joined_at: f64,
}

/// An active party session.
#[derive(Debug, Clone)]
pub struct ActiveParty {
    /// Current leader's address hash.
    pub leader: [u8; 16],
    /// All members including the leader.
    pub members: Vec<PartyMember>,
    pub created_at: f64,
}

impl ActiveParty {
    /// Create a new single-member party with the given player as leader.
    pub fn new(leader_hash: [u8; 16], leader_name: String, now: f64) -> Self {
        ActiveParty {
            leader: leader_hash,
            members: vec![PartyMember {
                address_hash: leader_hash,
                display_name: leader_name,
                joined_at: now,
            }],
            created_at: now,
        }
    }

    /// Returns true if `addr` is a member.
    pub fn is_member(&self, addr: &[u8; 16]) -> bool {
        self.members.iter().any(|m| &m.address_hash == addr)
    }

    /// Returns true if `addr` is the current leader.
    pub fn is_leader(&self, addr: &[u8; 16]) -> bool {
        self.leader == *addr
    }

    /// Returns the role of `addr`, or None if not a member.
    pub fn role_of(&self, addr: &[u8; 16]) -> Option<PartyRole> {
        if !self.is_member(addr) {
            return None;
        }
        if self.is_leader(addr) {
            Some(PartyRole::Leader)
        } else {
            Some(PartyRole::Member)
        }
    }

    /// Add a member to the party.
    ///
    /// Returns `Err` if the party is already full or the player is already a member.
    pub fn add_member(&mut self, member: PartyMember) -> Result<(), &'static str> {
        if self.members.len() >= MAX_PARTY_SIZE {
            return Err("party is full");
        }
        if self.is_member(&member.address_hash) {
            return Err("already a member");
        }
        self.members.push(member);
        Ok(())
    }

    /// Remove a member from the party.
    ///
    /// If the leaving member is the leader, leadership transfers to the
    /// longest-tenured remaining member (smallest `joined_at`).
    ///
    /// Returns `(remaining_count, new_leader)` where `new_leader` is `Some`
    /// only if leadership changed.
    pub fn remove_member(&mut self, addr: &[u8; 16]) -> (usize, Option<[u8; 16]>) {
        self.members.retain(|m| &m.address_hash != addr);
        let remaining = self.members.len();
        if remaining == 0 {
            return (0, None);
        }
        // Transfer leadership if the leader left.
        let new_leader = if self.leader == *addr {
            // Find longest-tenured member (minimum joined_at).
            let next = self
                .members
                .iter()
                .min_by(|a, b| a.joined_at.total_cmp(&b.joined_at))
                .map(|m| m.address_hash)
                .unwrap();
            self.leader = next;
            Some(next)
        } else {
            None
        };
        (remaining, new_leader)
    }

    /// Kick a member (leader-only; cannot kick self).
    ///
    /// Returns `Err` if the caller is not the leader, tries to kick themselves,
    /// or the target is not a member.
    pub fn kick_member(
        &mut self,
        caller: &[u8; 16],
        target: &[u8; 16],
    ) -> Result<(), &'static str> {
        if !self.is_leader(caller) {
            return Err("only the leader can kick");
        }
        if caller == target {
            return Err("leader cannot kick themselves");
        }
        if !self.is_member(target) {
            return Err("target is not a member");
        }
        self.members.retain(|m| &m.address_hash != target);
        Ok(())
    }

    /// Return all member address hashes.
    pub fn member_hashes(&self) -> Vec<[u8; 16]> {
        self.members.iter().map(|m| m.address_hash).collect()
    }
}

/// An incoming party invite that has not yet been accepted or declined.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingPartyInvite {
    pub leader: [u8; 16],
    pub leader_name: String,
    pub members: Vec<[u8; 16]>,
    /// Monotonic seconds since app epoch.
    pub received_at: f64,
}

/// An outgoing party invite we've sent (awaiting accept/decline/timeout).
#[derive(Debug, Clone, PartialEq)]
struct OutgoingPartyInvite {
    to: [u8; 16],
    sent_at: f64,
}

/// Transient state: we've sent PartyAccept but haven't received
/// the leader's PartyMemberJoined confirmation yet.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingJoin {
    pub leader: [u8; 16],
    pub leader_name: String,
    pub members: Vec<[u8; 16]>,
    pub accepted_at: f64,
}

/// Runtime party state for one player.
#[derive(Debug, Default, Clone)]
pub struct PartyState {
    pub party: Option<ActiveParty>,
    pub pending_invite: Option<PendingPartyInvite>,
    pub pending_join: Option<PendingJoin>,
    /// Outgoing party invites we've sent (not yet accepted/declined/expired).
    outgoing_invites: Vec<OutgoingPartyInvite>,
}

impl PartyState {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Queries ──────────────────────────────────────────────────────────

    pub fn in_party(&self) -> bool {
        self.party.is_some()
    }

    /// Returns true if the party has 2+ members (enables mood-decay bonus).
    pub fn has_party_bonus(&self) -> bool {
        self.party.as_ref().map_or(false, |p| p.members.len() >= 2)
    }

    // ── Party lifecycle ───────────────────────────────────────────────────

    /// Create a new single-member party. Replaces any existing party.
    pub fn create_party(&mut self, leader_hash: [u8; 16], leader_name: String, now: f64) {
        self.party = Some(ActiveParty::new(leader_hash, leader_name, now));
    }

    /// Accept the pending invite (if it is still within the 90-second window).
    ///
    /// On success the player joins the party and the pending invite is cleared.
    /// Returns `Err` if there is no pending invite, it has expired, or it is
    /// already accepted.
    pub fn accept_invite(
        &mut self,
        self_hash: [u8; 16],
        self_name: String,
        now: f64,
    ) -> Result<(), &'static str> {
        let invite = self.pending_invite.as_ref().ok_or("no pending invite")?;
        if now - invite.received_at > PARTY_INVITE_TIMEOUT {
            self.pending_invite = None;
            return Err("invite expired");
        }
        let leader_hash = invite.leader;
        let leader_name = invite.leader_name.clone();
        let members = invite.members.clone();

        // Build the party from the invite data.
        let created_at = invite.received_at;
        self.pending_invite = None;

        let mut party = ActiveParty {
            leader: leader_hash,
            members: vec![PartyMember {
                address_hash: leader_hash,
                display_name: leader_name,
                joined_at: created_at,
            }],
            created_at,
        };
        // Add existing members (best-effort — no joined_at info, use created_at).
        for &addr in &members {
            if addr != leader_hash && addr != self_hash {
                let _ = party.add_member(PartyMember {
                    address_hash: addr,
                    display_name: String::new(),
                    joined_at: created_at,
                });
            }
        }
        // Add self — propagate error (e.g. party full).
        party.add_member(PartyMember {
            address_hash: self_hash,
            display_name: self_name,
            joined_at: now,
        })?;
        self.party = Some(party);
        Ok(())
    }

    /// Decline the pending invite and clear it.
    pub fn decline_invite(&mut self) {
        self.pending_invite = None;
    }

    /// Move the pending invite into a deferred-join state.
    ///
    /// The invite is consumed but the player does NOT join the party yet.
    /// Call `confirm_join()` when the leader's `PartyMemberJoined` arrives.
    pub fn begin_join(&mut self, now: f64) -> Result<[u8; 16], &'static str> {
        let invite = self.pending_invite.take().ok_or("no pending invite")?;
        if now - invite.received_at > PARTY_INVITE_TIMEOUT {
            return Err("invite expired");
        }
        let leader = invite.leader;
        self.pending_join = Some(PendingJoin {
            leader,
            leader_name: invite.leader_name,
            members: invite.members,
            accepted_at: now,
        });
        Ok(leader)
    }

    /// Commit the deferred join after receiving the leader's confirmation.
    ///
    /// Builds the `ActiveParty` from the saved `PendingJoin` data
    /// (same logic as `accept_invite` but sourced from `pending_join`).
    pub fn confirm_join(
        &mut self,
        self_hash: [u8; 16],
        self_name: String,
        now: f64,
    ) -> Result<(), &'static str> {
        let pj = self.pending_join.take().ok_or("no pending join")?;
        let mut party = ActiveParty {
            leader: pj.leader,
            members: vec![PartyMember {
                address_hash: pj.leader,
                display_name: pj.leader_name,
                joined_at: pj.accepted_at,
            }],
            created_at: pj.accepted_at,
        };
        for &addr in &pj.members {
            if addr != pj.leader && addr != self_hash {
                let _ = party.add_member(PartyMember {
                    address_hash: addr,
                    display_name: String::new(),
                    joined_at: pj.accepted_at,
                });
            }
        }
        party.add_member(PartyMember {
            address_hash: self_hash,
            display_name: self_name,
            joined_at: now,
        })?;
        self.party = Some(party);
        Ok(())
    }

    /// Leave the current party.
    ///
    /// If the party would be reduced to ≤1 member it is dissolved entirely.
    /// Returns `(remaining_count, new_leader)`.
    pub fn leave_party(&mut self, self_hash: &[u8; 16]) -> Result<(usize, Option<[u8; 16]>), &'static str> {
        let party = self.party.as_mut().ok_or("not in a party")?;
        let (remaining, new_leader) = party.remove_member(self_hash);
        if remaining <= 1 {
            self.party = None;
            return Ok((remaining, new_leader));
        }
        Ok((remaining, new_leader))
    }

    // ── Invite management ─────────────────────────────────────────────────

    /// Store an incoming party invite, replacing any existing one.
    pub fn set_pending_invite(&mut self, invite: PendingPartyInvite) {
        self.pending_invite = Some(invite);
    }

    /// Expire the pending invite if it is older than 90 seconds.
    pub fn expire_invite(&mut self, now: f64) {
        if let Some(invite) = &self.pending_invite {
            if now - invite.received_at > PARTY_INVITE_TIMEOUT {
                self.pending_invite = None;
            }
        }
    }

    // ── Outgoing invites ──────────────────────────────────────────────────

    /// Record that we sent a party invite to `addr` at time `now`.
    /// If an invite already exists, refreshes its `sent_at` timestamp.
    pub fn record_outgoing_invite(&mut self, addr: [u8; 16], now: f64) {
        if let Some(invite) = self.outgoing_invites.iter_mut().find(|i| i.to == addr) {
            invite.sent_at = now;
        } else {
            self.outgoing_invites.push(OutgoingPartyInvite { to: addr, sent_at: now });
        }
    }

    /// Returns true if we have an outstanding outgoing invite to `addr`.
    pub fn has_outgoing_invite(&self, addr: &[u8; 16]) -> bool {
        self.outgoing_invites.iter().any(|i| &i.to == addr)
    }

    /// Remove and return true if we had an outgoing invite to `addr`.
    pub fn consume_outgoing_invite(&mut self, addr: &[u8; 16]) -> bool {
        let before = self.outgoing_invites.len();
        self.outgoing_invites.retain(|i| &i.to != addr);
        self.outgoing_invites.len() < before
    }

    /// Clear a pending join that has been waiting too long for leader confirmation.
    pub fn expire_pending_join(&mut self, now: f64) {
        if let Some(pj) = &self.pending_join {
            if now - pj.accepted_at > PARTY_INVITE_TIMEOUT {
                self.pending_join = None;
            }
        }
    }

    /// Remove outgoing invites older than 90 seconds.
    pub fn expire_outgoing_invites(&mut self, now: f64) {
        self.outgoing_invites
            .retain(|i| (now - i.sent_at) <= PARTY_INVITE_TIMEOUT);
    }

    /// Clear all outgoing invites (called on party dissolution/leave).
    pub fn clear_outgoing_invites(&mut self) {
        self.outgoing_invites.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> [u8; 16] {
        [b; 16]
    }

    fn member(b: u8, now: f64) -> PartyMember {
        PartyMember {
            address_hash: addr(b),
            display_name: format!("P{b:02x}"),
            joined_at: now,
        }
    }

    // ── ActiveParty ────────────────────────────────────────────────────────

    #[test]
    fn new_party_has_one_member_who_is_leader() {
        let p = ActiveParty::new(addr(0x01), "Alice".into(), 0.0);
        assert_eq!(p.members.len(), 1);
        assert!(p.is_leader(&addr(0x01)));
        assert!(p.is_member(&addr(0x01)));
    }

    #[test]
    fn add_member_succeeds() {
        let mut p = ActiveParty::new(addr(0x01), "Alice".into(), 0.0);
        assert!(p.add_member(member(0x02, 1.0)).is_ok());
        assert_eq!(p.members.len(), 2);
    }

    #[test]
    fn add_member_fails_when_full() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        for i in 2u8..=5 {
            p.add_member(member(i, i as f64)).unwrap();
        }
        assert_eq!(p.members.len(), MAX_PARTY_SIZE);
        let err = p.add_member(member(0x06, 6.0));
        assert!(err.is_err());
    }

    #[test]
    fn add_member_rejects_duplicate() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        assert!(p.add_member(member(0x01, 1.0)).is_err());
    }

    #[test]
    fn role_of_leader_and_member() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        p.add_member(member(0x02, 1.0)).unwrap();
        assert_eq!(p.role_of(&addr(0x01)), Some(PartyRole::Leader));
        assert_eq!(p.role_of(&addr(0x02)), Some(PartyRole::Member));
        assert_eq!(p.role_of(&addr(0x99)), None);
    }

    #[test]
    fn remove_non_leader_does_not_change_leader() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        p.add_member(member(0x02, 1.0)).unwrap();
        let (remaining, new_leader) = p.remove_member(&addr(0x02));
        assert_eq!(remaining, 1);
        assert_eq!(new_leader, None);
        assert!(p.is_leader(&addr(0x01)));
    }

    #[test]
    fn remove_leader_transfers_to_longest_tenured() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        // 0x02 joined at t=1, 0x03 joined at t=2 — 0x02 has longer tenure
        p.add_member(member(0x02, 1.0)).unwrap();
        p.add_member(member(0x03, 2.0)).unwrap();
        let (remaining, new_leader) = p.remove_member(&addr(0x01));
        assert_eq!(remaining, 2);
        assert_eq!(new_leader, Some(addr(0x02)));
        assert!(p.is_leader(&addr(0x02)));
    }

    #[test]
    fn remove_all_members_returns_zero() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        let (remaining, _) = p.remove_member(&addr(0x01));
        assert_eq!(remaining, 0);
    }

    #[test]
    fn kick_member_succeeds() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        p.add_member(member(0x02, 1.0)).unwrap();
        assert!(p.kick_member(&addr(0x01), &addr(0x02)).is_ok());
        assert!(!p.is_member(&addr(0x02)));
    }

    #[test]
    fn kick_requires_leader() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        p.add_member(member(0x02, 1.0)).unwrap();
        let err = p.kick_member(&addr(0x02), &addr(0x01));
        assert!(err.is_err());
    }

    #[test]
    fn kick_cannot_kick_self() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        let err = p.kick_member(&addr(0x01), &addr(0x01));
        assert!(err.is_err());
    }

    #[test]
    fn kick_nonmember_returns_err() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        let err = p.kick_member(&addr(0x01), &addr(0x99));
        assert!(err.is_err());
    }

    #[test]
    fn member_hashes_returns_all() {
        let mut p = ActiveParty::new(addr(0x01), "A".into(), 0.0);
        p.add_member(member(0x02, 1.0)).unwrap();
        let hashes = p.member_hashes();
        assert!(hashes.contains(&addr(0x01)));
        assert!(hashes.contains(&addr(0x02)));
        assert_eq!(hashes.len(), 2);
    }

    // ── PartyState ─────────────────────────────────────────────────────────

    #[test]
    fn create_party_sets_in_party() {
        let mut s = PartyState::new();
        assert!(!s.in_party());
        s.create_party(addr(0x01), "Alice".into(), 0.0);
        assert!(s.in_party());
    }

    #[test]
    fn has_party_bonus_requires_two_members() {
        let mut s = PartyState::new();
        s.create_party(addr(0x01), "Alice".into(), 0.0);
        assert!(!s.has_party_bonus()); // only 1 member
        s.party.as_mut().unwrap().add_member(member(0x02, 1.0)).unwrap();
        assert!(s.has_party_bonus());
    }

    #[test]
    fn accept_invite_within_timeout() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "Leader".into(),
            members: vec![],
            received_at: 100.0,
        });
        let result = s.accept_invite(addr(0x02), "Me".into(), 150.0);
        assert!(result.is_ok());
        assert!(s.in_party());
        assert!(s.pending_invite.is_none());
    }

    #[test]
    fn accept_invite_expired_returns_err() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "Leader".into(),
            members: vec![],
            received_at: 0.0,
        });
        let result = s.accept_invite(addr(0x02), "Me".into(), 91.0);
        assert!(result.is_err());
        assert!(!s.in_party());
    }

    #[test]
    fn accept_invite_no_invite_returns_err() {
        let mut s = PartyState::new();
        let result = s.accept_invite(addr(0x01), "Me".into(), 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn accept_invite_returns_err_when_party_full() {
        let mut s = PartyState::new();
        // Invite from leader with 4 existing members (leader + 3 others = 4).
        // Adding self would make 5, but if there are already 4 others + leader = 5,
        // self would be the 6th → party full.
        let existing_members: Vec<[u8; 16]> = (2u8..=5).map(|i| addr(i)).collect();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "Leader".into(),
            members: existing_members,
            received_at: 100.0,
        });
        // leader (0x01) + members 0x02..0x05 = 5 members, adding self (0x06) should fail
        let result = s.accept_invite(addr(0x06), "Me".into(), 150.0);
        assert!(result.is_err(), "should fail when party is full");
        assert!(!s.in_party(), "should not join party when add_member fails");
    }

    #[test]
    fn decline_invite_clears_it() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "L".into(),
            members: vec![],
            received_at: 0.0,
        });
        s.decline_invite();
        assert!(s.pending_invite.is_none());
    }

    #[test]
    fn leave_party_dissolves_at_one_member() {
        let mut s = PartyState::new();
        s.create_party(addr(0x01), "Alice".into(), 0.0);
        let result = s.leave_party(&addr(0x01));
        assert!(result.is_ok());
        assert!(!s.in_party());
    }

    #[test]
    fn leave_party_does_not_dissolve_at_two_plus() {
        let mut s = PartyState::new();
        s.create_party(addr(0x01), "Alice".into(), 0.0);
        s.party.as_mut().unwrap().add_member(member(0x02, 1.0)).unwrap();
        s.party.as_mut().unwrap().add_member(member(0x03, 2.0)).unwrap();
        let (remaining, _) = s.leave_party(&addr(0x03)).unwrap();
        assert_eq!(remaining, 2);
        assert!(s.in_party());
    }

    #[test]
    fn leave_party_when_leader_transfers_leadership() {
        let mut s = PartyState::new();
        s.create_party(addr(0x01), "Alice".into(), 0.0);
        s.party.as_mut().unwrap().add_member(member(0x02, 1.0)).unwrap();
        s.party.as_mut().unwrap().add_member(member(0x03, 2.0)).unwrap();
        let (remaining, new_leader) = s.leave_party(&addr(0x01)).unwrap();
        assert_eq!(remaining, 2);
        assert_eq!(new_leader, Some(addr(0x02)));
        assert!(s.in_party());
    }

    #[test]
    fn leave_party_when_not_in_party_returns_err() {
        let mut s = PartyState::new();
        assert!(s.leave_party(&addr(0x01)).is_err());
    }

    #[test]
    fn expire_invite_clears_old() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "L".into(),
            members: vec![],
            received_at: 0.0,
        });
        s.expire_invite(91.0);
        assert!(s.pending_invite.is_none());
    }

    #[test]
    fn expire_invite_keeps_fresh() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "L".into(),
            members: vec![],
            received_at: 0.0,
        });
        s.expire_invite(50.0);
        assert!(s.pending_invite.is_some());
    }

    // ── Outgoing invites ──────────────────────────────────────────────────

    #[test]
    fn record_outgoing_invite_tracks_address() {
        let mut s = PartyState::new();
        s.record_outgoing_invite(addr(0x10), 100.0);
        assert!(s.has_outgoing_invite(&addr(0x10)));
        assert!(!s.has_outgoing_invite(&addr(0x11)));
    }

    #[test]
    fn record_outgoing_invite_ignores_duplicates() {
        let mut s = PartyState::new();
        s.record_outgoing_invite(addr(0x10), 100.0);
        s.record_outgoing_invite(addr(0x10), 200.0);
        assert_eq!(s.outgoing_invites.len(), 1);
    }

    #[test]
    fn consume_outgoing_invite_returns_true_and_removes() {
        let mut s = PartyState::new();
        s.record_outgoing_invite(addr(0x10), 100.0);
        assert!(s.consume_outgoing_invite(&addr(0x10)));
        assert!(s.outgoing_invites.is_empty());
    }

    #[test]
    fn consume_outgoing_invite_returns_false_if_absent() {
        let mut s = PartyState::new();
        assert!(!s.consume_outgoing_invite(&addr(0x99)));
    }

    #[test]
    fn expire_outgoing_invites_removes_old() {
        let mut s = PartyState::new();
        s.record_outgoing_invite(addr(0x10), 0.0);
        s.record_outgoing_invite(addr(0x11), 80.0);
        s.expire_outgoing_invites(100.0); // 0x10 is 100s old (expired), 0x11 is 20s (fresh)
        assert!(!s.has_outgoing_invite(&addr(0x10)));
        assert!(s.has_outgoing_invite(&addr(0x11)));
    }

    #[test]
    fn clear_outgoing_invites_removes_all() {
        let mut s = PartyState::new();
        s.record_outgoing_invite(addr(0x10), 100.0);
        s.record_outgoing_invite(addr(0x11), 100.0);
        s.clear_outgoing_invites();
        assert!(s.outgoing_invites.is_empty());
    }

    #[test]
    fn confirm_join_builds_party_from_pending_join() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "Leader".into(),
            members: vec![addr(0x02)],
            received_at: 100.0,
        });
        s.begin_join(100.5).unwrap();

        let result = s.confirm_join(addr(0x03), "Me".into(), 101.0);
        assert!(result.is_ok());
        assert!(s.in_party(), "should be in party after confirm");
        assert!(s.pending_join.is_none(), "pending_join should be cleared");
        let party = s.party.as_ref().unwrap();
        assert!(party.is_leader(&addr(0x01)));
        assert!(party.is_member(&addr(0x01)));
        assert!(party.is_member(&addr(0x02)));
        assert!(party.is_member(&addr(0x03)));
    }

    #[test]
    fn confirm_join_without_pending_join_returns_err() {
        let mut s = PartyState::new();
        let result = s.confirm_join(addr(0x01), "Me".into(), 100.0);
        assert!(result.is_err());
    }

    #[test]
    fn begin_join_moves_invite_to_pending_join() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "Leader".into(),
            members: vec![addr(0x02)],
            received_at: 100.0,
        });
        let result = s.begin_join(100.5);
        assert!(result.is_ok());
        assert!(s.pending_invite.is_none(), "invite should be consumed");
        assert!(s.pending_join.is_some(), "pending_join should be set");
        assert!(!s.in_party(), "should NOT be in party yet");
        let pj = s.pending_join.as_ref().unwrap();
        assert_eq!(pj.leader, addr(0x01));
        assert_eq!(pj.leader_name, "Leader");
        assert_eq!(pj.members, vec![addr(0x02)]);
    }

    #[test]
    fn expire_pending_join_clears_old() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "L".into(),
            members: vec![],
            received_at: 0.0,
        });
        s.begin_join(0.5).unwrap();
        s.expire_pending_join(91.0);
        assert!(s.pending_join.is_none(), "stale pending_join should be cleared");
    }

    #[test]
    fn expire_pending_join_keeps_fresh() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "L".into(),
            members: vec![],
            received_at: 100.0,
        });
        s.begin_join(100.5).unwrap();
        s.expire_pending_join(150.0);
        assert!(s.pending_join.is_some(), "fresh pending_join should survive");
    }

    // ── Race condition tests ───────────────────────────────────────────────

    #[test]
    fn pending_join_timeout_does_not_create_phantom_party() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "Leader".into(),
            members: vec![],
            received_at: 0.0,
        });
        s.begin_join(0.5).unwrap();
        assert!(s.pending_join.is_some());
        assert!(!s.in_party());

        // Simulate 91 seconds passing with no confirmation
        s.expire_pending_join(91.0);
        assert!(s.pending_join.is_none(), "pending_join should expire");
        assert!(!s.in_party(), "must NOT be in a phantom party");
    }

    #[test]
    fn begin_join_fails_without_invite() {
        let mut s = PartyState::new();
        let result = s.begin_join(0.0);
        assert!(result.is_err());
    }

    #[test]
    fn confirm_join_after_expiry_returns_err() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "L".into(),
            members: vec![],
            received_at: 0.0,
        });
        s.begin_join(0.5).unwrap();
        s.expire_pending_join(91.0); // expired
        let result = s.confirm_join(addr(0x02), "Me".into(), 92.0);
        assert!(result.is_err(), "confirm after expiry should fail");
        assert!(!s.in_party());
    }

    #[test]
    fn begin_join_with_expired_invite_returns_err() {
        let mut s = PartyState::new();
        s.set_pending_invite(PendingPartyInvite {
            leader: addr(0x01),
            leader_name: "L".into(),
            members: vec![],
            received_at: 0.0,
        });
        let result = s.begin_join(91.0);
        assert!(result.is_err());
        assert!(s.pending_invite.is_none(), "expired invite should be cleared");
        assert!(s.pending_join.is_none(), "should not create pending_join from expired invite");
    }
}
