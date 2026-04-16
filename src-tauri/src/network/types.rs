use crate::avatar::types::AvatarAppearance;
use crate::trade::types::TradeMessage;
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
    /// 0 = idle, 1 = walking, 2 = jumping, 3 = falling
    pub animation: u8,
}

/// Chat channel — street (public) or party (private).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChatChannel {
    #[default]
    Street,
    Party,
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
    /// Chat channel (street or party).
    #[serde(default)]
    pub channel: ChatChannel,
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
    AvatarUpdate(Box<AvatarAppearance>),
    Trade(TradeMessage),
    Gossip(crate::trust::gossip::GossipEnvelope),
    Vouch(crate::trust::epoch::VouchMessage),
    Emote(crate::emote::EmoteMessage),
    Social(crate::social::SocialMessage),
    GroupOp { group_id: [u8; 16], op: harmony_groups::GroupOp },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avatar::types::AvatarAppearance;

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
            animation: 1,
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
            animation: 3,
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
            channel: ChatChannel::Street,
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
        // ASCII worst case: 200 bytes of text
        let msg = NetMessage::Chat(ChatMessage {
            text: "x".repeat(200),
            sender: [0xFF; 16],
            sender_name: "A".repeat(30),
            channel: ChatChannel::Street,
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "Max ASCII chat is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );

        // Emoji worst case: 200 bytes of 4-byte emoji (50 emoji max).
        // Truncation is byte-based, so this should still fit.
        let emoji_text: String = "😀".repeat(50); // 50 * 4 = 200 bytes
        let msg_emoji = NetMessage::Chat(ChatMessage {
            text: emoji_text,
            sender: [0xFF; 16],
            sender_name: "A".repeat(30),
            channel: ChatChannel::Street,
        });
        let bytes_emoji = serde_json::to_vec(&msg_emoji).unwrap();
        assert!(
            bytes_emoji.len() <= MAX_PAYLOAD,
            "Max emoji chat is {} bytes, max is {}",
            bytes_emoji.len(),
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
        let msg = NetMessage::Presence(PresenceEvent::Left { address_hash: hash });
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::Presence(PresenceEvent::Left { address_hash }) => {
                assert_eq!(address_hash, hash);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn avatar_update_round_trip() {
        let avatar = AvatarAppearance::default();
        let msg = NetMessage::AvatarUpdate(Box::new(avatar.clone()));
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::AvatarUpdate(a) => assert_eq!(*a, avatar),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn gossip_envelope_fits_in_mtu() {
        use crate::trust::gossip::GossipEnvelope;
        let envelope = GossipEnvelope {
            subject: [0xFF; 16],
            belief: 0.0,
            disbelief: 0.999999999,
            uncertainty: 0.000000001,
            violations: 999999,
            hop: 3,
        };
        let msg = NetMessage::Gossip(envelope);
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "GossipEnvelope is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn vouch_message_round_trip() {
        use crate::trust::epoch::VouchMessage;
        let vouch = VouchMessage {
            subject: [0x42; 16],
        };
        let msg = NetMessage::Vouch(vouch.clone());
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::Vouch(v) => assert_eq!(v, vouch),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn vouch_message_fits_in_mtu() {
        use crate::trust::epoch::VouchMessage;
        let vouch = VouchMessage {
            subject: [0xFF; 16],
        };
        let msg = NetMessage::Vouch(vouch);
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "VouchMessage is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn avatar_update_typical_fits_in_mtu() {
        // Typical avatar with real Glitch item names.
        // AvatarUpdate is infrequent (every 5s + on change), so
        // Reticulum link fragmentation handles edge cases gracefully.
        let avatar = AvatarAppearance {
            eyes: "eyes_01".into(),
            ears: "ears_0001".into(),
            nose: "nose_0001".into(),
            mouth: "mouth_01".into(),
            hair: "Buzzcut".into(),
            skin_color: "D4C159".into(),
            hair_color: "4A3728".into(),
            hat: None,
            coat: None,
            shirt: Some("Bandana_Tank".into()),
            pants: Some("Boardwalk_Empire_ladies_pants".into()),
            dress: None,
            skirt: None,
            shoes: Some("Men_DressShoes".into()),
            bracelet: None,
        };
        let msg = NetMessage::AvatarUpdate(Box::new(avatar));
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "Typical AvatarUpdate is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn net_message_emote_round_trip() {
        let msg = NetMessage::Emote(crate::emote::EmoteMessage {
            emote_type: crate::emote::EmoteType::Hi,
            variant: crate::emote::HiVariant::Hearts,
            target: Some([1u8; 16]),
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::Emote(e) => {
                assert_eq!(e.variant, crate::emote::HiVariant::Hearts);
                assert_eq!(e.target, Some([1u8; 16]));
            }
            _ => panic!("Expected Emote variant"),
        }
    }

    #[test]
    fn emote_message_fits_in_mtu() {
        let msg = NetMessage::Emote(crate::emote::EmoteMessage {
            emote_type: crate::emote::EmoteType::Hi,
            variant: crate::emote::HiVariant::Rocketships,
            target: Some([0xFF; 16]),
        });
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "EmoteMessage is {} bytes, max is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }

    #[test]
    fn chat_message_defaults_to_street_channel() {
        let json = r#"{"text":"hello","sender":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"sender_name":"Alice"}"#;
        let msg: ChatMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.channel, ChatChannel::Street);
    }

    #[test]
    fn chat_message_party_channel_round_trip() {
        let msg = ChatMessage {
            text: "hello team".to_string(),
            sender: [1u8; 16],
            sender_name: "Alice".to_string(),
            channel: ChatChannel::Party,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ChatMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.channel, ChatChannel::Party);
    }

    #[test]
    fn group_op_round_trip() {
        use harmony_groups::{GroupAction, GroupMode, GroupOp};
        let (op, _) = GroupOp::new_unsigned(
            vec![],
            [0x01; 16],
            1_700_000_000,
            GroupAction::Create {
                group_id: [0xAA; 16],
                name: "Test Group".into(),
                mode: GroupMode::InviteOnly,
            },
        );
        let msg = NetMessage::GroupOp { group_id: [0xAA; 16], op: op.clone() };
        let bytes = serde_json::to_vec(&msg).unwrap();
        let decoded: NetMessage = serde_json::from_slice(&bytes).unwrap();
        match decoded {
            NetMessage::GroupOp { group_id, op: decoded_op } => {
                assert_eq!(group_id, [0xAA; 16]);
                assert_eq!(decoded_op.id, op.id);
                assert_eq!(decoded_op.author, op.author);
                assert_eq!(decoded_op.timestamp, op.timestamp);
            }
            _ => panic!("wrong variant"),
        }
    }
}
