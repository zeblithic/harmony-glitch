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

    // ── Transitive trust operators (Josang 2016) ────────────────────

    /// Trust-discount operator (§3.7): weight a recommendation by our
    /// trust in the recommender.
    ///
    /// `self` is our opinion about recommender R. `recommendation` is R's
    /// opinion about subject X. Returns our derived opinion about X.
    ///
    /// If we don't trust R (vacuous → b=0), the result is vacuous.
    /// If we fully trust R (b=1), R's opinion passes through unchanged.
    pub fn discount(&self, recommendation: &Opinion) -> Opinion {
        let b = self.belief * recommendation.belief;
        let d = self.belief * recommendation.disbelief;
        let u = self.disbelief + self.uncertainty + self.belief * recommendation.uncertainty;
        let mut result = Opinion {
            belief: b,
            disbelief: d,
            uncertainty: u,
        };
        result.renormalize();
        result
    }

    /// Cumulative Belief Fusion (§12.3): combine two independent opinions
    /// about the same subject.
    ///
    /// Fusing with a vacuous opinion returns the other (identity element).
    /// Fusing two vacuous opinions returns vacuous.
    pub fn fuse(&self, other: &Opinion) -> Opinion {
        let ua = self.uncertainty;
        let ub = other.uncertainty;

        // Edge cases: one or both vacuous
        if ua >= 1.0 - 1e-12 && ub >= 1.0 - 1e-12 {
            return Opinion::vacuous();
        }
        if ua >= 1.0 - 1e-12 {
            return *other;
        }
        if ub >= 1.0 - 1e-12 {
            return *self;
        }

        let k = ua + ub - ua * ub;
        if k < 1e-15 {
            return Opinion::vacuous();
        }

        let b = (self.belief * ub + other.belief * ua) / k;
        let d = (self.disbelief * ub + other.disbelief * ua) / k;
        let u = (ua * ub) / k;

        let mut result = Opinion {
            belief: b,
            disbelief: d,
            uncertainty: u,
        };
        result.renormalize();
        result
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

    // ── Discount operator tests ─────────────────────────────────────

    #[test]
    fn discount_vacuous_recommender_returns_vacuous() {
        let vacuous = Opinion::vacuous();
        let distrust = Opinion::full_distrust();
        let result = vacuous.discount(&distrust);
        // b_AR=0 → numerator is zero → result is vacuous
        assert!(result.uncertainty > 1.0 - EPSILON);
        assert_invariant(&result);
    }

    #[test]
    fn discount_full_trust_passes_through() {
        let full = Opinion::full_trust();
        let recommendation = Opinion {
            belief: 0.3,
            disbelief: 0.5,
            uncertainty: 0.2,
        };
        let result = full.discount(&recommendation);
        // b_AR=1, d_AR=0, u_AR=0 → passes through
        assert!((result.belief - 0.3).abs() < EPSILON);
        assert!((result.disbelief - 0.5).abs() < EPSILON);
        assert!((result.uncertainty - 0.2).abs() < EPSILON);
        assert_invariant(&result);
    }

    #[test]
    fn discount_partial_trust_scales() {
        // We half-trust the recommender: (0.5, 0, 0.5)
        let our_trust = Opinion {
            belief: 0.5,
            disbelief: 0.0,
            uncertainty: 0.5,
        };
        // Recommender fully distrusts subject: (0, 1, 0)
        let rec = Opinion::full_distrust();
        let result = our_trust.discount(&rec);
        // b = 0.5*0 = 0, d = 0.5*1 = 0.5, u = 0 + 0.5 + 0.5*0 = 0.5
        assert!(result.belief < EPSILON);
        assert!((result.disbelief - 0.5).abs() < EPSILON);
        assert!((result.uncertainty - 0.5).abs() < EPSILON);
        assert_invariant(&result);
    }

    #[test]
    fn discount_preserves_invariant() {
        let a = Opinion {
            belief: 0.6,
            disbelief: 0.1,
            uncertainty: 0.3,
        };
        let b = Opinion {
            belief: 0.2,
            disbelief: 0.7,
            uncertainty: 0.1,
        };
        let result = a.discount(&b);
        assert_invariant(&result);
    }

    // ── Fusion operator tests ───────────────────────────────────────

    #[test]
    fn fuse_vacuous_identity() {
        let vacuous = Opinion::vacuous();
        let opinion = Opinion {
            belief: 0.4,
            disbelief: 0.3,
            uncertainty: 0.3,
        };
        let result = vacuous.fuse(&opinion);
        assert!((result.belief - 0.4).abs() < EPSILON);
        assert!((result.disbelief - 0.3).abs() < EPSILON);
        assert!((result.uncertainty - 0.3).abs() < EPSILON);
        // Commutative check
        let result2 = opinion.fuse(&vacuous);
        assert!((result2.belief - 0.4).abs() < EPSILON);
    }

    #[test]
    fn fuse_strengthens_agreement() {
        // Two independent sources both distrust the subject
        let a = Opinion {
            belief: 0.1,
            disbelief: 0.6,
            uncertainty: 0.3,
        };
        let b = Opinion {
            belief: 0.05,
            disbelief: 0.7,
            uncertainty: 0.25,
        };
        let fused = a.fuse(&b);
        // Fused disbelief should be stronger (higher) than either alone
        assert!(fused.disbelief > a.disbelief);
        assert!(fused.disbelief > b.disbelief);
        // Uncertainty should be lower
        assert!(fused.uncertainty < a.uncertainty);
        assert!(fused.uncertainty < b.uncertainty);
        assert_invariant(&fused);
    }

    #[test]
    fn fuse_opposing_opinions_average() {
        let trust = Opinion {
            belief: 0.8,
            disbelief: 0.0,
            uncertainty: 0.2,
        };
        let distrust = Opinion {
            belief: 0.0,
            disbelief: 0.8,
            uncertainty: 0.2,
        };
        let fused = trust.fuse(&distrust);
        // Opposing opinions should result in moderate belief and disbelief
        assert!(fused.belief > 0.1);
        assert!(fused.disbelief > 0.1);
        // And very low uncertainty (both sources are quite certain)
        assert!(fused.uncertainty < 0.15);
        assert_invariant(&fused);
    }

    #[test]
    fn fuse_preserves_invariant() {
        let a = Opinion {
            belief: 0.3,
            disbelief: 0.4,
            uncertainty: 0.3,
        };
        let b = Opinion {
            belief: 0.7,
            disbelief: 0.1,
            uncertainty: 0.2,
        };
        let result = a.fuse(&b);
        assert_invariant(&result);
    }

    #[test]
    fn fuse_is_commutative() {
        let a = Opinion {
            belief: 0.3,
            disbelief: 0.4,
            uncertainty: 0.3,
        };
        let b = Opinion {
            belief: 0.7,
            disbelief: 0.1,
            uncertainty: 0.2,
        };
        let ab = a.fuse(&b);
        let ba = b.fuse(&a);
        assert!((ab.belief - ba.belief).abs() < EPSILON);
        assert!((ab.disbelief - ba.disbelief).abs() < EPSILON);
        assert!((ab.uncertainty - ba.uncertainty).abs() < EPSILON);
    }
}
