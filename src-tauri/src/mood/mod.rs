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
    /// `party_bonus` reduces the effective decay by 25% when true.
    pub fn tick(&mut self, dt: f64, game_time: f64, in_dialogue: bool, party_bonus: bool) {
        if !dt.is_finite() || dt <= 0.0 || !game_time.is_finite() || game_time < 0.0 {
            return;
        }
        if in_dialogue || game_time < self.mood_grace_until {
            return;
        }
        let effective_dt = if party_bonus { dt * 0.75 } else { dt };
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
        s.tick(60.0, game_time, false, false);
        assert!(s.mood < 100.0, "mood should have decayed, got {}", s.mood);
    }

    #[test]
    fn tick_suppressed_during_dialogue() {
        let mut s = MoodState::default();
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, true, false);
        assert_eq!(s.mood, 100.0);
    }

    #[test]
    fn tick_suppressed_during_grace_period() {
        let mut s = MoodState::new_with_grace(100.0, 100.0, 0.0);
        // game_time is 0, grace_until is 300 — still in grace
        s.tick(60.0, 0.0, false, false);
        assert_eq!(s.mood, 100.0);
    }

    #[test]
    fn tick_resumes_after_grace_period() {
        let mut s = MoodState::new_with_grace(100.0, 100.0, 0.0);
        let game_time = s.mood_grace_until + 1.0;
        s.tick(60.0, game_time, false, false);
        assert!(s.mood < 100.0, "mood should decay after grace, got {}", s.mood);
    }

    #[test]
    fn tick_with_party_bonus_reduces_decay_by_25_percent() {
        // Two identical states; one ticked with bonus, one without
        let mut base = MoodState::default();
        let mut bonus = MoodState::default();
        let game_time = base.mood_grace_until + 1.0;
        let dt = 60.0_f64;

        base.tick(dt, game_time, false, false);
        bonus.tick(dt, game_time, false, true);

        let base_decay = 100.0 - base.mood;
        let bonus_decay = 100.0 - bonus.mood;
        // Bonus decay should be 75% of base decay
        let ratio = bonus_decay / base_decay;
        assert!((ratio - 0.75).abs() < 1e-10, "expected 0.75 ratio, got {ratio}");
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

        s.tick(f64::NAN, 1.0, false, false);
        assert_eq!(s.mood, 100.0);

        s.tick(f64::INFINITY, 1.0, false, false);
        assert_eq!(s.mood, 100.0);

        s.tick(60.0, f64::NAN, false, false);
        assert_eq!(s.mood, 100.0);

        s.tick(60.0, -1.0, false, false);
        assert_eq!(s.mood, 100.0);

        s.tick(-10.0, 1.0, false, false);
        assert_eq!(s.mood, 100.0);
    }
}
