use serde::{Deserialize, Serialize};

/// A complete street definition parsed from Glitch location XML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreetData {
    pub tsid: String,
    pub name: String,
    /// Left bound (negative)
    pub left: f64,
    /// Right bound (positive)
    pub right: f64,
    /// Top bound (negative — Y goes up)
    pub top: f64,
    /// Bottom bound (typically 0)
    pub bottom: f64,
    /// Ground Y level (typically 0)
    pub ground_y: f64,
    /// Background gradient colors (hex, no # prefix)
    pub gradient: Option<Gradient>,
    /// Parallax layers sorted by z (back to front)
    pub layers: Vec<Layer>,
    /// Signpost connections to other streets
    pub signposts: Vec<Signpost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gradient {
    pub top: String,
    pub bottom: String,
}

/// A parallax background layer containing decorative objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub name: String,
    pub z: i32,
    pub w: f64,
    pub h: f64,
    pub is_middleground: bool,
    pub decos: Vec<Deco>,
    /// Platform lines (only present on the middleground layer).
    pub platform_lines: Vec<PlatformLine>,
    /// Walls (only present on the middleground layer).
    pub walls: Vec<Wall>,
    /// Ladders (only present on the middleground layer).
    pub ladders: Vec<Ladder>,
    pub filters: Option<LayerFilters>,
}

/// A platform line segment. Players walk along these.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformLine {
    pub id: String,
    pub start: Point,
    pub end: Point,
    /// -1 = one-way from top, 1 = one-way from bottom, 0 = pass-through, None = solid
    pub pc_perm: Option<i32>,
    pub item_perm: Option<i32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A vertical collision barrier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wall {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub h: f64,
    /// Wall permeability for players: None = solid (blocks both directions),
    /// Some(-1) = one-way from left, Some(1) = one-way from right, Some(0) = pass-through.
    pub pc_perm: Option<i32>,
    pub item_perm: Option<i32>,
}

impl Wall {
    /// Whether this wall blocks movement from the left.
    /// Same bit-pattern as PlatformLine::solid_from_top but for horizontal direction.
    pub fn blocks_from_left(&self) -> bool {
        !matches!(self.pc_perm, Some(1) | Some(0))
    }

    /// Whether this wall blocks movement from the right.
    pub fn blocks_from_right(&self) -> bool {
        !matches!(self.pc_perm, Some(-1) | Some(0))
    }

    /// Bottom Y extent of the wall (y + h).
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
}

/// A climbable ladder zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ladder {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A decorative sprite placed in a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deco {
    pub id: String,
    pub name: String,
    pub sprite_class: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub z: i32,
    pub r: f64,
    pub h_flip: bool,
}

/// Visual filters applied to a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerFilters {
    pub brightness: Option<i32>,
    pub contrast: Option<i32>,
    pub saturation: Option<i32>,
    pub blur: Option<i32>,
    pub tint_color: Option<i32>,
    pub tint_amount: Option<i32>,
}

/// A navigation signpost connecting to other streets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signpost {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub connects: Vec<SignpostConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignpostConnection {
    pub target_tsid: String,
    pub target_label: String,
}

impl StreetData {
    /// Width of the street in pixels.
    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    /// Height of the street in pixels.
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }

    /// Get the middleground layer (z=0), where platforms live.
    pub fn middleground(&self) -> Option<&Layer> {
        self.layers.iter().find(|l| l.is_middleground)
    }

    /// All platform lines from the middleground layer.
    pub fn platforms(&self) -> &[PlatformLine] {
        self.middleground()
            .map(|l| l.platform_lines.as_slice())
            .unwrap_or(&[])
    }

    /// All walls from the middleground layer.
    pub fn walls(&self) -> &[Wall] {
        self.middleground()
            .map(|l| l.walls.as_slice())
            .unwrap_or(&[])
    }

    /// Compute parallax scroll factor for a layer relative to middleground.
    /// Returns 1.0 for middleground, <1.0 for backgrounds, >1.0 for foregrounds.
    pub fn parallax_factor(&self, layer: &Layer) -> f64 {
        let mg_w = self.middleground().map(|l| l.w).unwrap_or(self.width());
        if mg_w == 0.0 {
            return 1.0;
        }
        layer.w / mg_w
    }
}

impl PlatformLine {
    /// Whether the player can stand on this platform from above.
    pub fn solid_from_top(&self) -> bool {
        // Solid from top unless perm is 1 (bottom-only) or 0 (pass-through)
        !matches!(self.pc_perm, Some(1) | Some(0))
    }

    /// Whether this platform blocks from below.
    pub fn solid_from_bottom(&self) -> bool {
        !matches!(self.pc_perm, Some(-1) | Some(0))
    }

    /// Get the Y position on this platform at a given X coordinate.
    /// Linearly interpolates between start and end for sloped platforms.
    pub fn y_at(&self, x: f64) -> f64 {
        let dx = self.end.x - self.start.x;
        if dx.abs() < 0.001 {
            return self.start.y;
        }
        let t = ((x - self.start.x) / dx).clamp(0.0, 1.0);
        self.start.y + t * (self.end.y - self.start.y)
    }

    /// X extent of this platform.
    pub fn min_x(&self) -> f64 {
        self.start.x.min(self.end.x)
    }

    pub fn max_x(&self) -> f64 {
        self.start.x.max(self.end.x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_y_at_flat() {
        let p = PlatformLine {
            id: "test".into(),
            start: Point { x: 0.0, y: -100.0 },
            end: Point { x: 200.0, y: -100.0 },
            pc_perm: None,
            item_perm: None,
        };
        assert!((p.y_at(100.0) - (-100.0)).abs() < 0.001);
    }

    #[test]
    fn platform_y_at_sloped() {
        let p = PlatformLine {
            id: "test".into(),
            start: Point { x: 0.0, y: -100.0 },
            end: Point { x: 200.0, y: -200.0 },
            pc_perm: None,
            item_perm: None,
        };
        // Midpoint should be -150
        assert!((p.y_at(100.0) - (-150.0)).abs() < 0.001);
    }

    #[test]
    fn platform_one_way_from_top() {
        let p = PlatformLine {
            id: "test".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 100.0, y: 0.0 },
            pc_perm: Some(-1),
            item_perm: None,
        };
        assert!(p.solid_from_top());
        assert!(!p.solid_from_bottom());
    }

    #[test]
    fn platform_fully_solid() {
        let p = PlatformLine {
            id: "test".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 100.0, y: 0.0 },
            pc_perm: None,
            item_perm: None,
        };
        assert!(p.solid_from_top());
        assert!(p.solid_from_bottom());
    }

    #[test]
    fn street_width_height() {
        let s = StreetData {
            tsid: "test".into(),
            name: "Test Street".into(),
            left: -3000.0,
            right: 3000.0,
            top: -1000.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![],
            signposts: vec![],
        };
        assert!((s.width() - 6000.0).abs() < 0.001);
        assert!((s.height() - 1000.0).abs() < 0.001);
    }

    #[test]
    fn parallax_factor_middleground() {
        let mg = Layer {
            name: "middleground".into(),
            z: 0,
            w: 6000.0,
            h: 1000.0,
            is_middleground: true,
            decos: vec![],
            platform_lines: vec![],
            walls: vec![],
            ladders: vec![],
            filters: None,
        };
        let bg = Layer {
            name: "bg_2".into(),
            z: -2,
            w: 5460.0,
            h: 1000.0,
            is_middleground: false,
            decos: vec![],
            platform_lines: vec![],
            walls: vec![],
            ladders: vec![],
            filters: None,
        };
        let s = StreetData {
            tsid: "test".into(),
            name: "Test".into(),
            left: -3000.0,
            right: 3000.0,
            top: -1000.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![mg.clone(), bg.clone()],
            signposts: vec![],
        };
        assert!((s.parallax_factor(&mg) - 1.0).abs() < 0.001);
        assert!((s.parallax_factor(&bg) - 0.91).abs() < 0.01);
    }

    #[test]
    fn serializes_to_camel_case() {
        let p = PlatformLine {
            id: "p1".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 100.0, y: 0.0 },
            pc_perm: Some(-1),
            item_perm: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("pcPerm"));
        assert!(json.contains("itemPerm"));
    }

    #[test]
    fn wall_blocks_from_left() {
        let solid = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: None, item_perm: None };
        assert!(solid.blocks_from_left());
        let left_only = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(-1), item_perm: None };
        assert!(left_only.blocks_from_left());
        let right_only = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(1), item_perm: None };
        assert!(!right_only.blocks_from_left());
        let passthrough = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(0), item_perm: None };
        assert!(!passthrough.blocks_from_left());
    }

    #[test]
    fn wall_blocks_from_right() {
        let solid = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: None, item_perm: None };
        assert!(solid.blocks_from_right());
        let right_only = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(1), item_perm: None };
        assert!(right_only.blocks_from_right());
        let left_only = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(-1), item_perm: None };
        assert!(!left_only.blocks_from_right());
        let passthrough = Wall { id: "w".into(), x: 0.0, y: -100.0, h: 100.0, pc_perm: Some(0), item_perm: None };
        assert!(!passthrough.blocks_from_right());
    }

    #[test]
    fn wall_bottom_extent() {
        let wall = Wall { id: "w".into(), x: 0.0, y: -400.0, h: 400.0, pc_perm: None, item_perm: None };
        assert!((wall.bottom() - 0.0).abs() < 0.001);
        let wall2 = Wall { id: "w".into(), x: 0.0, y: -200.0, h: 100.0, pc_perm: None, item_perm: None };
        assert!((wall2.bottom() - (-100.0)).abs() < 0.001);
    }

    #[test]
    fn walls_accessor_returns_middleground_walls() {
        let wall = Wall { id: "w1".into(), x: -100.0, y: -50.0, h: 50.0, pc_perm: None, item_perm: None };
        let mg = Layer {
            name: "middleground".into(), z: 0, w: 200.0, h: 50.0,
            is_middleground: true, decos: vec![], platform_lines: vec![],
            walls: vec![wall.clone()], ladders: vec![], filters: None,
        };
        let s = StreetData {
            tsid: "test".into(), name: "Test".into(),
            left: -100.0, right: 100.0, top: -50.0, bottom: 0.0, ground_y: 0.0,
            gradient: None, layers: vec![mg], signposts: vec![],
        };
        assert_eq!(s.walls().len(), 1);
        assert_eq!(s.walls()[0].id, "w1");
    }
}
