use clap::Parser;
use flate2::read::ZlibDecoder;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "extract-swf", about = "Extract bitmaps from SWF files")]
struct Args {
    /// Source directory containing SWF files
    #[arg(long)]
    source: PathBuf,

    /// Output directory for extracted PNGs
    #[arg(long)]
    output: PathBuf,
}

struct ExtractedBitmap {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Parse a SWF file from raw bytes and extract the largest bitmap by pixel area.
fn extract_largest_bitmap(swf_data: &[u8]) -> Option<ExtractedBitmap> {
    let swf_buf = swf::decompress_swf(swf_data).ok()?;
    let swf = swf::parse_swf(&swf_buf).ok()?;

    let mut largest: Option<ExtractedBitmap> = None;
    let mut largest_area: u64 = 0;

    for tag in &swf.tags {
        let bitmap = match tag {
            swf::Tag::DefineBitsLossless(b) => decode_lossless(b),
            swf::Tag::DefineBitsJpeg2 { jpeg_data, .. } => {
                let data = strip_swf_jpeg_header(jpeg_data);
                decode_jpeg(data, None)
            }
            swf::Tag::DefineBitsJpeg3(j) => {
                let data = strip_swf_jpeg_header(j.data);
                let alpha = if j.alpha_data.is_empty() {
                    None
                } else {
                    Some(j.alpha_data)
                };
                decode_jpeg(data, alpha)
            }
            _ => None,
        };

        if let Some(bm) = bitmap {
            let area = bm.width as u64 * bm.height as u64;
            if area > largest_area {
                largest_area = area;
                largest = Some(bm);
            }
        }
    }

    largest
}

/// Decode a DefineBitsLossless / DefineBitsLossless2 tag into RGBA pixels.
fn decode_lossless(bitmap: &swf::DefineBitsLossless) -> Option<ExtractedBitmap> {
    let width = bitmap.width as u32;
    let height = bitmap.height as u32;

    // Decompress zlib data
    let mut decoder = ZlibDecoder::new(&bitmap.data[..]);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).ok()?;

    let is_v2 = bitmap.version == 2;

    match bitmap.format {
        swf::BitmapFormat::Rgb32 => {
            let expected = (width * height * 4) as usize;
            if decompressed.len() < expected {
                return None;
            }
            let mut rgba = Vec::with_capacity(expected);
            for pixel in decompressed[..expected].chunks_exact(4) {
                // SWF stores as ARGB (v2, premultiplied) or xRGB (v1)
                let a = pixel[0];
                let r = pixel[1];
                let g = pixel[2];
                let b = pixel[3];

                if is_v2 && a > 0 && a < 255 {
                    // Un-premultiply alpha
                    let un = |c: u8| -> u8 {
                        ((c as u16 * 255) / a as u16).min(255) as u8
                    };
                    rgba.extend_from_slice(&[un(r), un(g), un(b), a]);
                } else if is_v2 {
                    rgba.extend_from_slice(&[r, g, b, a]);
                } else {
                    // Version 1: no alpha channel, treat as fully opaque
                    rgba.extend_from_slice(&[r, g, b, 255]);
                }
            }
            Some(ExtractedBitmap { width, height, rgba })
        }
        swf::BitmapFormat::ColorMap8 { num_colors } => {
            let palette_size = (num_colors as usize + 1) * if is_v2 { 4 } else { 3 };
            if decompressed.len() < palette_size {
                return None;
            }
            let (palette_data, pixel_data) = decompressed.split_at(palette_size);

            // Parse palette entries (v2 uses premultiplied RGBA — un-premultiply)
            let entry_size = if is_v2 { 4 } else { 3 };
            let palette: Vec<[u8; 4]> = palette_data
                .chunks_exact(entry_size)
                .map(|c| {
                    if is_v2 {
                        let a = c[3];
                        if a == 0 {
                            [0, 0, 0, 0]
                        } else if a == 255 {
                            [c[0], c[1], c[2], 255]
                        } else {
                            let un = |v: u8| ((v as u16 * 255) / a as u16).min(255) as u8;
                            [un(c[0]), un(c[1]), un(c[2]), a]
                        }
                    } else {
                        [c[0], c[1], c[2], 255] // RGB, opaque
                    }
                })
                .collect();

            // Rows are padded to 4-byte boundaries
            let row_stride = ((width as usize) + 3) & !3;
            let expected_pixels = row_stride * height as usize;
            if pixel_data.len() < expected_pixels {
                return None;
            }

            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for y in 0..height as usize {
                let row_start = y * row_stride;
                for x in 0..width as usize {
                    let idx = pixel_data[row_start + x] as usize;
                    if let Some(color) = palette.get(idx) {
                        rgba.extend_from_slice(color);
                    } else {
                        rgba.extend_from_slice(&[0, 0, 0, 0]);
                    }
                }
            }
            Some(ExtractedBitmap { width, height, rgba })
        }
        swf::BitmapFormat::Rgb15 => {
            // Rgb15 is rare and not needed for Glitch assets
            None
        }
    }
}

/// Decode JPEG data, optionally applying a zlib-compressed alpha channel.
fn decode_jpeg(jpeg_data: &[u8], alpha_data: Option<&[u8]>) -> Option<ExtractedBitmap> {
    let mut decoder = jpeg_decoder::Decoder::new(jpeg_data);
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let width = info.width as u32;
    let height = info.height as u32;

    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

    match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            for chunk in pixels.chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
        }
        jpeg_decoder::PixelFormat::L8 => {
            for &gray in &pixels {
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
        }
        _ => return None,
    }

    // Apply alpha channel from DefineBitsJpeg3 if present
    if let Some(alpha_compressed) = alpha_data {
        let mut alpha_decoder = ZlibDecoder::new(alpha_compressed);
        let mut alpha = Vec::new();
        if alpha_decoder.read_to_end(&mut alpha).is_ok() {
            let pixel_count = (width * height) as usize;
            if alpha.len() >= pixel_count {
                for i in 0..pixel_count {
                    rgba[i * 4 + 3] = alpha[i];
                }
            }
        }
    }

    Some(ExtractedBitmap { width, height, rgba })
}

/// Strip the erroneous FF D9 FF D8 header that some SWF-embedded JPEGs have.
fn strip_swf_jpeg_header(data: &[u8]) -> &[u8] {
    if data.len() >= 4 && data[0] == 0xFF && data[1] == 0xD9 && data[2] == 0xFF && data[3] == 0xD8
    {
        &data[4..]
    } else {
        data
    }
}

/// Write RGBA pixel data as a PNG file.
fn write_png(path: &Path, bitmap: &ExtractedBitmap) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, bitmap.width, bitmap.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&bitmap.rgba)?;
    Ok(())
}

fn walk_swfs(
    dir: &std::path::Path,
    source_root: &std::path::Path,
    output_root: &std::path::Path,
    extracted: &mut u32,
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
                errors.push(format!("Directory entry error in {}: {}", dir.display(), err));
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
            walk_swfs(&path, source_root, output_root, extracted, skipped, errors);
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

        // Compute relative path from source root, then build output path with .png extension
        let rel = match path.strip_prefix(source_root) {
            Ok(r) => r,
            Err(_) => {
                errors.push(format!("Cannot relativize path: {}", path.display()));
                *skipped += 1;
                continue;
            }
        };
        let output_path = output_root.join(rel).with_extension("png");

        // Read SWF file
        let swf_data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(err) => {
                errors.push(format!("WARN: skipped {} — read error: {}", path.display(), err));
                *skipped += 1;
                continue;
            }
        };

        // Extract largest bitmap
        let bitmap = match extract_largest_bitmap(&swf_data) {
            Some(b) => b,
            None => {
                errors.push(format!("WARN: skipped {} — no extractable bitmap found", path.display()));
                *skipped += 1;
                continue;
            }
        };

        // Create parent directories as needed
        if let Some(parent) = output_path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                errors.push(format!("WARN: skipped {} — cannot create output dir: {}", path.display(), err));
                *skipped += 1;
                continue;
            }
        }

        // Write PNG
        if let Err(err) = write_png(&output_path, &bitmap) {
            errors.push(format!("WARN: skipped {} — write error: {}", path.display(), err));
            *skipped += 1;
            continue;
        }

        *extracted += 1;
    }
}

fn main() {
    let args = Args::parse();

    if !args.source.is_dir() {
        eprintln!("Error: source directory does not exist: {}", args.source.display());
        std::process::exit(1);
    }

    std::fs::create_dir_all(&args.output).unwrap_or_else(|e| {
        eprintln!("Error: cannot create output directory: {e}");
        std::process::exit(1);
    });

    let mut extracted: u32 = 0;
    let mut skipped: u32 = 0;
    let mut errors: Vec<String> = Vec::new();

    walk_swfs(
        &args.source,
        &args.source,
        &args.output,
        &mut extracted,
        &mut skipped,
        &mut errors,
    );

    let total = extracted + skipped;

    for warning in &errors {
        eprintln!("{}", warning);
    }

    println!("Extracted {}/{} items ({} skipped)", extracted, total, skipped);
}
