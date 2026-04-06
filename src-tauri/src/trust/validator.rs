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
    /// violations (empty = valid). Always stores the state for future
    /// delta checking regardless of violations.
    pub fn validate(
        &mut self,
        address_hash: &[u8; 16],
        state: &PlayerNetState,
        bounds: &StreetBounds,
        now: f64,
    ) -> Vec<Violation> {
        let mut violations = Vec::new();

        // 1. Position bounds
        let min_x = bounds.left + HALF_WIDTH;
        let max_x = bounds.right - HALF_WIDTH;
        if (state.x as f64) < min_x - 1.0 || (state.x as f64) > max_x + 1.0 {
            violations.push(Violation::OutOfBounds {
                x: state.x,
                y: state.y,
            });
        }
        // Y bounds: allow some margin above top (jumping) and below bottom
        if (state.y as f64) < bounds.top - 100.0 || (state.y as f64) > bounds.bottom + 50.0 {
            violations.push(Violation::OutOfBounds {
                x: state.x,
                y: state.y,
            });
        }

        // 2. Velocity bounds
        if state.vx.abs() > MAX_VX * JITTER_TOLERANCE {
            violations.push(Violation::InvalidVelocityX { vx: state.vx });
        }
        // Only check downward velocity (positive vy in Glitch coords = falling)
        if state.vy > MAX_VY_DOWN * JITTER_TOLERANCE {
            violations.push(Violation::InvalidVelocityY { vy: state.vy });
        }

        // 3. Teleport detection (delta check against previous state)
        if let Some(prev) = self.last_states.get(address_hash) {
            let elapsed = now - prev.timestamp;
            if elapsed >= MIN_DELTA_TIME {
                let dx = (state.x - prev.x).abs();
                let dy = (state.y - prev.y).abs();
                let distance = (dx * dx + dy * dy).sqrt();

                // Max possible movement considering physics limits + jitter
                let max_h = MAX_VX * elapsed as f32 * JITTER_TOLERANCE;
                // Vertical: consider gravity acceleration + terminal velocity
                let max_v = MAX_VY_DOWN * elapsed as f32 * JITTER_TOLERANCE;
                let max_possible = (max_h * max_h + max_v * max_v).sqrt();

                if distance > max_possible {
                    violations.push(Violation::TeleportDetected {
                        distance,
                        max_possible,
                    });
                }
            }
        }

        // Always store for next delta check
        self.last_states.insert(
            *address_hash,
            ValidatedState {
                x: state.x,
                y: state.y,
                timestamp: now,
            },
        );

        violations
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
        let violations = v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn first_state_after_join_always_passes() {
        let mut v = StateValidator::new();
        // Even a state at the far edge is accepted on first contact (no delta ref)
        let violations = v.validate(&hash(1), &valid_state(2980.0, -500.0), &bounds(), 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn out_of_bounds_x_detected() {
        let mut v = StateValidator::new();
        // Way past right bound
        let state = valid_state(5000.0, -10.0);
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::OutOfBounds { .. })));
    }

    #[test]
    fn out_of_bounds_y_detected() {
        let mut v = StateValidator::new();
        // Way above top bound
        let state = valid_state(0.0, -1200.0);
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::OutOfBounds { .. })));
    }

    #[test]
    fn within_bounds_with_margin_passes() {
        let mut v = StateValidator::new();
        // Just inside the margin (left + HALF_WIDTH - small tolerance)
        let state = valid_state(-2984.0, -10.0);
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn excessive_vx_detected() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vx = 500.0; // Way past WALK_SPEED * JITTER_TOLERANCE
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::InvalidVelocityX { .. })));
    }

    #[test]
    fn normal_vx_passes() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vx = 200.0; // Exactly WALK_SPEED
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn vx_within_jitter_tolerance_passes() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vx = 290.0; // Between WALK_SPEED and WALK_SPEED * JITTER_TOLERANCE
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn excessive_vy_detected() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vy = 1000.0; // Way past TERMINAL_VELOCITY * JITTER_TOLERANCE
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0);
        assert!(violations.iter().any(|v| matches!(v, Violation::InvalidVelocityY { .. })));
    }

    #[test]
    fn upward_vy_not_limited() {
        let mut v = StateValidator::new();
        let mut state = valid_state(0.0, -10.0);
        state.vy = -800.0; // Fast upward velocity (negative = up in Glitch)
        let violations = v.validate(&hash(1), &state, &bounds(), 1.0);
        // Upward velocity is not capped — only downward (falling) is
        assert!(!violations.iter().any(|v| matches!(v, Violation::InvalidVelocityY { .. })));
    }

    #[test]
    fn teleport_detected() {
        let mut v = StateValidator::new();
        // First update at (0, 0)
        v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0);
        // Second update 100ms later at (5000, 0) — impossible
        let violations = v.validate(&hash(1), &valid_state(5000.0, -10.0), &bounds(), 1.1);
        assert!(violations.iter().any(|v| matches!(v, Violation::TeleportDetected { .. })));
    }

    #[test]
    fn normal_movement_with_jitter_passes() {
        let mut v = StateValidator::new();
        // First update
        v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0);
        // 1 second later, moved 180px right (within 200 px/s * 1.5 tolerance)
        let violations = v.validate(&hash(1), &valid_state(180.0, -10.0), &bounds(), 2.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn rapid_updates_skip_delta_check() {
        let mut v = StateValidator::new();
        v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0);
        // Update 1ms later — within MIN_DELTA_TIME, delta check skipped
        let violations = v.validate(&hash(1), &valid_state(100.0, -10.0), &bounds(), 1.005);
        // No teleport violation because delta check is skipped for very short intervals
        assert!(!violations.iter().any(|v| matches!(v, Violation::TeleportDetected { .. })));
    }

    #[test]
    fn clear_peer_removes_tracking() {
        let mut v = StateValidator::new();
        v.validate(&hash(1), &valid_state(0.0, -10.0), &bounds(), 1.0);
        v.clear_peer(&hash(1));
        // Next update should be treated as first (no delta check)
        let violations = v.validate(&hash(1), &valid_state(5000.0, -10.0), &bounds(), 1.1);
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
}
