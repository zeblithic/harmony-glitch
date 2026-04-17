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
        let mut max_remaining = Duration::ZERO;

        // Global fire cooldown
        if let Some(last) = self.last_global_fire {
            let elapsed = now.saturating_duration_since(last);
            if elapsed < GLOBAL_FIRE_COOLDOWN {
                let r = GLOBAL_FIRE_COOLDOWN - elapsed;
                if r > max_remaining {
                    max_remaining = r;
                }
            }
        }

        // Per-pair fire cooldown (targeted emotes only)
        if let Some(pid) = pair_identity {
            let pair_cd = fire_cooldown_for(tag);
            if pair_cd > Duration::ZERO {
                if let Some(last) = self.fire_per_pair.get(&(pid, tag)) {
                    let elapsed = now.saturating_duration_since(*last);
                    if elapsed < pair_cd {
                        let r = pair_cd - elapsed;
                        if r > max_remaining {
                            max_remaining = r;
                        }
                    }
                }
            }
        }

        if max_remaining > Duration::ZERO {
            Err(CooldownRemaining {
                remaining_ms: max_remaining.as_millis() as u64,
            })
        } else {
            Ok(())
        }
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
        // Opportunistic eviction — keeps map bounded by active peers rather
        // than session-lifetime unique peers.
        self.evict_expired_fire(now);
    }

    /// Receiver-side validation mirror. Enforces ONLY the per-pair cooldown
    /// (e.g., hug 60s, high-five 30s) — never the global anti-mash cooldown,
    /// which is this client's own outbound throttle and must not be consumed
    /// by inbound traffic from unrelated senders.
    ///
    /// `sender_identity` is the peer that sent us this emote.
    pub fn check_receive(
        &self,
        now: Instant,
        kind: &EmoteKind,
        sender_identity: [u8; 16],
    ) -> Result<(), CooldownRemaining> {
        let tag = EmoteKindTag::from(kind);
        let pair_cd = fire_cooldown_for(tag);
        if pair_cd == Duration::ZERO {
            return Ok(());
        }
        if let Some(last) = self.fire_per_pair.get(&(sender_identity, tag)) {
            let elapsed = now.saturating_duration_since(*last);
            if elapsed < pair_cd {
                return Err(CooldownRemaining {
                    remaining_ms: (pair_cd - elapsed).as_millis() as u64,
                });
            }
        }
        Ok(())
    }

    /// Records that we accepted a received emote. Updates only the per-pair
    /// fire map — never `last_global_fire`, which is reserved for this
    /// client's own outbound fire rate.
    pub fn mark_receive(&mut self, now: Instant, kind: &EmoteKind, sender_identity: [u8; 16]) {
        let tag = EmoteKindTag::from(kind);
        if fire_cooldown_for(tag) > Duration::ZERO {
            self.fire_per_pair.insert((sender_identity, tag), now);
        }
        self.evict_expired_fire(now);
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
        self.evict_expired_reward(now);
        true
    }

    /// Evict fire-cooldown entries whose per-pair window has elapsed.
    /// Global state is single-slot and doesn't need sweeping.
    fn evict_expired_fire(&mut self, now: Instant) {
        self.fire_per_pair.retain(|(_, tag), last| {
            now.saturating_duration_since(*last) < fire_cooldown_for(*tag)
        });
    }

    /// Evict reward-cooldown entries whose window has elapsed.
    fn evict_expired_reward(&mut self, now: Instant) {
        self.reward_per_pair.retain(|(_, tag), last| {
            now.saturating_duration_since(*last) < reward_cooldown_for(*tag)
        });
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
    fn check_fire_returns_larger_of_both_cooldowns_when_both_active() {
        // Hug pair cooldown is 60s; global is 2s. Right after a hug, both are
        // active — check_fire should return the longer (per-pair) remaining
        // so the UI shows one countdown covering the full wait.
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        tracker.mark_fire(t0, &EmoteKind::Hug, Some(id(1)));

        // 1 second in: global has ~1s left, per-pair has ~59s left
        let t1 = t0 + Duration::from_secs(1);
        let err = tracker.check_fire(t1, &EmoteKind::Hug, Some(id(1))).unwrap_err();
        assert!(
            err.remaining_ms > 58_000 && err.remaining_ms <= 59_000,
            "expected ~59s (per-pair), got {}ms",
            err.remaining_ms
        );
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

    // ── check_receive / mark_receive ──────────────────────────────────

    #[test]
    fn receive_ignores_global_fire_cooldown() {
        // Two different remote senders firing dance within the sender-side
        // global window should BOTH be accepted — the global cooldown is
        // this client's own throttle, not the network's.
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();

        // Sender A's dance arrives
        assert!(tracker.check_receive(t0, &EmoteKind::Dance, id(1)).is_ok());
        tracker.mark_receive(t0, &EmoteKind::Dance, id(1));

        // 500ms later (well inside 2s global window), sender B's dance arrives
        let t1 = t0 + Duration::from_millis(500);
        assert!(
            tracker.check_receive(t1, &EmoteKind::Dance, id(2)).is_ok(),
            "receive must not enforce global cooldown"
        );
    }

    #[test]
    fn receive_does_not_mutate_global_fire_state() {
        // A received emote must not block this client's own next outbound fire.
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();

        // Receive sender A's dance
        tracker.mark_receive(t0, &EmoteKind::Dance, id(1));

        // Our own outbound dance 500ms later should be unaffected
        let t1 = t0 + Duration::from_millis(500);
        assert!(
            tracker.check_fire(t1, &EmoteKind::Dance, None).is_ok(),
            "receive must not consume the sender-side global cooldown"
        );
    }

    #[test]
    fn receive_enforces_per_pair_cooldown() {
        // Same sender hugging us twice within per-pair window should drop
        // the second hug.
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        tracker.mark_receive(t0, &EmoteKind::Hug, id(1));

        let t1 = t0 + Duration::from_secs(3);
        assert!(tracker.check_receive(t1, &EmoteKind::Hug, id(1)).is_err());
    }

    #[test]
    fn receive_per_pair_is_independent_of_sender_identity() {
        // Different senders each get their own per-pair window.
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        tracker.mark_receive(t0, &EmoteKind::Hug, id(1));

        let t1 = t0 + Duration::from_secs(3);
        assert!(tracker.check_receive(t1, &EmoteKind::Hug, id(2)).is_ok());
    }

    // ── Eviction ───────────────────────────────────────────────────────

    #[test]
    fn fire_map_evicts_expired_entries() {
        // After the hug pair cooldown elapses, subsequent mark_fire/mark_receive
        // should purge the stale entry so long-lived sessions don't accumulate
        // unbounded state.
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        tracker.mark_fire(t0, &EmoteKind::Hug, Some(id(1)));
        assert_eq!(tracker.fire_per_pair.len(), 1);

        // Well past the 60s hug window — a new fire for a different pair
        // should sweep the expired entry for id(1) away.
        let t1 = t0 + Duration::from_secs(120);
        tracker.mark_fire(t1, &EmoteKind::Hug, Some(id(2)));
        assert_eq!(tracker.fire_per_pair.len(), 1);
        assert!(tracker.fire_per_pair.contains_key(&(id(2), EmoteKindTag::Hug)));
    }

    #[test]
    fn reward_map_evicts_expired_entries() {
        // Dance reward cooldown is 5 minutes — past that, entries should
        // be evicted on the next try_reward.
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        assert!(tracker.try_reward(t0, &EmoteKind::Dance, id(1)));
        assert_eq!(tracker.reward_per_pair.len(), 1);

        let t1 = t0 + Duration::from_secs(600);
        assert!(tracker.try_reward(t1, &EmoteKind::Dance, id(2)));
        assert_eq!(tracker.reward_per_pair.len(), 1);
        assert!(tracker.reward_per_pair.contains_key(&(id(2), EmoteKindTag::Dance)));
    }

    #[test]
    fn eviction_preserves_active_entries() {
        // Active (still-in-window) entries must survive eviction sweeps.
        let mut tracker = CooldownTracker::default();
        let t0 = Instant::now();
        tracker.mark_fire(t0, &EmoteKind::Hug, Some(id(1)));

        // 30s later (inside the 60s hug window), mark a different pair.
        // Both should be retained.
        let t1 = t0 + Duration::from_secs(30);
        tracker.mark_fire(t1, &EmoteKind::Hug, Some(id(2)));
        assert_eq!(tracker.fire_per_pair.len(), 2);
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
