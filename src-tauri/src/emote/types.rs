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

/// Tagged union of all emote kinds. Hi carries its cosmetic variant; others
/// have no inner data. Wire format is `{"kind":{"hi":"bats"},"target":null}`
/// for Hi or `{"kind":"hug","target":"..."}` for unit variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmoteKind {
    Hi(HiVariant),
    Dance,
    Wave,
    Hug,
    HighFive,
    Applaud,
}

/// Discriminant of `EmoteKind` — collapses all Hi variants into a single
/// `Hi` tag. Used as a hashmap key for cooldown tracking where we care
/// about "which kind of emote" but not "which cosmetic variant of Hi".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmoteKindTag {
    Hi,
    Dance,
    Wave,
    Hug,
    HighFive,
    Applaud,
}

impl From<&EmoteKind> for EmoteKindTag {
    fn from(kind: &EmoteKind) -> Self {
        match kind {
            EmoteKind::Hi(_) => EmoteKindTag::Hi,
            EmoteKind::Dance => EmoteKindTag::Dance,
            EmoteKind::Wave => EmoteKindTag::Wave,
            EmoteKind::Hug => EmoteKindTag::Hug,
            EmoteKind::HighFive => EmoteKindTag::HighFive,
            EmoteKind::Applaud => EmoteKindTag::Applaud,
        }
    }
}

/// Wire message for any emote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmoteMessage {
    pub kind: EmoteKind,
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

/// Per-session emote state — tracks Hi-specific greeting state, shared
/// cooldowns, and per-emote privacy toggles.
#[derive(Debug, Clone)]
pub struct EmoteState {
    // Hi-specific (unchanged)
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

    // Shared cooldowns (NEW)
    pub cooldowns: super::cooldowns::CooldownTracker,

    // Per-emote privacy toggles (NEW — default true = accept)
    pub accept_hug: bool,
    pub accept_high_five: bool,
}

impl EmoteState {
    pub fn new(identity: [u8; 16], date: impl Into<String>) -> Self {
        Self {
            hi_today: HashSet::new(),
            hi_received_today: HashSet::new(),
            caught_variant: None,
            identity,
            current_date: date.into(),
            cooldowns: super::cooldowns::CooldownTracker::default(),
            accept_hug: true,
            accept_high_five: true,
        }
    }

    /// Update the identity used for daily variant seeding.
    pub fn set_identity(&mut self, identity: [u8; 16]) {
        self.identity = identity;
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
    /// Always adopts the sender's variant (viral spreading).
    ///
    /// Returns the mood delta:
    /// - `0.0` if sender is blocked or we already received a Hi from them today.
    /// - `5.0` if sender's variant didn't match our old active variant.
    /// - `10.0` if sender's variant matched our old active variant.
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

        let old_variant = self.active_variant();
        self.caught_variant = Some(sender_variant); // Always adopt (viral spreading)
        if sender_variant == old_variant {
            10.0 // Natural match bonus
        } else {
            5.0 // Non-matching, but adopted their variant
        }
    }

    /// Is this emote kind currently accepted by this player?
    /// Hug and HighFive have privacy toggles; others always accept.
    pub fn privacy_accepts(&self, tag: EmoteKindTag) -> bool {
        match tag {
            EmoteKindTag::Hug => self.accept_hug,
            EmoteKindTag::HighFive => self.accept_high_five,
            _ => true,
        }
    }

    /// Toggle a privacy flag. No-op for kinds without a toggle.
    pub fn set_privacy(&mut self, tag: EmoteKindTag, accept: bool) {
        match tag {
            EmoteKindTag::Hug => self.accept_hug = accept,
            EmoteKindTag::HighFive => self.accept_high_five = accept,
            _ => {}
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
    fn handle_incoming_hi_viral_spreading_adopts_non_matching_variant() {
        let date = "2026-04-10";
        let id = test_identity(0x42);
        let mut state = EmoteState::new(id, date);
        let our_variant = state.active_variant();
        let other_variant = HiVariant::ALL
            .iter()
            .copied()
            .find(|&v| v != our_variant)
            .unwrap();
        state.handle_incoming_hi(test_identity(0xAA), other_variant, false);
        // After receiving a non-matching Hi, active_variant should change
        assert_eq!(state.active_variant(), other_variant);
        assert_eq!(state.caught_variant, Some(other_variant));
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
    fn emote_kind_serde_round_trip_hi_with_variant() {
        let msg = EmoteMessage {
            kind: EmoteKind::Hi(HiVariant::Hearts),
            target: Some([0xAB; 16]),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let restored: EmoteMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.kind, EmoteKind::Hi(HiVariant::Hearts));
        assert_eq!(restored.target, msg.target);
    }

    #[test]
    fn emote_kind_serde_round_trip_unit_variants() {
        for kind in [EmoteKind::Dance, EmoteKind::Wave, EmoteKind::Hug, EmoteKind::HighFive, EmoteKind::Applaud] {
            let msg = EmoteMessage { kind: kind.clone(), target: None };
            let json = serde_json::to_string(&msg).unwrap();
            let restored: EmoteMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(restored.kind, kind);
            assert!(restored.target.is_none());
        }
    }

    #[test]
    fn emote_kind_wire_format_is_snake_case() {
        let msg = EmoteMessage { kind: EmoteKind::HighFive, target: None };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"high_five\""), "got: {json}");
    }

    #[test]
    fn emote_message_fits_within_mtu() {
        // Reticulum MTU 500, minus 35 header + 33 Zenoh overhead = 432 max payload.
        const MAX_PAYLOAD: usize = 500 - 35 - 33;
        let msg = EmoteMessage {
            kind: EmoteKind::Hi(HiVariant::Rocketships), // longest variant name
            target: Some([0xFF; 16]),
        };
        let bytes = serde_json::to_vec(&msg).unwrap();
        assert!(bytes.len() <= MAX_PAYLOAD, "EmoteMessage is {} bytes, max {}", bytes.len(), MAX_PAYLOAD);
    }

    // ── EmoteKindTag ──────────────────────────────────────────────────────────

    #[test]
    fn emote_kind_tag_collapses_hi_variants() {
        let tag_a: EmoteKindTag = (&EmoteKind::Hi(HiVariant::Stars)).into();
        let tag_b: EmoteKindTag = (&EmoteKind::Hi(HiVariant::Hearts)).into();
        assert_eq!(tag_a, tag_b);
        assert_eq!(tag_a, EmoteKindTag::Hi);
    }

    #[test]
    fn emote_kind_tag_distinct_per_non_hi_kind() {
        let tags: HashSet<EmoteKindTag> = [
            EmoteKind::Dance, EmoteKind::Wave, EmoteKind::Hug,
            EmoteKind::HighFive, EmoteKind::Applaud,
        ].iter().map(EmoteKindTag::from).collect();
        assert_eq!(tags.len(), 5);
    }

    #[test]
    fn emote_kind_tag_is_hash_key() {
        let mut map = std::collections::HashMap::new();
        map.insert(EmoteKindTag::Hug, 1);
        map.insert(EmoteKindTag::HighFive, 2);
        assert_eq!(map.get(&EmoteKindTag::Hug), Some(&1));
        assert_eq!(map.get(&EmoteKindTag::HighFive), Some(&2));
    }

    // ── EmoteState privacy and cooldowns ───────────────────────────────────

    #[test]
    fn emote_state_new_has_permissive_privacy_defaults() {
        let s = EmoteState::new(test_identity(0x01), "2026-04-10");
        assert!(s.accept_hug);
        assert!(s.accept_high_five);
    }

    #[test]
    fn set_emote_privacy_updates_only_named_kind() {
        let mut s = EmoteState::new(test_identity(0x01), "2026-04-10");
        s.set_privacy(EmoteKindTag::Hug, false);
        assert!(!s.accept_hug);
        assert!(s.accept_high_five);
    }

    #[test]
    fn privacy_accepts_returns_true_for_non_privacy_kinds() {
        let s = EmoteState::new(test_identity(0x01), "2026-04-10");
        assert!(s.privacy_accepts(EmoteKindTag::Dance));
        assert!(s.privacy_accepts(EmoteKindTag::Wave));
        assert!(s.privacy_accepts(EmoteKindTag::Applaud));
        assert!(s.privacy_accepts(EmoteKindTag::Hi));
    }

    #[test]
    fn privacy_accepts_gates_hug_and_high_five() {
        let mut s = EmoteState::new(test_identity(0x01), "2026-04-10");
        s.set_privacy(EmoteKindTag::Hug, false);
        assert!(!s.privacy_accepts(EmoteKindTag::Hug));
        assert!(s.privacy_accepts(EmoteKindTag::HighFive));
    }
}
