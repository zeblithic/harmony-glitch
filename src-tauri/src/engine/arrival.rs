use crate::engine::transition::PRE_SUBSCRIBE_DISTANCE;
use crate::street::types::{Facing, SignpostConnection, SpawnPoint, StreetData};

/// Resolve the point at which a player should land on `street` when arriving
/// via `connection`. Prefers the connection's explicit arrival_x/arrival_y;
/// falls back to the reciprocal signpost on the target street (legacy
/// Glitch-XML path); last-resorts to the street's default_spawn.
pub fn resolve_arrival(
    street: &StreetData,
    origin_tsid: &str,
    connection: Option<&SignpostConnection>,
) -> SpawnPoint {
    if let Some(c) = connection {
        if let (Some(x), Some(y)) = (c.arrival_x, c.arrival_y) {
            return SpawnPoint { x, y, facing: c.arrival_facing };
        }
    }
    if let Some(sp) = street.signposts.iter().find(|s|
        s.connects.iter().any(|c| c.target_tsid == origin_tsid)
    ) {
        // Nudge the player inward from the reciprocal signpost so they don't
        // sit inside the pre-subscribe distance and immediately re-trigger
        // another swoop back to the origin. Matches the pre-ZEB-132 behavior
        // that lived at state.rs:646. Clamp to street bounds so a narrow
        // street or a signpost near the midpoint can't push the spawn off-map.
        let street_mid = (street.left + street.right) / 2.0;
        let inward = if sp.x < street_mid { 1.0 } else { -1.0 };
        let nudge = PRE_SUBSCRIBE_DISTANCE + 50.0;
        let arrival_x = (sp.x + inward * nudge).clamp(street.left, street.right);
        return SpawnPoint {
            x: arrival_x,
            y: street.ground_y,
            facing: None,
        };
    }
    resolve_default_spawn(street)
}

/// Resolve the player's spawn location on `street` when there is no
/// origin signpost context — first entry, initial load, or final fallback
/// for resolve_arrival.
///
/// Never falls back to a signpost position: the transition system treats
/// `player.x == signpost.x` as already-crossed (inclusive `>=`/`<=`), so
/// spawning on a signpost would trigger an instant swoop on frame 1.
pub fn resolve_default_spawn(street: &StreetData) -> SpawnPoint {
    if let Some(s) = street.default_spawn {
        return s;
    }
    SpawnPoint {
        x: (street.left + street.right) / 2.0,
        y: street.ground_y,
        facing: None,
    }
}

/// Infer player facing from an arrival x-coordinate when the SpawnPoint
/// doesn't specify one. Faces inward from the nearer street edge.
pub fn infer_facing(arrival_x: f64, street: &StreetData) -> Facing {
    let street_mid = (street.left + street.right) / 2.0;
    if arrival_x < street_mid { Facing::Right } else { Facing::Left }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::types::{Signpost, SignpostConnection};

    fn test_street(signposts: Vec<Signpost>, default_spawn: Option<SpawnPoint>) -> StreetData {
        StreetData {
            tsid: "LATEST001".to_string(),
            name: "Test Street".to_string(),
            left: -1000.0,
            right: 1000.0,
            top: -500.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![],
            signposts,
            default_spawn,
        }
    }

    fn connection(
        target_tsid: &str,
        arrival_x: Option<f64>,
        arrival_y: Option<f64>,
        arrival_facing: Option<Facing>,
    ) -> SignpostConnection {
        SignpostConnection {
            target_tsid: target_tsid.to_string(),
            target_label: "Label".to_string(),
            arrival_x,
            arrival_y,
            arrival_facing,
        }
    }

    #[test]
    fn resolve_arrival_uses_explicit_connection_fields() {
        let street = test_street(vec![], None);
        let conn = connection("ORIGIN", Some(-750.0), Some(-25.0), Some(Facing::Right));
        let sp = resolve_arrival(&street, "ORIGIN", Some(&conn));
        assert_eq!(sp.x, -750.0);
        assert_eq!(sp.y, -25.0);
        assert_eq!(sp.facing, Some(Facing::Right));
    }

    #[test]
    fn resolve_arrival_falls_back_to_reciprocal_signpost() {
        // Connection has no arrival fields; street has a signpost connecting back to ORIGIN.
        let reciprocal = Signpost {
            id: "back_gate".to_string(),
            x: 800.0,
            y: 0.0,
            connects: vec![connection("ORIGIN", None, None, None)],
        };
        let street = test_street(vec![reciprocal], None);
        let conn = connection("TARGETTSID", None, None, None);
        let sp = resolve_arrival(&street, "ORIGIN", Some(&conn));
        // Reciprocal signpost is at x=800; street mid = 0; sp.x > mid so
        // inward = -1.0; nudge = PRE_SUBSCRIBE_DISTANCE (500) + 50 = 550.
        // Nudged x = 800 - 550 = 250. The nudge prevents the arrival landing
        // within pre-subscribe range of the return signpost.
        assert_eq!(sp.x, 250.0); // 800 - (PRE_SUBSCRIBE_DISTANCE + 50) = 800 - 550
        assert_eq!(sp.y, 0.0); // ground_y
    }

    #[test]
    fn resolve_arrival_reciprocal_signpost_nudges_inward() {
        // Signpost on right half of street: inward = -1.0.
        let sp_right = Signpost {
            id: "east".to_string(),
            x: 900.0,
            y: 0.0,
            connects: vec![connection("ORIGIN", None, None, None)],
        };
        let street = test_street(vec![sp_right], None);
        let conn = connection("TARGET", None, None, None);
        let result = resolve_arrival(&street, "ORIGIN", Some(&conn));
        // street mid = 0, sp.x=900 > mid, inward=-1, nudge=550 => 900-550=350
        assert_eq!(result.x, 350.0);

        // Signpost on left half: inward = +1.0.
        let sp_left = Signpost {
            id: "west".to_string(),
            x: -900.0,
            y: 0.0,
            connects: vec![connection("ORIGIN", None, None, None)],
        };
        let street2 = test_street(vec![sp_left], None);
        let result2 = resolve_arrival(&street2, "ORIGIN", Some(&conn));
        // sp.x=-900 < mid=0, inward=+1, nudge=550 => -900+550=-350
        assert_eq!(result2.x, -350.0);
    }

    #[test]
    fn resolve_arrival_reciprocal_clamps_to_street_bounds() {
        // Narrow street: reciprocal nudge of 550px would push spawn out.
        let mut street = test_street(
            vec![Signpost {
                id: "sp".to_string(),
                x: 100.0, // just right of center
                y: 0.0,
                connects: vec![connection("ORIGIN", None, None, None)],
            }],
            None,
        );
        street.left = -200.0;
        street.right = 200.0;
        let conn = connection("TARGET", None, None, None);
        let sp = resolve_arrival(&street, "ORIGIN", Some(&conn));
        // sp.x=100, mid=0, inward=-1, unclamped = 100-550 = -450.
        // Clamped to street.left = -200.
        assert_eq!(sp.x, -200.0);
        assert!(
            sp.x >= street.left && sp.x <= street.right,
            "arrival must stay inside bounds"
        );
    }

    #[test]
    fn resolve_arrival_last_resorts_to_default_spawn() {
        // No connection, no reciprocal signpost — falls to default_spawn.
        let street = test_street(
            vec![],
            Some(SpawnPoint { x: 42.0, y: -10.0, facing: Some(Facing::Left) }),
        );
        let sp = resolve_arrival(&street, "ORIGIN", None);
        assert_eq!(sp.x, 42.0);
        assert_eq!(sp.y, -10.0);
        assert_eq!(sp.facing, Some(Facing::Left));
    }

    #[test]
    fn resolve_default_spawn_prefers_explicit() {
        let street = test_street(
            vec![],
            Some(SpawnPoint { x: 7.0, y: 0.0, facing: None }),
        );
        let sp = resolve_default_spawn(&street);
        assert_eq!(sp.x, 7.0);
        assert_eq!(sp.y, 0.0);
        assert_eq!(sp.facing, None);
    }

    #[test]
    fn resolve_arrival_partial_fields_fall_through_to_reciprocal() {
        // arrival_x present but arrival_y absent — not enough to form a SpawnPoint,
        // should fall through to the reciprocal-signpost path per the both-or-neither
        // contract documented on SignpostConnection.
        let reciprocal = Signpost {
            id: "back".to_string(),
            x: 300.0,
            y: 0.0,
            connects: vec![connection("ORIGIN", None, None, None)],
        };
        let street = test_street(vec![reciprocal], None);
        let conn = connection("TARGET", Some(-500.0), None, None); // partial
        let sp = resolve_arrival(&street, "ORIGIN", Some(&conn));
        // Reciprocal wins (NOT the partial arrival_x). Signpost x=300, street
        // mid=0, sp>mid, inward=-1, nudge=550 => nudged x = 300 - 550 = -250.
        assert_eq!(sp.x, -250.0);
        assert_eq!(sp.y, 0.0);
    }

    #[test]
    fn resolve_arrival_with_barren_connection_falls_through_to_default_spawn() {
        // Connection is provided but has no arrival fields; no reciprocal signpost
        // on the street; default_spawn is present — should return default_spawn.
        let street = test_street(
            vec![],
            Some(SpawnPoint { x: 42.0, y: -10.0, facing: Some(Facing::Left) }),
        );
        let conn = connection("TARGET", None, None, None);
        let sp = resolve_arrival(&street, "ORIGIN", Some(&conn));
        assert_eq!(sp.x, 42.0);
        assert_eq!(sp.y, -10.0);
        assert_eq!(sp.facing, Some(Facing::Left));
    }

    #[test]
    fn resolve_default_spawn_with_signposts_but_no_explicit_uses_center() {
        // Critical: never fall back to a signpost position (would trigger
        // instant swoop at load because transition uses inclusive bounds).
        let signpost = Signpost {
            id: "sp".to_string(),
            x: -600.0,
            y: 0.0,
            connects: vec![],
        };
        let street = test_street(vec![signpost], None);
        let sp = resolve_default_spawn(&street);
        assert_eq!(sp.x, 0.0); // street center, NOT -600.0
        assert_eq!(sp.y, 0.0);
    }

    #[test]
    fn resolve_default_spawn_last_resorts_to_center() {
        let street = test_street(vec![], None);
        let sp = resolve_default_spawn(&street);
        assert_eq!(sp.x, 0.0); // (left=-1000 + right=1000) / 2
        assert_eq!(sp.y, 0.0); // ground_y
    }

    #[test]
    fn infer_facing_left_half_faces_right() {
        let street = test_street(vec![], None);
        assert_eq!(infer_facing(-500.0, &street), Facing::Right);
    }

    #[test]
    fn infer_facing_right_half_faces_left() {
        let street = test_street(vec![], None);
        assert_eq!(infer_facing(500.0, &street), Facing::Left);
    }
}
