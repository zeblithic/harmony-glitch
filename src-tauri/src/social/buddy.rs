use crate::social::types::BuddySaveEntry;

/// A confirmed buddy entry stored in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyEntry {
    pub address_hash: [u8; 16],
    pub display_name: String,
    pub added_date: String,
    pub co_presence_total: f64,
    pub last_seen_date: Option<String>,
}

impl BuddyEntry {
    /// Serialize to a save-file entry (hex-encodes the address hash).
    pub fn to_save_entry(&self) -> BuddySaveEntry {
        BuddySaveEntry {
            address_hash: hex::encode(self.address_hash),
            display_name: self.display_name.clone(),
            added_date: self.added_date.clone(),
            co_presence_total: self.co_presence_total,
            last_seen_date: self.last_seen_date.clone(),
        }
    }

    /// Deserialize from a save-file entry (hex-decodes, validates 16 bytes).
    pub fn from_save_entry(save: &BuddySaveEntry) -> Option<Self> {
        let bytes = hex::decode(&save.address_hash).ok()?;
        if bytes.len() != 16 {
            return None;
        }
        let mut address_hash = [0u8; 16];
        address_hash.copy_from_slice(&bytes);
        Some(BuddyEntry {
            address_hash,
            display_name: save.display_name.clone(),
            added_date: save.added_date.clone(),
            co_presence_total: save.co_presence_total,
            last_seen_date: save.last_seen_date.clone(),
        })
    }
}

/// An incoming buddy request that has not yet been accepted or declined.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingBuddyRequest {
    pub from: [u8; 16],
    pub from_name: String,
    /// Monotonic seconds since app epoch.
    pub received_at: f64,
}

/// Runtime buddy state for one player.
#[derive(Debug, Default, Clone)]
pub struct BuddyState {
    pub buddies: Vec<BuddyEntry>,
    pub blocked: Vec<[u8; 16]>,
    pub pending_requests: Vec<PendingBuddyRequest>,
}

/// Timeout (seconds) before a pending buddy request expires.
const PENDING_TIMEOUT_SECS: f64 = 90.0;

impl BuddyState {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Queries ──────────────────────────────────────────────────────────

    pub fn is_buddy(&self, addr: &[u8; 16]) -> bool {
        self.buddies.iter().any(|b| &b.address_hash == addr)
    }

    pub fn is_blocked(&self, addr: &[u8; 16]) -> bool {
        self.blocked.contains(addr)
    }

    // ── Buddy mutations ──────────────────────────────────────────────────

    /// Add a buddy entry. Ignores duplicates and blocked addresses.
    /// Clears any pending request from this address.
    pub fn add_buddy(&mut self, entry: BuddyEntry) {
        if self.is_blocked(&entry.address_hash) {
            return;
        }
        self.pending_requests
            .retain(|r| r.from != entry.address_hash);
        if !self.is_buddy(&entry.address_hash) {
            self.buddies.push(entry);
        }
    }

    /// Remove a buddy by address hash. Returns true if found and removed.
    pub fn remove_buddy(&mut self, addr: &[u8; 16]) -> bool {
        let before = self.buddies.len();
        self.buddies.retain(|b| &b.address_hash != addr);
        self.buddies.len() < before
    }

    // ── Block list ───────────────────────────────────────────────────────

    /// Block a player: removes them from buddies and pending requests.
    pub fn block_player(&mut self, addr: &[u8; 16]) {
        self.remove_buddy(addr);
        self.pending_requests.retain(|r| &r.from != addr);
        if !self.is_blocked(addr) {
            self.blocked.push(*addr);
        }
    }

    /// Remove a player from the block list. Returns true if found.
    pub fn unblock_player(&mut self, addr: &[u8; 16]) -> bool {
        let before = self.blocked.len();
        self.blocked.retain(|b| b != addr);
        self.blocked.len() < before
    }

    // ── Pending requests ─────────────────────────────────────────────────

    /// Enqueue an incoming buddy request (ignored if sender is blocked or already a buddy).
    pub fn add_pending_request(&mut self, request: PendingBuddyRequest) {
        if self.is_blocked(&request.from) || self.is_buddy(&request.from) {
            return;
        }
        // Replace any existing request from the same sender.
        self.pending_requests.retain(|r| r.from != request.from);
        self.pending_requests.push(request);
    }

    /// Return the pending request from `addr` if it is still within the 90-second window.
    pub fn get_pending_request(&self, addr: &[u8; 16], now: f64) -> Option<&PendingBuddyRequest> {
        self.pending_requests.iter().find(|r| {
            &r.from == addr && (now - r.received_at) <= PENDING_TIMEOUT_SECS
        })
    }

    /// Remove all pending requests older than 90 seconds.
    pub fn expire_requests(&mut self, now: f64) {
        self.pending_requests
            .retain(|r| (now - r.received_at) <= PENDING_TIMEOUT_SECS);
    }

    // ── Buddy data updates ───────────────────────────────────────────────

    /// Update the display name for an existing buddy.
    pub fn update_buddy_name(&mut self, addr: &[u8; 16], new_name: &str) -> bool {
        match self.buddies.iter_mut().find(|b| &b.address_hash == addr) {
            Some(b) => {
                b.display_name = new_name.to_owned();
                true
            }
            None => false,
        }
    }

    /// Add `seconds` to a buddy's co-presence total and update last_seen_date.
    pub fn record_copresence(&mut self, addr: &[u8; 16], seconds: f64, date: &str) -> bool {
        match self.buddies.iter_mut().find(|b| &b.address_hash == addr) {
            Some(b) => {
                b.co_presence_total += seconds;
                b.last_seen_date = Some(date.to_owned());
                true
            }
            None => false,
        }
    }

    // ── Persistence ──────────────────────────────────────────────────────

    /// Convert all buddy entries to save-file format.
    pub fn to_save_entries(&self) -> Vec<BuddySaveEntry> {
        self.buddies.iter().map(|b| b.to_save_entry()).collect()
    }

    /// Convert blocked addresses to hex strings for the save file.
    pub fn blocked_to_hex(&self) -> Vec<String> {
        self.blocked.iter().map(|a| hex::encode(a)).collect()
    }

    /// Restore buddies and blocked list from saved data.
    /// Skips any entry that fails to decode.
    pub fn restore_from_save(&mut self, entries: &[BuddySaveEntry], blocked_hex: &[String]) {
        self.buddies.clear();
        self.blocked.clear();
        self.pending_requests.clear();
        for entry in entries {
            if let Some(buddy) = BuddyEntry::from_save_entry(entry) {
                self.buddies.push(buddy);
            }
        }
        for hex_str in blocked_hex {
            if let Ok(bytes) = hex::decode(hex_str) {
                if bytes.len() == 16 {
                    let mut addr = [0u8; 16];
                    addr.copy_from_slice(&bytes);
                    self.blocked.push(addr);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_addr(b: u8) -> [u8; 16] {
        [b; 16]
    }

    fn make_entry(b: u8) -> BuddyEntry {
        BuddyEntry {
            address_hash: make_addr(b),
            display_name: format!("Player_{b:02x}"),
            added_date: "2026-01-01".into(),
            co_presence_total: 0.0,
            last_seen_date: None,
        }
    }

    fn make_save_entry(b: u8) -> BuddySaveEntry {
        BuddySaveEntry {
            address_hash: hex::encode(make_addr(b)),
            display_name: format!("Player_{b:02x}"),
            added_date: "2026-01-01".into(),
            co_presence_total: 0.0,
            last_seen_date: None,
        }
    }

    // ── BuddyEntry round-trip ────────────────────────────────────────────

    #[test]
    fn to_save_entry_hex_encodes_address() {
        let entry = make_entry(0xAB);
        let save = entry.to_save_entry();
        assert_eq!(save.address_hash, hex::encode([0xAB; 16]));
        assert_eq!(save.address_hash.len(), 32);
    }

    #[test]
    fn from_save_entry_round_trip() {
        let entry = make_entry(0x42);
        let save = entry.to_save_entry();
        let restored = BuddyEntry::from_save_entry(&save).unwrap();
        assert_eq!(entry, restored);
    }

    #[test]
    fn from_save_entry_rejects_bad_hex() {
        let mut save = make_save_entry(0x01);
        save.address_hash = "not-hex!!!".into();
        assert!(BuddyEntry::from_save_entry(&save).is_none());
    }

    #[test]
    fn from_save_entry_rejects_wrong_length() {
        let mut save = make_save_entry(0x01);
        // 8 bytes (16 hex chars) instead of 16 bytes
        save.address_hash = "0102030405060708".into();
        assert!(BuddyEntry::from_save_entry(&save).is_none());
    }

    // ── is_buddy / is_blocked ─────────────────────────────────────────────

    #[test]
    fn is_buddy_returns_true_after_add() {
        let mut state = BuddyState::new();
        state.add_buddy(make_entry(0x01));
        assert!(state.is_buddy(&make_addr(0x01)));
        assert!(!state.is_buddy(&make_addr(0x02)));
    }

    #[test]
    fn add_buddy_ignores_duplicates() {
        let mut state = BuddyState::new();
        state.add_buddy(make_entry(0x01));
        state.add_buddy(make_entry(0x01));
        assert_eq!(state.buddies.len(), 1);
    }

    #[test]
    fn remove_buddy_returns_true_when_found() {
        let mut state = BuddyState::new();
        state.add_buddy(make_entry(0x01));
        assert!(state.remove_buddy(&make_addr(0x01)));
        assert!(!state.is_buddy(&make_addr(0x01)));
    }

    #[test]
    fn remove_buddy_returns_false_when_not_found() {
        let mut state = BuddyState::new();
        assert!(!state.remove_buddy(&make_addr(0x99)));
    }

    // ── Block list ────────────────────────────────────────────────────────

    #[test]
    fn block_removes_buddy_and_pending() {
        let mut state = BuddyState::new();
        state.add_buddy(make_entry(0x01));
        state.add_pending_request(PendingBuddyRequest {
            from: make_addr(0x01),
            from_name: "Foo".into(),
            received_at: 0.0,
        });
        state.block_player(&make_addr(0x01));
        assert!(state.is_blocked(&make_addr(0x01)));
        assert!(!state.is_buddy(&make_addr(0x01)));
        assert!(state.pending_requests.is_empty());
    }

    #[test]
    fn block_ignores_duplicates() {
        let mut state = BuddyState::new();
        state.block_player(&make_addr(0x02));
        state.block_player(&make_addr(0x02));
        assert_eq!(state.blocked.len(), 1);
    }

    #[test]
    fn unblock_player() {
        let mut state = BuddyState::new();
        state.block_player(&make_addr(0x03));
        assert!(state.unblock_player(&make_addr(0x03)));
        assert!(!state.is_blocked(&make_addr(0x03)));
    }

    #[test]
    fn unblock_returns_false_when_not_blocked() {
        let mut state = BuddyState::new();
        assert!(!state.unblock_player(&make_addr(0x04)));
    }

    // ── Pending requests ──────────────────────────────────────────────────

    #[test]
    fn pending_request_ignored_if_sender_blocked() {
        let mut state = BuddyState::new();
        state.block_player(&make_addr(0x05));
        state.add_pending_request(PendingBuddyRequest {
            from: make_addr(0x05),
            from_name: "Blocked".into(),
            received_at: 0.0,
        });
        assert!(state.pending_requests.is_empty());
    }

    #[test]
    fn pending_request_ignored_if_already_buddy() {
        let mut state = BuddyState::new();
        state.add_buddy(make_entry(0x06));
        state.add_pending_request(PendingBuddyRequest {
            from: make_addr(0x06),
            from_name: "Buddy".into(),
            received_at: 0.0,
        });
        assert!(state.pending_requests.is_empty());
    }

    #[test]
    fn get_pending_request_within_timeout() {
        let mut state = BuddyState::new();
        state.add_pending_request(PendingBuddyRequest {
            from: make_addr(0x07),
            from_name: "Timely".into(),
            received_at: 1000.0,
        });
        assert!(state.get_pending_request(&make_addr(0x07), 1089.0).is_some());
    }

    #[test]
    fn get_pending_request_after_timeout_returns_none() {
        let mut state = BuddyState::new();
        state.add_pending_request(PendingBuddyRequest {
            from: make_addr(0x08),
            from_name: "Late".into(),
            received_at: 1000.0,
        });
        // 91 seconds later — expired
        assert!(state.get_pending_request(&make_addr(0x08), 1091.0).is_none());
    }

    #[test]
    fn expire_requests_removes_old() {
        let mut state = BuddyState::new();
        state.add_pending_request(PendingBuddyRequest {
            from: make_addr(0x09),
            from_name: "Old".into(),
            received_at: 0.0,
        });
        state.add_pending_request(PendingBuddyRequest {
            from: make_addr(0x0A),
            from_name: "Fresh".into(),
            received_at: 500.0,
        });
        state.expire_requests(100.0); // now=100 → first expired, second not
        assert_eq!(state.pending_requests.len(), 1);
        assert_eq!(state.pending_requests[0].from, make_addr(0x0A));
    }

    // ── Buddy data updates ────────────────────────────────────────────────

    #[test]
    fn update_buddy_name_succeeds() {
        let mut state = BuddyState::new();
        state.add_buddy(make_entry(0x0B));
        assert!(state.update_buddy_name(&make_addr(0x0B), "NewName"));
        assert_eq!(state.buddies[0].display_name, "NewName");
    }

    #[test]
    fn update_buddy_name_returns_false_if_not_found() {
        let mut state = BuddyState::new();
        assert!(!state.update_buddy_name(&make_addr(0x0C), "Ghost"));
    }

    #[test]
    fn record_copresence_accumulates() {
        let mut state = BuddyState::new();
        state.add_buddy(make_entry(0x0D));
        state.record_copresence(&make_addr(0x0D), 300.0, "2026-04-10");
        state.record_copresence(&make_addr(0x0D), 120.0, "2026-04-10");
        let b = &state.buddies[0];
        assert!((b.co_presence_total - 420.0).abs() < f64::EPSILON);
        assert_eq!(b.last_seen_date, Some("2026-04-10".into()));
    }

    #[test]
    fn record_copresence_returns_false_if_not_buddy() {
        let mut state = BuddyState::new();
        assert!(!state.record_copresence(&make_addr(0x0E), 100.0, "2026-04-10"));
    }

    // ── Persistence ───────────────────────────────────────────────────────

    #[test]
    fn restore_from_save_round_trips() {
        let mut state = BuddyState::new();
        state.add_buddy(make_entry(0x10));
        state.add_buddy(make_entry(0x11));
        state.block_player(&make_addr(0x20));

        let saves = state.to_save_entries();
        let blocked_hex = state.blocked_to_hex();

        let mut state2 = BuddyState::new();
        state2.restore_from_save(&saves, &blocked_hex);

        assert_eq!(state2.buddies.len(), 2);
        assert!(state2.is_buddy(&make_addr(0x10)));
        assert!(state2.is_buddy(&make_addr(0x11)));
        assert!(state2.is_blocked(&make_addr(0x20)));
    }

    #[test]
    fn restore_from_save_skips_bad_entries() {
        let mut state = BuddyState::new();
        let bad = BuddySaveEntry {
            address_hash: "not-valid-hex".into(),
            display_name: "Ghost".into(),
            added_date: "2026-01-01".into(),
            co_presence_total: 0.0,
            last_seen_date: None,
        };
        state.restore_from_save(&[bad], &[]);
        assert!(state.buddies.is_empty());
    }
}
