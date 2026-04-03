use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata for a single audio track.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackDef {
    pub title: String,
    pub artist: String,
    pub duration_secs: f64,
    pub file: String,
}

/// Full track catalog, keyed by track ID.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TrackCatalog {
    pub tracks: HashMap<String, TrackDef>,
}

/// Per-jukebox runtime playback state.
#[derive(Debug, Clone)]
pub struct JukeboxState {
    pub playlist: Vec<String>,
    pub current_track_index: usize,
    pub playing: bool,
    pub elapsed_secs: f64,
}

impl JukeboxState {
    /// Create a new state from a playlist. Auto-starts playing if non-empty.
    pub fn new(playlist: Vec<String>) -> Self {
        let playing = !playlist.is_empty();
        JukeboxState {
            playlist,
            current_track_index: 0,
            playing,
            elapsed_secs: 0.0,
        }
    }

    /// Advance playback by `dt` seconds. Auto-advances track on finish, wrapping.
    /// No-op when paused or playlist is empty.
    pub fn tick(&mut self, dt: f64, catalog: &TrackCatalog) {
        if !self.playing || self.playlist.is_empty() {
            return;
        }

        self.elapsed_secs += dt;

        // Check if the current track has finished.
        let current_id = &self.playlist[self.current_track_index];
        if let Some(track) = catalog.tracks.get(current_id) {
            if self.elapsed_secs >= track.duration_secs {
                // Advance to next track, wrapping around.
                self.current_track_index =
                    (self.current_track_index + 1) % self.playlist.len();
                self.elapsed_secs = 0.0;
            }
        } else {
            // Track not in catalog — skip to next.
            self.current_track_index =
                (self.current_track_index + 1) % self.playlist.len();
            self.elapsed_secs = 0.0;
        }
    }

    /// Returns the track ID at the current index, if the playlist is non-empty.
    pub fn current_track_id(&self) -> Option<&str> {
        self.playlist.get(self.current_track_index).map(|s| s.as_str())
    }

    /// Seek to a specific track index. Resets elapsed. Out-of-bounds indices are ignored.
    pub fn select_track(&mut self, index: usize) {
        if index < self.playlist.len() {
            self.current_track_index = index;
            self.elapsed_secs = 0.0;
            self.playing = true;
        }
    }

    /// Resume playback. No-op on empty playlist.
    pub fn play(&mut self) {
        if !self.playlist.is_empty() {
            self.playing = true;
        }
    }

    /// Pause playback.
    pub fn pause(&mut self) {
        self.playing = false;
    }
}

/// Returns `None` if the player is outside the audio radius, or
/// `Some(volume)` where volume is in `[0.0, 1.0]` — 1.0 at the entity, 0.0 at the edge.
pub fn distance_factor(player_x: f64, entity_x: f64, audio_radius: f64) -> Option<f64> {
    if audio_radius <= 0.0 {
        return None;
    }
    let distance = (player_x - entity_x).abs();
    if distance >= audio_radius {
        None
    } else {
        Some(1.0 - distance / audio_radius)
    }
}

/// Filters a playlist to only tracks that exist in the catalog.
/// Logs a warning for each missing track ID.
pub fn validate_playlist(
    playlist: Vec<String>,
    catalog: &TrackCatalog,
    entity_name: &str,
) -> Vec<String> {
    playlist
        .into_iter()
        .filter(|id| {
            if catalog.tracks.contains_key(id) {
                true
            } else {
                eprintln!(
                    "warning: jukebox '{}' references unknown track '{}' — skipping",
                    entity_name, id
                );
                false
            }
        })
        .collect()
}

/// Deserialize a `TrackCatalog` from JSON.
pub fn parse_catalog(json: &str) -> Result<TrackCatalog, String> {
    serde_json::from_str(json).map_err(|e| format!("Failed to parse music catalog: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_catalog() -> TrackCatalog {
        let mut tracks = HashMap::new();
        tracks.insert(
            "track_a".to_string(),
            TrackDef {
                title: "Song A".to_string(),
                artist: "Artist A".to_string(),
                duration_secs: 10.0,
                file: "a.ogg".to_string(),
            },
        );
        tracks.insert(
            "track_b".to_string(),
            TrackDef {
                title: "Song B".to_string(),
                artist: "Artist B".to_string(),
                duration_secs: 20.0,
                file: "b.ogg".to_string(),
            },
        );
        TrackCatalog { tracks }
    }

    #[test]
    fn jukebox_state_new_starts_playing_first_track() {
        let state = JukeboxState::new(vec!["track_a".to_string(), "track_b".to_string()]);
        assert!(state.playing);
        assert_eq!(state.current_track_index, 0);
        assert_eq!(state.current_track_id(), Some("track_a"));
    }

    #[test]
    fn jukebox_state_empty_playlist_not_playing() {
        let state = JukeboxState::new(vec![]);
        assert!(!state.playing);
        assert_eq!(state.current_track_id(), None);
    }

    #[test]
    fn tick_advances_elapsed() {
        let catalog = make_catalog();
        let mut state = JukeboxState::new(vec!["track_a".to_string()]);
        state.tick(3.0, &catalog);
        assert!((state.elapsed_secs - 3.0).abs() < f64::EPSILON);
        assert_eq!(state.current_track_index, 0);
    }

    #[test]
    fn tick_auto_advances_to_next_track() {
        let catalog = make_catalog();
        let mut state =
            JukeboxState::new(vec!["track_a".to_string(), "track_b".to_string()]);
        // track_a is 10s; tick past it
        state.tick(10.0, &catalog);
        assert_eq!(state.current_track_index, 1);
        assert!(state.elapsed_secs < f64::EPSILON);
    }

    #[test]
    fn tick_wraps_playlist() {
        let catalog = make_catalog();
        let mut state =
            JukeboxState::new(vec!["track_a".to_string(), "track_b".to_string()]);
        // Advance past track_a to track_b
        state.tick(10.0, &catalog);
        assert_eq!(state.current_track_index, 1);
        // Advance past track_b back to track_a
        state.tick(20.0, &catalog);
        assert_eq!(state.current_track_index, 0);
        assert!(state.elapsed_secs < f64::EPSILON);
    }

    #[test]
    fn tick_paused_does_not_advance() {
        let catalog = make_catalog();
        let mut state = JukeboxState::new(vec!["track_a".to_string()]);
        state.pause();
        state.tick(5.0, &catalog);
        assert!(state.elapsed_secs < f64::EPSILON);
        assert_eq!(state.current_track_index, 0);
    }

    #[test]
    fn select_track_resets_elapsed() {
        let catalog = make_catalog();
        let mut state =
            JukeboxState::new(vec!["track_a".to_string(), "track_b".to_string()]);
        state.tick(5.0, &catalog);
        state.select_track(1);
        assert_eq!(state.current_track_index, 1);
        assert!(state.elapsed_secs < f64::EPSILON);
        assert!(state.playing);
    }

    #[test]
    fn select_track_out_of_bounds_ignored() {
        let mut state =
            JukeboxState::new(vec!["track_a".to_string(), "track_b".to_string()]);
        state.select_track(99);
        // State unchanged
        assert_eq!(state.current_track_index, 0);
    }

    #[test]
    fn play_resumes_after_pause() {
        let mut state = JukeboxState::new(vec!["track_a".to_string()]);
        state.pause();
        assert!(!state.playing);
        state.play();
        assert!(state.playing);
    }

    #[test]
    fn play_on_empty_playlist_stays_not_playing() {
        let mut state = JukeboxState::new(vec![]);
        state.play();
        assert!(!state.playing);
    }

    #[test]
    fn distance_factor_at_entity_is_one() {
        let result = distance_factor(100.0, 100.0, 500.0);
        assert_eq!(result, Some(1.0));
    }

    #[test]
    fn distance_factor_at_radius_is_zero() {
        // Exactly at the radius edge => distance == radius => None
        let result = distance_factor(600.0, 100.0, 500.0);
        assert_eq!(result, None);
    }

    #[test]
    fn distance_factor_halfway() {
        // distance = 250, radius = 500 => 1 - 250/500 = 0.5
        let result = distance_factor(350.0, 100.0, 500.0);
        assert!(result.is_some());
        let v = result.unwrap();
        assert!((v - 0.5).abs() < 1e-10);
    }

    #[test]
    fn distance_factor_beyond_radius_is_none() {
        let result = distance_factor(700.0, 100.0, 500.0);
        assert_eq!(result, None);
    }

    #[test]
    fn distance_factor_negative_direction() {
        // player to the left of entity — distance is still absolute
        let result = distance_factor(-150.0, 100.0, 500.0);
        // distance = 250, same as halfway test
        assert!(result.is_some());
        let v = result.unwrap();
        assert!((v - 0.5).abs() < 1e-10);
    }

    #[test]
    fn validate_playlist_filters_unknown_tracks() {
        let catalog = make_catalog();
        let playlist = vec![
            "track_a".to_string(),
            "ghost_track".to_string(),
            "track_b".to_string(),
        ];
        let result = validate_playlist(playlist, &catalog, "test_jukebox");
        assert_eq!(result, vec!["track_a".to_string(), "track_b".to_string()]);
    }

    #[test]
    fn validate_playlist_empty_on_all_unknown() {
        let catalog = make_catalog();
        let playlist = vec!["nonexistent_1".to_string(), "nonexistent_2".to_string()];
        let result = validate_playlist(playlist, &catalog, "test_jukebox");
        assert!(result.is_empty());
    }

    #[test]
    fn tick_skips_track_missing_from_catalog() {
        let catalog = make_catalog(); // has track_a and track_b
        let mut state = JukeboxState::new(vec!["nonexistent".into(), "track_a".into()]);
        state.tick(0.1, &catalog);
        // Should skip nonexistent and land on track_a
        assert_eq!(state.current_track_index, 1);
        assert_eq!(state.current_track_id(), Some("track_a"));
    }

    #[test]
    fn distance_factor_zero_radius_returns_none() {
        assert_eq!(distance_factor(100.0, 100.0, 0.0), None);
    }

    #[test]
    fn parse_catalog_from_json() {
        let json = r#"{
            "tracks": {
                "track_1": {
                    "title": "Hello World",
                    "artist": "Test Band",
                    "durationSecs": 180.0,
                    "file": "hello.ogg"
                }
            }
        }"#;
        let catalog = parse_catalog(json).expect("should parse");
        assert_eq!(catalog.tracks.len(), 1);
        let t = &catalog.tracks["track_1"];
        assert_eq!(t.title, "Hello World");
        assert_eq!(t.artist, "Test Band");
        assert!((t.duration_secs - 180.0).abs() < f64::EPSILON);
        assert_eq!(t.file, "hello.ogg");
    }
}
