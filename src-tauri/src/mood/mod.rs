pub mod decay;

use serde::{Deserialize, Serialize};

const MOOD_GRACE_DURATION: f64 = 300.0; // 5 minutes

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoodState {
    pub mood: f64,
    pub max_mood: f64,
    pub mood_grace_until: f64, // game time at which grace period expires
}

impl Default for MoodState {
    fn default() -> Self {
        Self {
            mood: 100.0,
            max_mood: 100.0,
            mood_grace_until: 0.0, // No grace for fresh games; only restore gets grace via new_with_grace()
        }
    }
}

impl MoodState {
    /// Creates a new MoodState with a grace period starting from `game_time`.
    /// Clamps `max_mood` to non-negative and `mood` to `[0.0, max_mood]`.
    pub fn new_with_grace(mood: f64, max_mood: f64, game_time: f64) -> Self {
        let max_mood = max_mood.max(0.0);
        let mood = mood.clamp(0.0, max_mood);
        Self {
            mood,
            max_mood,
            mood_grace_until: game_time + MOOD_GRACE_DURATION,
        }
    }

    /// Advances mood by `dt` seconds of game time.
    /// Decay is suppressed during dialogue or while the grace period is active.
    /// `decay_modifier` scales the effective decay (1.0 = normal, 0.5 = half,
    /// 0.0 = halted, >1.0 accelerates). Negative values are clamped to 0.0.
    pub fn tick(&mut self, dt: f64, game_time: f64, in_dialogue: bool, decay_modifier: f64) {
        if !dt.is_finite() || dt <= 0.0 || !game_time.is_finite() || game_time < 0.0 {
            return;
        }
        if in_dialogue || game_time < self.mood_grace_until {
            return;
        }
        let safe_modifier = decay_modifier.max(0.0);
        let effective_dt = dt * safe_modifier;
        self.mood = decay::mood_decay(self.mood, self.max_mood, effective_dt);
    }

    /// Applies a mood delta, clamping the result to [0.0, max_mood].
    /// Guards against negative max_mood.
    pub fn apply_mood_change(&mut self, delta: f64) {
        let max = self.max_mood.max(0.0);
        self.mood = (self.mood + delta).clamp(0.0, max);
    }

    /// Returns the iMG multiplier for the current mood state.
    pub fn multiplier(&self) -> f64 {
        decay::mood_multiplier(self.mood, self.max_mood)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_100_over_100() {
        let s = MoodState::default();
        assert_eq!(s.mood, 100.0);
        assert_eq!(s.max_mood, 100.0);
    }

    #[test]
    fn apply_mood_change_capped_at_max() {
        let mut s = MoodState::default();
        s.apply_mood_change(50.0);
        assert_eq!(s.mood, 100.0);
    }

    #[test]
    fn apply_mood_change_clamped_at_zero() {
        let mut s = MoodState::default();
        s.apply_mood_change(-200.0);
        assert_eq!(s.mood, 0.0);
    }

    #[test]
    fn apply_mood_change_normal_case() {
        let mut s = MoodState::default();
        s.apply_mood_change(-30.0);
        assert!((s.mood - 70.0).abs() < 1e-10);
    }

    #[test]
    fn tick_decays_mood() {
        let mut s = MoodState::default();
        // Bypass grace period by placing game_time well past grace_until
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, false, 1.0);
        assert!(s.mood < 100.0, "mood should have decayed, got {}", s.mood);
    }

    #[test]
    fn tick_suppressed_during_dialogue() {
        let mut s = MoodState::default();
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, true, 1.0);
        assert_eq!(s.mood, 100.0);
    }

    #[test]
    fn tick_suppressed_during_grace_period() {
        let mut s = MoodState::new_with_grace(100.0, 100.0, 0.0);
        // game_time is 0, grace_until is 300 — still in grace
        s.tick(60.0, 0.0, false, 1.0);
        assert_eq!(s.mood, 100.0);
    }

    #[test]
    fn tick_resumes_after_grace_period() {
        let mut s = MoodState::new_with_grace(100.0, 100.0, 0.0);
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, false, 1.0);
        assert!(s.mood < 100.0, "mood should decay after grace, got {}", s.mood);
    }

    #[test]
    fn multiplier_delegates_correctly() {
        let mut s = MoodState::default();
        // At 100% mood → multiplier should be 1.0
        assert_eq!(s.multiplier(), 1.0);
        // Drop to 25% → multiplier should be 0.75
        s.mood = 25.0;
        assert!((s.multiplier() - 0.75).abs() < 1e-10);
    }

    #[test]
    fn new_with_grace_sets_correct_grace_period() {
        let game_time = 1000.0_f64;
        let s = MoodState::new_with_grace(80.0, 100.0, game_time);
        assert_eq!(s.mood, 80.0);
        assert_eq!(s.max_mood, 100.0);
        assert!((s.mood_grace_until - (game_time + MOOD_GRACE_DURATION)).abs() < 1e-10);
    }

    #[test]
    fn tick_ignores_invalid_inputs() {
        let mut s = MoodState::default();

        s.tick(f64::NAN, 1.0, false, 1.0);
        assert_eq!(s.mood, 100.0);

        s.tick(f64::INFINITY, 1.0, false, 1.0);
        assert_eq!(s.mood, 100.0);

        s.tick(60.0, f64::NAN, false, 1.0);
        assert_eq!(s.mood, 100.0);

        s.tick(60.0, -1.0, false, 1.0);
        assert_eq!(s.mood, 100.0);

        s.tick(-10.0, 1.0, false, 1.0);
        assert_eq!(s.mood, 100.0);
    }

    #[test]
    fn tick_with_decay_modifier_of_one_matches_unmodified_baseline() {
        let mut s = MoodState::default();
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, false, 1.0);
        // Same as prior baseline test (no party, no buffs)
        assert!(s.mood < 100.0);
    }

    #[test]
    fn tick_with_decay_modifier_of_zero_halts_decay() {
        let mut s = MoodState::default();
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, false, 0.0);
        assert_eq!(s.mood, 100.0);
    }

    #[test]
    fn tick_with_decay_modifier_of_0_75_reduces_by_25_percent() {
        let mut base = MoodState::default();
        let mut reduced = MoodState::default();
        let game_time = base.mood_grace_until + 1.0;
        base.tick(60.0, game_time, false, 1.0);
        reduced.tick(60.0, game_time, false, 0.75);
        let base_decay = 100.0 - base.mood;
        let reduced_decay = 100.0 - reduced.mood;
        let ratio = reduced_decay / base_decay;
        assert!((ratio - 0.75).abs() < 1e-9, "got {ratio}");
    }

    #[test]
    fn tick_with_decay_modifier_above_one_accelerates_decay() {
        let mut base = MoodState::default();
        let mut debuffed = MoodState::default();
        let game_time = base.mood_grace_until + 1.0;
        base.tick(60.0, game_time, false, 1.0);
        debuffed.tick(60.0, game_time, false, 2.0);
        let base_decay = 100.0 - base.mood;
        let debuff_decay = 100.0 - debuffed.mood;
        assert!(debuff_decay > base_decay, "debuff should decay faster");
    }

    #[test]
    fn tick_clamps_negative_decay_modifier_to_zero() {
        let mut s = MoodState::default();
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, false, -5.0);
        // Clamped to 0.0, so mood is unchanged (not increased).
        assert_eq!(s.mood, 100.0);
    }
}
