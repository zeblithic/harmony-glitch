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
        return SpawnPoint { x: sp.x, y: street.ground_y, facing: None };
    }
    resolve_default_spawn(street)
}

/// Resolve the player's spawn location on `street` when there is no
/// origin signpost context — first entry, initial load, or final fallback
/// for resolve_arrival.
pub fn resolve_default_spawn(street: &StreetData) -> SpawnPoint {
    if let Some(s) = street.default_spawn {
        return s;
    }
    if let Some(sp) = street.signposts.first() {
        return SpawnPoint { x: sp.x, y: street.ground_y, facing: None };
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
        assert_eq!(sp.x, 800.0);
        assert_eq!(sp.y, 0.0); // ground_y
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
        assert_eq!(sp.x, 300.0); // reciprocal wins, NOT the partial arrival_x
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
    fn resolve_default_spawn_falls_back_to_first_signpost() {
        let first = Signpost {
            id: "first".to_string(),
            x: -600.0,
            y: 0.0,
            connects: vec![],
        };
        let second = Signpost {
            id: "second".to_string(),
            x: 600.0,
            y: 0.0,
            connects: vec![],
        };
        let street = test_street(vec![first, second], None);
        let sp = resolve_default_spawn(&street);
        assert_eq!(sp.x, -600.0);
        assert_eq!(sp.y, 0.0); // ground_y
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
