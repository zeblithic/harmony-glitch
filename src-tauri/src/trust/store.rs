use std::collections::HashMap;

use super::opinion::Opinion;

/// Weight applied to trust opinion on a successful trade.
const TRADE_SUCCESS_WEIGHT: f64 = 0.1;
/// Weight applied to trust opinion on a failed/cancelled trade.
const TRADE_FAILURE_WEIGHT: f64 = 0.15;
/// Base weight per second of co-presence (logarithmically capped).
const COPRESENCE_WEIGHT_PER_SEC: f64 = 0.001;
/// Co-presence trust accumulation caps at this many seconds (diminishing returns).
const COPRESENCE_CAP_SECS: f64 = 3600.0;
/// Passive decay rate per second (fraction of certainty that fades).
const PASSIVE_DECAY_RATE: f64 = 0.0001;
/// Violation severity that triggers immediate full distrust.
const CRITICAL_SEVERITY: f64 = 1.0;

/// Per-peer trust record with metadata.
#[derive(Debug, Clone)]
pub struct PeerTrust {
    pub opinion: Opinion,
    pub successful_trades: u32,
    pub failed_trades: u32,
    pub violations: u32,
    pub copresence_secs: f64,
    pub last_seen: f64,
    /// Address hash of the Citizen who vouched for this peer (first wins).
    pub vouched_by: Option<[u8; 16]>,
}

impl PeerTrust {
    fn new() -> Self {
        Self {
            opinion: Opinion::vacuous(),
            successful_trades: 0,
            failed_trades: 0,
            violations: 0,
            copresence_secs: 0.0,
            last_seen: 0.0,
            vouched_by: None,
        }
    }
}

/// Stores per-peer trust opinions keyed by address hash.
pub struct TrustStore {
    peers: HashMap<[u8; 16], PeerTrust>,
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TrustStore {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    /// Get or create a trust record for a peer.
    pub fn get_or_insert(&mut self, hash: &[u8; 16]) -> &mut PeerTrust {
        self.peers.entry(*hash).or_insert_with(PeerTrust::new)
    }

    /// Record a successfully completed trade with a peer.
    pub fn record_trade_success(&mut self, hash: &[u8; 16]) {
        let pt = self.get_or_insert(hash);
        pt.successful_trades += 1;
        pt.opinion.update_positive(TRADE_SUCCESS_WEIGHT);
    }

    /// Record a provably dishonest trade with a peer (e.g. terms hash
    /// manipulation). Not wired for normal cancellations/timeouts — those
    /// are legitimate cooperative behavior, not trust violations.
    /// ZEB-22 (adaptive validation) will wire this for specific abuse patterns.
    pub fn record_trade_failure(&mut self, hash: &[u8; 16]) {
        let pt = self.get_or_insert(hash);
        pt.failed_trades += 1;
        pt.opinion.update_negative(TRADE_FAILURE_WEIGHT);
    }

    /// Accumulate co-presence time. Trust grows with linear diminishing
    /// returns, capping at COPRESENCE_CAP_SECS to prevent passive farming.
    pub fn record_copresence(&mut self, hash: &[u8; 16], dt: f64) {
        let pt = self.get_or_insert(hash);
        pt.copresence_secs += dt;
        // Linear diminishing returns: effective weight drops as copresence grows
        let ratio = (pt.copresence_secs / COPRESENCE_CAP_SECS).min(1.0);
        let effective_weight = COPRESENCE_WEIGHT_PER_SEC * dt * (1.0 - ratio);
        if effective_weight > 0.0 {
            pt.opinion.update_positive(effective_weight);
        }
    }

    /// Record a state validation violation. Severity in [0, 1]:
    /// - < 1.0: proportional negative update
    /// - >= 1.0 (CRITICAL): immediate slash to full distrust
    pub fn record_violation(&mut self, hash: &[u8; 16], severity: f64) {
        let pt = self.get_or_insert(hash);
        pt.violations += 1;
        if severity >= CRITICAL_SEVERITY {
            pt.opinion = Opinion::full_distrust();
        } else {
            let weight = severity.clamp(0.0, 1.0) * 0.3;
            pt.opinion.update_negative(weight);
        }
    }

    /// Decay all peer opinions toward vacuous. Called each tick.
    pub fn tick_decay(&mut self, dt: f64) {
        let factor = PASSIVE_DECAY_RATE * dt;
        if factor <= 0.0 {
            return;
        }
        for pt in self.peers.values_mut() {
            pt.opinion.decay(factor);
        }
    }

    /// Get the direct observation opinion for a peer (None if unknown).
    pub fn direct_opinion(&self, hash: &[u8; 16]) -> Option<Opinion> {
        self.peers.get(hash).map(|pt| pt.opinion)
    }

    /// Get the accumulated copresence seconds for a peer. Returns 0.0 for unknown peers.
    pub fn copresence_secs(&self, hash: &[u8; 16]) -> f64 {
        self.peers
            .get(hash)
            .map(|pt| pt.copresence_secs)
            .unwrap_or(0.0)
    }

    /// Get who vouched for a peer (None if not vouched or unknown).
    pub fn vouched_by(&self, hash: &[u8; 16]) -> Option<[u8; 16]> {
        self.peers.get(hash).and_then(|pt| pt.vouched_by)
    }

    /// Record a vouch from a Citizen for a peer. First vouch wins —
    /// subsequent vouches are silently ignored.
    pub fn record_vouch(&mut self, subject: &[u8; 16], voucher: &[u8; 16]) {
        let pt = self.get_or_insert(subject);
        if pt.vouched_by.is_none() {
            pt.vouched_by = Some(*voucher);
        }
    }

    /// Apply voucher liability: penalize a voucher when their vouchee
    /// commits a critical violation.
    pub fn apply_vouch_liability(&mut self, voucher: &[u8; 16], weight: f64) {
        let pt = self.get_or_insert(voucher);
        pt.opinion.update_negative(weight);
    }

    /// Get the trust expectation for a peer. Returns 0.5 (vacuous base rate)
    /// for unknown peers.
    pub fn expectation(&self, hash: &[u8; 16]) -> f64 {
        self.peers
            .get(hash)
            .map(|pt| pt.opinion.expectation())
            .unwrap_or(0.5)
    }

    /// Get the violation count for a peer. Returns 0 for unknown peers.
    pub fn violation_count(&self, hash: &[u8; 16]) -> u32 {
        self.peers.get(hash).map(|pt| pt.violations).unwrap_or(0)
    }

    /// Whether a peer is fully distrusted and should be ignored.
    pub fn is_blackholed(&self, hash: &[u8; 16]) -> bool {
        self.peers
            .get(hash)
            .is_some_and(|pt| pt.opinion.disbelief >= 0.99 && pt.opinion.uncertainty < 0.01)
    }

    /// Whether a peer's messages should be silently discarded based on
    /// direct observation alone. The full suppression check (direct +
    /// gossip-derived) is `NetworkState::is_peer_suppressed()`.
    pub fn is_suppressed(&self, hash: &[u8; 16]) -> bool {
        self.is_blackholed(hash)
    }

    /// Remove trust data for a peer (e.g. on disconnect).
    pub fn remove(&mut self, hash: &[u8; 16]) {
        self.peers.remove(hash);
    }

    /// Clear all trust data (e.g. on street change).
    pub fn clear(&mut self) {
        self.peers.clear();
    }

    /// Number of tracked peers.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.peers.len()
    }

    /// Direct access for tests.
    #[cfg(test)]
    pub fn get(&self, hash: &[u8; 16]) -> Option<&PeerTrust> {
        self.peers.get(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(id: u8) -> [u8; 16] {
        [id; 16]
    }

    #[test]
    fn new_peer_starts_vacuous() {
        let mut store = TrustStore::new();
        let pt = store.get_or_insert(&hash(1));
        assert_eq!(pt.opinion.uncertainty, 1.0);
        assert_eq!(pt.successful_trades, 0);
        assert_eq!(pt.violations, 0);
    }

    #[test]
    fn unknown_peer_expectation_is_base_rate() {
        let store = TrustStore::new();
        assert!((store.expectation(&hash(99)) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn trade_success_increases_trust() {
        let mut store = TrustStore::new();
        let before = store.expectation(&hash(1));
        store.record_trade_success(&hash(1));
        assert!(store.expectation(&hash(1)) > before);
        assert_eq!(store.get(&hash(1)).unwrap().successful_trades, 1);
    }

    #[test]
    fn trade_failure_decreases_trust() {
        let mut store = TrustStore::new();
        // Build some trust first
        for _ in 0..5 {
            store.record_trade_success(&hash(1));
        }
        let before = store.expectation(&hash(1));
        store.record_trade_failure(&hash(1));
        assert!(store.expectation(&hash(1)) < before);
        assert_eq!(store.get(&hash(1)).unwrap().failed_trades, 1);
    }

    #[test]
    fn violation_decreases_trust() {
        let mut store = TrustStore::new();
        for _ in 0..5 {
            store.record_trade_success(&hash(1));
        }
        let before = store.expectation(&hash(1));
        store.record_violation(&hash(1), 0.5);
        assert!(store.expectation(&hash(1)) < before);
        assert_eq!(store.get(&hash(1)).unwrap().violations, 1);
    }

    #[test]
    fn critical_violation_slashes_to_distrust() {
        let mut store = TrustStore::new();
        for _ in 0..20 {
            store.record_trade_success(&hash(1));
        }
        assert!(store.expectation(&hash(1)) > 0.7);

        store.record_violation(&hash(1), 1.0); // Critical
        assert!(store.expectation(&hash(1)) < 0.01);
        assert!(store.is_blackholed(&hash(1)));
    }

    #[test]
    fn decay_reduces_certainty_over_time() {
        let mut store = TrustStore::new();
        for _ in 0..10 {
            store.record_trade_success(&hash(1));
        }
        let before = store.expectation(&hash(1));

        // Simulate 1000 seconds of decay
        store.tick_decay(1000.0);
        let after = store.expectation(&hash(1));
        // Expectation should move toward 0.5 (vacuous)
        assert!((after - 0.5).abs() < (before - 0.5).abs());
    }

    #[test]
    fn copresence_accumulates_slowly() {
        let mut store = TrustStore::new();
        let before = store.expectation(&hash(1));
        // 60 seconds of co-presence
        for _ in 0..60 {
            store.record_copresence(&hash(1), 1.0);
        }
        let after = store.expectation(&hash(1));
        assert!(after > before);
        // But not by a huge amount — copresence is a weak signal
        assert!(after < 0.6);
    }

    #[test]
    fn copresence_has_diminishing_returns() {
        let mut store = TrustStore::new();
        // Accumulate to near the cap
        store.get_or_insert(&hash(1)).copresence_secs = 3500.0;
        let before = store.expectation(&hash(1));
        // 100 more seconds near the cap should barely move the needle
        for _ in 0..100 {
            store.record_copresence(&hash(1), 1.0);
        }
        let after = store.expectation(&hash(1));
        assert!((after - before).abs() < 0.01);
    }

    #[test]
    fn is_blackholed_false_for_unknown() {
        let store = TrustStore::new();
        assert!(!store.is_blackholed(&hash(99)));
    }

    #[test]
    fn is_blackholed_false_for_vacuous() {
        let mut store = TrustStore::new();
        store.get_or_insert(&hash(1));
        assert!(!store.is_blackholed(&hash(1)));
    }

    #[test]
    fn remove_clears_peer() {
        let mut store = TrustStore::new();
        store.record_trade_success(&hash(1));
        assert_eq!(store.count(), 1);
        store.remove(&hash(1));
        assert_eq!(store.count(), 0);
    }

    #[test]
    fn violation_count_returns_zero_for_unknown() {
        let store = TrustStore::new();
        assert_eq!(store.violation_count(&hash(99)), 0);
    }

    #[test]
    fn violation_count_increments() {
        let mut store = TrustStore::new();
        store.record_violation(&hash(1), 0.3);
        assert_eq!(store.violation_count(&hash(1)), 1);
        store.record_violation(&hash(1), 0.3);
        assert_eq!(store.violation_count(&hash(1)), 2);
    }

    #[test]
    fn is_suppressed_delegates_to_blackholed() {
        let mut store = TrustStore::new();
        assert!(!store.is_suppressed(&hash(1)));

        // Critical violation → blackholed → suppressed
        store.record_violation(&hash(1), 1.0);
        assert!(store.is_blackholed(&hash(1)));
        assert!(store.is_suppressed(&hash(1)));
    }

    #[test]
    fn direct_opinion_returns_none_for_unknown() {
        let store = TrustStore::new();
        assert!(store.direct_opinion(&hash(99)).is_none());
    }

    #[test]
    fn direct_opinion_returns_opinion_for_known() {
        let mut store = TrustStore::new();
        store.record_trade_success(&hash(1));
        let op = store.direct_opinion(&hash(1)).unwrap();
        assert!(op.belief > 0.0);
    }

    // ── Vouch tests ─────────────────────────────────────────────────

    #[test]
    fn new_peer_not_vouched() {
        let mut store = TrustStore::new();
        store.get_or_insert(&hash(1));
        assert!(store.vouched_by(&hash(1)).is_none());
    }

    #[test]
    fn record_vouch_sets_voucher() {
        let mut store = TrustStore::new();
        store.record_vouch(&hash(1), &hash(2));
        assert_eq!(store.vouched_by(&hash(1)), Some(hash(2)));
    }

    #[test]
    fn record_vouch_first_wins() {
        let mut store = TrustStore::new();
        store.record_vouch(&hash(1), &hash(2));
        store.record_vouch(&hash(1), &hash(3));
        // Second vouch ignored — first wins
        assert_eq!(store.vouched_by(&hash(1)), Some(hash(2)));
    }

    #[test]
    fn apply_vouch_liability_decreases_trust() {
        let mut store = TrustStore::new();
        // Build up some trust first
        for _ in 0..10 {
            store.record_trade_success(&hash(1));
        }
        let before = store.expectation(&hash(1));
        store.apply_vouch_liability(&hash(1), 0.2);
        assert!(store.expectation(&hash(1)) < before);
    }
}
