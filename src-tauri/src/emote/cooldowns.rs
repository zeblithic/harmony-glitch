use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::types::{EmoteKind, EmoteKindTag};

// ── Fire cooldowns (gate whether the message ships) ─────────────────────

/// Anti-mash — minimum gap between any two emote fires by this client.
pub const GLOBAL_FIRE_COOLDOWN: Duration = Duration::from_secs(2);

/// Per-pair fire cooldown for targeted emotes (0 = no per-pair limit).
pub fn fire_cooldown_for(tag: EmoteKindTag) -> Duration {
    match tag {
        EmoteKindTag::Hug => Duration::from_secs(60),
        EmoteKindTag::HighFive => Duration::from_secs(30),
        EmoteKindTag::Hi | EmoteKindTag::Dance | EmoteKindTag::Wave | EmoteKindTag::Applaud => {
            Duration::ZERO
        }
    }
}

// ── Reward cooldowns (gate whether mood is credited) ────────────────────

pub fn reward_cooldown_for(tag: EmoteKindTag) -> Duration {
    match tag {
        EmoteKindTag::Dance => Duration::from_secs(300),    // 5 min
        EmoteKindTag::Wave => Duration::from_secs(30),
        EmoteKindTag::Hug => Duration::from_secs(60),       // = fire cd
        EmoteKindTag::HighFive => Duration::from_secs(30),  // = fire cd
        EmoteKindTag::Applaud => Duration::from_secs(300),  // 5 min
        EmoteKindTag::Hi => Duration::ZERO, // Hi uses once-per-day, gated elsewhere
    }
}

// ── CooldownTracker ─────────────────────────────────────────────────────

/// Remaining-time in a rejection. UI uses this to drive countdown display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CooldownRemaining {
    pub remaining_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CooldownTracker {
    last_global_fire: Option<Instant>,
    fire_per_pair: HashMap<([u8; 16], EmoteKindTag), Instant>,
    reward_per_pair: HashMap<([u8; 16], EmoteKindTag), Instant>,
}

impl CooldownTracker {
    /// Returns `Ok(())` if the emote can fire now, or `Err(remaining)` with
    /// the longer of (global, per-pair) cooldowns remaining.
    ///
    /// `pair_identity` is the OTHER party for per-pair lookup — for targeted
    /// emotes, the target's identity; for broadcast emotes, use `None`.
    pub fn check_fire(
        &self,
        now: Instant,
        kind: &EmoteKind,
        pair_identity: Option<[u8; 16]>,
    ) -> Result<(), CooldownRemaining> {
        let tag = EmoteKindTag::from(kind);

        // Global fire cooldown
        if let Some(last) = self.last_global_fire {
            let elapsed = now.saturating_duration_since(last);
            if elapsed < GLOBAL_FIRE_COOLDOWN {
                return Err(CooldownRemaining {
                    remaining_ms: (GLOBAL_FIRE_COOLDOWN - elapsed).as_millis() as u64,
                });
            }
        }

        // Per-pair fire cooldown (targeted emotes only)
        if let Some(pid) = pair_identity {
            let pair_cd = fire_cooldown_for(tag);
            if pair_cd > Duration::ZERO {
                if let Some(last) = self.fire_per_pair.get(&(pid, tag)) {
                    let elapsed = now.saturating_duration_since(*last);
                    if elapsed < pair_cd {
                        return Err(CooldownRemaining {
                            remaining_ms: (pair_cd - elapsed).as_millis() as u64,
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Records that a fire just happened. Caller is expected to have called
    /// `check_fire` first and received `Ok(())`.
    pub fn mark_fire(&mut self, now: Instant, kind: &EmoteKind, pair_identity: Option<[u8; 16]>) {
        self.last_global_fire = Some(now);
        if let Some(pid) = pair_identity {
            let tag = EmoteKindTag::from(kind);
            if fire_cooldown_for(tag) > Duration::ZERO {
                self.fire_per_pair.insert((pid, tag), now);
            }
        }
    }

    /// Atomically checks the reward cooldown. Returns `true` if the reward
    /// should be applied (and records this moment). Returns `false` if the
    /// last reward was inside the window.
    ///
    /// Use for both sender self-mood and receiver target/witness mood.
    /// `pair_identity` is the OTHER party (sender from receiver's view,
    /// target from sender's view; for self-dance, use our own identity).
    pub fn try_reward(
        &mut self,
        now: Instant,
        kind: &EmoteKind,
        pair_identity: [u8; 16],
    ) -> bool {
        let tag = EmoteKindTag::from(kind);
        let window = reward_cooldown_for(tag);
        if window == Duration::ZERO {
            return true; // no reward gate for this kind (Hi uses other semantics)
        }
        let key = (pair_identity, tag);
        if let Some(last) = self.reward_per_pair.get(&key) {
            if now.saturating_duration_since(*last) < window {
                return false;
            }
        }
        self.reward_per_pair.insert(key, now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emote::types::HiVariant;

    fn id(seed: u8) -> [u8; 16] {
        [seed; 16]
    }

    // ── check_fire / mark_fire ─────────────────────────────────────────

    #[test]
    fn global_fire_cooldown_blocks_within_window() {
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        assert!(tracker.check_fire(t0, &EmoteKind::Dance, None).is_ok());
        tracker.mark_fire(t0, &EmoteKind::Dance, None);

        let t1 = t0 + Duration::from_millis(500);
        let err = tracker.check_fire(t1, &EmoteKind::Wave, None).unwrap_err();
        assert!(err.remaining_ms > 1400 && err.remaining_ms <= 1500);
    }

    #[test]
    fn global_fire_cooldown_clears_after_window() {
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        tracker.mark_fire(t0, &EmoteKind::Dance, None);
        let t1 = t0 + GLOBAL_FIRE_COOLDOWN + Duration::from_millis(1);
        assert!(tracker.check_fire(t1, &EmoteKind::Wave, None).is_ok());
    }

    #[test]
    fn per_pair_fire_cooldown_applies_only_to_same_pair() {
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        tracker.mark_fire(t0, &EmoteKind::Hug, Some(id(1)));

        // Past global cooldown, same pair — should still be blocked by per-pair
        let t1 = t0 + Duration::from_secs(3);
        let err = tracker.check_fire(t1, &EmoteKind::Hug, Some(id(1))).unwrap_err();
        assert!(err.remaining_ms > 56_000 && err.remaining_ms <= 57_000);

        // Same time, different pair — allowed
        assert!(tracker.check_fire(t1, &EmoteKind::Hug, Some(id(2))).is_ok());
    }

    #[test]
    fn fire_cooldown_does_not_affect_reward_window() {
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        tracker.mark_fire(t0, &EmoteKind::Dance, None);
        // Reward cooldown for Dance is 5 min; fire cooldown is 2s.
        // Marking fire should not touch reward_per_pair.
        assert!(tracker.try_reward(t0, &EmoteKind::Dance, id(1)));
    }

    // ── try_reward ─────────────────────────────────────────────────────

    #[test]
    fn try_reward_first_call_succeeds() {
        let mut tracker = CooldownTracker::default();
        assert!(tracker.try_reward(Instant::now(), &EmoteKind::Dance, id(1)));
    }

    #[test]
    fn try_reward_second_call_within_window_fails() {
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        assert!(tracker.try_reward(t0, &EmoteKind::Dance, id(1)));
        let t1 = t0 + Duration::from_secs(60);
        assert!(!tracker.try_reward(t1, &EmoteKind::Dance, id(1)));
    }

    #[test]
    fn try_reward_after_window_succeeds() {
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        assert!(tracker.try_reward(t0, &EmoteKind::Dance, id(1)));
        let t1 = t0 + Duration::from_secs(301);
        assert!(tracker.try_reward(t1, &EmoteKind::Dance, id(1)));
    }

    #[test]
    fn try_reward_is_per_pair() {
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        assert!(tracker.try_reward(t0, &EmoteKind::Dance, id(1)));
        // Same kind, different pair — independent
        assert!(tracker.try_reward(t0, &EmoteKind::Dance, id(2)));
    }

    #[test]
    fn try_reward_hi_passes_through() {
        // Hi's reward cooldown is ZERO — other semantics (once-per-day)
        // gate Hi mood rewards, not this tracker.
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        assert!(tracker.try_reward(t0, &EmoteKind::Hi(HiVariant::Stars), id(1)));
        assert!(tracker.try_reward(t0, &EmoteKind::Hi(HiVariant::Stars), id(1)));
    }
}
