const DECAY_HIGH: f64 = 0.015 / 60.0;
const DECAY_MID: f64 = 0.005 / 60.0;
const DECAY_LOW: f64 = 0.0025 / 60.0;
const THRESHOLD_HIGH: f64 = 0.80;
const THRESHOLD_MID: f64 = 0.50;

/// Returns the new mood value after applying decay over `dt` seconds.
/// Decay is tiered based on current mood as a fraction of `max_mood`:
///   - >80%: fast decay (1.5% of max per minute)
///   - 50-80%: moderate decay (0.5% of max per minute)
///   - <50%: slow decay (0.25% of max per minute)
/// Result is clamped to [0.0, max_mood].
/// If `max_mood` is zero, returns `mood` unchanged.
pub fn mood_decay(mood: f64, max_mood: f64, dt: f64) -> f64 {
    if max_mood <= 0.0 {
        return mood;
    }
    let pct = mood / max_mood;
    let rate = if pct > THRESHOLD_HIGH {
        DECAY_HIGH
    } else if pct > THRESHOLD_MID {
        DECAY_MID
    } else {
        DECAY_LOW
    };
    (mood - rate * max_mood * dt).clamp(0.0, max_mood)
}

/// Returns the iMG multiplier for the current mood.
/// At or above 50% mood: returns 1.0.
/// Below 50% mood: scales linearly from 1.0 down to 0.5 at zero mood.
///   multiplier = 0.5 + (mood / max_mood)
/// If `max_mood` is zero, returns 1.0.
pub fn mood_multiplier(mood: f64, max_mood: f64) -> f64 {
    if max_mood <= 0.0 {
        return 1.0;
    }
    let pct = (mood / max_mood).clamp(0.0, 1.0);
    if pct >= THRESHOLD_MID {
        1.0
    } else {
        0.5 + pct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── mood_decay tests ────────────────────────────────────────────────────

    #[test]
    fn decay_high_tier_applies_fast_rate() {
        // 90% mood → high tier
        let mood = 90.0_f64;
        let max_mood = 100.0_f64;
        let dt = 60.0_f64; // one minute
        let result = mood_decay(mood, max_mood, dt);
        let expected = mood - DECAY_HIGH * max_mood * dt;
        assert!((result - expected).abs() < 1e-10, "got {result}, expected {expected}");
    }

    #[test]
    fn decay_mid_tier_applies_moderate_rate() {
        // 65% mood → mid tier
        let mood = 65.0_f64;
        let max_mood = 100.0_f64;
        let dt = 60.0_f64;
        let result = mood_decay(mood, max_mood, dt);
        let expected = mood - DECAY_MID * max_mood * dt;
        assert!((result - expected).abs() < 1e-10, "got {result}, expected {expected}");
    }

    #[test]
    fn decay_low_tier_applies_slow_rate() {
        // 30% mood → low tier
        let mood = 30.0_f64;
        let max_mood = 100.0_f64;
        let dt = 60.0_f64;
        let result = mood_decay(mood, max_mood, dt);
        let expected = mood - DECAY_LOW * max_mood * dt;
        assert!((result - expected).abs() < 1e-10, "got {result}, expected {expected}");
    }

    #[test]
    fn decay_clamped_at_zero() {
        // Very long dt should not produce negative mood
        let result = mood_decay(1.0, 100.0, 1_000_000.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn decay_zero_mood_stays_at_zero() {
        let result = mood_decay(0.0, 100.0, 60.0);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn decay_zero_max_mood_returns_unchanged() {
        let result = mood_decay(50.0, 0.0, 60.0);
        assert_eq!(result, 50.0);
    }

    #[test]
    fn decay_boundary_exactly_80_is_mid_tier() {
        // pct == 0.80, which is NOT >0.80, so mid tier applies
        let mood = 80.0_f64;
        let max_mood = 100.0_f64;
        let dt = 60.0_f64;
        let result = mood_decay(mood, max_mood, dt);
        let expected = mood - DECAY_MID * max_mood * dt;
        assert!((result - expected).abs() < 1e-10, "got {result}, expected {expected}");
    }

    #[test]
    fn decay_boundary_exactly_50_is_low_tier() {
        // pct == 0.50, which is NOT >0.50, so low tier applies
        let mood = 50.0_f64;
        let max_mood = 100.0_f64;
        let dt = 60.0_f64;
        let result = mood_decay(mood, max_mood, dt);
        let expected = mood - DECAY_LOW * max_mood * dt;
        assert!((result - expected).abs() < 1e-10, "got {result}, expected {expected}");
    }

    // ── mood_multiplier tests ───────────────────────────────────────────────

    #[test]
    fn multiplier_full_at_exactly_50_percent() {
        let result = mood_multiplier(50.0, 100.0);
        assert_eq!(result, 1.0);
    }

    #[test]
    fn multiplier_full_above_50_percent() {
        let result = mood_multiplier(75.0, 100.0);
        assert_eq!(result, 1.0);
    }

    #[test]
    fn multiplier_scales_linearly_below_50_percent() {
        // 25% mood → 0.5 + 0.25 = 0.75
        let result = mood_multiplier(25.0, 100.0);
        assert!((result - 0.75).abs() < 1e-10, "got {result}");
    }

    #[test]
    fn multiplier_minimum_0_5_at_zero_mood() {
        let result = mood_multiplier(0.0, 100.0);
        assert!((result - 0.5).abs() < 1e-10, "got {result}");
    }

    #[test]
    fn multiplier_zero_max_mood_returns_1() {
        let result = mood_multiplier(0.0, 0.0);
        assert_eq!(result, 1.0);
    }
}
