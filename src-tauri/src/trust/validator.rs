use std::collections::HashMap;

use crate::network::types::PlayerNetState;

/// Maximum horizontal speed (px/s). From physics/movement.rs WALK_SPEED.
const MAX_VX: f32 = 200.0;
/// Maximum downward speed (px/s). From physics/movement.rs TERMINAL_VELOCITY.
const MAX_VY_DOWN: f32 = 600.0;
/// Player AABB half-width. From physics/movement.rs.
const HALF_WIDTH: f64 = 15.0;
/// Tolerance multiplier for velocity/position checks (absorbs network jitter).
const JITTER_TOLERANCE: f32 = 1.5;
/// Minimum elapsed time between updates to consider for delta checking (seconds).
/// Prevents division-by-near-zero for rapid successive updates.
const MIN_DELTA_TIME: f64 = 0.01;

/// Street boundary rectangle.
#[derive(Debug, Clone, Copy)]
pub struct StreetBounds {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

/// A detected violation in peer state.
#[derive(Debug, Clone)]
pub enum Violation {
    /// Position outside street bounds.
    OutOfBounds { x: f32, y: f32 },
    /// Horizontal velocity exceeds physics limits.
    InvalidVelocityX { vx: f32 },
    /// Vertical velocity exceeds physics limits (downward).
    InvalidVelocityY { vy: f32 },
    /// Position changed more than physically possible since last update.
    TeleportDetected { distance: f32, max_possible: f32 },
}

impl Violation {
    /// Severity in [0, 1]. Used by TrustStore to scale the trust penalty.
    /// - Out of bounds / invalid velocity: 0.3 (could be edge-case / interpolation)
    /// - Teleport: 0.6 (strong evidence of manipulation)
    pub fn severity(&self) -> f64 {
        match self {
            Violation::OutOfBounds { .. } => 0.3,
            Violation::InvalidVelocityX { .. } => 0.3,
            Violation::InvalidVelocityY { .. } => 0.3,
            Violation::TeleportDetected { .. } => 0.6,
        }
    }
}

/// Last validated state for a peer, used for delta checking.
#[derive(Debug, Clone)]
struct ValidatedState {
    x: f32,
    y: f32,
    timestamp: f64,
}

/// Validates incoming peer state against physics constraints.
pub struct StateValidator {
    last_states: HashMap<[u8; 16], ValidatedState>,
}

impl Default for StateValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl StateValidator {
    pub fn new() -> Self {
        Self {
            last_states: HashMap::new(),
        }
    }

    /// Validate an incoming state update from a peer. Returns a list of
    /// violations (empty = valid).
    ///
    /// `tolerance_multiplier` scales the jitter tolerance — higher values
    /// are more lenient (used for high-trust peers). Pass `1.0` for default
    /// behavior.
    ///
    /// **Does not update the baseline position.** Call [`accept_state()`]
    /// after accepting a valid frame so future delta checks work correctly.
    /// This split prevents rejected positions from becoming the new
    /// baseline (which would make the next legitimate update look like
    /// a teleport back).
    pub fn validate(
        &self,
        address_hash: &[u8; 16],
        state: &PlayerNetState,
        bounds: &StreetBounds,
        now: f64,
        tolerance_multiplier: f32,
    ) -> Vec<Violation> {
        let mut violations = Vec::new();
        let effective_tolerance = JITTER_TOLERANCE * tolerance_multiplier;

        // 1. Position bounds (single violation for any axis out of range)
        let min_x = bounds.left + HALF_WIDTH;
        let max_x = bounds.right - HALF_WIDTH;
        let x_oob =
            (state.x as f64) < min_x - 1.0 || (state.x as f64) > max_x + 1.0;
        let y_oob = (state.y as f64) < bounds.top - 100.0
            || (state.y as f64) > bounds.bottom + 50.0;
        if x_oob || y_oob {
            violations.push(Violation::OutOfBounds {
                x: state.x,
                y: state.y,
            });
        }

        // 2. Velocity bounds
        if state.vx.abs() > MAX_VX * effective_tolerance {
            violations.push(Violation::InvalidVelocityX { vx: state.vx });
        }
        // Only check downward velocity (positive vy in Glitch coords = falling)
        if state.vy > MAX_VY_DOWN * effective_tolerance {
            violations.push(Violation::InvalidVelocityY { vy: state.vy });
        }

        // 3. Teleport detection (delta check against previous state).
        if let Some(prev) = self.last_states.get(address_hash) {
            let elapsed = now - prev.timestamp;
            if elapsed >= MIN_DELTA_TIME {
                let dx = (state.x - prev.x).abs();
                let dy = (state.y - prev.y).abs();
                let distance = (dx * dx + dy * dy).sqrt();

                // Max possible movement considering physics limits + jitter
                let max_h = MAX_VX * elapsed as f32 * effective_tolerance;
                // Vertical: consider gravity acceleration + terminal velocity
                let max_v = MAX_VY_DOWN * elapsed as f32 * effective_tolerance;
                let max_possible = (max_h * max_h + max_v * max_v).sqrt();

                if distance > max_possible {
                    violations.push(Violation::TeleportDetected {
                        distance,
                        max_possible,
                    });
                }
            }
        }

        violations
    }

    /// Whether a baseline exists for this peer (for first-contact handling).
    pub fn has_baseline(&self, hash: &[u8; 16]) -> bool {
        self.last_states.contains_key(hash)
    }

    /// Record a peer's position as the baseline for future delta checks.
    /// Call this only when the state update is accepted (not rejected).
    pub fn accept_state(&mut self, hash: &[u8; 16], x: f32, y: f32, now: f64) {
        self.last_states.insert(
            *hash,
            ValidatedState {
                x,
                y,
                timestamp: now,
            },
        );
    }

    /// Remove tracking for a disconnected peer.
    pub fn clear_peer(&mut self, hash: &[u8; 16]) {
        self.last_states.remove(hash);
    }

    /// Clear all tracking (e.g. on street change).
    pub fn clear_all(&mut self) {
        self.last_states.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> StreetBounds {
        StreetBounds {
            left: -3000.0,
            right: 3000.0,
            top: -1000.0,
            bottom: 0.0,
        }
    }

    fn valid_state(x: f32, y: f32) -> PlayerNetState {
        PlayerNetState {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            facing: 1,
            on_ground: true,
            animation: 0,
        }
    }

    fn hash(id: u8) -> [u8; 16] {
        [id; 16]
    }

    #[test]
    fn valid_state_passes() {
        let mut v = StateValidator::new();
        let violations = v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0, 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn first_state_after_join_always_passes() {
        let mut v = StateValidator::new();
        // Even a state at the far edge is accepted on first contact (no delta ref)
        let violations = v.validate(&hash(1), &valid_state(2980.0, -500.0), &bounds(), 1.0, 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn out_of_bounds_x_detected() {
        let mut v = StateValidator::new();
        // Way past right bound
        let state = valid_state(5000.0, -10.0);
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::OutOfBounds { .. })));
    }

    #[test]
    fn out_of_bounds_y_detected() {
        let mut v = StateValidator::new();
        // Way above top bound
        let state = valid_state(0.0, -1200.0);
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::OutOfBounds { .. })));
    }

    #[test]
    fn within_bounds_with_margin_passes() {
        let mut v = StateValidator::new();
        // Just inside the margin (left + HALF_WIDTH - small tolerance)
        let state = valid_state(-2984.0, -10.0);
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn excessive_vx_detected() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vx = 500.0; // Way past WALK_SPEED * JITTER_TOLERANCE
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::InvalidVelocityX { .. })));
    }

    #[test]
    fn normal_vx_passes() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vx = 200.0; // Exactly WALK_SPEED
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn vx_within_jitter_tolerance_passes() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vx = 290.0; // Between WALK_SPEED and WALK_SPEED * JITTER_TOLERANCE
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn excessive_vy_detected() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vy = 1000.0; // Way past TERMINAL_VELOCITY * JITTER_TOLERANCE
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::InvalidVelocityY { .. })));
    }

    #[test]
    fn upward_vy_not_limited() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vy = -800.0; // Fast upward velocity (negative = up in Glitch)
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 1.0);
        // Upward velocity is not capped — only downward (falling) is
        assert!(!violations.iter().any(|v| matches!(v, Violation::InvalidVelocityY { .. })));
    }

    #[test]
    fn teleport_detected() {
        let mut v = StateValidator::new();
        // First update at (0, -10) — accept to establish baseline
        v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0, 1.0);
        v.accept_state(&hash(1), 0.0, -10.0, 1.0);
        // Second update 100ms later at (5000, -10) — impossible
        let violations = v.validate(&hash(1), &valid_state(5000.0, -10.0), &bounds(), 1.1, 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::TeleportDetected { .. })));
    }

    #[test]
    fn normal_movement_with_jitter_passes() {
        let mut v = StateValidator::new();
        // First update — accept to establish baseline
        v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0, 1.0);
        v.accept_state(&hash(1), 0.0, -10.0, 1.0);
        // 1 second later, moved 180px right (within 200 px/s * 1.5 tolerance)
        let violations = v.validate(&hash(1), &valid_state(180.0, -10.0), &bounds(), 2.0, 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn rapid_updates_do_not_reset_baseline() {
        let mut v = StateValidator::new();
        v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0, 1.0);
        v.accept_state(&hash(1), 0.0, -10.0, 1.0);
        // Rapid update 1ms later at a far position — delta check skipped
        // (elapsed < MIN_DELTA_TIME), no teleport detected
        let violations = v.validate(&hash(1), &valid_state(5000.0, -10.0), &bounds(), 1.005, 1.0);
        assert!(!violations.iter().any(|v| matches!(v, Violation::TeleportDetected { .. })));
        // Don't accept the bad state — baseline stays at (0, -10)
        // Next update at legitimate interval still sees the original baseline
        let violations = v.validate(&hash(1), &valid_state(5000.0, -10.0), &bounds(), 1.1, 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::TeleportDetected { .. })));
    }

    #[test]
    fn clear_peer_removes_tracking() {
        let mut v = StateValidator::new();
        v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0, 1.0);
        v.accept_state(&hash(1), 0.0, -10.0, 1.0);
        v.clear_peer(&hash(1));
        // Next update should be treated as first (no delta check)
        let violations = v.validate(&hash(1), &valid_state(5000.0, -10.0), &bounds(), 1.1, 1.0);
        // No teleport since it's treated as first state
        assert!(!violations.iter().any(|v| matches!(v, Violation::TeleportDetected { .. })));
    }

    #[test]
    fn violation_severity_values() {
        assert!((Violation::OutOfBounds { x: 0.0, y: 0.0 }.severity() - 0.3).abs() < 1e-10);
        assert!((Violation::InvalidVelocityX { vx: 0.0 }.severity() - 0.3).abs() < 1e-10);
        assert!(
            (Violation::TeleportDetected {
                distance: 0.0,
                max_possible: 0.0
            }
            .severity()
                - 0.6)
                .abs()
                < 1e-10
        );
    }

    // ── Tolerance multiplier tests ──────────────────────────────────

    #[test]
    fn tolerance_multiplier_relaxes_velocity_check() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        // 290 px/s: just under WALK_SPEED (200) * JITTER_TOLERANCE (1.5) = 300
        // but above WALK_SPEED * JITTER_TOLERANCE * 1.0 = 300
        // Use 310 to exceed 300 at mult=1.0 but pass at mult=2.0 (threshold=600)
        state.vx = 310.0;

        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::InvalidVelocityX { .. })));

        let violations = v.validate(&hash(1), &state, &bounds(), 1.0, 2.0);
        assert!(!violations.iter().any(|v| matches!(v, Violation::InvalidVelocityX { .. })));
    }

    #[test]
    fn tolerance_multiplier_relaxes_teleport_check() {
        let mut v = StateValidator::new();
        // Establish baseline
        v.accept_state(&hash(1), 0.0, -10.0, 1.0);

        // Move 1000px in 1 second.
        // At mult=1.0: max_h = 200*1*1.5 = 300, max_v = 600*1*1.5 = 900.
        // max_possible = sqrt(300² + 900²) ≈ 949. 1000 > 949 → teleport.
        // At mult=2.0: max_h = 200*1*3.0 = 600, max_v = 600*1*3.0 = 1800.
        // max_possible = sqrt(600² + 1800²) ≈ 1897. 1000 < 1897 → no teleport.
        let violations = v.validate(&hash(1), &valid_state(1000.0, -10.0), &bounds(), 2.0, 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::TeleportDetected { .. })));

        let violations = v.validate(&hash(1), &valid_state(1000.0, -10.0), &bounds(), 2.0, 2.0);
        assert!(!violations.iter().any(|v| matches!(v, Violation::TeleportDetected { .. })));
    }

    #[test]
    fn rejected_state_preserves_old_baseline() {
        let mut v = StateValidator::new();
        // Accept initial position
        v.accept_state(&hash(1), 0.0, -10.0, 1.0);

        // Validate a teleport — violation detected
        let state = valid_state(5000.0, -10.0);
        let violations = v.validate(&hash(1), &state, &bounds(), 1.1, 1.0);
        assert!(!violations.is_empty());

        // Do NOT call accept_state — simulate rejection.
        // Next validation should still use (0, -10) as baseline.
        let violations = v.validate(&hash(1), &valid_state(5000.0, -10.0), &bounds(), 1.2, 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::TeleportDetected { .. })));
    }

    #[test]
    fn accept_state_updates_baseline() {
        let mut v = StateValidator::new();
        v.accept_state(&hash(1), 0.0, -10.0, 1.0);

        // Move to (100, -10) and accept
        let violations = v.validate(&hash(1), &valid_state(100.0, -10.0), &bounds(), 2.0, 1.0);
        assert!(violations.is_empty());
        v.accept_state(&hash(1), 100.0, -10.0, 2.0);

        // Now a small move from (100, -10) to (200, -10) should pass
        let violations = v.validate(&hash(1), &valid_state(200.0, -10.0), &bounds(), 3.0, 1.0);
        assert!(violations.is_empty());
    }
}
