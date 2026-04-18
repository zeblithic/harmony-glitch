use clap::Parser;
use harmony_glitch::street::manifest::{StreetEntry, StreetManifest};
use harmony_glitch::street::parser::parse_street;
use harmony_glitch::street::types::StreetData;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "street-import",
    about = "Import Glitch location XMLs into harmony-glitch"
)]
struct Args {
    /// Path to locations-xml.zip
    #[arg(long)]
    source: PathBuf,

    /// Output directory for extracted streets and manifest
    #[arg(long)]
    output: PathBuf,
}

fn main() {
    let args = Args::parse();

    let file = std::fs::File::open(&args.source)
        .unwrap_or_else(|e| panic!("Failed to open {}: {e}", args.source.display()));
    let mut archive =
        zip::ZipArchive::new(file).unwrap_or_else(|e| panic!("Failed to read zip: {e}"));

    std::fs::create_dir_all(&args.output)
        .unwrap_or_else(|e| panic!("Failed to create output dir: {e}"));

    let mut streets: HashMap<String, StreetEntry> = HashMap::new();
    let mut parsed_streets: Vec<StreetData> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut skipped = 0u32;

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("Failed to read zip entry {i}: {e}"));
                continue;
            }
        };

        let name = entry.name().to_string();

        // Only process G-prefixed XML files (street geometry), skip L-prefixed (metadata)
        let filename = match name.rsplit('/').next() {
            Some(f) if f.ends_with(".xml") && f.starts_with('G') => f.to_string(),
            _ => {
                skipped += 1;
                continue;
            }
        };

        let mut xml = String::new();
        if let Err(e) = entry.read_to_string(&mut xml) {
            errors.push(format!("{filename}: failed to read: {e}"));
            continue;
        }

        match parse_street(&xml) {
            Ok(street) => {
                // Reject TSIDs that could escape the output directory
                if street.tsid.contains('/')
                    || street.tsid.contains('\\')
                    || street.tsid.contains("..")
                {
                    errors.push(format!(
                        "{filename}: suspicious TSID '{}', skipping",
                        street.tsid
                    ));
                    continue;
                }
                let out_filename = format!("{}.xml", street.tsid);
                let out_path = args.output.join(&out_filename);
                if let Err(e) = std::fs::write(&out_path, &xml) {
                    errors.push(format!("{}: failed to write: {e}", street.tsid));
                    continue;
                }

                streets.insert(
                    street.tsid.clone(),
                    StreetEntry {
                        name: street.name.clone(),
                        filename: out_filename,
                    },
                );
                parsed_streets.push(street);
            }
            Err(e) => {
                errors.push(format!("{filename}: parse error: {e}"));
            }
        }
    }

    // Write manifest
    let manifest = StreetManifest {
        version: 1,
        streets,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).unwrap();
    let manifest_path = args.output.join("manifest.json");
    std::fs::write(&manifest_path, &manifest_json)
        .unwrap_or_else(|e| panic!("Failed to write manifest: {e}"));

    // Signpost connectivity analysis
    let signpost_report = analyze_signpost_connectivity(&parsed_streets, &manifest);

    println!(
        "\n\u{2550}\u{2550}\u{2550} Street Import Complete \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}"
    );
    println!(
        "  Streets: {} imported to {}",
        manifest.streets.len(),
        args.output.display()
    );
    println!("  Skipped: {skipped} non-street entries (non-G-prefix or non-XML)");
    println!(
        "  Signposts: {} total across {} streets",
        signpost_report.total_signposts, signpost_report.streets_with_signposts
    );
    println!(
        "  Connections: {}/{} targets resolve ({} unreachable)",
        signpost_report.resolved_targets,
        signpost_report.total_targets,
        signpost_report.unreachable_targets
    );
    println!(
        "  Written: {}",
        manifest_path.display()
    );

    if !errors.is_empty() {
        eprintln!(
            "\n  {} parse warnings (test/incomplete streets):",
            errors.len()
        );
        for e in &errors {
            eprintln!("    {e}");
        }
        // Don't exit(1) — parse failures for upstream test streets are expected.
        // The import succeeded for all parseable streets.
    }
}

struct SignpostReport {
    streets_with_signposts: usize,
    total_signposts: usize,
    total_targets: usize,
    resolved_targets: usize,
    unreachable_targets: usize,
}

fn analyze_signpost_connectivity(streets: &[StreetData], manifest: &StreetManifest) -> SignpostReport {
    let mut streets_with_signposts = 0usize;
    let mut total_signposts = 0usize;
    let mut all_targets: Vec<String> = Vec::new();
    let mut unreachable: HashSet<String> = HashSet::new();

    for street in streets {
        if !street.signposts.is_empty() {
            streets_with_signposts += 1;
            total_signposts += street.signposts.len();
        }
        for sp in &street.signposts {
            for conn in &sp.connects {
                all_targets.push(conn.target_tsid.clone());
                if !manifest.streets.contains_key(&conn.target_tsid) {
                    unreachable.insert(conn.target_tsid.clone());
                }
            }
        }
    }

    let total_targets = all_targets.len();
    let unreachable_count = all_targets
        .iter()
        .filter(|t| unreachable.contains(t.as_str()))
        .count();

    SignpostReport {
        streets_with_signposts,
        total_signposts,
        total_targets,
        resolved_targets: total_targets - unreachable_count,
        unreachable_targets: unreachable_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest(tsids: &[&str]) -> StreetManifest {
        let mut streets = HashMap::new();
        for tsid in tsids {
            streets.insert(
                tsid.to_string(),
                StreetEntry {
                    name: format!("Street {tsid}"),
                    filename: format!("{tsid}.xml"),
                },
            );
        }
        StreetManifest { version: 1, streets }
    }

    fn make_street_with_signposts(tsid: &str, targets: &[&str]) -> StreetData {
        use harmony_glitch::street::types::{Signpost, SignpostConnection};
        StreetData {
            tsid: tsid.to_string(),
            name: format!("Street {tsid}"),
            left: -3000.0,
            right: 3000.0,
            top: -1000.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![],
            signposts: if targets.is_empty() {
                vec![]
            } else {
                vec![Signpost {
                    id: "sp1".into(),
                    x: -2500.0,
                    y: 0.0,
                    connects: targets
                        .iter()
                        .map(|t| SignpostConnection {
                            target_tsid: t.to_string(),
                            target_label: format!("Target {t}"),
                            arrival_x: None,
                            arrival_y: None,
                            arrival_facing: None,
                        })
                        .collect(),
                }]
            },
            default_spawn: None,
        }
    }

    #[test]
    fn connectivity_all_resolved() {
        let streets = vec![
            make_street_with_signposts("LA001", &["LA002"]),
            make_street_with_signposts("LA002", &["LA001"]),
        ];
        let manifest = make_manifest(&["LA001", "LA002"]);
        let report = analyze_signpost_connectivity(&streets, &manifest);

        assert_eq!(report.streets_with_signposts, 2);
        assert_eq!(report.total_signposts, 2);
        assert_eq!(report.total_targets, 2);
        assert_eq!(report.resolved_targets, 2);
        assert_eq!(report.unreachable_targets, 0);
    }

    #[test]
    fn connectivity_some_unreachable() {
        let streets = vec![
            make_street_with_signposts("LA001", &["LA002", "LA003"]),
            make_street_with_signposts("LA002", &["LA001"]),
        ];
        // LA003 not in manifest
        let manifest = make_manifest(&["LA001", "LA002"]);
        let report = analyze_signpost_connectivity(&streets, &manifest);

        assert_eq!(report.total_targets, 3);
        assert_eq!(report.resolved_targets, 2);
        assert_eq!(report.unreachable_targets, 1);
    }

    #[test]
    fn connectivity_no_signposts() {
        let streets = vec![
            make_street_with_signposts("LA001", &[]),
            make_street_with_signposts("LA002", &[]),
        ];
        let manifest = make_manifest(&["LA001", "LA002"]);
        let report = analyze_signpost_connectivity(&streets, &manifest);

        assert_eq!(report.streets_with_signposts, 0);
        assert_eq!(report.total_signposts, 0);
        assert_eq!(report.total_targets, 0);
        assert_eq!(report.resolved_targets, 0);
        assert_eq!(report.unreachable_targets, 0);
    }

    #[test]
    fn connectivity_empty() {
        let manifest = make_manifest(&[]);
        let report = analyze_signpost_connectivity(&[], &manifest);
        assert_eq!(report.streets_with_signposts, 0);
        assert_eq!(report.total_signposts, 0);
        assert_eq!(report.total_targets, 0);
        assert_eq!(report.resolved_targets, 0);
        assert_eq!(report.unreachable_targets, 0);
    }

    #[test]
    fn parse_real_glitch_street() {
        // Verify the parser handles a real Glitch G-prefix geometry XML
        let xml = r#"
        <game_object tsid="GCR10157DMK1L0G" ts="1337364130126" label="Addingfoot Trip" class_tsid="" lastUpdateTime="1337364119975">
          <object id="dynamic">
            <int id="l">-3000</int>
            <int id="r">3000</int>
            <int id="t">-1000</int>
            <int id="b">0</int>
            <str id="label">Addingfoot Trip</str>
            <str id="tsid">LCR10157DMK1L0G</str>
            <int id="ground_y">0</int>
            <object id="gradient">
              <str id="top">87a8c9</str>
              <str id="bottom">ffc400</str>
            </object>
            <object id="layers">
              <object id="middleground">
                <int id="w">6000</int>
                <int id="h">1000</int>
                <int id="z">0</int>
                <str id="name">middleground</str>
                <object id="platform_lines">
                  <object id="plat_1">
                    <object id="start"><int id="x">-2800</int><int id="y">0</int></object>
                    <object id="end"><int id="x">2800</int><int id="y">0</int></object>
                  </object>
                </object>
                <object id="signposts">
                  <object id="signpost_1">
                    <int id="x">-2500</int>
                    <int id="y">-400</int>
                    <object id="connects">
                      <object id="0">
                        <objref id="target" tsid="LCR103UREMK11MT" label="Fairgower Lane"/>
                      </object>
                    </object>
                  </object>
                </object>
              </object>
            </object>
          </object>
        </game_object>
        "#;

        let street = parse_street(xml).unwrap();
        // Parser reads <str id="tsid"> (L-prefix), not the game_object attribute (G-prefix)
        assert_eq!(street.tsid, "LCR10157DMK1L0G");
        assert_eq!(street.name, "Addingfoot Trip");
        assert_eq!(street.left, -3000.0);
        assert_eq!(street.right, 3000.0);

        // Signpost with L-prefix target
        assert_eq!(street.signposts.len(), 1);
        assert_eq!(street.signposts[0].connects[0].target_tsid, "LCR103UREMK11MT");
        assert_eq!(street.signposts[0].connects[0].target_label, "Fairgower Lane");
    }
}
