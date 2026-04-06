/// A Subjective Logic opinion: belief + disbelief + uncertainty = 1.0.
///
/// - Vacuous `(0, 0, 1)` = total uncertainty (new/unknown peer)
/// - Full trust `(1, 0, 0)` = complete belief
/// - Full distrust `(0, 1, 0)` = complete disbelief (blackholed peer)
///
/// The base rate `a` (prior probability of trust) is fixed at 0.5,
/// giving the expectation formula: `E = b + a * u`.
#[derive(Debug, Clone, Copy)]
pub struct Opinion {
    pub belief: f64,
    pub disbelief: f64,
    pub uncertainty: f64,
}

/// Base rate for expectation calculation.
const BASE_RATE: f64 = 0.5;

impl Opinion {
    /// Total uncertainty — the default for an unknown peer.
    pub fn vacuous() -> Self {
        Self {
            belief: 0.0,
            disbelief: 0.0,
            uncertainty: 1.0,
        }
    }

    /// Complete belief — used in tests.
    pub fn full_trust() -> Self {
        Self {
            belief: 1.0,
            disbelief: 0.0,
            uncertainty: 0.0,
        }
    }

    /// Complete disbelief — a slashed/blackholed peer.
    pub fn full_distrust() -> Self {
        Self {
            belief: 0.0,
            disbelief: 1.0,
            uncertainty: 0.0,
        }
    }

    /// Expected trust value in [0, 1]. Maps the three-component opinion
    /// to a single scalar: `b + base_rate * u`.
    pub fn expectation(&self) -> f64 {
        self.belief + BASE_RATE * self.uncertainty
    }

    /// Shift opinion toward belief. Weight in (0, 1] controls how much
    /// uncertainty is converted to belief.
    pub fn update_positive(&mut self, weight: f64) {
        let w = weight.clamp(0.0, 1.0);
        let shift = self.uncertainty * w;
        self.belief += shift;
        self.uncertainty -= shift;
        self.renormalize();
    }

    /// Shift opinion toward disbelief. Weight in (0, 1] controls how much
    /// uncertainty is converted to disbelief.
    pub fn update_negative(&mut self, weight: f64) {
        let w = weight.clamp(0.0, 1.0);
        let shift = self.uncertainty * w;
        self.disbelief += shift;
        self.uncertainty -= shift;
        self.renormalize();
    }

    /// Decay belief and disbelief toward zero, increasing uncertainty.
    /// `factor` in (0, 1] — fraction of certainty to decay.
    pub fn decay(&mut self, factor: f64) {
        let f = factor.clamp(0.0, 1.0);
        let lost_b = self.belief * f;
        let lost_d = self.disbelief * f;
        self.belief -= lost_b;
        self.disbelief -= lost_d;
        self.uncertainty += lost_b + lost_d;
        self.renormalize();
    }

    /// Enforce b + d + u = 1.0 invariant (absorbs floating-point drift).
    fn renormalize(&mut self) {
        let sum = self.belief + self.disbelief + self.uncertainty;
        if sum > 0.0 {
            self.belief /= sum;
            self.disbelief /= sum;
            self.uncertainty /= sum;
        } else {
            *self = Self::vacuous();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-10;

    fn assert_invariant(o: &Opinion) {
        let sum = o.belief + o.disbelief + o.uncertainty;
        assert!(
            (sum - 1.0).abs() < EPSILON,
            "Invariant violated: b={} d={} u={} sum={}",
            o.belief,
            o.disbelief,
            o.uncertainty,
            sum
        );
    }

    #[test]
    fn vacuous_has_max_uncertainty() {
        let o = Opinion::vacuous();
        assert_eq!(o.belief, 0.0);
        assert_eq!(o.disbelief, 0.0);
        assert_eq!(o.uncertainty, 1.0);
        assert_invariant(&o);
    }

    #[test]
    fn full_trust_has_max_belief() {
        let o = Opinion::full_trust();
        assert_eq!(o.belief, 1.0);
        assert_eq!(o.uncertainty, 0.0);
        assert_invariant(&o);
    }

    #[test]
    fn full_distrust_has_max_disbelief() {
        let o = Opinion::full_distrust();
        assert_eq!(o.disbelief, 1.0);
        assert_eq!(o.uncertainty, 0.0);
        assert_invariant(&o);
    }

    #[test]
    fn vacuous_expectation_is_base_rate() {
        let o = Opinion::vacuous();
        assert!((o.expectation() - 0.5).abs() < EPSILON);
    }

    #[test]
    fn full_trust_expectation_is_one() {
        let o = Opinion::full_trust();
        assert!((o.expectation() - 1.0).abs() < EPSILON);
    }

    #[test]
    fn full_distrust_expectation_is_zero() {
        let o = Opinion::full_distrust();
        assert!((o.expectation() - 0.0).abs() < EPSILON);
    }

    #[test]
    fn positive_update_increases_belief() {
        let mut o = Opinion::vacuous();
        let before = o.belief;
        o.update_positive(0.1);
        assert!(o.belief > before);
        assert!(o.uncertainty < 1.0);
        assert_invariant(&o);
    }

    #[test]
    fn negative_update_increases_disbelief() {
        let mut o = Opinion::vacuous();
        let before = o.disbelief;
        o.update_negative(0.1);
        assert!(o.disbelief > before);
        assert!(o.uncertainty < 1.0);
        assert_invariant(&o);
    }

    #[test]
    fn decay_moves_toward_vacuous() {
        let mut o = Opinion::full_trust();
        o.decay(0.5);
        assert!(o.belief < 1.0);
        assert!(o.uncertainty > 0.0);
        assert_invariant(&o);
    }

    #[test]
    fn decay_of_vacuous_stays_vacuous() {
        let mut o = Opinion::vacuous();
        o.decay(0.5);
        assert!((o.uncertainty - 1.0).abs() < EPSILON);
        assert_invariant(&o);
    }

    #[test]
    fn multiple_positive_updates_accumulate() {
        let mut o = Opinion::vacuous();
        for _ in 0..10 {
            o.update_positive(0.1);
        }
        assert!(o.belief > 0.5);
        assert!(o.expectation() > 0.75);
        assert_invariant(&o);
    }

    #[test]
    fn invariant_holds_after_mixed_operations() {
        let mut o = Opinion::vacuous();
        o.update_positive(0.3);
        o.update_negative(0.1);
        o.decay(0.2);
        o.update_positive(0.05);
        o.update_negative(0.4);
        o.decay(0.1);
        assert_invariant(&o);
    }

    #[test]
    fn weight_clamped_to_unit_range() {
        let mut o = Opinion::vacuous();
        o.update_positive(5.0); // Should clamp to 1.0
        assert!((o.belief - 1.0).abs() < EPSILON);
        assert_invariant(&o);
    }

    #[test]
    fn expectation_reflects_belief_and_uncertainty() {
        let mut o = Opinion::vacuous();
        // Start at 0.5 expectation (vacuous)
        assert!((o.expectation() - 0.5).abs() < EPSILON);

        o.update_positive(0.5);
        // With some belief and remaining uncertainty, expectation should be > 0.5
        assert!(o.expectation() > 0.5);
        assert!(o.expectation() < 1.0);
        assert_invariant(&o);
    }
}
