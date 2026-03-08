use super::types::*;
use super::xml_helpers::{parse_glitch_xml, XmlValue};

/// Parse a Glitch location XML string into a StreetData struct.
pub fn parse_street(xml: &str) -> Result<StreetData, String> {
    let root = parse_glitch_xml(xml)?;

    // The actual data is nested under "dynamic" in the game_object
    let dynamic = root.get("dynamic").unwrap_or(&root);

    let tsid = dynamic.get("tsid")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let name = dynamic.get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("Unnamed Street")
        .to_string();

    let left = dynamic.get("l").and_then(|v| v.as_f64()).unwrap_or(-3000.0);
    let right = dynamic.get("r").and_then(|v| v.as_f64()).unwrap_or(3000.0);
    let top = dynamic.get("t").and_then(|v| v.as_f64()).unwrap_or(-1000.0);
    let bottom = dynamic.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let ground_y = dynamic.get("ground_y").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let gradient = dynamic.get("gradient").and_then(|g| {
        Some(Gradient {
            top: g.get("top")?.as_str()?.to_string(),
            bottom: g.get("bottom")?.as_str()?.to_string(),
        })
    });

    let layers = parse_layers(dynamic)?;
    let signposts = parse_signposts(dynamic);

    Ok(StreetData {
        tsid,
        name,
        left,
        right,
        top,
        bottom,
        ground_y,
        gradient,
        layers,
        signposts,
    })
}

fn parse_layers(dynamic: &XmlValue) -> Result<Vec<Layer>, String> {
    let layers_obj = match dynamic.get("layers") {
        Some(v) => v,
        None => return Ok(vec![]),
    };
    let layers_map = match layers_obj.as_object() {
        Some(m) => m,
        None => return Ok(vec![]),
    };

    let mut layers = Vec::new();

    for (layer_id, layer_val) in layers_map {
        let is_middleground = layer_id == "middleground"
            || layer_val.get("name").and_then(|v| v.as_str()) == Some("middleground");

        let z = layer_val.get("z").and_then(|v| v.as_int()).unwrap_or(0) as i32;
        let w = layer_val.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let h = layer_val.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let name = layer_val.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(layer_id)
            .to_string();

        let decos = parse_decos(layer_val);
        let filters = parse_filters(layer_val);

        let (platform_lines, walls, ladders) = if is_middleground {
            (
                parse_platform_lines(layer_val),
                parse_walls(layer_val),
                parse_ladders(layer_val),
            )
        } else {
            (vec![], vec![], vec![])
        };

        layers.push(Layer {
            name,
            z,
            w,
            h,
            is_middleground,
            decos,
            platform_lines,
            walls,
            ladders,
            filters,
        });
    }

    // Sort layers by z (back to front)
    layers.sort_by_key(|l| l.z);
    Ok(layers)
}

fn parse_decos(layer: &XmlValue) -> Vec<Deco> {
    let decos_obj = match layer.get("decos").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    decos_obj.iter().map(|(deco_id, d)| {
        Deco {
            id: deco_id.clone(),
            name: d.get("name").and_then(|v| v.as_str()).unwrap_or(deco_id).to_string(),
            sprite_class: d.get("sprite_class").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            x: d.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: d.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            w: d.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0),
            h: d.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0),
            z: d.get("z").and_then(|v| v.as_int()).unwrap_or(0) as i32,
            r: d.get("r").and_then(|v| v.as_f64()).unwrap_or(0.0),
            h_flip: d.get("h_flip").and_then(|v| v.as_bool()).unwrap_or(false),
        }
    }).collect()
}

fn parse_platform_lines(layer: &XmlValue) -> Vec<PlatformLine> {
    let plats = match layer.get("platform_lines").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    let mut result: Vec<PlatformLine> = plats.iter().map(|(plat_id, p)| {
        let start = p.get("start").map(|s| Point {
            x: s.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: s.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }).unwrap_or(Point { x: 0.0, y: 0.0 });

        let end = p.get("end").map(|e| Point {
            x: e.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: e.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }).unwrap_or(Point { x: 0.0, y: 0.0 });

        let pc_perm = p.get("platform_pc_perm").and_then(|v| v.as_int()).map(|v| v as i32);
        let item_perm = p.get("platform_item_perm").and_then(|v| v.as_int()).map(|v| v as i32);

        PlatformLine {
            id: plat_id.clone(),
            start,
            end,
            pc_perm,
            item_perm,
        }
    }).collect();

    // Sort by start.x for deterministic collision order (HashMap has no guaranteed order)
    result.sort_by(|a, b| a.start.x.partial_cmp(&b.start.x).unwrap_or(std::cmp::Ordering::Equal));
    result
}

fn parse_walls(layer: &XmlValue) -> Vec<Wall> {
    let walls = match layer.get("walls").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    walls.iter().map(|(wall_id, w)| {
        Wall {
            id: wall_id.clone(),
            x: w.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: w.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            h: w.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0),
            pc_perm: w.get("pc_perm").and_then(|v| v.as_int()).map(|v| v as i32),
            item_perm: w.get("item_perm").and_then(|v| v.as_int()).map(|v| v as i32),
        }
    }).collect()
}

fn parse_ladders(layer: &XmlValue) -> Vec<Ladder> {
    let ladders = match layer.get("ladders").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    ladders.iter().map(|(ladder_id, l)| {
        Ladder {
            id: ladder_id.clone(),
            x: l.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: l.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            w: l.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0),
            h: l.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }
    }).collect()
}

fn parse_filters(layer: &XmlValue) -> Option<LayerFilters> {
    let f = layer.get("filtersNEW")?.as_object()?;
    Some(LayerFilters {
        brightness: f.get("brightness").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        contrast: f.get("contrast").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        saturation: f.get("saturation").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        blur: f.get("blur").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        tint_color: f.get("tintColor").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        tint_amount: f.get("tintAmount").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
    })
}

fn parse_signposts(dynamic: &XmlValue) -> Vec<Signpost> {
    let sps = match dynamic.get("signposts").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    sps.iter().map(|(sp_id, s)| {
        let connects = s.get("connects")
            .and_then(|v| v.as_object())
            .map(|conns| {
                conns.values().filter_map(|c| {
                    match c.get("target") {
                        Some(XmlValue::ObjRef { tsid, label }) => Some(SignpostConnection {
                            target_tsid: tsid.clone(),
                            target_label: label.clone(),
                        }),
                        _ => None,
                    }
                }).collect()
            })
            .unwrap_or_default();

        Signpost {
            id: sp_id.clone(),
            x: s.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: s.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            connects,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_STREET_XML: &str = r#"
    <game_object tsid="GA5101HF7F429V5" label="Test Meadow">
      <object id="dynamic">
        <int id="l">-3000</int>
        <int id="r">3000</int>
        <int id="t">-1000</int>
        <int id="b">0</int>
        <str id="label">Groddle Meadow</str>
        <str id="tsid">LA5101HF7F429V5</str>
        <int id="ground_y">0</int>
        <object id="gradient">
          <str id="top">87A8C9</str>
          <str id="bottom">FFC400</str>
        </object>
        <object id="layers">
          <object id="T_sky">
            <int id="w">5160</int>
            <int id="h">1000</int>
            <int id="z">-4</int>
            <str id="name">sky</str>
            <object id="decos">
              <object id="cloud_1">
                <int id="x">100</int>
                <int id="y">200</int>
                <int id="w">80</int>
                <int id="h">40</int>
                <int id="z">0</int>
                <int id="r">0</int>
                <str id="sprite_class">cloud_fluffy</str>
                <str id="name">cloud_1</str>
              </object>
            </object>
          </object>
          <object id="middleground">
            <int id="w">6000</int>
            <int id="h">1000</int>
            <int id="z">0</int>
            <str id="name">middleground</str>
            <object id="decos"></object>
            <object id="platform_lines">
              <object id="plat_ground">
                <object id="start">
                  <int id="x">-2800</int>
                  <int id="y">0</int>
                </object>
                <object id="end">
                  <int id="x">2800</int>
                  <int id="y">0</int>
                </object>
                <int id="platform_pc_perm">-1</int>
              </object>
              <object id="plat_hill">
                <object id="start">
                  <int id="x">500</int>
                  <int id="y">0</int>
                </object>
                <object id="end">
                  <int id="x">1000</int>
                  <int id="y">-100</int>
                </object>
              </object>
            </object>
            <object id="walls">
              <object id="wall_left">
                <int id="x">-3000</int>
                <int id="y">-500</int>
                <int id="h">500</int>
              </object>
            </object>
            <object id="ladders">
              <object id="ladder_1">
                <int id="x">200</int>
                <int id="y">-200</int>
                <int id="w">50</int>
                <int id="h">200</int>
              </object>
            </object>
          </object>
        </object>
      </object>
    </game_object>
    "#;

    #[test]
    fn parse_street_metadata() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        assert_eq!(street.name, "Groddle Meadow");
        assert_eq!(street.left, -3000.0);
        assert_eq!(street.right, 3000.0);
        assert_eq!(street.top, -1000.0);
        assert_eq!(street.bottom, 0.0);
        assert_eq!(street.ground_y, 0.0);
        assert!((street.width() - 6000.0).abs() < 0.001);
    }

    #[test]
    fn parse_gradient() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let grad = street.gradient.unwrap();
        assert_eq!(grad.top, "87A8C9");
        assert_eq!(grad.bottom, "FFC400");
    }

    #[test]
    fn parse_layers_sorted() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        assert_eq!(street.layers.len(), 2);
        // Should be sorted by z: sky (-4) first, middleground (0) second
        assert_eq!(street.layers[0].name, "sky");
        assert_eq!(street.layers[0].z, -4);
        assert_eq!(street.layers[1].name, "middleground");
        assert!(street.layers[1].is_middleground);
    }

    #[test]
    fn parse_platforms() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let platforms = street.platforms();
        assert_eq!(platforms.len(), 2);

        // Find the ground platform
        let ground = platforms.iter().find(|p| p.id == "plat_ground").unwrap();
        assert_eq!(ground.start.x, -2800.0);
        assert_eq!(ground.start.y, 0.0);
        assert_eq!(ground.end.x, 2800.0);
        assert!(ground.solid_from_top());
        assert!(!ground.solid_from_bottom()); // pc_perm = -1

        // Find the hill (sloped)
        let hill = platforms.iter().find(|p| p.id == "plat_hill").unwrap();
        assert_eq!(hill.start.y, 0.0);
        assert_eq!(hill.end.y, -100.0);
        // Midpoint of slope: y should be -50
        assert!((hill.y_at(750.0) - (-50.0)).abs() < 0.001);
    }

    #[test]
    fn parse_walls() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let mg = street.middleground().unwrap();
        assert_eq!(mg.walls.len(), 1);
        assert_eq!(mg.walls[0].x, -3000.0);
    }

    #[test]
    fn parse_ladders() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let mg = street.middleground().unwrap();
        assert_eq!(mg.ladders.len(), 1);
        assert_eq!(mg.ladders[0].w, 50.0);
        assert_eq!(mg.ladders[0].h, 200.0);
    }

    #[test]
    fn parse_decos() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let sky = &street.layers[0];
        assert_eq!(sky.decos.len(), 1);
        assert_eq!(sky.decos[0].sprite_class, "cloud_fluffy");
    }

    #[test]
    fn parse_parallax_factor() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let sky = &street.layers[0];
        let factor = street.parallax_factor(sky);
        assert!((factor - 5160.0 / 6000.0).abs() < 0.001);
    }

    #[test]
    fn parse_empty_street_no_layers() {
        let xml = r#"
        <game_object tsid="GTEST" label="Empty">
          <object id="dynamic">
            <str id="tsid">LTEST</str>
            <str id="label">Empty Street</str>
            <int id="l">-100</int>
            <int id="r">100</int>
            <int id="t">-50</int>
            <int id="b">0</int>
            <int id="ground_y">0</int>
          </object>
        </game_object>
        "#;

        let street = parse_street(xml).unwrap();
        assert_eq!(street.name, "Empty Street");
        assert!(street.layers.is_empty());
        assert!(street.platforms().is_empty());
    }

    #[test]
    fn parse_missing_optional_fields() {
        // No gradient, no filters on the layer
        let xml = r#"
        <game_object tsid="GTEST" label="Bare">
          <object id="dynamic">
            <str id="tsid">LBARE</str>
            <str id="label">Bare Street</str>
            <int id="l">-500</int>
            <int id="r">500</int>
            <int id="t">-200</int>
            <int id="b">0</int>
            <int id="ground_y">0</int>
            <object id="layers">
              <object id="middleground">
                <int id="w">1000</int>
                <int id="h">200</int>
                <int id="z">0</int>
                <str id="name">middleground</str>
              </object>
            </object>
          </object>
        </game_object>
        "#;

        let street = parse_street(xml).unwrap();
        assert!(street.gradient.is_none());
        assert_eq!(street.layers.len(), 1);
        let mg = &street.layers[0];
        assert!(mg.filters.is_none());
        assert!(mg.decos.is_empty());
        assert!(mg.platform_lines.is_empty());
        assert!(mg.walls.is_empty());
        assert!(mg.ladders.is_empty());
    }

    #[test]
    fn parse_unknown_xml_elements_ignored() {
        let xml = r#"
        <game_object tsid="GTEST" label="Custom">
          <object id="dynamic">
            <str id="tsid">LCUSTOM</str>
            <str id="label">Custom Street</str>
            <int id="l">-100</int>
            <int id="r">100</int>
            <int id="t">-50</int>
            <int id="b">0</int>
            <int id="ground_y">0</int>
            <custom id="foo">bar</custom>
            <widget id="baz">qux</widget>
            <object id="layers">
              <object id="middleground">
                <int id="w">200</int>
                <int id="h">50</int>
                <int id="z">0</int>
                <str id="name">middleground</str>
                <sparkle id="glitter">shine</sparkle>
              </object>
            </object>
          </object>
        </game_object>
        "#;

        let street = parse_street(xml).unwrap();
        assert_eq!(street.name, "Custom Street");
        assert_eq!(street.layers.len(), 1);
        assert!(street.layers[0].is_middleground);
    }
}
