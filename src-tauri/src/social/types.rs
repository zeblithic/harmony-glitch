use serde::{Deserialize, Serialize};

/// All social-layer messages sent over the network.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SocialMessage {
    BuddyRequest { from: [u8; 16] },
    BuddyAccept { from: [u8; 16] },
    BuddyDecline { from: [u8; 16] },
    BuddyRemove { from: [u8; 16] },
    PartyInvite { leader: [u8; 16], members: Vec<[u8; 16]> },
    PartyAccept { from: [u8; 16] },
    PartyDecline { from: [u8; 16] },
    PartyLeave { from: [u8; 16] },
    PartyKick { target: [u8; 16] },
    PartyMemberJoined { member: [u8; 16], display_name: String },
    PartyMemberLeft { member: [u8; 16] },
    PartyDissolved,
    PartyLeaderChanged { new_leader: [u8; 16] },
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
        let msg = SocialMessage::BuddyRequest { from: [0x01; 16] };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SocialMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn party_invite_round_trip() {
        let msg = SocialMessage::PartyInvite {
            leader: [0xAA; 16],
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
