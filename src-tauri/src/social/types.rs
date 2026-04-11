use serde::{Deserialize, Serialize};

/// All social-layer messages sent over the network.
///
/// **Directed** variants carry a `to` field — receivers whose address doesn't
/// match should silently drop the message.  **Party-control** variants carry a
/// `from` field that must equal the authenticated sender (spoofing protection)
/// and should be authorised against local party state (e.g. leader check).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SocialMessage {
    // ── Directed buddy messages ────────────────────────────────────────
    BuddyRequest { from: [u8; 16], to: [u8; 16] },
    BuddyAccept { from: [u8; 16], to: [u8; 16] },
    BuddyDecline { from: [u8; 16], to: [u8; 16] },
    BuddyRemove { from: [u8; 16], to: [u8; 16] },

    // ── Directed party messages ────────────────────────────────────────
    PartyInvite { leader: [u8; 16], to: [u8; 16], members: Vec<[u8; 16]> },
    PartyAccept { from: [u8; 16], to: [u8; 16] },
    PartyDecline { from: [u8; 16], to: [u8; 16] },

    // ── Self-authenticating party broadcasts ───────────────────────────
    PartyLeave { from: [u8; 16] },
    PartyKick { from: [u8; 16], target: [u8; 16] },

    // ── Leader-authorised party notifications ──────────────────────────
    PartyMemberJoined { from: [u8; 16], member: [u8; 16], display_name: String },
    PartyMemberLeft { from: [u8; 16], member: [u8; 16] },
    PartyDissolved { from: [u8; 16] },
    PartyLeaderChanged { from: [u8; 16], new_leader: [u8; 16] },
}

/// Buddy entry as stored in the persistent save file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BuddySaveEntry {
    pub address_hash: String,
    pub display_name: String,
    pub added_date: String,
    pub co_presence_total: f64,
    pub last_seen_date: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buddy_request_round_trip() {
        let msg = SocialMessage::BuddyRequest { from: [0x01; 16], to: [0x02; 16] };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SocialMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn party_invite_round_trip() {
        let msg = SocialMessage::PartyInvite {
            leader: [0xAA; 16],
            to: [0xDD; 16],
            members: vec![[0xBB; 16], [0xCC; 16]],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SocialMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn buddy_save_entry_round_trip() {
        let entry = BuddySaveEntry {
            address_hash: "deadbeefdeadbeefdeadbeefdeadbeef".into(),
            display_name: "Alice".into(),
            added_date: "2026-01-01".into(),
            co_presence_total: 3600.5,
            last_seen_date: Some("2026-04-10".into()),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: BuddySaveEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, decoded);
        // Verify camelCase keys
        assert!(json.contains("addressHash"));
        assert!(json.contains("displayName"));
        assert!(json.contains("addedDate"));
        assert!(json.contains("coPresenceTotal"));
        assert!(json.contains("lastSeenDate"));
    }
}
