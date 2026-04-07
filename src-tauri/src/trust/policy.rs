// Trust-tier policy: maps Subjective Logic trust expectations to
// validation intensity, tolerance, and shadow-ban decisions.
//
// Pure functions + constants — no I/O, no state.

// ── Trust tier thresholds ──────────────────────────────────────────────

/// Peers with expectation below this are considered low trust.
const TRUST_LOW_THRESHOLD: f64 = 0.35;
/// Peers with expectation above this are considered high trust.
const TRUST_HIGH_THRESHOLD: f64 = 0.70;

// ── Validation frequency (1 = every frame, N = every Nth frame) ─────

const LOW_TRUST_CHECK_INTERVAL: u32 = 1;
const MEDIUM_TRUST_CHECK_INTERVAL: u32 = 5;
const HIGH_TRUST_CHECK_INTERVAL: u32 = 15;

// ── Jitter tolerance multipliers ─────────────────────────────────────

const LOW_TRUST_JITTER_MULT: f32 = 1.0;
const MEDIUM_TRUST_JITTER_MULT: f32 = 1.5;
const HIGH_TRUST_JITTER_MULT: f32 = 2.0;

// ── Shadow-ban thresholds ────────────────────────────────────────────

/// Minimum violation count before shadow-ban is considered.
const SHADOW_BAN_VIOLATION_COUNT: u32 = 5;
/// Trust expectation must be below this for a shadow-ban to trigger.
const SHADOW_BAN_EXPECTATION: f64 = 0.15;
/// Duration of a temporary shadow-ban (seconds).
const SHADOW_BAN_DURATION_SECS: f64 = 300.0;
/// Trust expectation below this triggers a permanent shadow-ban.
const SHADOW_BAN_PERMANENT_THRESHOLD: f64 = 0.05;

// ── Types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustTier {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy)]
pub struct ValidationParams {
    pub check_interval: u32,
    pub jitter_multiplier: f32,
    pub reject_on_violation: bool,
}

/// Per-peer ephemeral enforcement state. Stored in NetworkState, NOT
/// persisted — cleared on reconnect/street-change.
#[derive(Debug, Clone)]
pub struct PeerValidationState {
    pub frame_count: u32,
    /// `None` = not banned. `Some(f64::INFINITY)` = permanent.
    pub shadow_banned_until: Option<f64>,
}

impl PeerValidationState {
    pub fn new() -> Self {
        Self {
            frame_count: 0,
            shadow_banned_until: None,
        }
    }
}

impl Default for PeerValidationState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Public functions ─────────────────────────────────────────────────

/// Map a trust expectation (0.0–1.0) to a tier.
pub fn trust_tier(expectation: f64) -> TrustTier {
    if expectation < TRUST_LOW_THRESHOLD {
        TrustTier::Low
    } else if expectation > TRUST_HIGH_THRESHOLD {
        TrustTier::High
    } else {
        TrustTier::Medium
    }
}

/// Get validation parameters for a trust tier.
pub fn validation_params(tier: TrustTier) -> ValidationParams {
    match tier {
        TrustTier::Low => ValidationParams {
            check_interval: LOW_TRUST_CHECK_INTERVAL,
            jitter_multiplier: LOW_TRUST_JITTER_MULT,
            reject_on_violation: true,
        },
        TrustTier::Medium => ValidationParams {
            check_interval: MEDIUM_TRUST_CHECK_INTERVAL,
            jitter_multiplier: MEDIUM_TRUST_JITTER_MULT,
            reject_on_violation: true,
        },
        TrustTier::High => ValidationParams {
            check_interval: HIGH_TRUST_CHECK_INTERVAL,
            jitter_multiplier: HIGH_TRUST_JITTER_MULT,
            reject_on_violation: false,
        },
    }
}

/// Whether a given frame should be validated based on the check interval.
pub fn should_validate(frame_count: u32, check_interval: u32) -> bool {
    if check_interval <= 1 {
        return true;
    }
    frame_count.is_multiple_of(check_interval)
}

/// Check whether a peer should be shadow-banned based on their violation
/// count and current trust expectation. Returns the ban duration in seconds
/// (`f64::INFINITY` for permanent), or `None` if no ban is warranted.
pub fn should_shadow_ban(violations: u32, expectation: f64) -> Option<f64> {
    if violations < SHADOW_BAN_VIOLATION_COUNT || expectation > SHADOW_BAN_EXPECTATION {
        return None;
    }
    if expectation < SHADOW_BAN_PERMANENT_THRESHOLD {
        Some(f64::INFINITY)
    } else {
        Some(SHADOW_BAN_DURATION_SECS)
    }
}

/// Whether a peer's shadow-ban is currently active.
pub fn is_shadow_banned(state: &PeerValidationState, now: f64) -> bool {
    match state.shadow_banned_until {
        None => false,
        Some(until) => now < until, // f64::INFINITY is always > now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_tier_boundaries() {
        // Below low threshold
        assert_eq!(trust_tier(0.0), TrustTier::Low);
        assert_eq!(trust_tier(0.34), TrustTier::Low);

        // At low threshold → medium (not strictly less than)
        assert_eq!(trust_tier(0.35), TrustTier::Medium);

        // Vacuous (0.5) → medium
        assert_eq!(trust_tier(0.5), TrustTier::Medium);

        // At high threshold → medium (not strictly greater than)
        assert_eq!(trust_tier(0.70), TrustTier::Medium);

        // Above high threshold
        assert_eq!(trust_tier(0.71), TrustTier::High);
        assert_eq!(trust_tier(1.0), TrustTier::High);
    }

    #[test]
    fn validation_params_low_trust() {
        let p = validation_params(TrustTier::Low);
        assert_eq!(p.check_interval, 1);
        assert!((p.jitter_multiplier - 1.0).abs() < 1e-6);
        assert!(p.reject_on_violation);
    }

    #[test]
    fn validation_params_medium_trust() {
        let p = validation_params(TrustTier::Medium);
        assert_eq!(p.check_interval, 5);
        assert!((p.jitter_multiplier - 1.5).abs() < 1e-6);
        assert!(p.reject_on_violation);
    }

    #[test]
    fn validation_params_high_trust() {
        let p = validation_params(TrustTier::High);
        assert_eq!(p.check_interval, 15);
        assert!((p.jitter_multiplier - 2.0).abs() < 1e-6);
        assert!(!p.reject_on_violation);
    }

    #[test]
    fn should_validate_every_frame() {
        // check_interval=1 → always validate
        for i in 0..10 {
            assert!(should_validate(i, 1));
        }
    }

    #[test]
    fn should_validate_spot_check() {
        // check_interval=5 → validate frames 0, 5, 10, ...
        assert!(should_validate(0, 5));
        assert!(!should_validate(1, 5));
        assert!(!should_validate(2, 5));
        assert!(!should_validate(3, 5));
        assert!(!should_validate(4, 5));
        assert!(should_validate(5, 5));
        assert!(should_validate(10, 5));
    }

    #[test]
    fn should_shadow_ban_below_thresholds() {
        // violations=5, expectation=0.10 → triggers temp ban
        let duration = should_shadow_ban(5, 0.10);
        assert!(duration.is_some());
        assert!((duration.unwrap() - 300.0).abs() < 1e-10);
    }

    #[test]
    fn should_not_shadow_ban_insufficient_violations() {
        // violations=4, expectation=0.10 → not enough violations
        assert!(should_shadow_ban(4, 0.10).is_none());
    }

    #[test]
    fn should_not_shadow_ban_high_trust() {
        // violations=10, expectation=0.80 → trust too high
        assert!(should_shadow_ban(10, 0.80).is_none());
    }

    #[test]
    fn permanent_ban_threshold() {
        // violations=5, expectation=0.03 → permanent
        let duration = should_shadow_ban(5, 0.03);
        assert!(duration.is_some());
        assert!(duration.unwrap().is_infinite());
    }

    #[test]
    fn is_shadow_banned_not_banned() {
        let state = PeerValidationState::new();
        assert!(!is_shadow_banned(&state, 100.0));
    }

    #[test]
    fn is_shadow_banned_active() {
        let state = PeerValidationState {
            frame_count: 0,
            shadow_banned_until: Some(200.0),
        };
        assert!(is_shadow_banned(&state, 100.0));
        assert!(is_shadow_banned(&state, 199.9));
    }

    #[test]
    fn is_shadow_banned_expired() {
        let state = PeerValidationState {
            frame_count: 0,
            shadow_banned_until: Some(100.0),
        };
        assert!(!is_shadow_banned(&state, 100.0));
        assert!(!is_shadow_banned(&state, 200.0));
    }

    #[test]
    fn is_shadow_banned_permanent() {
        let state = PeerValidationState {
            frame_count: 0,
            shadow_banned_until: Some(f64::INFINITY),
        };
        assert!(is_shadow_banned(&state, 0.0));
        assert!(is_shadow_banned(&state, 1e18));
    }
}
