use crate::engine::state::RemotePlayerFrame;
use crate::network::types::{PlayerNetState, PresenceEvent};
use std::collections::HashMap;

/// Players with no state update for this many seconds are removed.
const STALE_TIMEOUT: f64 = 10.0;

#[derive(Debug, Clone)]
struct RemotePlayer {
    address_hash: [u8; 16],
    display_name: String,
    state: PlayerNetState,
    last_update: f64,
}

/// Tracks remote players by address hash.
///
/// Receives presence events (join/leave) and position updates, and
/// produces `Vec<RemotePlayerFrame>` for rendering. Automatically
/// purges players that have gone stale (no updates for `STALE_TIMEOUT`
/// seconds).
#[derive(Debug)]
pub struct RemotePlayerRegistry {
    players: HashMap<[u8; 16], RemotePlayer>,
}

impl Default for RemotePlayerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RemotePlayerRegistry {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
        }
    }

    /// Handle a presence event: insert on join, remove on leave.
    /// `now` is the current monotonic time in seconds — used to initialize
    /// `last_update` so that newly joined peers that never send state
    /// updates are still subject to stale purging.
    pub fn handle_presence(&mut self, event: &PresenceEvent, now: f64) {
        match event {
            PresenceEvent::Joined {
                address_hash,
                display_name,
            } => {
                self.players.insert(
                    *address_hash,
                    RemotePlayer {
                        address_hash: *address_hash,
                        display_name: display_name.clone(),
                        state: PlayerNetState {
                            x: 0.0,
                            y: 0.0,
                            vx: 0.0,
                            vy: 0.0,
                            facing: 1,
                            on_ground: true,
                        },
                        last_update: now,
                    },
                );
            }
            PresenceEvent::Left { address_hash } => {
                self.players.remove(address_hash);
            }
        }
    }

    /// Update the display name for a known player (e.g. after a re-announce).
    pub fn update_display_name(&mut self, address_hash: &[u8; 16], name: String) {
        if let Some(player) = self.players.get_mut(address_hash) {
            player.display_name = name;
        }
    }

    /// Refresh a player's liveness timestamp (e.g. on re-announce).
    /// Prevents active-but-silent peers from being evicted by `purge_stale`.
    pub fn refresh_liveness(&mut self, address_hash: &[u8; 16], now: f64) {
        if let Some(player) = self.players.get_mut(address_hash) {
            player.last_update = now;
        }
    }

    /// Update position/velocity for a known player. Silently ignores
    /// updates for players not in the registry.
    pub fn update_state(&mut self, address_hash: &[u8; 16], state: PlayerNetState, now: f64) {
        if let Some(player) = self.players.get_mut(address_hash) {
            player.state = state;
            player.last_update = now;
        }
    }

    /// Remove players whose `last_update` is more than `STALE_TIMEOUT`
    /// seconds behind `now`.
    pub fn purge_stale(&mut self, now: f64) {
        self.players
            .retain(|_, player| (now - player.last_update) < STALE_TIMEOUT);
    }

    /// Produce render frames for all tracked players, sorted by
    /// hex-encoded address_hash for deterministic ordering.
    pub fn frames(&self) -> Vec<RemotePlayerFrame> {
        let mut frames: Vec<RemotePlayerFrame> = self
            .players
            .values()
            .map(|p| {
                let facing = if p.state.facing == 0 { "left" } else { "right" };
                RemotePlayerFrame {
                    address_hash: hex::encode(p.address_hash),
                    display_name: p.display_name.clone(),
                    x: p.state.x as f64,
                    y: p.state.y as f64,
                    facing: facing.to_string(),
                    on_ground: p.state.on_ground,
                }
            })
            .collect();
        frames.sort_by(|a, b| a.address_hash.cmp(&b.address_hash));
        frames
    }

    /// Remove all players (e.g. on street change).
    pub fn clear(&mut self) {
        self.players.clear();
    }

    /// Number of tracked players.
    pub fn count(&self) -> usize {
        self.players.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hash(id: u8) -> [u8; 16] {
        [id; 16]
    }

    fn make_state(x: f32, y: f32) -> PlayerNetState {
        PlayerNetState {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            facing: 1,
            on_ground: true,
        }
    }

    #[test]
    fn join_and_leave() {
        let mut reg = RemotePlayerRegistry::new();
        assert_eq!(reg.count(), 0);

        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(1),
                display_name: "Alice".into(),
            },
            1.0,
        );
        assert_eq!(reg.count(), 1);

        reg.handle_presence(
            &PresenceEvent::Left {
                address_hash: make_hash(1),
            },
            2.0,
        );
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn update_position() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(1),
                display_name: "Alice".into(),
            },
            1.0,
        );

        reg.update_state(&make_hash(1), make_state(100.0, -50.0), 2.0);

        let frames = reg.frames();
        assert_eq!(frames.len(), 1);
        assert!((frames[0].x - 100.0).abs() < f64::EPSILON);
        assert!((frames[0].y - -50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ignores_update_for_unknown_player() {
        let mut reg = RemotePlayerRegistry::new();
        // Should not panic or insert a new player
        reg.update_state(&make_hash(99), make_state(1.0, 2.0), 1.0);
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn purges_stale_players() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(1),
                display_name: "Alice".into(),
            },
            1.0,
        );
        reg.update_state(&make_hash(1), make_state(0.0, 0.0), 1.0);

        // At t=12.0, player's last_update was 1.0 — 11 seconds ago, exceeds STALE_TIMEOUT
        reg.purge_stale(12.0);
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn purges_newly_joined_without_updates() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(1),
                display_name: "Alice".into(),
            },
            1.0,
        );
        // Within timeout — should NOT be purged
        reg.purge_stale(5.0);
        assert_eq!(reg.count(), 1);

        // Past timeout with no state updates — should be purged (no ghost players)
        reg.purge_stale(12.0);
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn clear_removes_all() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(1),
                display_name: "Alice".into(),
            },
            1.0,
        );
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(2),
                display_name: "Bob".into(),
            },
            1.0,
        );
        assert_eq!(reg.count(), 2);

        reg.clear();
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn rejoin_resets_player_state() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(1),
                display_name: "Alice".into(),
            },
            1.0,
        );
        reg.update_state(&make_hash(1), make_state(500.0, -200.0), 2.0);

        // Leave and rejoin
        reg.handle_presence(
            &PresenceEvent::Left {
                address_hash: make_hash(1),
            },
            3.0,
        );
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(1),
                display_name: "Alice v2".into(),
            },
            4.0,
        );

        assert_eq!(reg.count(), 1);
        let frames = reg.frames();
        assert_eq!(frames[0].display_name, "Alice v2");
        // Position should reset to defaults, not retain old state
        assert!((frames[0].x - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn refresh_liveness_prevents_stale_purge() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(1),
                display_name: "Alice".into(),
            },
            1.0,
        );

        // At t=8, refresh liveness (simulates re-announce).
        reg.refresh_liveness(&make_hash(1), 8.0);

        // At t=12, player would be stale relative to join time (1.0)
        // but liveness was refreshed at 8.0 — only 4s ago, within timeout.
        reg.purge_stale(12.0);
        assert_eq!(
            reg.count(),
            1,
            "Should survive purge after liveness refresh"
        );

        // At t=19, 11s since last refresh — should be purged.
        reg.purge_stale(19.0);
        assert_eq!(reg.count(), 0);
    }

    #[test]
    fn update_display_name_propagates_to_frames() {
        let mut reg = RemotePlayerRegistry::new();
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(1),
                display_name: "OldName".into(),
            },
            1.0,
        );
        assert_eq!(reg.frames()[0].display_name, "OldName");

        reg.update_display_name(&make_hash(1), "NewName".into());
        assert_eq!(reg.frames()[0].display_name, "NewName");
    }

    #[test]
    fn frames_sorted_deterministically() {
        let mut reg = RemotePlayerRegistry::new();
        // Insert in reverse order: hash 0xFF before 0x01
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(0xFF),
                display_name: "Zara".into(),
            },
            1.0,
        );
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(0x01),
                display_name: "Alice".into(),
            },
            1.0,
        );
        reg.handle_presence(
            &PresenceEvent::Joined {
                address_hash: make_hash(0x80),
                display_name: "Mid".into(),
            },
            1.0,
        );

        let frames = reg.frames();
        assert_eq!(frames.len(), 3);
        // 0x01 < 0x80 < 0xFF in hex
        assert_eq!(frames[0].address_hash, hex::encode([0x01u8; 16]));
        assert_eq!(frames[1].address_hash, hex::encode([0x80u8; 16]));
        assert_eq!(frames[2].address_hash, hex::encode([0xFFu8; 16]));
    }
}
