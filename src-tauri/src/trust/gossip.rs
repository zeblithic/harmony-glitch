// Reputation gossip: filtered trust dissemination via Zenoh.
//
// Peers share trust observations about each other. Incoming gossip is
// discounted by our trust in the reporter (SL discount operator), then
// fused with existing gossip from other sources (cumulative fusion).
// Gossip-derived opinions are stored separately from direct observations
// and decay faster, allowing peers to redeem themselves.
//
// Sybil resistance: untrusted reporters produce vacuous discounted
// opinions that contribute nothing to fusion.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::opinion::Opinion;

// ── Constants ────────────────────────────────────────────────────────

// Decay rate for gossip opinions (per second). 3x faster than direct
// observation (0.0001) so gossip fades sooner, allowing redemption.
const GOSSIP_DECAY_RATE: f64 = 0.0003;

// Fused gossip expectation must be below this for suppression.
const GOSSIP_SUPPRESSION_EXPECTATION: f64 = 0.20;

// Fused gossip uncertainty must be below this for suppression —
// prevents a single weakly-trusted report from triggering suppression.
const GOSSIP_SUPPRESSION_MAX_UNCERTAINTY: f64 = 0.50;

// Maximum hop count accepted. Reports beyond this are discarded.
const MAX_HOPS: u8 = 3;

// Memory cap: maximum gossip reports stored per subject.
const MAX_REPORTS_PER_SUBJECT: usize = 16;

// Minimum seconds between outbound gossip bursts.
const GOSSIP_COOLDOWN_SECS: f64 = 30.0;

// Maximum distinct subjects per cooldown window.
const MAX_GOSSIP_SUBJECTS_PER_WINDOW: usize = 3;

// Only relay gossip if our fused opinion about the subject has
// expectation below this threshold (we also distrust them).
const RELAY_EXPECTATION_THRESHOLD: f64 = 0.30;

// ── Wire format ──────────────────────────────────────────────────────

/// Gossip envelope: one peer's trust assessment of another,
/// serialized as `NetMessage::Gossip` over the event topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEnvelope {
    /// Subject peer's address hash (who this report is about).
    pub subject: [u8; 16],
    /// Reporter's belief that the subject is trustworthy.
    pub belief: f64,
    /// Reporter's disbelief (certainty that subject is untrustworthy).
    pub disbelief: f64,
    /// Reporter's uncertainty about the subject.
    pub uncertainty: f64,
    /// Number of violations the reporter observed.
    pub violations: u32,
    /// Hop count: 0 = originator, incremented on relay.
    pub hop: u8,
}

impl GossipEnvelope {
    /// Convert the envelope's opinion fields to an Opinion struct.
    fn to_opinion(&self) -> Opinion {
        Opinion {
            belief: self.belief,
            disbelief: self.disbelief,
            uncertainty: self.uncertainty,
        }
    }
}

// ── Internal storage ─────────────────────────────────────────────────

/// A single ingested gossip report (already discounted by trust in reporter).
#[derive(Debug, Clone)]
struct GossipReport {
    /// The discounted opinion (trust-weighted at ingest time).
    discounted_opinion: Opinion,
}

/// Per-subject gossip aggregation.
#[derive(Debug, Clone)]
struct SubjectGossip {
    /// Individual reports, keyed by reporter address hash.
    reports: HashMap<[u8; 16], GossipReport>,
}

impl SubjectGossip {
    fn new() -> Self {
        Self {
            reports: HashMap::new(),
        }
    }
}

// ── GossipStore ──────────────────────────────────────────────────────

/// Stores gossip-derived trust opinions, separate from direct observations.
pub struct GossipStore {
    /// Per-subject gossip data.
    subjects: HashMap<[u8; 16], SubjectGossip>,
    /// Timestamp of last outbound gossip emission.
    last_gossip_time: f64,
    /// Subjects gossiped about in the current cooldown window.
    gossip_window_count: usize,
    /// Pending outbound gossip envelopes.
    outbound_queue: Vec<GossipEnvelope>,
}

impl Default for GossipStore {
    fn default() -> Self {
        Self::new()
    }
}

impl GossipStore {
    pub fn new() -> Self {
        Self {
            subjects: HashMap::new(),
            last_gossip_time: 0.0,
            gossip_window_count: 0,
            outbound_queue: Vec::new(),
        }
    }

    /// Ingest an incoming gossip report. The raw opinion is discounted by
    /// `trust_in_reporter` (our direct trust in whoever sent this) and
    /// stored. Returns `true` if the report was accepted.
    ///
    /// If `hop < MAX_HOPS` and our fused opinion about the subject shows
    /// distrust, a relay is automatically queued.
    pub fn ingest(
        &mut self,
        envelope: &GossipEnvelope,
        reporter: &[u8; 16],
        trust_in_reporter: &Opinion,
        _now: f64,
    ) -> bool {
        // Reject over hop limit
        if envelope.hop >= MAX_HOPS {
            return false;
        }

        let raw_opinion = envelope.to_opinion();
        let discounted = trust_in_reporter.discount(&raw_opinion);

        let subject_gossip = self
            .subjects
            .entry(envelope.subject)
            .or_insert_with(SubjectGossip::new);

        // Enforce per-subject report cap (don't insert if at cap and new reporter)
        if subject_gossip.reports.len() >= MAX_REPORTS_PER_SUBJECT
            && !subject_gossip.reports.contains_key(reporter)
        {
            return false;
        }

        // Insert or update (deduplicates by reporter)
        subject_gossip.reports.insert(
            *reporter,
            GossipReport {
                discounted_opinion: discounted,
            },
        );

        // Consider relay: only if within hop limit and we also distrust
        if envelope.hop + 1 < MAX_HOPS {
            if let Some(fused) = self.fused_opinion(&envelope.subject) {
                if fused.expectation() < RELAY_EXPECTATION_THRESHOLD {
                    self.outbound_queue.push(GossipEnvelope {
                        subject: envelope.subject,
                        belief: fused.belief,
                        disbelief: fused.disbelief,
                        uncertainty: fused.uncertainty,
                        violations: envelope.violations,
                        hop: envelope.hop + 1,
                    });
                }
            }
        }

        true
    }

    /// Compute the fused gossip opinion about a subject by iteratively
    /// fusing all stored (already-discounted) reports.
    ///
    /// Returns `None` if no gossip exists for this subject.
    pub fn fused_opinion(&self, subject: &[u8; 16]) -> Option<Opinion> {
        let sg = self.subjects.get(subject)?;
        if sg.reports.is_empty() {
            return None;
        }

        let mut fused = Opinion::vacuous();
        for report in sg.reports.values() {
            fused = fused.fuse(&report.discounted_opinion);
        }
        Some(fused)
    }

    /// Whether gossip indicates this peer should be suppressed.
    ///
    /// Requires both low expectation (distrust signal) AND low
    /// uncertainty (confidence in that distrust). A single weakly-trusted
    /// report produces high uncertainty after discounting, so it alone
    /// cannot trigger suppression.
    pub fn is_gossip_suppressed(&self, subject: &[u8; 16]) -> bool {
        match self.fused_opinion(subject) {
            Some(fused) => {
                fused.expectation() < GOSSIP_SUPPRESSION_EXPECTATION
                    && fused.uncertainty < GOSSIP_SUPPRESSION_MAX_UNCERTAINTY
            }
            None => false,
        }
    }

    /// Queue an outbound gossip report. Called when we shadow-ban a peer
    /// or detect a critical violation.
    pub fn queue_outbound(
        &mut self,
        subject: &[u8; 16],
        opinion: &Opinion,
        violations: u32,
        _now: f64,
    ) {
        self.outbound_queue.push(GossipEnvelope {
            subject: *subject,
            belief: opinion.belief,
            disbelief: opinion.disbelief,
            uncertainty: opinion.uncertainty,
            violations,
            hop: 0,
        });
    }

    /// Drain outbound gossip envelopes that are ready to send, respecting
    /// rate limits. Returns envelopes to serialize and publish.
    pub fn drain_outbound(&mut self, now: f64) -> Vec<GossipEnvelope> {
        if self.outbound_queue.is_empty() {
            return Vec::new();
        }

        // Check cooldown
        if now - self.last_gossip_time < GOSSIP_COOLDOWN_SECS
            && self.gossip_window_count >= MAX_GOSSIP_SUBJECTS_PER_WINDOW
        {
            return Vec::new();
        }

        // Reset window if cooldown has elapsed
        if now - self.last_gossip_time >= GOSSIP_COOLDOWN_SECS {
            self.gossip_window_count = 0;
        }

        let remaining = MAX_GOSSIP_SUBJECTS_PER_WINDOW - self.gossip_window_count;
        let drain_count = self.outbound_queue.len().min(remaining);
        let drained: Vec<GossipEnvelope> = self.outbound_queue.drain(..drain_count).collect();

        if !drained.is_empty() {
            self.gossip_window_count += drained.len();
            self.last_gossip_time = now;
        }

        drained
    }

    /// Decay all gossip opinions toward vacuous. Called each tick.
    pub fn tick_decay(&mut self, dt: f64) {
        let factor = GOSSIP_DECAY_RATE * dt;
        if factor <= 0.0 {
            return;
        }
        for sg in self.subjects.values_mut() {
            for report in sg.reports.values_mut() {
                report.discounted_opinion.decay(factor);
            }
        }
    }

    /// Remove all gossip for a specific subject (e.g., on peer disconnect).
    pub fn clear_subject(&mut self, subject: &[u8; 16]) {
        self.subjects.remove(subject);
    }

    /// Clear all gossip data (e.g., on street change).
    pub fn clear(&mut self) {
        self.subjects.clear();
        self.outbound_queue.clear();
    }

    /// Number of tracked subjects (for tests).
    #[cfg(test)]
    pub fn subject_count(&self) -> usize {
        self.subjects.len()
    }

    /// Number of reports for a subject (for tests).
    #[cfg(test)]
    pub fn report_count(&self, subject: &[u8; 16]) -> usize {
        self.subjects
            .get(subject)
            .map(|sg| sg.reports.len())
            .unwrap_or(0)
    }

    /// Number of pending outbound envelopes (for tests).
    #[cfg(test)]
    pub fn outbound_count(&self) -> usize {
        self.outbound_queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-10;

    fn hash(id: u8) -> [u8; 16] {
        [id; 16]
    }

    fn make_envelope(subject: u8, b: f64, d: f64, u: f64, violations: u32, hop: u8) -> GossipEnvelope {
        GossipEnvelope {
            subject: hash(subject),
            belief: b,
            disbelief: d,
            uncertainty: u,
            violations,
            hop,
        }
    }

    #[test]
    fn new_store_is_empty() {
        let store = GossipStore::new();
        assert_eq!(store.subject_count(), 0);
        assert_eq!(store.outbound_count(), 0);
    }

    #[test]
    fn ingest_from_trusted_peer_stores_discounted() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();
        // Reporter fully trusts us, we fully trust them → passes through
        let envelope = make_envelope(1, 0.0, 0.8, 0.2, 3, 0);
        assert!(store.ingest(&envelope, &hash(10), &trust, 100.0));

        let fused = store.fused_opinion(&hash(1)).unwrap();
        // With full trust in reporter, discount passes through
        assert!((fused.disbelief - 0.8).abs() < EPSILON);
        assert!((fused.uncertainty - 0.2).abs() < EPSILON);
    }

    #[test]
    fn ingest_from_untrusted_peer_nearly_vacuous() {
        let mut store = GossipStore::new();
        let vacuous = Opinion::vacuous();
        let envelope = make_envelope(1, 0.0, 1.0, 0.0, 5, 0);
        assert!(store.ingest(&envelope, &hash(10), &vacuous, 100.0));

        let fused = store.fused_opinion(&hash(1)).unwrap();
        // Discounting by vacuous → result is vacuous (b_AR=0)
        assert!(fused.uncertainty > 1.0 - EPSILON);
    }

    #[test]
    fn ingest_respects_hop_limit() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();
        // hop=3 = MAX_HOPS → rejected
        let envelope = make_envelope(1, 0.0, 0.8, 0.2, 3, 3);
        assert!(!store.ingest(&envelope, &hash(10), &trust, 100.0));
        assert_eq!(store.subject_count(), 0);
    }

    #[test]
    fn ingest_deduplicates_by_reporter() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();

        let envelope1 = make_envelope(1, 0.0, 0.5, 0.5, 2, 0);
        assert!(store.ingest(&envelope1, &hash(10), &trust, 100.0));
        assert_eq!(store.report_count(&hash(1)), 1);

        // Same reporter, updated opinion → overwrites
        let envelope2 = make_envelope(1, 0.0, 0.9, 0.1, 4, 0);
        assert!(store.ingest(&envelope2, &hash(10), &trust, 110.0));
        assert_eq!(store.report_count(&hash(1)), 1);

        let fused = store.fused_opinion(&hash(1)).unwrap();
        assert!((fused.disbelief - 0.9).abs() < EPSILON);
    }

    #[test]
    fn ingest_caps_reports_per_subject() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();

        // Fill to capacity
        for i in 0..MAX_REPORTS_PER_SUBJECT {
            let reporter = hash(i as u8 + 100);
            let envelope = make_envelope(1, 0.0, 0.5, 0.5, 1, 0);
            assert!(store.ingest(&envelope, &reporter, &trust, 100.0));
        }
        assert_eq!(store.report_count(&hash(1)), MAX_REPORTS_PER_SUBJECT);

        // One more from a new reporter → rejected
        let envelope = make_envelope(1, 0.0, 0.5, 0.5, 1, 0);
        assert!(!store.ingest(&envelope, &hash(200), &trust, 100.0));
        assert_eq!(store.report_count(&hash(1)), MAX_REPORTS_PER_SUBJECT);
    }

    #[test]
    fn fused_opinion_combines_multiple() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();

        // Two trusted reporters both distrust subject
        let e1 = make_envelope(1, 0.05, 0.7, 0.25, 3, 0);
        store.ingest(&e1, &hash(10), &trust, 100.0);
        let e2 = make_envelope(1, 0.0, 0.8, 0.2, 5, 0);
        store.ingest(&e2, &hash(11), &trust, 100.0);

        let fused = store.fused_opinion(&hash(1)).unwrap();
        // Two agreeing distrust reports → high disbelief, low uncertainty
        assert!(fused.disbelief > 0.7);
        assert!(fused.uncertainty < 0.2);
    }

    #[test]
    fn is_gossip_suppressed_requires_both_conditions() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();

        // Single strong distrust report from a trusted peer
        let envelope = make_envelope(1, 0.0, 0.9, 0.1, 5, 0);
        store.ingest(&envelope, &hash(10), &trust, 100.0);

        let fused = store.fused_opinion(&hash(1)).unwrap();
        // Check: expectation should be low (b=0, u=0.1 → E=0.05)
        assert!(fused.expectation() < GOSSIP_SUPPRESSION_EXPECTATION);
        // Check: uncertainty should be low enough
        assert!(fused.uncertainty < GOSSIP_SUPPRESSION_MAX_UNCERTAINTY);
        // Therefore suppressed
        assert!(store.is_gossip_suppressed(&hash(1)));
    }

    #[test]
    fn is_gossip_suppressed_false_for_unknown() {
        let store = GossipStore::new();
        assert!(!store.is_gossip_suppressed(&hash(99)));
    }

    #[test]
    fn tick_decay_increases_uncertainty() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();
        let envelope = make_envelope(1, 0.0, 0.9, 0.1, 5, 0);
        store.ingest(&envelope, &hash(10), &trust, 100.0);

        let before = store.fused_opinion(&hash(1)).unwrap();
        // Simulate 1000 seconds of decay
        store.tick_decay(1000.0);
        let after = store.fused_opinion(&hash(1)).unwrap();

        assert!(after.uncertainty > before.uncertainty);
        assert!(after.disbelief < before.disbelief);
    }

    #[test]
    fn queue_and_drain_outbound() {
        let mut store = GossipStore::new();
        let opinion = Opinion::full_distrust();
        store.queue_outbound(&hash(1), &opinion, 5, 100.0);
        store.queue_outbound(&hash(2), &opinion, 3, 100.0);

        assert_eq!(store.outbound_count(), 2);
        let drained = store.drain_outbound(200.0);
        assert_eq!(drained.len(), 2);
        assert_eq!(store.outbound_count(), 0);
    }

    #[test]
    fn drain_respects_cooldown() {
        let mut store = GossipStore::new();
        let opinion = Opinion::full_distrust();

        // Fill window
        for i in 0..MAX_GOSSIP_SUBJECTS_PER_WINDOW {
            store.queue_outbound(&hash(i as u8), &opinion, 1, 100.0);
        }
        let drained = store.drain_outbound(100.0);
        assert_eq!(drained.len(), MAX_GOSSIP_SUBJECTS_PER_WINDOW);

        // Queue more — should be blocked until cooldown
        store.queue_outbound(&hash(50), &opinion, 1, 100.0);
        let drained = store.drain_outbound(100.0 + GOSSIP_COOLDOWN_SECS - 1.0);
        assert!(drained.is_empty());

        // After cooldown → allowed
        let drained = store.drain_outbound(100.0 + GOSSIP_COOLDOWN_SECS);
        assert_eq!(drained.len(), 1);
    }

    #[test]
    fn clear_subject_removes_gossip() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();
        let envelope = make_envelope(1, 0.0, 0.9, 0.1, 5, 0);
        store.ingest(&envelope, &hash(10), &trust, 100.0);
        assert_eq!(store.subject_count(), 1);

        store.clear_subject(&hash(1));
        assert_eq!(store.subject_count(), 0);
        assert!(store.fused_opinion(&hash(1)).is_none());
    }

    #[test]
    fn relay_blocked_when_we_trust_subject() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();

        // First, add a positive gossip report about subject (from another peer)
        let positive = make_envelope(1, 0.8, 0.0, 0.2, 0, 0);
        store.ingest(&positive, &hash(20), &trust, 100.0);
        let outbound_before = store.outbound_count();

        // Now receive a distrust report from a different reporter
        let distrust = make_envelope(1, 0.0, 0.9, 0.1, 5, 0);
        store.ingest(&distrust, &hash(10), &trust, 101.0);

        // Fused opinion should be mixed, expectation above relay threshold
        let fused = store.fused_opinion(&hash(1)).unwrap();
        // With one strong trust and one strong distrust, expectation is moderate
        // The relay should NOT have been queued because fused expectation > 0.30
        // Note: the exact expectation depends on fusion, but with strong
        // opposing opinions the result should be around 0.5
        assert!(fused.expectation() > RELAY_EXPECTATION_THRESHOLD);
        // No relay should have been added (only the positive report's relay check)
        // The positive report's expectation would be high, so no relay either
        assert_eq!(store.outbound_count(), outbound_before);
    }

    #[test]
    fn relay_triggered_when_we_distrust() {
        let mut store = GossipStore::new();
        let trust = Opinion::full_trust();

        // Receive a strong distrust report at hop=0 → relay should be queued
        // since fused expectation will be low and hop+1 < MAX_HOPS
        let envelope = make_envelope(1, 0.0, 0.9, 0.1, 5, 0);
        store.ingest(&envelope, &hash(10), &trust, 100.0);

        // Should have queued a relay (hop=1)
        assert!(store.outbound_count() > 0);
        let queued = store.drain_outbound(200.0);
        assert_eq!(queued[0].hop, 1);
        assert_eq!(queued[0].subject, hash(1));
    }

    #[test]
    fn sybil_resistance_vacuous_reporters() {
        let mut store = GossipStore::new();
        let vacuous = Opinion::vacuous();

        // 10 "Sybil" reporters we don't know (vacuous trust)
        for i in 0..10 {
            let envelope = make_envelope(1, 0.0, 1.0, 0.0, 10, 0);
            store.ingest(&envelope, &hash(100 + i), &vacuous, 100.0);
        }

        // Despite 10 reports of full distrust, fused should be vacuous
        // because all are discounted to vacuous
        assert!(!store.is_gossip_suppressed(&hash(1)));
        let fused = store.fused_opinion(&hash(1)).unwrap();
        assert!(fused.uncertainty > 0.99);
    }
}
