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
                .min_by(|a, b| a.joined_at.partial_cmp(&b.joined_at).unwrap())
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

/// Runtime party state for one player.
#[derive(Debug, Default, Clone)]
pub struct PartyState {
    pub party: Option<ActiveParty>,
    pub pending_invite: Option<PendingPartyInvite>,
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
        // Add self.
        let _ = party.add_member(PartyMember {
            address_hash: self_hash,
            display_name: self_name,
            joined_at: now,
        });
        self.party = Some(party);
        Ok(())
    }

    /// Decline the pending invite and clear it.
    pub fn decline_invite(&mut self) {
        self.pending_invite = None;
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
}
