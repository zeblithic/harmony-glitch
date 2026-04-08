// Capability epoch policy: maps peer maturity signals to progressive
// capability tiers, preventing fresh Sybil identities from immediately
// participating in chat, trade, or gossip.
//
// Pure functions + constants — no I/O, no state.

use serde::{Deserialize, Serialize};

// ── Epoch thresholds ─────────────────────────────────────────────────

/// Copresence seconds required to advance from Sandbox to Initiate.
const SANDBOX_TO_INITIATE_COPRESENCE: f64 = 300.0; // 5 minutes

/// Copresence seconds required to advance from Initiate to Citizen (without vouch).
const INITIATE_TO_CITIZEN_COPRESENCE: f64 = 1800.0; // 30 minutes

/// Trust expectation required (alongside copresence) for Citizen epoch.
const INITIATE_TO_CITIZEN_EXPECTATION: f64 = 0.6;

/// Trust penalty applied to a voucher when their vouchee commits a critical violation.
pub const VOUCH_LIABILITY_WEIGHT: f64 = 0.2;

// ── Types ────────────────────────────────────────────────────────────

/// Progressive capability epoch for a peer.
///
/// - **Sandbox**: new/unknown — can move and update avatar only
/// - **Initiate**: established copresence — can chat, trade, send gossip
/// - **Citizen**: trusted veteran or vouched — full capabilities, can vouch
///
/// Ordering matches progression: `Sandbox < Initiate < Citizen`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PeerEpoch {
    Sandbox,
    Initiate,
    Citizen,
}

/// Wire-format vouch message (serialized as NetMessage::Vouch).
/// Only the subject is needed — the voucher's identity is established
/// by the authenticated link context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VouchMessage {
    pub subject: [u8; 16],
}

// ── Public functions ─────────────────────────────────────────────────

/// Determine a peer's epoch from observable state.
///
/// - A vouched peer is always Citizen (vouch overrides time).
/// - Without a vouch: Citizen requires both copresence and trust.
/// - Initiate requires only copresence.
/// - Otherwise: Sandbox.
pub fn determine_epoch(copresence_secs: f64, expectation: f64, is_vouched: bool) -> PeerEpoch {
    if is_vouched {
        return PeerEpoch::Citizen;
    }
    if copresence_secs >= INITIATE_TO_CITIZEN_COPRESENCE
        && expectation >= INITIATE_TO_CITIZEN_EXPECTATION
    {
        return PeerEpoch::Citizen;
    }
    if copresence_secs >= SANDBOX_TO_INITIATE_COPRESENCE {
        return PeerEpoch::Initiate;
    }
    PeerEpoch::Sandbox
}

/// Whether the epoch permits sending chat messages.
pub fn can_chat(epoch: PeerEpoch) -> bool {
    epoch >= PeerEpoch::Initiate
}

/// Whether the epoch permits initiating/participating in trades.
pub fn can_trade(epoch: PeerEpoch) -> bool {
    epoch >= PeerEpoch::Initiate
}

/// Whether the epoch permits sending gossip (trust opinions).
pub fn can_gossip(epoch: PeerEpoch) -> bool {
    epoch >= PeerEpoch::Initiate
}

/// Whether the epoch permits vouching for other peers.
pub fn can_vouch(epoch: PeerEpoch) -> bool {
    epoch >= PeerEpoch::Citizen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_for_new_peer() {
        assert_eq!(determine_epoch(0.0, 0.5, false), PeerEpoch::Sandbox);
    }

    #[test]
    fn sandbox_at_boundary() {
        assert_eq!(determine_epoch(299.9, 0.5, false), PeerEpoch::Sandbox);
    }

    #[test]
    fn initiate_at_threshold() {
        assert_eq!(determine_epoch(300.0, 0.5, false), PeerEpoch::Initiate);
    }

    #[test]
    fn initiate_with_low_trust() {
        // Enough copresence for Citizen but trust too low → Initiate
        assert_eq!(determine_epoch(1800.0, 0.59, false), PeerEpoch::Initiate);
    }

    #[test]
    fn citizen_by_time_and_trust() {
        assert_eq!(determine_epoch(1800.0, 0.6, false), PeerEpoch::Citizen);
    }

    #[test]
    fn citizen_by_vouch() {
        // Vouch overrides copresence requirements
        assert_eq!(determine_epoch(100.0, 0.5, true), PeerEpoch::Citizen);
    }

    #[test]
    fn citizen_by_vouch_even_in_sandbox() {
        // Vouch works even with zero copresence
        assert_eq!(determine_epoch(0.0, 0.5, true), PeerEpoch::Citizen);
    }

    #[test]
    fn can_chat_sandbox_false() {
        assert!(!can_chat(PeerEpoch::Sandbox));
    }

    #[test]
    fn can_chat_initiate_true() {
        assert!(can_chat(PeerEpoch::Initiate));
        assert!(can_chat(PeerEpoch::Citizen));
    }

    #[test]
    fn can_trade_sandbox_false() {
        assert!(!can_trade(PeerEpoch::Sandbox));
        assert!(can_trade(PeerEpoch::Initiate));
        assert!(can_trade(PeerEpoch::Citizen));
    }

    #[test]
    fn can_vouch_initiate_false() {
        assert!(!can_vouch(PeerEpoch::Sandbox));
        assert!(!can_vouch(PeerEpoch::Initiate));
    }

    #[test]
    fn can_vouch_citizen_true() {
        assert!(can_vouch(PeerEpoch::Citizen));
    }
}
