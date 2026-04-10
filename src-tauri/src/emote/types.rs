use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Which animation variant the Hi emote plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiVariant {
    Bats,
    Birds,
    Butterflies,
    Cubes,
    Flowers,
    Hands,
    Hearts,
    Hi,
    Pigs,
    Rocketships,
    Stars,
}

impl HiVariant {
    pub const ALL: [HiVariant; 11] = [
        HiVariant::Bats,
        HiVariant::Birds,
        HiVariant::Butterflies,
        HiVariant::Cubes,
        HiVariant::Flowers,
        HiVariant::Hands,
        HiVariant::Hearts,
        HiVariant::Hi,
        HiVariant::Pigs,
        HiVariant::Rocketships,
        HiVariant::Stars,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            HiVariant::Bats => "bats",
            HiVariant::Birds => "birds",
            HiVariant::Butterflies => "butterflies",
            HiVariant::Cubes => "cubes",
            HiVariant::Flowers => "flowers",
            HiVariant::Hands => "hands",
            HiVariant::Hearts => "hearts",
            HiVariant::Hi => "hi",
            HiVariant::Pigs => "pigs",
            HiVariant::Rocketships => "rocketships",
            HiVariant::Stars => "stars",
        }
    }
}

/// Discriminant for which emote family is being sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmoteType {
    Hi,
}

/// Wire message for an emote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmoteMessage {
    pub emote_type: EmoteType,
    pub variant: HiVariant,
    /// Targeted player identity (16 bytes), or None for a broadcast.
    pub target: Option<[u8; 16]>,
}

/// Derive this player's daily Hi variant from their identity and today's date.
///
/// Hashes `identity || "hi-variant" || date` with BLAKE3, then takes
/// `hash[0] % 11` to select a variant index.
pub fn daily_variant(identity: &[u8; 16], date: &str) -> HiVariant {
    let mut hasher = blake3::Hasher::new();
    hasher.update(identity);
    hasher.update(b"hi-variant");
    hasher.update(date.as_bytes());
    let hash = hasher.finalize();
    let idx = (hash.as_bytes()[0] as usize) % HiVariant::ALL.len();
    HiVariant::ALL[idx]
}

/// Per-session emote state — tracks who we've greeted and who greeted us.
pub struct EmoteState {
    /// Players we've sent a Hi to today (no repeats per day).
    pub hi_today: HashSet<[u8; 16]>,
    /// Players who sent us a Hi today.
    pub hi_received_today: HashSet<[u8; 16]>,
    /// Variant we "caught" from a matching Hi — overrides daily seed.
    pub caught_variant: Option<HiVariant>,
    /// Our own identity, used for daily variant seeding.
    pub identity: [u8; 16],
    /// The date string we were initialised for (YYYY-MM-DD).
    pub current_date: String,
}

impl EmoteState {
    pub fn new(identity: [u8; 16], date: impl Into<String>) -> Self {
        Self {
            hi_today: HashSet::new(),
            hi_received_today: HashSet::new(),
            caught_variant: None,
            identity,
            current_date: date.into(),
        }
    }

    /// The variant we're currently broadcasting.
    /// Uses `caught_variant` if set, otherwise falls back to the daily seed.
    pub fn active_variant(&self) -> HiVariant {
        self.caught_variant
            .unwrap_or_else(|| daily_variant(&self.identity, &self.current_date))
    }

    /// If the date has changed, clear all daily tracking state.
    pub fn check_date_change(&mut self, date: &str) {
        if self.current_date != date {
            self.current_date = date.to_owned();
            self.hi_today.clear();
            self.hi_received_today.clear();
            self.caught_variant = None;
        }
    }

    /// Returns `true` if we haven't sent a Hi to `target` today.
    pub fn can_hi(&self, target: &[u8; 16]) -> bool {
        !self.hi_today.contains(target)
    }

    /// Record that we sent a Hi to `target`.
    pub fn record_hi_sent(&mut self, target: [u8; 16]) {
        self.hi_today.insert(target);
    }

    /// Handle an incoming Hi from `sender` carrying `sender_variant`.
    ///
    /// Returns the mood delta:
    /// - `0.0` if sender is blocked or we already received a Hi from them today.
    /// - `5.0` if variants don't match.
    /// - `10.0` if variants match (also catches the sender's variant).
    pub fn handle_incoming_hi(
        &mut self,
        sender: [u8; 16],
        sender_variant: HiVariant,
        blocked: bool,
    ) -> f64 {
        if blocked {
            return 0.0;
        }
        if self.hi_received_today.contains(&sender) {
            return 0.0;
        }
        self.hi_received_today.insert(sender);

        if sender_variant == self.active_variant() {
            self.caught_variant = Some(sender_variant);
            10.0
        } else {
            5.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_identity(seed: u8) -> [u8; 16] {
        [seed; 16]
    }

    // ── daily_variant ──────────────────────────────────────────────────────────

    #[test]
    fn daily_variant_deterministic_same_inputs() {
        let id = test_identity(0xAA);
        let date = "2026-04-10";
        assert_eq!(daily_variant(&id, date), daily_variant(&id, date));
    }

    #[test]
    fn daily_variant_differs_across_dates() {
        let id = test_identity(0x01);
        let dates = [
            "2026-01-01", "2026-01-02", "2026-01-03", "2026-01-04", "2026-01-05",
            "2026-01-06", "2026-01-07", "2026-01-08", "2026-01-09", "2026-01-10",
        ];
        let variants: HashSet<_> = dates.iter().map(|d| daily_variant(&id, d)).collect();
        assert!(
            variants.len() >= 2,
            "Expected at least 2 distinct variants across 10 dates, got {}",
            variants.len()
        );
    }

    #[test]
    fn daily_variant_differs_across_identities() {
        let date = "2026-04-10";
        let variants: HashSet<_> = (0u8..20).map(|i| daily_variant(&test_identity(i), date)).collect();
        assert!(
            variants.len() >= 2,
            "Expected at least 2 distinct variants across 20 identities, got {}",
            variants.len()
        );
    }

    // ── active_variant ─────────────────────────────────────────────────────────

    #[test]
    fn active_variant_uses_daily_seed_by_default() {
        let id = test_identity(0x10);
        let date = "2026-04-10";
        let state = EmoteState::new(id, date);
        assert_eq!(state.active_variant(), daily_variant(&id, date));
    }

    #[test]
    fn active_variant_uses_caught_variant_when_set() {
        let id = test_identity(0x20);
        let mut state = EmoteState::new(id, "2026-04-10");
        state.caught_variant = Some(HiVariant::Stars);
        assert_eq!(state.active_variant(), HiVariant::Stars);
    }

    // ── can_hi / record_hi_sent ────────────────────────────────────────────────

    #[test]
    fn can_hi_true_for_new_target() {
        let state = EmoteState::new(test_identity(0x30), "2026-04-10");
        assert!(state.can_hi(&test_identity(0x99)));
    }

    #[test]
    fn can_hi_false_after_sending() {
        let mut state = EmoteState::new(test_identity(0x31), "2026-04-10");
        let target = test_identity(0x99);
        state.record_hi_sent(target);
        assert!(!state.can_hi(&target));
    }

    #[test]
    fn can_hi_true_for_different_player_after_sending() {
        let mut state = EmoteState::new(test_identity(0x32), "2026-04-10");
        state.record_hi_sent(test_identity(0x01));
        assert!(state.can_hi(&test_identity(0x02)));
    }

    // ── handle_incoming_hi ────────────────────────────────────────────────────

    /// Force `active_variant` to return a specific value by picking an identity
    /// whose daily seed for the test date matches `wanted`, or by using
    /// `caught_variant`.
    fn state_with_active_variant(active: HiVariant) -> EmoteState {
        let date = "2026-04-10";
        // Brute-force a seed identity whose daily variant matches `active`.
        for seed in 0u8..=255 {
            let id = test_identity(seed);
            if daily_variant(&id, date) == active {
                return EmoteState::new(id, date);
            }
        }
        // Fallback: any identity + override via caught_variant.
        let mut s = EmoteState::new(test_identity(0xFF), date);
        s.caught_variant = Some(active);
        s
    }

    #[test]
    fn handle_incoming_hi_no_match_gives_5() {
        // Find a sender variant that doesn't match ours.
        let date = "2026-04-10";
        let id = test_identity(0x42);
        let mut state = EmoteState::new(id, date);
        let our_variant = state.active_variant();
        let other_variant = HiVariant::ALL
            .iter()
            .copied()
            .find(|&v| v != our_variant)
            .unwrap();
        let delta = state.handle_incoming_hi(test_identity(0xAA), other_variant, false);
        assert_eq!(delta, 5.0);
    }

    #[test]
    fn handle_incoming_hi_match_gives_10() {
        let date = "2026-04-10";
        let id = test_identity(0x55);
        let our_variant = daily_variant(&id, date);
        let mut state = EmoteState::new(id, date);
        let delta = state.handle_incoming_hi(test_identity(0xBB), our_variant, false);
        assert_eq!(delta, 10.0);
    }

    #[test]
    fn handle_incoming_hi_catches_senders_variant() {
        let date = "2026-04-10";
        let id = test_identity(0x66);
        let our_variant = daily_variant(&id, date);
        let mut state = EmoteState::new(id, date);
        // Send a matching Hi so the variant gets caught.
        state.handle_incoming_hi(test_identity(0xCC), our_variant, false);
        assert_eq!(state.caught_variant, Some(our_variant));
    }

    #[test]
    fn handle_incoming_hi_blocked_returns_0() {
        let date = "2026-04-10";
        let id = test_identity(0x77);
        let our_variant = daily_variant(&id, date);
        let mut state = EmoteState::new(id, date);
        let delta = state.handle_incoming_hi(test_identity(0xDD), our_variant, true);
        assert_eq!(delta, 0.0);
    }

    #[test]
    fn handle_incoming_hi_duplicate_returns_0() {
        let date = "2026-04-10";
        let id = test_identity(0x88);
        let our_variant = daily_variant(&id, date);
        let mut state = EmoteState::new(id, date);
        let sender = test_identity(0xEE);
        state.handle_incoming_hi(sender, our_variant, false); // first — counted
        let delta = state.handle_incoming_hi(sender, our_variant, false); // duplicate
        assert_eq!(delta, 0.0);
    }

    // ── date change ────────────────────────────────────────────────────────────

    #[test]
    fn date_change_clears_daily_state() {
        let id = test_identity(0xA0);
        let mut state = EmoteState::new(id, "2026-04-10");
        let target = test_identity(0x01);
        state.record_hi_sent(target);
        state.hi_received_today.insert(test_identity(0x02));
        state.caught_variant = Some(HiVariant::Stars);

        state.check_date_change("2026-04-11");

        assert!(state.hi_today.is_empty());
        assert!(state.hi_received_today.is_empty());
        assert!(state.caught_variant.is_none());
        assert_eq!(state.current_date, "2026-04-11");
    }

    #[test]
    fn date_change_noop_when_same_date() {
        let id = test_identity(0xB0);
        let mut state = EmoteState::new(id, "2026-04-10");
        let target = test_identity(0x01);
        state.record_hi_sent(target);
        state.caught_variant = Some(HiVariant::Stars);

        state.check_date_change("2026-04-10"); // same date

        assert!(!state.hi_today.is_empty());
        assert_eq!(state.caught_variant, Some(HiVariant::Stars));
    }

    // ── serialization ─────────────────────────────────────────────────────────

    #[test]
    fn emote_message_serialization_round_trip() {
        let msg = EmoteMessage {
            emote_type: EmoteType::Hi,
            variant: HiVariant::Hearts,
            target: Some([0xAB; 16]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: EmoteMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.variant, msg.variant);
        assert_eq!(restored.target, msg.target);
        match restored.emote_type {
            EmoteType::Hi => {}
        }
    }

    #[test]
    fn emote_message_fits_within_mtu() {
        // Reticulum MTU 500, minus 35 header + 33 Zenoh overhead = 432 max payload.
        const MAX_PAYLOAD: usize = 500 - 35 - 33;
        let msg = EmoteMessage {
            emote_type: EmoteType::Hi,
            variant: HiVariant::Rocketships, // longest variant name
            target: Some([0xFF; 16]),
        };
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(
            bytes.len() < 500,
            "EmoteMessage is {} bytes, must be < 500",
            bytes.len()
        );
        assert!(
            bytes.len() <= MAX_PAYLOAD,
            "EmoteMessage is {} bytes, max payload is {}",
            bytes.len(),
            MAX_PAYLOAD
        );
    }
}
