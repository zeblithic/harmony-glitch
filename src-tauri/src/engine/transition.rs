use crate::street::types::Signpost;

const PRE_SUBSCRIBE_DISTANCE: f64 = 500.0;
const MIN_SWOOP_SECS: f64 = 0.3;
const MAX_SWOOP_SECS: f64 = 2.0;

/// Current phase of a street-to-street transition.
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionPhase {
    /// No transition in progress.
    None,
    /// Player is near a signpost; the target street should be pre-loaded.
    PreSubscribed {
        target_street: String,
        signpost_x: f64,
        direction: TransitionDirection,
    },
    /// The swoop animation is in progress.
    Swooping {
        from_street: String,
        to_street: String,
        direction: TransitionDirection,
        progress: f64,
        elapsed: f64,
        target_duration: f64,
        street_ready: bool,
    },
    /// The transition is done; the caller should finalize the street swap.
    Complete {
        new_street: String,
    },
}

/// Which direction the transition slides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionDirection {
    Left,
    Right,
}

/// Pure state machine for street transitions.
///
/// Lifecycle: `None` -> `PreSubscribed` -> `Swooping` -> `Complete` -> `None` (via reset).
pub struct TransitionState {
    pub phase: TransitionPhase,
}

impl TransitionState {
    pub fn new() -> Self {
        Self {
            phase: TransitionPhase::None,
        }
    }

    /// Check whether the player is within pre-subscribe distance of any signpost.
    ///
    /// Only acts when phase is `None`. Sets phase to `PreSubscribed` for the first
    /// matching signpost that has at least one connection.
    pub fn check_signposts(
        &mut self,
        player_x: f64,
        signposts: &[Signpost],
        street_left: f64,
        street_right: f64,
    ) {
        // If already pre-subscribed, check if the player retreated from the signpost.
        if let TransitionPhase::PreSubscribed { signpost_x, .. } = &self.phase {
            if (player_x - signpost_x).abs() > PRE_SUBSCRIBE_DISTANCE {
                self.phase = TransitionPhase::None;
            } else {
                return; // Still near the signpost — no change needed.
            }
        }

        if self.phase != TransitionPhase::None {
            return;
        }

        let street_mid = (street_left + street_right) / 2.0;

        for signpost in signposts {
            if signpost.connects.is_empty() {
                continue;
            }

            let distance = (player_x - signpost.x).abs();
            if distance <= PRE_SUBSCRIBE_DISTANCE {
                let direction = if signpost.x < street_mid {
                    TransitionDirection::Left
                } else {
                    TransitionDirection::Right
                };

                self.phase = TransitionPhase::PreSubscribed {
                    target_street: signpost.connects[0].target_tsid.clone(),
                    signpost_x: signpost.x,
                    direction,
                };
                return;
            }
        }
    }

    /// Begin the swoop animation. Only acts when phase is `PreSubscribed`.
    pub fn trigger_swoop(&mut self, from_street: String) {
        if let TransitionPhase::PreSubscribed {
            target_street,
            direction,
            ..
        } = &self.phase
        {
            self.phase = TransitionPhase::Swooping {
                from_street,
                to_street: target_street.clone(),
                direction: *direction,
                progress: 0.0,
                elapsed: 0.0,
                target_duration: MAX_SWOOP_SECS,
                street_ready: false,
            };
        }
    }

    /// Signal that the target street data has been loaded and is ready to display.
    ///
    /// Only acts when phase is `Swooping`. Shrinks the remaining duration so the
    /// animation completes promptly, but never faster than `MIN_SWOOP_SECS`.
    pub fn mark_street_ready(&mut self) {
        if let TransitionPhase::Swooping {
            elapsed,
            target_duration,
            street_ready,
            ..
        } = &mut self.phase
        {
            *street_ready = true;
            // Finish the swoop in MIN_SWOOP_SECS from now. The total duration
            // is always at least MIN_SWOOP_SECS (guards against near-instant swoops).
            *target_duration = (*elapsed + MIN_SWOOP_SECS).max(MIN_SWOOP_SECS);
        }
    }

    /// Advance the swoop animation by `dt` seconds.
    ///
    /// While the target street is not ready, progress caps at 0.9 (stalls).
    /// Once ready, progress advances to 1.0 and the phase becomes `Complete`.
    pub fn tick(&mut self, dt: f64) {
        let next_phase = match &mut self.phase {
            TransitionPhase::Swooping {
                to_street,
                elapsed,
                target_duration,
                street_ready,
                progress,
                ..
            } => {
                *elapsed += dt;

                if *street_ready {
                    *progress = (*elapsed / *target_duration).min(1.0);
                    if *progress >= 1.0 {
                        Some(TransitionPhase::Complete {
                            new_street: to_street.clone(),
                        })
                    } else {
                        Option::None
                    }
                } else if *elapsed >= MAX_SWOOP_SECS {
                    // Street data never arrived — cancel so the player isn't
                    // stuck at 90% swoop forever.
                    Some(TransitionPhase::None)
                } else {
                    *progress = (*elapsed / MAX_SWOOP_SECS).min(0.9);
                    Option::None
                }
            }
            _ => Option::None,
        };

        if let Some(phase) = next_phase {
            self.phase = phase;
        }
    }

    /// Returns `(progress, direction)` if currently swooping, `None` otherwise.
    pub fn swoop_progress(&self) -> Option<(f64, TransitionDirection)> {
        if let TransitionPhase::Swooping {
            progress,
            direction,
            ..
        } = &self.phase
        {
            Some((*progress, *direction))
        } else {
            Option::None
        }
    }

    /// Reset the transition state back to `None`.
    pub fn reset(&mut self) {
        self.phase = TransitionPhase::None;
    }
}

impl Default for TransitionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::types::{Signpost, SignpostConnection};

    fn make_signpost(x: f64, target: &str) -> Signpost {
        Signpost {
            id: "sign".into(),
            x,
            y: 0.0,
            connects: vec![SignpostConnection {
                target_tsid: target.into(),
                target_label: "Go".into(),
            }],
        }
    }

    #[test]
    fn detects_pre_subscribe_zone() {
        let mut ts = TransitionState::new();
        let signposts = vec![make_signpost(1950.0, "LADEMO002")];

        // Player at 1500, signpost at 1950 => distance 450 < 500
        ts.check_signposts(1500.0, &signposts, -2000.0, 2000.0);

        match &ts.phase {
            TransitionPhase::PreSubscribed {
                target_street,
                signpost_x,
                direction,
            } => {
                assert_eq!(target_street, "LADEMO002");
                assert!((signpost_x - 1950.0).abs() < 0.001);
                // signpost.x=1950 >= street_mid=0, so direction is Right
                assert_eq!(*direction, TransitionDirection::Right);
            }
            other => panic!("Expected PreSubscribed, got {:?}", other),
        }
    }

    #[test]
    fn no_detection_when_far_from_signpost() {
        let mut ts = TransitionState::new();
        let signposts = vec![make_signpost(1950.0, "LADEMO002")];

        // Player at 0, signpost at 1950 => distance 1950 > 500
        ts.check_signposts(0.0, &signposts, -2000.0, 2000.0);

        assert_eq!(ts.phase, TransitionPhase::None);
    }

    #[test]
    fn swoop_completes_when_ready() {
        let mut ts = TransitionState::new();
        let signposts = vec![make_signpost(1950.0, "LADEMO002")];

        ts.check_signposts(1500.0, &signposts, -2000.0, 2000.0);
        ts.trigger_swoop("LADEMO001".into());

        // Simulate network delay: tick for 1 second before street data arrives
        for _ in 0..60 {
            ts.tick(1.0 / 60.0);
        }

        // Street data arrives at t=1.0s — shrinks remaining from 1.0 to 0.3
        ts.mark_street_ready();

        // Tick another 30 frames (0.5s) — well past the 0.3s minimum remaining
        for _ in 0..30 {
            ts.tick(1.0 / 60.0);
        }

        match &ts.phase {
            TransitionPhase::Complete { new_street } => {
                assert_eq!(new_street, "LADEMO002");
            }
            other => panic!("Expected Complete, got {:?}", other),
        }
    }

    #[test]
    fn swoop_stalls_then_cancels_without_ready() {
        let mut ts = TransitionState::new();
        let signposts = vec![make_signpost(1950.0, "LADEMO002")];

        ts.check_signposts(1500.0, &signposts, -2000.0, 2000.0);
        ts.trigger_swoop("LADEMO001".into());
        // Do NOT mark street ready

        // Tick 60 times at 1/60s = 1 second (within MAX_SWOOP_SECS) — should stall at ≤0.9
        for _ in 0..60 {
            ts.tick(1.0 / 60.0);
        }
        match &ts.phase {
            TransitionPhase::Swooping { progress, .. } => {
                assert!(
                    *progress <= 0.9,
                    "Progress should be capped at 0.9, got {}",
                    progress
                );
            }
            other => panic!("Expected Swooping (stalled), got {:?}", other),
        }

        // Tick past MAX_SWOOP_SECS (2.0s total) — should cancel
        for _ in 0..120 {
            ts.tick(1.0 / 60.0);
        }
        assert_eq!(
            ts.phase,
            TransitionPhase::None,
            "Should cancel after MAX_SWOOP_SECS timeout"
        );
    }

    #[test]
    fn retreating_from_signpost_resets_to_none() {
        let mut ts = TransitionState::new();
        let signposts = vec![make_signpost(1950.0, "LADEMO002")];

        // Enter pre-subscribe zone
        ts.check_signposts(1500.0, &signposts, -2000.0, 2000.0);
        assert!(matches!(ts.phase, TransitionPhase::PreSubscribed { .. }));

        // Walk away from the signpost
        ts.check_signposts(0.0, &signposts, -2000.0, 2000.0);
        assert_eq!(ts.phase, TransitionPhase::None);

        // Should be able to re-enter the zone
        ts.check_signposts(1600.0, &signposts, -2000.0, 2000.0);
        assert!(matches!(ts.phase, TransitionPhase::PreSubscribed { .. }));
    }

    #[test]
    fn swoop_cancels_on_timeout() {
        let mut ts = TransitionState::new();
        let signposts = vec![make_signpost(1950.0, "LADEMO002")];

        ts.check_signposts(1500.0, &signposts, -2000.0, 2000.0);
        ts.trigger_swoop("LADEMO001".into());
        // Do NOT mark street ready — simulate failed load

        // Tick past MAX_SWOOP_SECS (2.0s) — 150 frames at 1/60s = 2.5s
        for _ in 0..150 {
            ts.tick(1.0 / 60.0);
        }

        // Should have cancelled back to None, not stuck at 90%
        assert_eq!(ts.phase, TransitionPhase::None);
    }

    #[test]
    fn minimum_swoop_duration_respected() {
        let mut ts = TransitionState::new();
        let signposts = vec![make_signpost(1950.0, "LADEMO002")];

        ts.check_signposts(1500.0, &signposts, -2000.0, 2000.0);
        ts.trigger_swoop("LADEMO001".into());
        // Mark ready immediately — target_duration should shrink to MIN_SWOOP_SECS (0.3)
        ts.mark_street_ready();

        // Tick 6 times at 1/60s = 0.1 seconds (less than MIN_SWOOP_SECS=0.3)
        for _ in 0..6 {
            ts.tick(1.0 / 60.0);
        }

        match &ts.phase {
            TransitionPhase::Swooping { progress, .. } => {
                assert!(
                    *progress < 1.0,
                    "Progress should be < 1.0 (min swoop not elapsed), got {}",
                    progress
                );
            }
            other => panic!(
                "Expected still Swooping (min duration not elapsed), got {:?}",
                other
            ),
        }
    }
}
