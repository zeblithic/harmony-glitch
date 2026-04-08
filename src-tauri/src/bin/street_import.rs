use clap::Parser;
use harmony_glitch::street::manifest::{StreetEntry, StreetManifest};
use harmony_glitch::street::parser::parse_street;
use std::collections::HashMap;
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
                let out_filename = format!("{}.xml", street.tsid);
                let out_path = args.output.join(&out_filename);
                if let Err(e) = std::fs::write(&out_path, &xml) {
                    errors.push(format!("{}: failed to write: {e}", street.tsid));
                    continue;
                }

                streets.insert(
                    street.tsid.clone(),
                    StreetEntry {
                        name: street.name,
                        filename: out_filename,
                    },
                );
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

    println!(
        "Imported {} streets to {}",
        manifest.streets.len(),
        args.output.display()
    );
    println!("Skipped {skipped} non-street entries");

    if !errors.is_empty() {
        eprintln!("\n{} errors:", errors.len());
        for e in &errors {
            eprintln!("  {e}");
        }
        std::process::exit(1);
    }
}
