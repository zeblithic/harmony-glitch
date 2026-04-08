mod bitmap;
mod svg;

use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "extract-swf",
    about = "Extract bitmaps or convert vectors from SWF files"
)]
struct Args {
    /// Source directory containing SWF files
    #[arg(long)]
    source: PathBuf,

    /// Output directory for extracted PNGs / SVGs
    #[arg(long)]
    output: PathBuf,
}

enum ProcessResult {
    Bitmap,
    Svg,
    Skipped,
}

/// Process a single SWF file: extract bitmap or convert to SVG.
///
/// Parses the SWF once, then decides: if bitmap tags exist, extract the largest
/// bitmap as PNG. If only vector shapes, convert to SVG. Otherwise skip.
fn process_swf(
    swf_data: &[u8],
    output_path_no_ext: &Path,
    errors: &mut Vec<String>,
    swf_path: &Path,
) -> ProcessResult {
    // Parse once — used for both detection and conversion
    let swf_buf = match swf::decompress_swf(swf_data) {
        Ok(b) => b,
        Err(err) => {
            errors.push(format!(
                "WARN: skipped {} — SWF decompress error: {}",
                swf_path.display(),
                err
            ));
            return ProcessResult::Skipped;
        }
    };
    let parsed = match swf::parse_swf(&swf_buf) {
        Ok(s) => s,
        Err(err) => {
            errors.push(format!(
                "WARN: skipped {} — SWF parse error: {}",
                swf_path.display(),
                err
            ));
            return ProcessResult::Skipped;
        }
    };

    // Check what tags are present
    let has_bitmaps = parsed.tags.iter().any(|tag| {
        matches!(
            tag,
            swf::Tag::DefineBitsLossless(_)
                | swf::Tag::DefineBitsJpeg2 { .. }
                | swf::Tag::DefineBitsJpeg3(_)
        )
    });
    let has_shapes = parsed
        .tags
        .iter()
        .any(|tag| matches!(tag, swf::Tag::DefineShape(_)));

    if has_bitmaps {
        let output_path = output_path_no_ext.with_extension("png");
        match bitmap::extract_largest_bitmap(&parsed) {
            Some(bm) => match bitmap::write_png(&output_path, &bm) {
                Ok(()) => return ProcessResult::Bitmap,
                Err(err) => {
                    errors.push(format!(
                        "WARN: skipped {} — write error: {}",
                        swf_path.display(),
                        err
                    ));
                    return ProcessResult::Skipped;
                }
            },
            None => {
                // Bitmap tags exist but none were decodable (e.g., Rgb15).
                // Fall through to SVG path if shapes are available.
            }
        }
    }

    if has_shapes {
        let output_path = output_path_no_ext.with_extension("svg");
        let svg_content = svg::convert_swf_to_svg(&parsed);
        match std::fs::write(&output_path, svg_content) {
            Ok(()) => return ProcessResult::Svg,
            Err(err) => {
                errors.push(format!(
                    "WARN: skipped {} — SVG write error: {}",
                    swf_path.display(),
                    err
                ));
                return ProcessResult::Skipped;
            }
        }
    }

    errors.push(format!(
        "WARN: skipped {} — no extractable bitmap or shape found",
        swf_path.display()
    ));
    ProcessResult::Skipped
}

fn walk_swfs(
    dir: &Path,
    source_root: &Path,
    output_root: &Path,
    bitmaps: &mut u32,
    svgs: &mut u32,
    skipped: &mut u32,
    errors: &mut Vec<String>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            errors.push(format!("Cannot read directory {}: {}", dir.display(), err));
            *skipped += 1;
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                errors.push(format!(
                    "Directory entry error in {}: {}",
                    dir.display(),
                    err
                ));
                *skipped += 1;
                continue;
            }
        };

        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(err) => {
                errors.push(format!("Cannot stat {}: {}", path.display(), err));
                *skipped += 1;
                continue;
            }
        };

        if file_type.is_dir() {
            walk_swfs(
                &path,
                source_root,
                output_root,
                bitmaps,
                svgs,
                skipped,
                errors,
            );
            continue;
        }

        // Only process .swf files
        let is_swf = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("swf"))
            .unwrap_or(false);
        if !is_swf {
            continue;
        }

        // Compute relative path from source root, then build base output path (no extension)
        let rel = match path.strip_prefix(source_root) {
            Ok(r) => r,
            Err(_) => {
                errors.push(format!("Cannot relativize path: {}", path.display()));
                *skipped += 1;
                continue;
            }
        };
        let output_base = output_root.join(rel).with_extension("");

        // Read SWF file
        let swf_data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(err) => {
                errors.push(format!(
                    "WARN: skipped {} — read error: {}",
                    path.display(),
                    err
                ));
                *skipped += 1;
                continue;
            }
        };

        // Create parent directories as needed
        if let Some(parent) = output_base.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                errors.push(format!(
                    "WARN: skipped {} — cannot create output dir: {}",
                    path.display(),
                    err
                ));
                *skipped += 1;
                continue;
            }
        }

        match process_swf(&swf_data, &output_base, errors, &path) {
            ProcessResult::Bitmap => *bitmaps += 1,
            ProcessResult::Svg => *svgs += 1,
            ProcessResult::Skipped => *skipped += 1,
        }
    }
}

fn main() {
    let args = Args::parse();

    if !args.source.is_dir() {
        eprintln!(
            "Error: source directory does not exist: {}",
            args.source.display()
        );
        std::process::exit(1);
    }

    std::fs::create_dir_all(&args.output).unwrap_or_else(|e| {
        eprintln!("Error: cannot create output directory: {e}");
        std::process::exit(1);
    });

    let mut bitmaps: u32 = 0;
    let mut svgs: u32 = 0;
    let mut skipped: u32 = 0;
    let mut errors: Vec<String> = Vec::new();

    walk_swfs(
        &args.source,
        &args.source,
        &args.output,
        &mut bitmaps,
        &mut svgs,
        &mut skipped,
        &mut errors,
    );

    let total = bitmaps + svgs + skipped;

    for warning in &errors {
        eprintln!("{}", warning);
    }

    println!(
        "Extracted {} bitmaps + {} SVGs / {} items ({} skipped)",
        bitmaps, svgs, total, skipped
    );
}
