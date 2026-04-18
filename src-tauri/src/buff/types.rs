use serde::{Deserialize, Serialize};

/// Effect kinds. v1 ships one variant. Future types are additive — new variants
/// are ignored by `BuffState::mood_decay_multiplier()` via `filter_map`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BuffEffect {
    /// Multiplies mood decay rate. 0.5 = half-rate. 0.0 = halts decay.
    /// Values > 1.0 accelerate decay (debuff). Negative values are clamped
    /// to 0.0 by the mood tick to prevent effective mood gain.
    MoodDecayMultiplier { value: f64 },
}

/// Static buff template loaded from JSON (e.g. `ItemDef.buff_effect`).
/// Duration is relative; resolved to absolute `expires_at` at application time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BuffSpec {
    pub kind: String,
    pub effect: BuffEffect,
    pub duration_secs: f64,
    #[serde(default)]
    pub on_expire: Option<Box<BuffSpec>>,
}

/// Live buff instance in the `BuffState` map.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveBuff {
    pub kind: String,
    pub effect: BuffEffect,
    pub expires_at: f64,
    pub source: String,
    pub on_expire: Option<Box<BuffSpec>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buff_effect_tagged_serialization_shape() {
        // Locks the JSON shape so item catalog files don't silently break.
        let e = BuffEffect::MoodDecayMultiplier { value: 0.5 };
        let json = serde_json::to_string(&e).unwrap();
        assert_eq!(json, r#"{"type":"moodDecayMultiplier","value":0.5}"#);
    }

    #[test]
    fn buff_spec_roundtrips_json() {
        let spec = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            duration_secs: 600.0,
            on_expire: None,
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: BuffSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn buff_spec_with_on_expire_chain_roundtrips_json() {
        let inner = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.75 },
            duration_secs: 180.0,
            on_expire: None,
        };
        let spec = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            duration_secs: 600.0,
            on_expire: Some(Box::new(inner)),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: BuffSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn active_buff_roundtrips_json() {
        let buff = ActiveBuff {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            expires_at: 1234.5,
            source: "rookswort".into(),
            on_expire: None,
        };
        let json = serde_json::to_string(&buff).unwrap();
        let back: ActiveBuff = serde_json::from_str(&json).unwrap();
        assert_eq!(back, buff);
    }

    #[test]
    fn buff_spec_without_on_expire_field_in_json_defaults_to_none() {
        let json = r#"{
            "kind": "rookswort",
            "effect": { "type": "moodDecayMultiplier", "value": 0.5 },
            "durationSecs": 600.0
        }"#;
        let spec: BuffSpec = serde_json::from_str(json).unwrap();
        assert!(spec.on_expire.is_none());
    }
}
