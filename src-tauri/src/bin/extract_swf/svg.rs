use std::collections::HashMap;
use std::fmt::Write as FmtWrite;

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

/// A point in twip-space (i32 coordinates).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TwipsPoint {
    x: i32,
    y: i32,
}

/// A single edge segment produced by the edge walker.
#[derive(Clone, Debug)]
enum Edge {
    Straight {
        start: TwipsPoint,
        end: TwipsPoint,
    },
    Curved {
        start: TwipsPoint,
        control: TwipsPoint,
        end: TwipsPoint,
    },
}

impl Edge {
    fn start(&self) -> TwipsPoint {
        match self {
            Edge::Straight { start, .. } => *start,
            Edge::Curved { start, .. } => *start,
        }
    }

    fn end(&self) -> TwipsPoint {
        match self {
            Edge::Straight { end, .. } => *end,
            Edge::Curved { end, .. } => *end,
        }
    }

    fn reversed(&self) -> Edge {
        match self {
            Edge::Straight { start, end } => Edge::Straight {
                start: *end,
                end: *start,
            },
            Edge::Curved {
                start,
                control,
                end,
            } => Edge::Curved {
                start: *end,
                control: *control,
                end: *start,
            },
        }
    }
}

/// Collected edges grouped by style index.
struct EdgeGroups {
    /// fill_id -> edges (1-based style index with table_offset applied)
    fills: HashMap<u32, Vec<Edge>>,
    /// line_id -> edges
    lines: HashMap<u32, Vec<Edge>>,
}

// ---------------------------------------------------------------------------
// Stage 1: Edge Walker
// ---------------------------------------------------------------------------

type StyleCollection = (EdgeGroups, Vec<(u32, swf::FillStyle)>, Vec<(u32, swf::LineStyle)>);

/// Walk the shape records and collect edges grouped by fill and line style.
/// Returns (edge_groups, all_fill_styles, all_line_styles) where the style
/// vecs are indexed by the 1-based IDs used in edge_groups (offset by table).
fn walk_shape_edges(shape: &swf::Shape) -> StyleCollection {
    let mut fills: HashMap<u32, Vec<Edge>> = HashMap::new();
    let mut lines: HashMap<u32, Vec<Edge>> = HashMap::new();

    // Current pen position in twips
    let mut pen = TwipsPoint { x: 0, y: 0 };

    // Active style IDs (0 = none). Stored with offset applied for global uniqueness.
    let mut fill0: u32 = 0;
    let mut fill1: u32 = 0;
    let mut line: u32 = 0;

    // Separate offsets for fill and line namespaces
    let mut current_fill_styles = shape.styles.fill_styles.clone();
    let mut current_line_styles = shape.styles.line_styles.clone();
    let mut fill_offset: u32 = 0;
    let mut line_offset: u32 = 0;

    // Collect all styles globally indexed
    let mut all_fills: Vec<(u32, swf::FillStyle)> = Vec::new();
    let mut all_lines: Vec<(u32, swf::LineStyle)> = Vec::new();

    // Add initial styles
    for (i, fs) in current_fill_styles.iter().enumerate() {
        all_fills.push((fill_offset + i as u32 + 1, fs.clone()));
    }
    for (i, ls) in current_line_styles.iter().enumerate() {
        all_lines.push((line_offset + i as u32 + 1, ls.clone()));
    }

    for record in &shape.shape {
        match record {
            swf::ShapeRecord::StyleChange(sc) => {
                if let Some(ref new_styles) = sc.new_styles {
                    // Advance each offset past its own current table
                    fill_offset += current_fill_styles.len() as u32;
                    line_offset += current_line_styles.len() as u32;
                    current_fill_styles = new_styles.fill_styles.clone();
                    current_line_styles = new_styles.line_styles.clone();

                    // Reset active styles
                    fill0 = 0;
                    fill1 = 0;
                    line = 0;

                    // Add new styles to global collection
                    for (i, fs) in current_fill_styles.iter().enumerate() {
                        all_fills.push((fill_offset + i as u32 + 1, fs.clone()));
                    }
                    for (i, ls) in current_line_styles.iter().enumerate() {
                        all_lines.push((line_offset + i as u32 + 1, ls.clone()));
                    }
                }

                if let Some(move_to) = &sc.move_to {
                    pen = TwipsPoint {
                        x: move_to.x.get(),
                        y: move_to.y.get(),
                    };
                }
                if let Some(fs0) = sc.fill_style_0 {
                    fill0 = if fs0 == 0 {
                        0
                    } else {
                        fill_offset + fs0
                    };
                }
                if let Some(fs1) = sc.fill_style_1 {
                    fill1 = if fs1 == 0 {
                        0
                    } else {
                        fill_offset + fs1
                    };
                }
                if let Some(ls) = sc.line_style {
                    line = if ls == 0 { 0 } else { line_offset + ls };
                }
            }
            swf::ShapeRecord::StraightEdge { delta } => {
                let end = TwipsPoint {
                    x: pen.x + delta.dx.get(),
                    y: pen.y + delta.dy.get(),
                };
                let edge = Edge::Straight { start: pen, end };

                if fill0 != 0 {
                    fills.entry(fill0).or_default().push(edge.clone());
                }
                if fill1 != 0 {
                    fills.entry(fill1).or_default().push(edge.reversed());
                }
                if line != 0 {
                    lines.entry(line).or_default().push(edge);
                }

                pen = end;
            }
            swf::ShapeRecord::CurvedEdge {
                control_delta,
                anchor_delta,
            } => {
                let control = TwipsPoint {
                    x: pen.x + control_delta.dx.get(),
                    y: pen.y + control_delta.dy.get(),
                };
                let end = TwipsPoint {
                    x: control.x + anchor_delta.dx.get(),
                    y: control.y + anchor_delta.dy.get(),
                };
                let edge = Edge::Curved {
                    start: pen,
                    control,
                    end,
                };

                if fill0 != 0 {
                    fills.entry(fill0).or_default().push(edge.clone());
                }
                if fill1 != 0 {
                    fills.entry(fill1).or_default().push(edge.reversed());
                }
                if line != 0 {
                    lines.entry(line).or_default().push(edge);
                }

                pen = end;
            }
        }
    }

    (EdgeGroups { fills, lines }, all_fills, all_lines)
}

// ---------------------------------------------------------------------------
// Stage 2: Path Connector
// ---------------------------------------------------------------------------

/// A connected sub-path (sequence of edges forming a chain).
struct SubPath {
    edges: Vec<Edge>,
}

/// Connect a bag of edges into closed sub-paths by matching end→start.
fn connect_edges(edges: &[Edge]) -> Vec<SubPath> {
    if edges.is_empty() {
        return Vec::new();
    }

    let mut used = vec![false; edges.len()];
    let mut paths = Vec::new();

    while let Some(start_idx) = used.iter().position(|&u| !u) {
        let mut path_edges = Vec::new();
        used[start_idx] = true;
        path_edges.push(edges[start_idx].clone());

        let chain_start = edges[start_idx].start();

        loop {
            let current_end = path_edges.last().unwrap().end();

            // If we've returned to the start, close the path
            if current_end == chain_start && path_edges.len() > 1 {
                break;
            }

            // Find an unused edge whose start matches current_end
            let next = used
                .iter()
                .enumerate()
                .position(|(i, &u)| !u && edges[i].start() == current_end);

            match next {
                Some(i) => {
                    used[i] = true;
                    path_edges.push(edges[i].clone());
                }
                None => break, // No connecting edge found
            }
        }

        paths.push(SubPath { edges: path_edges });
    }

    paths
}

// ---------------------------------------------------------------------------
// Float formatting helper
// ---------------------------------------------------------------------------

/// Format a float cleanly: no trailing zeros, no unnecessary decimal point.
fn fmt_float(v: f64) -> String {
    if v == v.floor() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        // Use enough precision, then strip trailing zeros
        let s = format!("{:.6}", v);
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }
}

/// Convert twips (i32) to pixel string.
fn twips_to_px(t: i32) -> String {
    fmt_float(t as f64 / 20.0)
}

// ---------------------------------------------------------------------------
// Stage 3: SVG Emitter
// ---------------------------------------------------------------------------

/// Build the `d` attribute string for a set of sub-paths.
fn paths_to_d(subpaths: &[SubPath]) -> String {
    let mut d = String::new();
    for sp in subpaths {
        if sp.edges.is_empty() {
            continue;
        }
        let first = sp.edges[0].start();
        write!(d, "M{} {}", twips_to_px(first.x), twips_to_px(first.y)).unwrap();

        for edge in &sp.edges {
            match edge {
                Edge::Straight { end, .. } => {
                    write!(d, " L{} {}", twips_to_px(end.x), twips_to_px(end.y)).unwrap();
                }
                Edge::Curved { control, end, .. } => {
                    write!(
                        d,
                        " Q{} {} {} {}",
                        twips_to_px(control.x),
                        twips_to_px(control.y),
                        twips_to_px(end.x),
                        twips_to_px(end.y)
                    )
                    .unwrap();
                }
            }
        }

        // Close the sub-path if it forms a loop
        let last_end = sp.edges.last().unwrap().end();
        if last_end == first {
            d.push('Z');
        }
    }
    d
}

/// Emit SVG color attribute value from a Color.
fn svg_color(c: &swf::Color) -> String {
    format!("rgb({},{},{})", c.r, c.g, c.b)
}

/// Build SVG for a single shape, returning (defs_xml, paths_xml).
/// `gradient_id` is shared across all shapes in the SVG to avoid ID collisions.
fn shape_to_svg(shape: &swf::Shape, gradient_id: &mut u32) -> (String, String) {
    let (groups, all_fills, all_lines) = walk_shape_edges(shape);

    let mut defs = String::new();
    let mut body = String::new();

    // Build a lookup: style_id -> FillStyle
    let fill_map: HashMap<u32, &swf::FillStyle> = all_fills.iter().map(|(id, fs)| (*id, fs)).collect();
    let line_map: HashMap<u32, &swf::LineStyle> = all_lines.iter().map(|(id, ls)| (*id, ls)).collect();

    // Render fills first (painter's order: fills under lines)
    let mut fill_ids: Vec<u32> = groups.fills.keys().copied().collect();
    fill_ids.sort();
    for fid in fill_ids {
        let edges = &groups.fills[&fid];
        let subpaths = connect_edges(edges);
        let d = paths_to_d(&subpaths);
        if d.is_empty() {
            continue;
        }

        let fill_style = fill_map.get(&fid);
        match fill_style {
            Some(swf::FillStyle::Color(c)) => {
                let mut attrs = format!(r#"<path d="{}" fill="{}""#, d, svg_color(c));
                if c.a < 255 {
                    write!(attrs, r#" fill-opacity="{}""#, fmt_float(c.a as f64 / 255.0)).unwrap();
                }
                attrs.push_str("/>");
                writeln!(body, "  {}", attrs).unwrap();
            }
            Some(swf::FillStyle::LinearGradient(g)) => {
                let gid = format!("g{}", *gradient_id);
                *gradient_id += 1;
                write_linear_gradient(&mut defs, &gid, g);
                writeln!(body, r#"  <path d="{}" fill="url(#{})"/>"#, d, gid).unwrap();
            }
            Some(swf::FillStyle::RadialGradient(g)) => {
                let gid = format!("g{}", *gradient_id);
                *gradient_id += 1;
                write_radial_gradient(&mut defs, &gid, g, None);
                writeln!(body, r#"  <path d="{}" fill="url(#{})"/>"#, d, gid).unwrap();
            }
            Some(swf::FillStyle::FocalGradient {
                gradient,
                focal_point,
            }) => {
                let gid = format!("g{}", *gradient_id);
                *gradient_id += 1;
                write_radial_gradient(&mut defs, &gid, gradient, Some(focal_point.to_f64()));
                writeln!(body, r#"  <path d="{}" fill="url(#{})"/>"#, d, gid).unwrap();
            }
            Some(swf::FillStyle::Bitmap { .. }) | None => {
                writeln!(body, r#"  <path d="{}" fill="none"/>"#, d).unwrap();
            }
        }
    }

    // Render lines
    let mut line_ids: Vec<u32> = groups.lines.keys().copied().collect();
    line_ids.sort();
    for lid in line_ids {
        let edges = &groups.lines[&lid];
        let subpaths = connect_edges(edges);
        let d = paths_to_d(&subpaths);
        if d.is_empty() {
            continue;
        }

        let line_style = line_map.get(&lid);
        match line_style {
            Some(ls) => {
                let width_px = ls.width().to_pixels();
                let stroke = match ls.fill_style() {
                    swf::FillStyle::Color(c) => svg_color(c),
                    _ => "none".to_string(),
                };
                let stroke_opacity = match ls.fill_style() {
                    swf::FillStyle::Color(c) if c.a < 255 => {
                        Some(fmt_float(c.a as f64 / 255.0))
                    }
                    _ => None,
                };

                let mut attrs = format!(
                    r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}""#,
                    d,
                    stroke,
                    fmt_float(width_px)
                );
                if let Some(opacity) = stroke_opacity {
                    write!(attrs, r#" stroke-opacity="{}""#, opacity).unwrap();
                }

                // Line cap
                match ls.start_cap() {
                    swf::LineCapStyle::None => attrs.push_str(r#" stroke-linecap="butt""#),
                    swf::LineCapStyle::Square => attrs.push_str(r#" stroke-linecap="square""#),
                    swf::LineCapStyle::Round => {} // default
                }

                // Line join
                match ls.join_style() {
                    swf::LineJoinStyle::Bevel => attrs.push_str(r#" stroke-linejoin="bevel""#),
                    swf::LineJoinStyle::Miter(limit) => {
                        write!(
                            attrs,
                            r#" stroke-linejoin="miter" stroke-miterlimit="{}""#,
                            fmt_float(limit.to_f64())
                        )
                        .unwrap();
                    }
                    swf::LineJoinStyle::Round => {} // default
                }

                attrs.push_str("/>");
                writeln!(body, "  {}", attrs).unwrap();
            }
            None => {
                writeln!(body, r#"  <path d="{}" fill="none" stroke="none"/>"#, d).unwrap();
            }
        }
    }

    (defs, body)
}

/// Write a `<linearGradient>` element into the defs string.
fn write_linear_gradient(defs: &mut String, id: &str, g: &swf::Gradient) {
    let transform = matrix_to_svg(&g.matrix);
    let spread = spread_method(g.spread);

    write!(
        defs,
        r#"  <linearGradient id="{}" gradientUnits="userSpaceOnUse" x1="-819.2" y1="0" x2="819.2" y2="0""#,
        id
    )
    .unwrap();
    if !transform.is_empty() {
        write!(defs, r#" gradientTransform="{}""#, transform).unwrap();
    }
    if !spread.is_empty() {
        write!(defs, r#" spreadMethod="{}""#, spread).unwrap();
    }
    defs.push_str(">\n");
    write_gradient_stops(defs, &g.records);
    defs.push_str("  </linearGradient>\n");
}

/// Write a `<radialGradient>` element into the defs string.
fn write_radial_gradient(
    defs: &mut String,
    id: &str,
    g: &swf::Gradient,
    focal_point: Option<f64>,
) {
    let transform = matrix_to_svg(&g.matrix);
    let spread = spread_method(g.spread);

    write!(
        defs,
        r#"  <radialGradient id="{}" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="819.2""#,
        id
    )
    .unwrap();
    if let Some(fp) = focal_point {
        write!(defs, r#" fx="{}""#, fmt_float(fp * 819.2)).unwrap();
    }
    if !transform.is_empty() {
        write!(defs, r#" gradientTransform="{}""#, transform).unwrap();
    }
    if !spread.is_empty() {
        write!(defs, r#" spreadMethod="{}""#, spread).unwrap();
    }
    defs.push_str(">\n");
    write_gradient_stops(defs, &g.records);
    defs.push_str("  </radialGradient>\n");
}

/// Write gradient stop elements.
fn write_gradient_stops(defs: &mut String, records: &[swf::GradientRecord]) {
    for rec in records {
        let offset = fmt_float(rec.ratio as f64 / 255.0);
        let color = svg_color(&rec.color);
        if rec.color.a < 255 {
            writeln!(
                defs,
                r#"    <stop offset="{}" stop-color="{}" stop-opacity="{}"/>"#,
                offset,
                color,
                fmt_float(rec.color.a as f64 / 255.0)
            )
            .unwrap();
        } else {
            writeln!(
                defs,
                r#"    <stop offset="{}" stop-color="{}"/>"#,
                offset, color
            )
            .unwrap();
        }
    }
}

/// Convert GradientSpread to SVG spreadMethod value.
fn spread_method(spread: swf::GradientSpread) -> &'static str {
    match spread {
        swf::GradientSpread::Pad => "",        // SVG default
        swf::GradientSpread::Reflect => "reflect",
        swf::GradientSpread::Repeat => "repeat",
    }
}

/// Format a Matrix as an SVG `matrix(a,b,c,d,tx,ty)` string.
/// tx/ty are converted from twips to pixels. Returns empty string for identity.
fn matrix_to_svg(m: &swf::Matrix) -> String {
    if *m == swf::Matrix::IDENTITY {
        return String::new();
    }
    format!(
        "matrix({},{},{},{},{},{})",
        fmt_float(m.a.to_f64()),
        fmt_float(m.b.to_f64()),
        fmt_float(m.c.to_f64()),
        fmt_float(m.d.to_f64()),
        fmt_float(m.tx.to_pixels()),
        fmt_float(m.ty.to_pixels()),
    )
}

// ---------------------------------------------------------------------------
// Stage 4: Main entry point
// ---------------------------------------------------------------------------

/// Convert a parsed SWF to an SVG string.
pub fn convert_swf_to_svg(swf: &swf::Swf) -> String {
    let stage = swf.header.stage_size();
    let x = stage.x_min.to_pixels();
    let y = stage.y_min.to_pixels();
    let width = (stage.x_max - stage.x_min).to_pixels();
    let height = (stage.y_max - stage.y_min).to_pixels();

    // Collect DefineShape tags and PlaceObject tags up to first ShowFrame
    let mut shapes: HashMap<swf::CharacterId, &swf::Shape> = HashMap::new();
    let mut placements: Vec<&swf::PlaceObject> = Vec::new();

    for tag in &swf.tags {
        match tag {
            swf::Tag::DefineShape(shape) => {
                shapes.insert(shape.id, shape);
            }
            swf::Tag::PlaceObject(place) => {
                placements.push(place);
            }
            swf::Tag::ShowFrame => break,
            _ => {}
        }
    }

    let mut all_defs = String::new();
    let mut all_body = String::new();
    let mut gradient_id = 0u32;

    for place in &placements {
        let char_id = match place.action {
            swf::PlaceObjectAction::Place(id) => id,
            swf::PlaceObjectAction::Replace(id) => id,
            swf::PlaceObjectAction::Modify => continue,
        };

        let shape = match shapes.get(&char_id) {
            Some(s) => s,
            None => continue,
        };

        let (defs, paths) = shape_to_svg(shape, &mut gradient_id);
        all_defs.push_str(&defs);

        // Wrap in a <g> with transform if non-identity
        let has_transform = place
            .matrix
            .as_ref()
            .map(|m| *m != swf::Matrix::IDENTITY)
            .unwrap_or(false);

        if has_transform {
            let m = place.matrix.as_ref().unwrap();
            writeln!(
                all_body,
                r#"  <g transform="{}">"#,
                matrix_to_svg(m)
            )
            .unwrap();
            // Indent the paths within the group
            for line in paths.lines() {
                if !line.is_empty() {
                    writeln!(all_body, "  {}", line).unwrap();
                }
            }
            writeln!(all_body, "  </g>").unwrap();
        } else {
            all_body.push_str(&paths);
        }
    }

    let mut svg = String::new();
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}" width="{}" height="{}">"#,
        fmt_float(x),
        fmt_float(y),
        fmt_float(width),
        fmt_float(height),
        fmt_float(width),
        fmt_float(height)
    )
    .unwrap();

    if !all_defs.is_empty() {
        writeln!(svg, "<defs>").unwrap();
        svg.push_str(&all_defs);
        writeln!(svg, "</defs>").unwrap();
    }

    svg.push_str(&all_body);
    svg.push_str("</svg>");

    svg
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use swf::*;

    /// Helper: create a simple triangle shape with fill_style_0 = 1, solid red.
    /// Triangle: (0,0) -> (100px,0) -> (50px, 100px) -> close
    fn make_triangle_shape(fill_style: FillStyle) -> Shape {
        Shape {
            version: 1,
            id: 1,
            shape_bounds: Rectangle {
                x_min: Twips::ZERO,
                x_max: Twips::from_pixels(100.0),
                y_min: Twips::ZERO,
                y_max: Twips::from_pixels(100.0),
            },
            edge_bounds: Rectangle {
                x_min: Twips::ZERO,
                x_max: Twips::from_pixels(100.0),
                y_min: Twips::ZERO,
                y_max: Twips::from_pixels(100.0),
            },
            flags: ShapeFlag::empty(),
            styles: ShapeStyles {
                fill_styles: vec![fill_style],
                line_styles: vec![],
            },
            shape: vec![
                // Move to (0,0), set fill0=1
                ShapeRecord::StyleChange(Box::new(StyleChangeData {
                    move_to: Some(Point::new(Twips::ZERO, Twips::ZERO)),
                    fill_style_0: Some(1),
                    fill_style_1: None,
                    line_style: None,
                    new_styles: None,
                })),
                // Line to (100px, 0) = (2000tw, 0)
                ShapeRecord::StraightEdge {
                    delta: PointDelta::new(Twips::from_pixels(100.0), Twips::ZERO),
                },
                // Line to (50px, 100px) = delta (-50px, 100px)
                ShapeRecord::StraightEdge {
                    delta: PointDelta::new(Twips::from_pixels(-50.0), Twips::from_pixels(100.0)),
                },
                // Line back to (0,0) = delta (-50px, -100px)
                ShapeRecord::StraightEdge {
                    delta: PointDelta::new(Twips::from_pixels(-50.0), Twips::from_pixels(-100.0)),
                },
            ],
        }
    }

    #[test]
    fn test_walk_edges_collects_fill0_edges() {
        let shape = make_triangle_shape(FillStyle::Color(Color::RED));
        let (groups, _fills, _lines) = walk_shape_edges(&shape);

        // fill0=1 should have 3 edges
        let edges = groups.fills.get(&1).expect("fill0=1 should have edges");
        assert_eq!(edges.len(), 3, "Triangle should produce 3 edges");

        // Verify first edge starts at origin
        assert_eq!(edges[0].start(), TwipsPoint { x: 0, y: 0 });
    }

    #[test]
    fn test_walk_edges_reverses_fill1() {
        // Create a shape with fill0=1 AND fill1=2
        let shape = Shape {
            version: 1,
            id: 1,
            shape_bounds: Rectangle {
                x_min: Twips::ZERO,
                x_max: Twips::from_pixels(100.0),
                y_min: Twips::ZERO,
                y_max: Twips::from_pixels(100.0),
            },
            edge_bounds: Rectangle {
                x_min: Twips::ZERO,
                x_max: Twips::from_pixels(100.0),
                y_min: Twips::ZERO,
                y_max: Twips::from_pixels(100.0),
            },
            flags: ShapeFlag::empty(),
            styles: ShapeStyles {
                fill_styles: vec![
                    FillStyle::Color(Color::RED),
                    FillStyle::Color(Color::BLUE),
                ],
                line_styles: vec![],
            },
            shape: vec![
                ShapeRecord::StyleChange(Box::new(StyleChangeData {
                    move_to: Some(Point::new(Twips::ZERO, Twips::ZERO)),
                    fill_style_0: Some(1),
                    fill_style_1: Some(2),
                    line_style: None,
                    new_styles: None,
                })),
                // Single edge: (0,0) -> (100px,0)
                ShapeRecord::StraightEdge {
                    delta: PointDelta::new(Twips::from_pixels(100.0), Twips::ZERO),
                },
            ],
        };

        let (groups, _, _) = walk_shape_edges(&shape);

        // fill0=1: edge should be (0,0) -> (2000,0)
        let fill0_edges = groups.fills.get(&1).unwrap();
        assert_eq!(fill0_edges.len(), 1);
        assert_eq!(fill0_edges[0].start(), TwipsPoint { x: 0, y: 0 });
        assert_eq!(fill0_edges[0].end(), TwipsPoint { x: 2000, y: 0 });

        // fill1=2: edge should be REVERSED: (2000,0) -> (0,0)
        let fill1_edges = groups.fills.get(&2).unwrap();
        assert_eq!(fill1_edges.len(), 1);
        assert_eq!(fill1_edges[0].start(), TwipsPoint { x: 2000, y: 0 });
        assert_eq!(fill1_edges[0].end(), TwipsPoint { x: 0, y: 0 });
    }

    #[test]
    fn test_connect_edges_forms_closed_path() {
        // Three edges forming a triangle: A->B, B->C, C->A
        let a = TwipsPoint { x: 0, y: 0 };
        let b = TwipsPoint { x: 200, y: 0 };
        let c = TwipsPoint { x: 100, y: 200 };

        let edges = vec![
            Edge::Straight { start: a, end: b },
            Edge::Straight { start: b, end: c },
            Edge::Straight { start: c, end: a },
        ];

        let paths = connect_edges(&edges);
        assert_eq!(paths.len(), 1, "Should form 1 closed path");
        assert_eq!(paths[0].edges.len(), 3);
    }

    #[test]
    fn test_connect_edges_multiple_subpaths() {
        // Two disjoint triangles
        let a1 = TwipsPoint { x: 0, y: 0 };
        let b1 = TwipsPoint { x: 200, y: 0 };
        let c1 = TwipsPoint { x: 100, y: 200 };

        let a2 = TwipsPoint { x: 1000, y: 1000 };
        let b2 = TwipsPoint { x: 1200, y: 1000 };
        let c2 = TwipsPoint { x: 1100, y: 1200 };

        let edges = vec![
            Edge::Straight {
                start: a1,
                end: b1,
            },
            Edge::Straight {
                start: a2,
                end: b2,
            },
            Edge::Straight {
                start: b1,
                end: c1,
            },
            Edge::Straight {
                start: b2,
                end: c2,
            },
            Edge::Straight {
                start: c1,
                end: a1,
            },
            Edge::Straight {
                start: c2,
                end: a2,
            },
        ];

        let paths = connect_edges(&edges);
        assert_eq!(paths.len(), 2, "Should form 2 separate sub-paths");
    }

    #[test]
    fn test_shape_to_svg_solid_color() {
        let shape = make_triangle_shape(FillStyle::Color(Color::RED));
        let (defs, body) = shape_to_svg(&shape, &mut 0);

        assert!(defs.is_empty(), "Solid color should not need defs");
        assert!(body.contains(r#"fill="rgb(255,0,0)""#), "Should have red fill: {}", body);
        assert!(body.contains('M'), "Should have M command");
        assert!(body.contains('L'), "Should have L command");
        assert!(body.contains('Z'), "Should have Z close command");
    }

    #[test]
    fn test_shape_to_svg_linear_gradient() {
        let gradient = Gradient {
            matrix: Matrix::IDENTITY,
            spread: GradientSpread::Pad,
            interpolation: GradientInterpolation::Rgb,
            records: vec![
                GradientRecord {
                    ratio: 0,
                    color: Color::RED,
                },
                GradientRecord {
                    ratio: 255,
                    color: Color::BLUE,
                },
            ],
        };

        let shape = make_triangle_shape(FillStyle::LinearGradient(gradient));
        let (defs, body) = shape_to_svg(&shape, &mut 0);

        assert!(
            defs.contains("<linearGradient"),
            "Should have linearGradient in defs: {}",
            defs
        );
        assert!(defs.contains(r#"x1="-819.2""#), "Should have x1: {}", defs);
        assert!(defs.contains(r#"x2="819.2""#), "Should have x2: {}", defs);
        assert!(defs.contains("<stop"), "Should have stops: {}", defs);
        assert!(
            defs.contains(r#"offset="0""#),
            "First stop at 0: {}",
            defs
        );
        assert!(
            defs.contains(r#"offset="1""#),
            "Last stop at 1: {}",
            defs
        );
        assert!(body.contains("url(#g0)"), "Should reference gradient: {}", body);
    }

    #[test]
    fn test_shape_to_svg_with_line_style() {
        // Shape with only a line style, no fill
        let shape = Shape {
            version: 1,
            id: 1,
            shape_bounds: Rectangle {
                x_min: Twips::ZERO,
                x_max: Twips::from_pixels(100.0),
                y_min: Twips::ZERO,
                y_max: Twips::from_pixels(100.0),
            },
            edge_bounds: Rectangle {
                x_min: Twips::ZERO,
                x_max: Twips::from_pixels(100.0),
                y_min: Twips::ZERO,
                y_max: Twips::from_pixels(100.0),
            },
            flags: ShapeFlag::empty(),
            styles: ShapeStyles {
                fill_styles: vec![],
                line_styles: vec![
                    LineStyle::new()
                        .with_width(Twips::from_pixels(2.0))
                        .with_color(Color::from_rgb(0x00FF00, 255)),
                ],
            },
            shape: vec![
                ShapeRecord::StyleChange(Box::new(StyleChangeData {
                    move_to: Some(Point::new(Twips::ZERO, Twips::ZERO)),
                    fill_style_0: None,
                    fill_style_1: None,
                    line_style: Some(1),
                    new_styles: None,
                })),
                ShapeRecord::StraightEdge {
                    delta: PointDelta::new(Twips::from_pixels(100.0), Twips::ZERO),
                },
                ShapeRecord::StraightEdge {
                    delta: PointDelta::new(Twips::ZERO, Twips::from_pixels(100.0)),
                },
            ],
        };

        let (_defs, body) = shape_to_svg(&shape, &mut 0);

        assert!(
            body.contains(r#"stroke="rgb(0,255,0)""#),
            "Should have green stroke: {}",
            body
        );
        assert!(
            body.contains(r#"stroke-width="2""#),
            "Should have stroke-width 2: {}",
            body
        );
        assert!(
            body.contains(r#"fill="none""#),
            "Line paths should have fill=none: {}",
            body
        );
    }
}
