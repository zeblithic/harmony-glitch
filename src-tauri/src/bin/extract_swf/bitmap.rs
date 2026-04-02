use flate2::read::ZlibDecoder;
use std::io::Read;
use std::path::Path;

pub struct ExtractedBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Parse a SWF file from raw bytes and extract the largest bitmap by pixel area.
pub fn extract_largest_bitmap(swf_data: &[u8]) -> Option<ExtractedBitmap> {
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

/// Returns true if the SWF data contains any bitmap tags.
pub fn has_bitmap_tags(swf_data: &[u8]) -> bool {
    let Ok(swf_buf) = swf::decompress_swf(swf_data) else {
        return false;
    };
    let Ok(swf) = swf::parse_swf(&swf_buf) else {
        return false;
    };
    swf.tags.iter().any(|tag| {
        matches!(
            tag,
            swf::Tag::DefineBitsLossless(_)
                | swf::Tag::DefineBitsJpeg2 { .. }
                | swf::Tag::DefineBitsJpeg3(_)
        )
    })
}

/// Returns true if the SWF data contains DefineShape tags.
pub fn has_shape_tags(swf_data: &[u8]) -> bool {
    let Ok(swf_buf) = swf::decompress_swf(swf_data) else {
        return false;
    };
    let Ok(swf) = swf::parse_swf(&swf_buf) else {
        return false;
    };
    swf.tags
        .iter()
        .any(|tag| matches!(tag, swf::Tag::DefineShape(_)))
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
                    let un = |c: u8| -> u8 { ((c as u16 * 255) / a as u16).min(255) as u8 };
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
pub fn write_png(
    path: &Path,
    bitmap: &ExtractedBitmap,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, bitmap.width, bitmap.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&bitmap.rgba)?;
    Ok(())
}
