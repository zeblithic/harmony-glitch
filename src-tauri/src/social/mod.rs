pub mod buddy;
pub mod groups;
pub mod party;
pub mod types;

pub use buddy::BuddyState;
pub use party::PartyState;
pub use types::{BuddySaveEntry, SocialMessage};

use crate::buff::BuffState;
use crate::emote::EmoteState;
use crate::mood::MoodState;

#[derive(Debug, Clone)]
pub struct SocialState {
    pub mood: MoodState,
    pub emotes: EmoteState,
    pub buddies: BuddyState,
    pub party: PartyState,
    pub buffs: BuffState,
}

pub struct SocialTickContext<'a> {
    pub current_date: &'a str,
    pub in_dialogue: bool,
    pub game_time: f64,
}

impl SocialState {
    pub fn new(identity: [u8; 16], date: &str) -> Self {
        Self {
            mood: MoodState::default(),
            emotes: EmoteState::new(identity, date),
            buddies: BuddyState::default(),
            party: PartyState::default(),
            buffs: BuffState::default(),
        }
    }

    /// Update the identity used for emote daily variant seeding.
    pub fn set_identity(&mut self, identity: [u8; 16]) {
        self.emotes.set_identity(identity);
    }

    pub fn tick(&mut self, dt: f64, ctx: &SocialTickContext) {
        self.emotes.check_date_change(ctx.current_date);

        // Expire buffs before reading the modifier so the current frame
        // sees a consistent active set.
        self.buffs.tick(ctx.game_time);

        let party_factor = if self.party.has_party_bonus() { 0.75 } else { 1.0 };
        let buff_factor = self.buffs.mood_decay_multiplier();
        let decay_modifier = party_factor * buff_factor;

        self.mood.tick(dt, ctx.game_time, ctx.in_dialogue, decay_modifier);
        self.buddies.expire_requests(ctx.game_time);
        self.buddies.expire_outgoing_requests(ctx.game_time);
        self.party.expire_invite(ctx.game_time);
        self.party.expire_outgoing_invites(ctx.game_time);
        self.party.expire_pending_join(ctx.game_time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::social::buddy::PendingBuddyRequest;
    use crate::social::party::{PartyMember};

    fn make_social() -> SocialState {
        SocialState::new([0u8; 16], "2026-04-10")
    }

    #[test]
    fn social_state_tick_decays_mood() {
        let mut s = make_social();
        // Give mood a high value so we can detect decay
        s.mood.mood = 100.0;
        s.mood.max_mood = 100.0;
        let ctx = SocialTickContext {
            current_date: "2026-04-10",
            in_dialogue: false,
            // game_time > mood_grace_until (which starts at 0), so decay should happen
            game_time: 1000.0,
        };
        s.tick(60.0, &ctx);
        assert!(s.mood.mood < 100.0, "mood should have decayed");
    }

    #[test]
    fn social_state_tick_with_party_bonus() {
        // Two members → party bonus active → less decay than without party
        let mut s_party = make_social();
        s_party.mood.mood = 100.0;
        s_party.mood.max_mood = 100.0;
        s_party.party.create_party([1u8; 16], "Alice".into(), 0.0);
        s_party
            .party
            .party
            .as_mut()
            .unwrap()
            .add_member(PartyMember {
                address_hash: [2u8; 16],
                display_name: "Bob".into(),
                joined_at: 0.0,
            })
            .unwrap();

        let mut s_solo = make_social();
        s_solo.mood.mood = 100.0;
        s_solo.mood.max_mood = 100.0;

        let ctx_party = SocialTickContext {
            current_date: "2026-04-10",
            in_dialogue: false,
            game_time: 1000.0,
        };
        let ctx_solo = SocialTickContext {
            current_date: "2026-04-10",
            in_dialogue: false,
            game_time: 1000.0,
        };

        s_party.tick(60.0, &ctx_party);
        s_solo.tick(60.0, &ctx_solo);

        // Party members decay less
        assert!(
            s_party.mood.mood >= s_solo.mood.mood,
            "party member should have >= mood than solo player"
        );
    }

    #[test]
    fn social_state_tick_clears_expired_requests() {
        let mut s = make_social();
        s.buddies.add_pending_request(PendingBuddyRequest {
            from: [9u8; 16],
            from_name: "Old".into(),
            received_at: 0.0,
        });
        let ctx = SocialTickContext {
            current_date: "2026-04-10",
            in_dialogue: false,
            game_time: 200.0, // well past 90-second timeout
        };
        s.tick(1.0, &ctx);
        assert!(
            s.buddies.pending_requests.is_empty(),
            "expired request should be cleared"
        );
    }

    #[test]
    fn social_state_tick_date_change_clears_emote_state() {
        let mut s = make_social();
        // Record a hi sent today
        s.emotes.record_hi_sent([5u8; 16]);
        assert!(!s.emotes.hi_today.is_empty());

        // Tick with a new date
        let ctx = SocialTickContext {
            current_date: "2026-04-11",
            in_dialogue: false,
            game_time: 0.0,
        };
        s.tick(1.0, &ctx);
        assert!(
            s.emotes.hi_today.is_empty(),
            "date change should clear hi_today"
        );
        assert_eq!(s.emotes.current_date, "2026-04-11");
    }

    #[test]
    fn tick_composes_party_bonus_and_buff_multiplicatively() {
        use crate::buff::{BuffEffect, BuffSpec};

        let mut base = make_social();
        let mut both = make_social();

        // Skip grace period by pushing game_time past mood_grace_until.
        let game_time = base.mood.mood_grace_until + 1.0;

        // "Both" gets a party bonus (2 members) AND a rookswort buff.
        both.party.create_party([1u8; 16], "Me".into(), 0.0);
        both.party
            .party
            .as_mut()
            .unwrap()
            .add_member(PartyMember {
                address_hash: [2u8; 16],
                display_name: "Peer".into(),
                joined_at: 1.0,
            })
            .unwrap();
        assert!(both.party.has_party_bonus()); // sanity: need 2 members

        let spec = BuffSpec {
            kind: "rookswort".into(),
            effect: BuffEffect::MoodDecayMultiplier { value: 0.5 },
            duration_secs: 600.0,
            on_expire: None,
        };
        both.buffs.apply(&spec, game_time, "rookswort".into());

        let ctx = SocialTickContext {
            current_date: "2026-04-18",
            in_dialogue: false,
            game_time,
        };
        base.tick(60.0, &ctx);
        both.tick(60.0, &ctx);

        let base_decay = 100.0 - base.mood.mood;
        let both_decay = 100.0 - both.mood.mood;
        // Expected: party (0.75) × buff (0.5) = 0.375
        let ratio = both_decay / base_decay;
        assert!((ratio - 0.375).abs() < 1e-9, "got {ratio}");
    }

    #[test]
    fn tick_with_no_buffs_or_party_preserves_baseline() {
        let mut s = make_social();
        let game_time = s.mood.mood_grace_until + 1.0;
        let ctx = SocialTickContext {
            current_date: "2026-04-18",
            in_dialogue: false,
            game_time,
        };
        let before = s.mood.mood;
        s.tick(60.0, &ctx);
        assert!(s.mood.mood < before, "baseline decay still occurs");
    }
}
