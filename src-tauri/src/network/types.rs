use serde::{Deserialize, Serialize};

/// Compact player state for 60Hz network updates.
/// Uses f32 (not f64) to save wire bytes — sub-pixel precision is unnecessary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PlayerNetState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    /// 0 = left, 1 = right
    pub facing: u8,
    pub on_ground: bool,
}

/// Chat message — ephemeral, no history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    /// UTF-8 text, capped at ~200 chars by sender.
    pub text: String,
    /// Sender's address hash (16 bytes).
    pub sender: [u8; 16],
    /// Sender's display name at time of sending.
    pub sender_name: String,
}

/// Presence event — join/leave a street.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PresenceEvent {
    Joined {
        address_hash: [u8; 16],
        display_name: String,
    },
    Left {
        address_hash: [u8; 16],
    },
}

/// Tagged wrapper for all network messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetMessage {
    PlayerState(PlayerNetState),
    Chat(ChatMessage),
    Presence(PresenceEvent),
}

#[cfg(test)]
mod tests {
    use super::*;

    const RETICULUM_MTU: usize = 500;
    // Worst-case overhead: Reticulum Type2 header (35 bytes) + Zenoh envelope (33 bytes)
    const MAX_PAYLOAD: usize = RETICULUM_MTU - 35 - 33;

    #[test]
    fn player_net_state_round_trip() {
        let state = PlayerNetState {
            x: 123.456,
            y: -789.012,
            vx: 200.0,
            vy: -400.0,
            facing: 1,
            on_ground: true,
        };
        let msg = NetMessage::PlayerState(state);
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::PlayerState(s) => assert_eq!(s, state),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn player_state_fits_in_mtu() {
        let state = PlayerNetState {
            x: -99999.99,
            y: -99999.99,
            vx: 999.99,
            vy: 999.99,
            facing: 1,
            on_ground: true,
        };
        let msg = NetMessage::PlayerState(state);
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "PlayerState is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn chat_message_round_trip() {
        let chat = ChatMessage {
            text: "Hello world!".into(),
            sender: [0xAB; 16],
            sender_name: "Alice".into(),
        };
        let msg = NetMessage::Chat(chat.clone());
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::Chat(c) => {
                assert_eq!(c.text, "Hello world!");
                assert_eq!(c.sender_name, "Alice");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn max_chat_fits_in_mtu() {
        let msg = NetMessage::Chat(ChatMessage {
            text: "x".repeat(200),
            sender: [0xFF; 16],
            sender_name: "A".repeat(30),
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "Max chat is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn presence_joined_round_trip() {
        let msg = NetMessage::Presence(PresenceEvent::Joined {
            address_hash: [0x42; 16],
            display_name: "Bob".into(),
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::Presence(PresenceEvent::Joined { display_name, .. }) => {
                assert_eq!(display_name, "Bob");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn presence_left_round_trip() {
        let hash = [0x99; 16];
        let msg = NetMessage::Presence(PresenceEvent::Left {
            address_hash: hash,
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::Presence(PresenceEvent::Left { address_hash }) => {
                assert_eq!(address_hash, hash);
            }
            _ => panic!("wrong variant"),
        }
    }
}
