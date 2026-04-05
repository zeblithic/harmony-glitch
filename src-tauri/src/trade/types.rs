use crate::item::types::ItemStack;
use serde::{Deserialize, Serialize};

pub type TradeId = u64;

/// Items and currants offered by one side of a trade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TradeOffer {
    /// Aggregated by item_id — one entry per distinct item type.
    pub items: Vec<ItemStack>,
    pub currants: u64,
}

impl TradeOffer {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            currants: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.currants == 0
    }
}

/// Trade protocol messages broadcast to all peers (filtered by recipient).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TradeMessage {
    Request {
        trade_id: TradeId,
        initiator: [u8; 16],
        recipient: [u8; 16],
    },
    Accept {
        trade_id: TradeId,
        responder: [u8; 16],
    },
    Decline {
        trade_id: TradeId,
        responder: [u8; 16],
    },
    Update {
        trade_id: TradeId,
        sender: [u8; 16],
        offer: TradeOffer,
    },
    Lock {
        trade_id: TradeId,
        sender: [u8; 16],
        terms_hash: [u8; 16],
    },
    Unlock {
        trade_id: TradeId,
        sender: [u8; 16],
    },
    Cancel {
        trade_id: TradeId,
        sender: [u8; 16],
    },
    Complete {
        trade_id: TradeId,
        sender: [u8; 16],
    },
}

/// Trade session phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TradePhase {
    /// Waiting for the other party to accept or decline.
    PendingResponse,
    /// Both parties are adjusting offers.
    Negotiating,
    /// Local player locked; waiting for peer.
    LockedLocal,
    /// Peer locked; local player hasn't yet.
    LockedRemote,
    /// Both locked with matching terms_hash — executing.
    Executing,
    /// Trade completed successfully.
    Completed,
    /// Trade was cancelled or failed.
    Cancelled,
}

/// Which role the local player has in this trade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TradeRole {
    Initiator,
    Responder,
}

/// A single active trade session.
#[derive(Debug, Clone)]
pub struct TradeSession {
    pub trade_id: TradeId,
    pub phase: TradePhase,
    pub role: TradeRole,
    pub our_hash: [u8; 16],
    pub peer_hash: [u8; 16],
    pub peer_name: String,
    pub local_offer: TradeOffer,
    pub remote_offer: TradeOffer,
    pub local_terms_hash: Option<[u8; 16]>,
    pub remote_terms_hash: Option<[u8; 16]>,
    pub last_activity: f64,
}

/// Frame sent to frontend for rendering the trade UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeFrame {
    pub trade_id: TradeId,
    pub phase: String,
    pub peer_name: String,
    pub local_offer: TradeOfferFrame,
    pub remote_offer: TradeOfferFrame,
    pub local_locked: bool,
    pub remote_locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeOfferFrame {
    pub items: Vec<TradeItemFrame>,
    pub currants: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeItemFrame {
    pub item_id: String,
    pub name: String,
    pub icon: String,
    pub count: u32,
}

/// Compute BLAKE3 hash of canonical trade terms, truncated to 16 bytes.
/// Both peers compute the same hash regardless of who is initiator/responder.
pub fn compute_terms_hash(
    offer_a: &TradeOffer,
    offer_b: &TradeOffer,
    hash_a: &[u8; 16],
    hash_b: &[u8; 16],
) -> [u8; 16] {
    // Sort by peer hash for deterministic ordering.
    let (first, second) = if hash_a < hash_b {
        (offer_a, offer_b)
    } else {
        (offer_b, offer_a)
    };

    let mut data = Vec::new();
    append_offer_bytes(&mut data, first);
    append_offer_bytes(&mut data, second);

    let hash = blake3::hash(&data);
    hash.as_bytes()[..16].try_into().unwrap()
}

fn append_offer_bytes(data: &mut Vec<u8>, offer: &TradeOffer) {
    let mut items: Vec<&ItemStack> = offer.items.iter().collect();
    items.sort_by(|a, b| a.item_id.cmp(&b.item_id));
    for item in &items {
        // Length-prefix the item_id to prevent collisions (e.g., "ab" + "cd" vs "abc" + "d").
        let id_len = item.item_id.len() as u32;
        data.extend_from_slice(&id_len.to_le_bytes());
        data.extend_from_slice(item.item_id.as_bytes());
        data.extend_from_slice(&item.count.to_le_bytes());
    }
    data.extend_from_slice(&offer.currants.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::types::NetMessage;

    const RETICULUM_MTU: usize = 500;
    const MAX_PAYLOAD: usize = RETICULUM_MTU - 35 - 33;

    #[test]
    fn trade_message_round_trip() {
        let msg = TradeMessage::Request {
            trade_id: 12345,
            initiator: [0xAA; 16],
            recipient: [0xBB; 16],
        };
        let net = NetMessage::Trade(msg);
        let bytes = serde_json::to_vec(&net).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::Trade(TradeMessage::Request {
                trade_id,
                initiator,
                recipient,
            }) => {
                assert_eq!(trade_id, 12345);
                assert_eq!(initiator, [0xAA; 16]);
                assert_eq!(recipient, [0xBB; 16]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn trade_update_fits_in_mtu() {
        // Worst case: 8 distinct items with realistic Glitch item names.
        let offer = TradeOffer {
            items: vec![
                ItemStack { item_id: "cherry_pie".into(), count: 20 },
                ItemStack { item_id: "grain".into(), count: 99 },
                ItemStack { item_id: "bubble_wand".into(), count: 5 },
                ItemStack { item_id: "plank".into(), count: 50 },
                ItemStack { item_id: "steak".into(), count: 10 },
                ItemStack { item_id: "butter".into(), count: 15 },
                ItemStack { item_id: "bread".into(), count: 20 },
                ItemStack { item_id: "pot".into(), count: 1 },
            ],
            currants: 999_999,
        };
        let msg = NetMessage::Trade(TradeMessage::Update {
            trade_id: u64::MAX,
            sender: [0xFF; 16],
            offer,
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "Trade Update is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn trade_lock_fits_in_mtu() {
        let msg = NetMessage::Trade(TradeMessage::Lock {
            trade_id: u64::MAX,
            sender: [0xFF; 16],
            terms_hash: [0xFF; 16],
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "Trade Lock is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn terms_hash_deterministic_regardless_of_role() {
        let offer_a = TradeOffer {
            items: vec![ItemStack { item_id: "cherry".into(), count: 5 }],
            currants: 100,
        };
        let offer_b = TradeOffer {
            items: vec![ItemStack { item_id: "grain".into(), count: 3 }],
            currants: 0,
        };
        let hash_a = [0x01; 16];
        let hash_b = [0x02; 16];

        let h1 = compute_terms_hash(&offer_a, &offer_b, &hash_a, &hash_b);
        let h2 = compute_terms_hash(&offer_b, &offer_a, &hash_b, &hash_a);
        assert_eq!(h1, h2);
    }

    #[test]
    fn terms_hash_differs_for_different_offers() {
        let offer_a = TradeOffer {
            items: vec![ItemStack { item_id: "cherry".into(), count: 5 }],
            currants: 0,
        };
        let offer_b = TradeOffer {
            items: vec![ItemStack { item_id: "cherry".into(), count: 6 }],
            currants: 0,
        };
        let empty = TradeOffer::empty();
        let hash_a = [0x01; 16];
        let hash_b = [0x02; 16];

        let h1 = compute_terms_hash(&offer_a, &empty, &hash_a, &hash_b);
        let h2 = compute_terms_hash(&offer_b, &empty, &hash_a, &hash_b);
        assert_ne!(h1, h2);
    }

    #[test]
    fn empty_offer() {
        let offer = TradeOffer::empty();
        assert!(offer.is_empty());
        assert_eq!(offer.items.len(), 0);
        assert_eq!(offer.currants, 0);
    }
}
