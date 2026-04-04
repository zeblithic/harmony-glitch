use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationState {
    Idle,
    Walking,
    Jumping,
    Falling,
}

/// Complete avatar appearance — stored in GameState, persisted to disk,
/// and eventually broadcast to peers in Phase B.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarAppearance {
    // Vanity (always present)
    pub eyes: String,
    pub ears: String,
    pub nose: String,
    pub mouth: String,
    pub hair: String,

    // Colors (hex strings, no # prefix)
    pub skin_color: String,
    pub hair_color: String,

    // Wardrobe (None = slot empty)
    pub hat: Option<String>,
    pub coat: Option<String>,
    pub shirt: Option<String>,
    pub pants: Option<String>,
    pub dress: Option<String>,
    pub skirt: Option<String>,
    pub shoes: Option<String>,
    pub bracelet: Option<String>,
}

impl Default for AvatarAppearance {
    fn default() -> Self {
        Self {
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
        }
    }
}
