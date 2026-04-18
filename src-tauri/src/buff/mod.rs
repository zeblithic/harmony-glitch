pub mod types;

pub use types::{ActiveBuff, BuffEffect, BuffSpec};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-player container for all active buffs.
/// Keyed by `kind` — same-kind apply overwrites (refresh semantics).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuffState {
    pub active: HashMap<String, ActiveBuff>,
}

impl BuffState {
    /// Apply a buff. If a buff with the same `kind` is already active,
    /// it is replaced in place (refresh semantics).
    pub fn apply(&mut self, spec: &BuffSpec, game_time: f64, source: String) {
        let active = ActiveBuff {
            kind: spec.kind.clone(),
            effect: spec.effect.clone(),
            expires_at: game_time + spec.duration_secs,
            source,
            on_expire: spec.on_expire.clone(),
        };
        self.active.insert(spec.kind.clone(), active);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rookswort_spec(value: f64, duration: f64) -> BuffSpec {
        BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value },
            duration_secs: duration,
            on_expire: None,
        }
    }

    #[test]
    fn apply_inserts_new_buff() {
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 100.0, "rookswort".into());
        assert_eq!(s.active.len(), 1);
        let b = s.active.get("rookswort").unwrap();
        assert!((b.expires_at - 700.0).abs() < 1e-9);
        assert_eq!(b.source, "rookswort");
    }

    #[test]
    fn apply_same_kind_refreshes_expires_at() {
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 100.0, "rookswort".into());
        // 5 minutes later, re-apply
        s.apply(&rookswort_spec(0.5, 600.0), 400.0, "rookswort".into());
        assert_eq!(s.active.len(), 1, "still one buff, not two");
        assert!((s.active["rookswort"].expires_at - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn apply_same_kind_replaces_effect_magnitude() {
        // Tier-upgrade pattern: content layer applies a stronger buff with same kind.
        let mut s = BuffState::default();
        s.apply(&rookswort_spec(0.5, 600.0), 0.0, "rookswort".into());
        s.apply(&rookswort_spec(0.25, 600.0), 0.0, "rookswort_tier2".into());
        let b = s.active.get("rookswort").unwrap();
        assert_eq!(b.effect, BuffEffect::MoodDecayMultiplier { value: 0.25 });
        assert_eq!(b.source, "rookswort_tier2");
    }
}
