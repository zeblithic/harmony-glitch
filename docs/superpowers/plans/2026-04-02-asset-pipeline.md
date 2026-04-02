# Asset Pipeline Tooling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build-time tooling to extract bitmaps from Glitch SWF files and pack them into sprite sheet atlases for PixiJS.

**Architecture:** Two composable CLI tools — a Rust SWF bitmap extractor (using Ruffle's `swf` crate) and a Node.js sprite sheet packer (using `sharp`). The packer outputs TexturePacker JSON Hash format, consumed natively by the existing `SpriteManager`. The `SpriteManager` gains atlas loading so packed sprites and individual PNGs coexist.

**Tech Stack:** Rust (`swf`, `png`, `flate2`, `clap`), Node.js (`sharp`), PixiJS v8, vitest

**Spec:** `docs/superpowers/specs/2026-04-02-asset-pipeline-design.md`

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `src-tauri/src/bin/extract_swf.rs` | Rust CLI binary — parses SWFs, extracts largest bitmap, writes PNGs |
| `tools/asset-pipeline/pack.mjs` | Node.js CLI — packs PNGs into sprite sheet atlases with JSON metadata |
| `tools/asset-pipeline/pack.test.mjs` | Packer unit tests (vitest) |

### Modified Files

| File | Change |
|------|--------|
| `src-tauri/Cargo.toml` | Add `[[bin]]` target, `swf`, `png`, `flate2`, `clap` deps |
| `src/lib/engine/sprites.ts` | Add atlas loading in `init()`, check atlas textures in `tryLoadEntityTexture`/`tryLoadItemTexture` |
| `src/lib/engine/sprites.test.ts` | Add atlas loading tests |
| `package.json` | Add `sharp` dev dep, pipeline npm scripts |
| `.gitignore` | Add `tools/asset-pipeline/extracted/` |
| `vitest.config.ts` | Add `tools/` to test include paths |

---

### Task 1: Set Up extract-swf Binary Target

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/bin/extract_swf.rs`

This task adds the Rust binary target with dependencies and a minimal main that parses CLI args. No SWF logic yet — just the skeleton.

- [ ] **Step 1: Add dependencies and binary target to Cargo.toml**

Add to `src-tauri/Cargo.toml`:

```toml
[[bin]]
name = "extract-swf"
path = "src/bin/extract_swf.rs"

[dependencies]
# ... existing deps ...
swf = "0.2"
png = "0.17"
flate2 = "1"
clap = { version = "4", features = ["derive"] }
```

Note: `flate2` is needed for decompressing DefineBitsLossless pixel data (zlib). The `swf` crate handles SWF-level CWS decompression itself.

- [ ] **Step 2: Create minimal extract_swf.rs with CLI args**

Create `src-tauri/src/bin/extract_swf.rs`:

```rust
use clap::Parser;
use std::path::PathBuf;

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

    println!("Source: {}", args.source.display());
    println!("Output: {}", args.output.display());
    println!("(extraction not yet implemented)");
}
```

- [ ] **Step 3: Verify the binary builds and runs**

Run from repo root:

```bash
cd src-tauri && cargo build --bin extract-swf
```

Expected: builds successfully.

```bash
cd src-tauri && cargo run --bin extract-swf -- --source /tmp --output /tmp/out
```

Expected: prints source/output paths and "(extraction not yet implemented)".

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/bin/extract_swf.rs
git commit -m "feat: add extract-swf binary target with CLI skeleton"
```

---

### Task 2: SWF Bitmap Extraction Core

**Files:**
- Modify: `src-tauri/src/bin/extract_swf.rs`

This task adds the core logic: parse a single SWF file, find bitmap tags, decode the largest bitmap to RGBA pixels, and write a PNG. Tests use a real SWF fixture from the Glitch art repos.

- [ ] **Step 1: Add bitmap extraction function**

Add to `src-tauri/src/bin/extract_swf.rs`, above `main()`:

```rust
use std::io::Cursor;

/// Represents a decoded bitmap from a SWF file.
struct ExtractedBitmap {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Parse a SWF file and extract the largest embedded bitmap as RGBA pixels.
fn extract_largest_bitmap(swf_data: &[u8]) -> Option<ExtractedBitmap> {
    let swf_buf = swf::decompress_swf(swf_data).ok()?;
    let swf = swf::parse_swf(&swf_buf).ok()?;

    let mut best: Option<ExtractedBitmap> = None;

    for tag in &swf.tags {
        let bitmap = match tag {
            swf::Tag::DefineBitsLossless(b) | swf::Tag::DefineBitsLossless2(b) => {
                decode_lossless(b)
            }
            swf::Tag::DefineBitsJpeg2 { id: _, jpeg_data } => {
                decode_jpeg(jpeg_data, None)
            }
            swf::Tag::DefineBitsJpeg3(j) => {
                decode_jpeg(&j.data, Some(&j.alpha_data))
            }
            _ => None,
        };

        if let Some(bmp) = bitmap {
            let area = bmp.width as u64 * bmp.height as u64;
            let best_area = best.as_ref().map_or(0, |b| b.width as u64 * b.height as u64);
            if area > best_area {
                best = Some(bmp);
            }
        }
    }

    best
}
```

- [ ] **Step 2: Add DefineBitsLossless decoder**

Add to `src-tauri/src/bin/extract_swf.rs`:

```rust
fn decode_lossless(bitmap: &swf::DefineBitsLossless) -> Option<ExtractedBitmap> {
    let w = bitmap.width as u32;
    let h = bitmap.height as u32;

    // Decompress zlib pixel data
    let mut decoder = flate2::read::ZlibDecoder::new(&bitmap.data[..]);
    let mut decompressed = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut decompressed).ok()?;

    let mut rgba = Vec::with_capacity((w * h * 4) as usize);

    match bitmap.format {
        swf::BitmapFormat::Rgb32 => {
            // Each row is padded to 4-byte alignment (but at 4 bytes/pixel it already is)
            for pixel in decompressed.chunks_exact(4) {
                if bitmap.version == 2 {
                    // DefineBitsLossless2: ARGB pre-multiplied alpha
                    let a = pixel[0];
                    if a == 0 {
                        rgba.extend_from_slice(&[0, 0, 0, 0]);
                    } else {
                        // Un-premultiply: channel = channel * 255 / alpha
                        let r = ((pixel[1] as u16 * 255) / a as u16).min(255) as u8;
                        let g = ((pixel[2] as u16 * 255) / a as u16).min(255) as u8;
                        let b = ((pixel[3] as u16 * 255) / a as u16).min(255) as u8;
                        rgba.extend_from_slice(&[r, g, b, a]);
                    }
                } else {
                    // DefineBitsLossless: xRGB (no alpha, x is padding)
                    rgba.extend_from_slice(&[pixel[1], pixel[2], pixel[3], 255]);
                }
            }
        }
        swf::BitmapFormat::ColorMap8 { num_colors } => {
            // Color table followed by pixel indices
            let palette_size = (num_colors as usize + 1) * if bitmap.version == 2 { 4 } else { 3 };
            if decompressed.len() < palette_size {
                return None;
            }
            let (palette_bytes, indices) = decompressed.split_at(palette_size);
            let bytes_per_color = if bitmap.version == 2 { 4 } else { 3 };
            let palette: Vec<[u8; 4]> = palette_bytes
                .chunks_exact(bytes_per_color)
                .map(|c| {
                    if bitmap.version == 2 {
                        [c[0], c[1], c[2], c[3]] // RGBA
                    } else {
                        [c[0], c[1], c[2], 255] // RGB + opaque
                    }
                })
                .collect();

            // Rows are padded to 4-byte alignment
            let row_stride = ((w as usize) + 3) & !3;
            for row in 0..h as usize {
                let row_start = row * row_stride;
                for col in 0..w as usize {
                    let idx = indices.get(row_start + col).copied().unwrap_or(0) as usize;
                    let color = palette.get(idx).copied().unwrap_or([0, 0, 0, 255]);
                    rgba.extend_from_slice(&color);
                }
            }
        }
        swf::BitmapFormat::Rgb15 => {
            // Rare format — skip for now
            return None;
        }
    }

    Some(ExtractedBitmap { width: w, height: h, rgba })
}
```

- [ ] **Step 3: Add JPEG decoder**

Add to `src-tauri/src/bin/extract_swf.rs`:

```rust
fn decode_jpeg(jpeg_data: &[u8], alpha_data: Option<&[u8]>) -> Option<ExtractedBitmap> {
    // SWF JPEG data may have erroneous headers — strip them
    let data = strip_swf_jpeg_header(jpeg_data);

    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(data));
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let w = info.width as u32;
    let h = info.height as u32;

    let mut rgba = Vec::with_capacity((w * h * 4) as usize);

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
        let mut decoder = flate2::read::ZlibDecoder::new(alpha_compressed);
        let mut alpha = Vec::new();
        std::io::Read::read_to_end(&mut decoder, &mut alpha).ok()?;
        for (i, &a) in alpha.iter().enumerate() {
            if let Some(pixel) = rgba.get_mut(i * 4 + 3) {
                *pixel = a;
            }
        }
    }

    Some(ExtractedBitmap { width: w, height: h, rgba })
}

/// Strip erroneous SWF JPEG header (FF D9 FF D8 sequence at start).
fn strip_swf_jpeg_header(data: &[u8]) -> &[u8] {
    if data.len() >= 4 && data[0] == 0xFF && data[1] == 0xD9 && data[2] == 0xFF && data[3] == 0xD8 {
        &data[4..]
    } else {
        data
    }
}
```

Also add `jpeg-decoder` to `src-tauri/Cargo.toml` dependencies:

```toml
jpeg-decoder = "0.3"
```

- [ ] **Step 4: Add PNG writing function**

Add to `src-tauri/src/bin/extract_swf.rs`:

```rust
fn write_png(path: &std::path::Path, bitmap: &ExtractedBitmap) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let w = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, bitmap.width, bitmap.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&bitmap.rgba)?;
    Ok(())
}
```

- [ ] **Step 5: Test with a real Glitch SWF file**

Run from repo root:

```bash
cd src-tauri && cargo build --bin extract-swf
```

Expected: builds successfully.

Create a quick manual test by running the extraction on a single known SWF:

```bash
mkdir -p /tmp/swf-test-out
cd src-tauri && cargo run --bin extract-swf -- \
  --source ~/work/tinyspeck/glitch-items/food/apple \
  --output /tmp/swf-test-out
```

This will fail because `main()` doesn't call the extraction yet — that's Task 3. But the build confirms the bitmap decoding code compiles.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/bin/extract_swf.rs
git commit -m "feat: add SWF bitmap extraction core (lossless + JPEG decoding)"
```

---

### Task 3: Directory Walking & PNG Output

**Files:**
- Modify: `src-tauri/src/bin/extract_swf.rs`

This task wires the extraction into `main()` with recursive directory traversal and summary reporting.

- [ ] **Step 1: Add directory walking and extraction loop to main()**

Replace the `main()` function in `src-tauri/src/bin/extract_swf.rs`:

```rust
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

    let mut extracted = 0u32;
    let mut skipped = 0u32;
    let mut errors: Vec<String> = Vec::new();

    walk_swfs(&args.source, &args.source, &args.output, &mut extracted, &mut skipped, &mut errors);

    for err in &errors {
        eprintln!("  WARN: {err}");
    }
    let total = extracted + skipped;
    println!("Extracted {extracted}/{total} items ({skipped} skipped)");
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
        Err(e) => {
            errors.push(format!("{}: {e}", dir.display()));
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_swfs(&path, source_root, output_root, extracted, skipped, errors);
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) != Some("swf") {
            continue;
        }

        // Compute output path preserving directory structure relative to source root
        let rel = path.strip_prefix(source_root).unwrap_or(&path);
        let out_path = output_root.join(rel).with_extension("png");

        if let Some(parent) = out_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match std::fs::read(&path) {
            Ok(data) => match extract_largest_bitmap(&data) {
                Some(bitmap) => match write_png(&out_path, &bitmap) {
                    Ok(()) => {
                        *extracted += 1;
                    }
                    Err(e) => {
                        errors.push(format!("{}: write error: {e}", rel.display()));
                        *skipped += 1;
                    }
                },
                None => {
                    errors.push(format!("{}: no bitmaps found", rel.display()));
                    *skipped += 1;
                }
            },
            Err(e) => {
                errors.push(format!("{}: read error: {e}", rel.display()));
                *skipped += 1;
            }
        }
    }
}
```

- [ ] **Step 2: Build and test with real Glitch items**

```bash
cd src-tauri && cargo build --bin extract-swf
```

Expected: builds successfully.

```bash
cd src-tauri && cargo run --bin extract-swf -- \
  --source ~/work/tinyspeck/glitch-items/food \
  --output /tmp/swf-test-out/food
```

Expected: extracts PNGs for most food items. Some may skip (no bitmaps or parse errors). Output should look like:

```
Extracted 150/193 items (43 skipped)
```

Verify a few outputs exist and are valid PNGs:

```bash
file /tmp/swf-test-out/food/apple/apple.png
```

Expected: `PNG image data, <width> x <height>, 8-bit/color RGBA, non-interlaced`

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/bin/extract_swf.rs
git commit -m "feat: wire extract-swf with directory walking and PNG output"
```

---

### Task 4: Node.js Sprite Packer — Atlas Mode

**Files:**
- Create: `tools/asset-pipeline/pack.mjs`
- Create: `tools/asset-pipeline/pack.test.mjs`
- Modify: `package.json` (add `sharp` dev dep)
- Modify: `vitest.config.ts` (include `tools/` in test paths)

This task builds the sprite sheet packer in atlas mode: takes a directory of PNGs, packs them into a sprite sheet, and outputs TexturePacker JSON Hash metadata.

- [ ] **Step 1: Install sharp**

```bash
npm install --save-dev sharp
```

- [ ] **Step 2: Create the packer with atlas mode**

Create `tools/asset-pipeline/pack.mjs`:

```javascript
#!/usr/bin/env node
import { readdir, mkdir, writeFile } from 'node:fs/promises';
import { join, basename, extname, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import sharp from 'sharp';

const { values: args } = parseArgs({
  options: {
    input: { type: 'string' },
    output: { type: 'string' },
    name: { type: 'string' },
    animation: { type: 'boolean', default: false },
  },
});

if (!args.input || !args.output || !args.name) {
  console.error('Usage: node pack.mjs --input <dir> --output <dir> --name <name> [--animation]');
  process.exit(1);
}

const inputDir = resolve(args.input);
const outputDir = resolve(args.output);
const name = args.name;
const animationMode = args.animation ?? false;

await mkdir(outputDir, { recursive: true });

// Collect all PNGs (recursive)
const pngs = await collectPngs(inputDir);
if (pngs.length === 0) {
  console.error(`No PNG files found in ${inputDir}`);
  process.exit(1);
}

// Read image metadata
const images = [];
for (const pngPath of pngs) {
  try {
    const meta = await sharp(pngPath).metadata();
    images.push({
      path: pngPath,
      name: basename(pngPath, extname(pngPath)),
      width: meta.width,
      height: meta.height,
    });
  } catch (err) {
    console.warn(`WARN: skipping ${pngPath}: ${err.message}`);
  }
}

if (images.length === 0) {
  console.error('No valid PNG files could be read');
  process.exit(1);
}

// Sort by height descending for shelf packing
images.sort((a, b) => b.height - a.height || a.name.localeCompare(b.name));

// Shelf packing
const { frames, sheetWidth, sheetHeight } = shelfPack(images);

// Composite onto output image
const composites = [];
for (const frame of frames) {
  composites.push({
    input: frame.path,
    left: frame.x,
    top: frame.y,
  });
}

const sheet = sharp({
  create: {
    width: sheetWidth,
    height: sheetHeight,
    channels: 4,
    background: { r: 0, g: 0, b: 0, alpha: 0 },
  },
})
  .composite(composites)
  .png();

const imagePath = join(outputDir, `${name}.png`);
const jsonPath = join(outputDir, `${name}.json`);

await sheet.toFile(imagePath);

// Build TexturePacker JSON Hash
const json = buildJson(frames, name, sheetWidth, sheetHeight, animationMode);
await writeFile(jsonPath, JSON.stringify(json, null, 2) + '\n');

console.log(`Packed ${frames.length} sprites → ${name}.png (${sheetWidth}x${sheetHeight}) + ${name}.json`);

// --- Functions ---

async function collectPngs(dir) {
  const results = [];
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...(await collectPngs(full)));
    } else if (entry.name.endsWith('.png')) {
      results.push(full);
    }
  }
  return results;
}

/**
 * Shelf-pack images into rows. Returns frames with positions and sheet dimensions.
 */
function shelfPack(images) {
  // Try increasingly large power-of-two widths until everything fits
  for (let widthExp = 6; widthExp <= 14; widthExp++) {
    const maxWidth = 1 << widthExp; // 64, 128, ..., 16384
    const result = tryShelfPack(images, maxWidth);
    if (result) return result;
  }
  // Fallback: unlimited width
  return tryShelfPack(images, 16384) ?? { frames: [], sheetWidth: 0, sheetHeight: 0 };
}

function tryShelfPack(images, maxWidth) {
  const frames = [];
  let x = 0;
  let y = 0;
  let rowHeight = 0;
  let maxX = 0;

  for (const img of images) {
    if (img.width > maxWidth) return null; // Image wider than sheet

    if (x + img.width > maxWidth) {
      // New row
      y += rowHeight;
      x = 0;
      rowHeight = 0;
    }

    frames.push({
      path: img.path,
      name: img.name,
      x,
      y,
      width: img.width,
      height: img.height,
    });

    x += img.width;
    if (x > maxX) maxX = x;
    if (img.height > rowHeight) rowHeight = img.height;
  }

  const totalHeight = y + rowHeight;
  const sheetWidth = nextPowerOfTwo(maxX);
  const sheetHeight = nextPowerOfTwo(totalHeight);

  return { frames, sheetWidth, sheetHeight };
}

function nextPowerOfTwo(n) {
  if (n <= 0) return 1;
  let p = 1;
  while (p < n) p <<= 1;
  return p;
}

function buildJson(frames, name, sheetWidth, sheetHeight, animationMode) {
  const json = {
    frames: {},
    meta: {
      image: `${name}.png`,
      format: 'RGBA8888',
      size: { w: sheetWidth, h: sheetHeight },
      scale: 1,
    },
  };

  for (const f of frames) {
    json.frames[f.name] = {
      frame: { x: f.x, y: f.y, w: f.width, h: f.height },
    };
  }

  if (animationMode) {
    json.animations = groupAnimations(frames);
  }

  return json;
}

/**
 * Group frames into animations by name prefix.
 * "walk_0", "walk_1" → animations.walk = ["walk_0", "walk_1"]
 * The prefix before the last underscore is the animation name.
 */
function groupAnimations(frames) {
  const groups = {};
  for (const f of frames) {
    const lastUnderscore = f.name.lastIndexOf('_');
    if (lastUnderscore === -1) continue;
    const animName = f.name.substring(0, lastUnderscore);
    const index = parseInt(f.name.substring(lastUnderscore + 1), 10);
    if (isNaN(index)) continue;
    if (!groups[animName]) groups[animName] = [];
    groups[animName].push({ name: f.name, index });
  }
  const animations = {};
  for (const [animName, entries] of Object.entries(groups)) {
    entries.sort((a, b) => a.index - b.index);
    animations[animName] = entries.map((e) => e.name);
  }
  return animations;
}

export { shelfPack, nextPowerOfTwo, buildJson, groupAnimations };
```

- [ ] **Step 3: Write packer tests**

Create `tools/asset-pipeline/pack.test.mjs`:

```javascript
import { describe, it, expect } from 'vitest';
import { shelfPack, nextPowerOfTwo, buildJson, groupAnimations } from './pack.mjs';

describe('nextPowerOfTwo', () => {
  it('returns 1 for 0', () => {
    expect(nextPowerOfTwo(0)).toBe(1);
  });

  it('returns exact power of two when input is power of two', () => {
    expect(nextPowerOfTwo(64)).toBe(64);
    expect(nextPowerOfTwo(256)).toBe(256);
  });

  it('rounds up to next power of two', () => {
    expect(nextPowerOfTwo(65)).toBe(128);
    expect(nextPowerOfTwo(100)).toBe(128);
    expect(nextPowerOfTwo(300)).toBe(512);
  });
});

describe('shelfPack', () => {
  it('packs images into power-of-two dimensions', () => {
    const images = [
      { path: 'a.png', name: 'a', width: 32, height: 32 },
      { path: 'b.png', name: 'b', width: 32, height: 32 },
      { path: 'c.png', name: 'c', width: 32, height: 32 },
    ];
    const result = shelfPack(images);
    expect(result.sheetWidth).toBe(nextPowerOfTwo(result.sheetWidth));
    expect(result.sheetHeight).toBe(nextPowerOfTwo(result.sheetHeight));
  });

  it('places frames without overlap', () => {
    const images = [
      { path: 'a.png', name: 'a', width: 50, height: 40 },
      { path: 'b.png', name: 'b', width: 30, height: 30 },
      { path: 'c.png', name: 'c', width: 60, height: 20 },
    ];
    const { frames } = shelfPack(images);
    // Check no two frames overlap
    for (let i = 0; i < frames.length; i++) {
      for (let j = i + 1; j < frames.length; j++) {
        const a = frames[i];
        const b = frames[j];
        const overlap =
          a.x < b.x + b.width &&
          a.x + a.width > b.x &&
          a.y < b.y + b.height &&
          a.y + a.height > b.y;
        expect(overlap, `frames ${a.name} and ${b.name} overlap`).toBe(false);
      }
    }
  });

  it('handles single image', () => {
    const images = [{ path: 'a.png', name: 'a', width: 16, height: 16 }];
    const result = shelfPack(images);
    expect(result.frames).toHaveLength(1);
    expect(result.frames[0].x).toBe(0);
    expect(result.frames[0].y).toBe(0);
    expect(result.sheetWidth).toBe(16);
    expect(result.sheetHeight).toBe(16);
  });
});

describe('buildJson', () => {
  it('produces TexturePacker JSON Hash format', () => {
    const frames = [
      { path: 'a.png', name: 'apple', x: 0, y: 0, width: 64, height: 64 },
      { path: 'b.png', name: 'cherry', x: 64, y: 0, width: 16, height: 16 },
    ];
    const json = buildJson(frames, 'items', 128, 64, false);
    expect(json.frames.apple.frame).toEqual({ x: 0, y: 0, w: 64, h: 64 });
    expect(json.frames.cherry.frame).toEqual({ x: 64, y: 0, w: 16, h: 16 });
    expect(json.meta.image).toBe('items.png');
    expect(json.meta.size).toEqual({ w: 128, h: 64 });
    expect(json.meta.format).toBe('RGBA8888');
    expect(json.meta.scale).toBe(1);
    expect(json.animations).toBeUndefined();
  });
});

describe('groupAnimations', () => {
  it('groups frames by name prefix', () => {
    const frames = [
      { name: 'walk_0' },
      { name: 'walk_1' },
      { name: 'walk_2' },
      { name: 'idle_0' },
      { name: 'idle_1' },
    ];
    const anims = groupAnimations(frames);
    expect(anims.walk).toEqual(['walk_0', 'walk_1', 'walk_2']);
    expect(anims.idle).toEqual(['idle_0', 'idle_1']);
  });

  it('sorts frames numerically within groups', () => {
    const frames = [
      { name: 'run_2' },
      { name: 'run_0' },
      { name: 'run_1' },
    ];
    const anims = groupAnimations(frames);
    expect(anims.run).toEqual(['run_0', 'run_1', 'run_2']);
  });

  it('skips frames without underscore-number suffix', () => {
    const frames = [
      { name: 'background' },
      { name: 'walk_0' },
    ];
    const anims = groupAnimations(frames);
    expect(anims.walk).toEqual(['walk_0']);
    expect(anims.background).toBeUndefined();
  });

  it('builds animations block in buildJson when animation mode is on', () => {
    const frames = [
      { path: 'a.png', name: 'walk_0', x: 0, y: 0, width: 30, height: 60 },
      { path: 'b.png', name: 'walk_1', x: 30, y: 0, width: 30, height: 60 },
    ];
    const json = buildJson(frames, 'avatar', 64, 64, true);
    expect(json.animations.walk).toEqual(['walk_0', 'walk_1']);
  });
});
```

- [ ] **Step 4: Update vitest config to include tools/**

Modify `vitest.config.ts` to include the tools directory in test discovery:

```typescript
import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';

export default defineConfig({
  plugins: [svelte({ hot: false }), svelteTesting()],
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts', 'tools/**/*.test.mjs'],
  },
});
```

- [ ] **Step 5: Run tests**

```bash
npx vitest run tools/asset-pipeline/pack.test.mjs
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add tools/asset-pipeline/pack.mjs tools/asset-pipeline/pack.test.mjs vitest.config.ts package.json package-lock.json
git commit -m "feat: add sprite sheet packer with atlas and animation modes"
```

---

### Task 5: SpriteManager Atlas Loading

**Files:**
- Modify: `src/lib/engine/sprites.ts`
- Modify: `src/lib/engine/sprites.test.ts`

This task teaches `SpriteManager` to load atlas sprite sheets so that packed sprites and individual PNGs coexist. PixiJS's `Assets.load()` for a `.json` spritesheet automatically registers all frame textures by name.

- [ ] **Step 1: Write failing tests for atlas loading**

Add to `src/lib/engine/sprites.test.ts`, inside the top-level `describe('SpriteManager', ...)` block:

```typescript
describe('atlas loading', () => {
  it('makes item textures available after atlas loads', async () => {
    const { Assets } = await import('pixi.js');
    const mockAtlas = {
      textures: {
        apple: { width: 64, height: 64 },
        cherry: { width: 16, height: 16 },
      },
    };
    vi.mocked(Assets.load).mockImplementation(async (path: string) => {
      if (path === 'sprites/items/items.json') return mockAtlas;
      throw new Error('not found');
    });

    await manager.loadAtlas('items', 'sprites/items/items.json');

    expect(manager.hasItemTexture('apple')).toBe(true);
    expect(manager.hasItemTexture('cherry')).toBe(true);
    expect(manager.hasItemTexture('nonexistent')).toBe(false);
  });

  it('individual PNGs still work when no atlas exists', async () => {
    const { Assets } = await import('pixi.js');
    vi.mocked(Assets.load).mockRejectedValue(new Error('not found'));

    await manager.loadAtlas('items', 'sprites/items/items.json');

    // No atlas loaded — hasItemTexture returns false
    expect(manager.hasItemTexture('apple')).toBe(false);
  });

  it('entity atlas textures are available', async () => {
    const { Assets } = await import('pixi.js');
    const mockAtlas = {
      textures: {
        tree_fruit: { width: 60, height: 80 },
      },
    };
    vi.mocked(Assets.load).mockImplementation(async (path: string) => {
      if (path === 'sprites/entities/entities.json') return mockAtlas;
      throw new Error('not found');
    });

    await manager.loadAtlas('entities', 'sprites/entities/entities.json');

    expect(manager.hasEntityTexture('tree_fruit')).toBe(true);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
npx vitest run src/lib/engine/sprites.test.ts
```

Expected: FAIL — `loadAtlas` method does not exist on `SpriteManager`.

- [ ] **Step 3: Add loadAtlas method to SpriteManager**

Add to `src/lib/engine/sprites.ts`, in the `SpriteManager` class, after `init()`:

```typescript
async loadAtlas(category: 'items' | 'entities', jsonPath: string): Promise<void> {
  try {
    const sheet = await Assets.load(jsonPath);
    if (sheet?.textures) {
      const prefix = category === 'items' ? 'item' : 'entity';
      for (const [name, texture] of Object.entries(sheet.textures)) {
        this.textureCache.set(`${prefix}:${name}`, texture as Texture);
      }
    }
  } catch {
    // Atlas not available — individual PNGs will be used as fallback
  }
}
```

- [ ] **Step 4: Call loadAtlas in init()**

Modify the `init()` method in `src/lib/engine/sprites.ts` to also attempt loading known atlases:

```typescript
async init(): Promise<void> {
  try {
    this.avatarSheet = await Assets.load('sprites/avatar/avatar.json');
  } catch {
    console.warn('[SpriteManager] Avatar spritesheet not found, using fallback');
  }

  // Load atlases if they exist — individual PNGs still work as fallback
  await this.loadAtlas('items', 'sprites/items/items.json');
  await this.loadAtlas('entities', 'sprites/entities/entities.json');
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
npx vitest run src/lib/engine/sprites.test.ts
```

Expected: all tests pass, including the new atlas loading tests.

- [ ] **Step 6: Run all frontend tests**

```bash
npx vitest run
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/lib/engine/sprites.ts src/lib/engine/sprites.test.ts
git commit -m "feat: add atlas sprite sheet loading to SpriteManager"
```

---

### Task 6: Pipeline Orchestration

**Files:**
- Modify: `package.json` (add npm scripts)
- Modify: `.gitignore` (add extracted/ directory)

- [ ] **Step 1: Add pipeline npm scripts to package.json**

Add to the `"scripts"` section in `package.json`:

```json
"extract-items": "cargo run --manifest-path src-tauri/Cargo.toml --bin extract-swf -- --source ${GLITCH_ART_PATH:-$HOME/work/tinyspeck/glitch-items} --output tools/asset-pipeline/extracted/items",
"pack-items": "node tools/asset-pipeline/pack.mjs --input tools/asset-pipeline/extracted/items --output assets/sprites/items --name items",
"pipeline-items": "npm run extract-items && npm run pack-items"
```

- [ ] **Step 2: Add extracted/ to .gitignore**

Add to `.gitignore`:

```
# Asset pipeline intermediate files
tools/asset-pipeline/extracted/
```

- [ ] **Step 3: Verify extract-items runs**

```bash
npm run extract-items
```

Expected: extracts PNGs from `~/work/tinyspeck/glitch-items/` into `tools/asset-pipeline/extracted/items/`. Reports summary like `Extracted N/M items (K skipped)`.

- [ ] **Step 4: Verify pack-items runs**

```bash
npm run pack-items
```

Expected: packs extracted PNGs into `assets/sprites/items/items.png` + `assets/sprites/items/items.json`.

- [ ] **Step 5: Verify pipeline-items runs end-to-end**

```bash
npm run pipeline-items
```

Expected: runs both steps in sequence, produces final sprite sheet.

- [ ] **Step 6: Commit**

```bash
git add package.json .gitignore
git commit -m "feat: add pipeline orchestration npm scripts and gitignore"
```

---

### Task 7: End-to-End Validation

**Files:** None modified — this is a validation task.

- [ ] **Step 1: Run the full pipeline**

```bash
npm run pipeline-items
```

Expected: successful extraction and packing.

- [ ] **Step 2: Inspect the generated atlas**

```bash
file assets/sprites/items/items.png
cat assets/sprites/items/items.json | head -20
```

Expected: `items.png` is a valid PNG with power-of-two dimensions. `items.json` has `frames` with entries for extracted items, and `meta` with correct `image`, `size`, `format`, `scale` fields.

- [ ] **Step 3: Run all Rust tests**

```bash
cd src-tauri && cargo test
```

Expected: all existing tests pass. No Rust game logic was changed.

- [ ] **Step 4: Run all frontend tests**

```bash
npx vitest run
```

Expected: all tests pass, including new packer and atlas tests.

- [ ] **Step 5: Commit generated atlas (if it looks correct)**

```bash
git add assets/sprites/items/items.png assets/sprites/items/items.json
git commit -m "feat: add pipeline-generated item sprite atlas from Glitch art"
```

Note: this step depends on the extracted art looking correct. If it doesn't, this commit can be skipped — the tooling is still valuable without the generated output.

- [ ] **Step 6: Run clippy**

```bash
cd src-tauri && cargo clippy
```

Expected: no warnings.
