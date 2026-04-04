#!/usr/bin/env node
/**
 * Avatar extraction pipeline — extracts Glitch avatar SWF art into
 * PixiJS-compatible sprite sheets with trim metadata for layer composition.
 *
 * Pipeline: SWF → swf-wrapper → ruffle exporter → trim → sprite sheet + manifest
 *
 * Components are rendered at the base body's stage size so all layers share
 * the same coordinate space (544×1013 at 8x). Per-frame container positions
 * from Avatar.swf are used to place components at the correct body position
 * via PixiJS trim metadata (spriteSourceSize + sourceSize).
 *
 * Usage:
 *   node extract.mjs                          # Extract all categories
 *   node extract.mjs --category eyes          # Single category
 *   node extract.mjs --item eyes/eyes_01      # Single item
 *   node extract.mjs --limit 5                # First 5 items per category
 *   node extract.mjs --scale 4                # Render at 4x (default: 8)
 *   node extract.mjs --base-only              # Extract base body only
 */

import { parseArgs } from 'node:util';
import { execFileSync } from 'node:child_process';
import { readdir, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import sharp from 'sharp';

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const REPO_ROOT = path.resolve(import.meta.dirname, '../..');
const TAURI_DIR = path.join(REPO_ROOT, 'src-tauri');
const SWF_WRAPPER = path.join(TAURI_DIR, 'target/release/swf-wrapper');
const RUFFLE_EXPORTER = process.env.RUFFLE_EXPORTER
  || path.resolve(REPO_ROOT, '../../ruffle-rs/ruffle/target/release/exporter');
const GLITCH_AVATARS = process.env.GLITCH_AVATARS
  || path.resolve(REPO_ROOT, '../../tinyspeck/glitch-avatars');
const OUTPUT_DIR = path.join(REPO_ROOT, 'assets/sprites/avatar');
const TEMP_DIR = path.join(REPO_ROOT, '.avatar-pipeline-tmp');
const TRANSFORMS_PATH = path.join(import.meta.dirname, 'avatar-transforms.json');
const BODY_SWF = path.join(GLITCH_AVATARS, 'base_avatar/Avatar.swf');

// All Glitch avatar SWFs share this 1233-frame timeline.
// We sample key frames for each animation state.
const ANIMATIONS = {
  idle:    { start: 804, count: 46, step: 6 },   // idle1: ~8 frames
  walking: { start: 0,   count: 24, step: 3 },   // walk1x: 8 frames
  jumping: { start: 157, count: 13, step: 2 },   // jumpUp_lift: ~7 frames
  falling: { start: 170, count: 12, step: 2 },   // jumpUp_fall: 6 frames
};

const VANITY_CATEGORIES = ['eyes', 'ears', 'nose', 'mouth', 'hair'];
const WARDROBE_CATEGORIES = ['hat', 'coat', 'shirt', 'pants', 'dress', 'skirt', 'shoes', 'bracelet'];

// Categories that map to specific container names in avatar-transforms.json
const CATEGORY_CONTAINERS = {
  eyes: 'eyes', ears: 'ears', nose: 'nose', mouth: 'mouth',
  hair: 'hair', hat: 'hat', shirt: 'shirt', pants: 'pants',
  dress: 'dress', coat: 'coat', shoes: 'shoes', bracelet: 'bracelet',
  skirt: 'skirt',
};

// Threshold (pixels) for detecting content "at origin" vs "already positioned"
const ORIGIN_THRESHOLD = 16;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

const { values: opts } = parseArgs({
  options: {
    category: { type: 'string' },
    item: { type: 'string' },
    limit: { type: 'string', default: '0' },
    scale: { type: 'string', default: '8' },
    'base-only': { type: 'boolean', default: false },
    help: { type: 'boolean', default: false },
  },
});

if (opts.help) {
  console.log('Usage: node extract.mjs [--category <cat>] [--item <cat/name>] [--limit N] [--scale N] [--base-only]');
  process.exit(0);
}

const SCALE = parseFloat(opts.scale);
const LIMIT = parseInt(opts.limit, 10);

// Body frame dimensions at current scale (used as sourceSize for all layers)
const BODY_FRAME_W = Math.round(68 * SCALE);
const BODY_FRAME_H = Math.round(126.65 * SCALE);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Run a command and return stdout. */
function run(cmd, args, opts = {}) {
  return execFileSync(cmd, args, { encoding: 'utf-8', maxBuffer: 10 * 1024 * 1024, ...opts });
}

/**
 * Find content bounds in a rendered frame (non-white, non-transparent pixels).
 * Returns { x, y, w, h } or null if no content.
 */
async function findContentBounds(inputPath) {
  const { data, info } = await sharp(inputPath)
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });

  let minX = info.width, minY = info.height, maxX = 0, maxY = 0;
  for (let y = 0; y < info.height; y++) {
    for (let x = 0; x < info.width; x++) {
      const idx = (y * info.width + x) * 4;
      const a = data[idx + 3];
      const r = data[idx], g = data[idx + 1], b = data[idx + 2];
      if (a > 10 && (r < 245 || g < 245 || b < 245)) {
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
      }
    }
  }
  if (maxX < minX) return null;
  return { x: minX, y: minY, w: maxX - minX + 1, h: maxY - minY + 1 };
}

/**
 * Trim a rendered frame: remove white background, crop to content bounds,
 * add padding, and return the trimmed buffer + trim metadata.
 *
 * @param {string} inputPath - Path to raw rendered PNG
 * @param {string} outputPath - Path to write trimmed PNG
 * @param {{x: number, y: number}|null} containerPos - Container position (stage coords × scale).
 *   If the content is at origin, this offset is applied.
 * @param {number} padding - Transparent padding around content
 * @returns {{ width, height, spriteSourceSize: {x,y,w,h} } | null}
 */
async function trimWithMetadata(inputPath, outputPath, containerPos, padding = 2) {
  const bounds = await findContentBounds(inputPath);
  if (!bounds) return null;

  // Determine if content is at the origin (needs container offset) or already positioned
  const atOrigin = bounds.x < ORIGIN_THRESHOLD && bounds.y < ORIGIN_THRESHOLD;

  // Read the image and make white pixels transparent
  const { data, info } = await sharp(inputPath)
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  const buf = Buffer.from(data);
  for (let i = 0; i < buf.length; i += 4) {
    if (buf[i] > 245 && buf[i + 1] > 245 && buf[i + 2] > 245) {
      buf[i + 3] = 0;
    }
  }

  // Extract just the content region + padding
  const extractX = Math.max(0, bounds.x - padding);
  const extractY = Math.max(0, bounds.y - padding);
  const extractW = Math.min(info.width - extractX, bounds.w + 2 * padding);
  const extractH = Math.min(info.height - extractY, bounds.h + 2 * padding);

  const contentImg = await sharp(buf, {
    raw: { width: info.width, height: info.height, channels: 4 },
  })
    .extract({ left: extractX, top: extractY, width: extractW, height: extractH })
    .png()
    .toBuffer();

  const contentMeta = await sharp(contentImg).metadata();
  await sharp(contentImg).toFile(outputPath);

  // Compute spriteSourceSize: where this content sits within the source frame
  let ssX, ssY;
  if (atOrigin && containerPos) {
    // Content at origin → place at container position
    ssX = Math.round(containerPos.x * SCALE) - padding;
    ssY = Math.round(containerPos.y * SCALE) - padding;
  } else {
    // Content already at body position → use its rendered position
    ssX = extractX;
    ssY = extractY;
  }

  return {
    width: contentMeta.width,
    height: contentMeta.height,
    spriteSourceSize: {
      x: Math.max(0, ssX),
      y: Math.max(0, ssY),
      w: contentMeta.width,
      h: contentMeta.height,
    },
  };
}

/**
 * Build a PixiJS sprite sheet JSON from trimmed frame PNGs with trim metadata.
 * All frames share the same sourceSize (body frame dimensions).
 */
async function packSpriteSheet(itemName, framesDir, outputDir, hasTrimData = false) {
  const frames = {};
  const animations = {};
  const framePngs = [];

  // Collect all frames organized by animation state
  for (const anim of Object.keys(ANIMATIONS)) {
    const animDir = path.join(framesDir, anim);
    if (!existsSync(animDir)) continue;

    const files = (await readdir(animDir))
      .filter(f => f.endsWith('.png') && !f.includes('_raw'))
      .sort();

    if (files.length === 0) continue;

    animations[anim] = [];
    for (const file of files) {
      const frameName = `${anim}_${path.basename(file, '.png')}`;
      animations[anim].push(frameName);
      framePngs.push({
        name: frameName,
        path: path.join(animDir, file),
        metaPath: path.join(animDir, file.replace('.png', '.meta.json')),
      });
    }
  }

  if (framePngs.length === 0) return null;

  // Get dimensions of all frames
  const frameMeta = await Promise.all(
    framePngs.map(async (f) => {
      const meta = await sharp(f.path).metadata();
      let trimData = null;
      if (hasTrimData && existsSync(f.metaPath)) {
        trimData = JSON.parse(await readFile(f.metaPath, 'utf-8'));
      }
      return { ...f, width: meta.width, height: meta.height, trimData };
    })
  );

  // Resize all frames to uniform height, then pack horizontally
  const maxH = Math.max(...frameMeta.map(f => f.height));
  const resizedFrames = await Promise.all(
    frameMeta.map(async (f) => {
      const resized = await sharp(f.path)
        .resize({ height: maxH, fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
        .png()
        .toBuffer();
      const meta = await sharp(resized).metadata();
      return { ...f, resized, renderedWidth: meta.width, renderedHeight: meta.height };
    })
  );

  let totalW = 0;
  for (const f of resizedFrames) {
    f.x = totalW;
    f.y = 0;
    const frameEntry = {
      frame: { x: f.x, y: f.y, w: f.renderedWidth, h: f.renderedHeight },
    };

    if (f.trimData) {
      frameEntry.trimmed = true;
      frameEntry.spriteSourceSize = f.trimData.spriteSourceSize;
      frameEntry.sourceSize = { w: BODY_FRAME_W, h: BODY_FRAME_H };
    } else {
      // Body frames: not trimmed, sourceSize = frame size
      frameEntry.sourceSize = { w: f.renderedWidth, h: f.renderedHeight };
    }

    frames[f.name] = frameEntry;
    totalW += f.renderedWidth;
  }

  // Composite all frames into a single sprite sheet
  const composites = resizedFrames.map((f) => ({
    input: f.resized,
    left: f.x,
    top: 0,
  }));

  const sheetPng = await sharp({
    create: { width: totalW, height: maxH, channels: 4, background: { r: 0, g: 0, b: 0, alpha: 0 } },
  })
    .composite(composites)
    .png()
    .toBuffer();

  await mkdir(outputDir, { recursive: true });
  const pngPath = path.join(outputDir, `${itemName}.png`);
  const jsonPath = path.join(outputDir, `${itemName}.json`);

  await sharp(sheetPng).toFile(pngPath);

  const sheetJson = {
    frames,
    animations,
    meta: {
      image: `${itemName}.png`,
      format: 'RGBA8888',
      size: { w: totalW, h: maxH },
      scale: 1,
    },
  };
  await writeFile(jsonPath, JSON.stringify(sheetJson, null, 2));

  return { frames: framePngs.length, width: totalW, height: maxH };
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/**
 * Get the per-frame container position for a category and animation.
 * Returns an array of {x, y} in stage coords, one per sampled frame.
 */
function getContainerPositions(transforms, category, anim) {
  const animFrames = transforms.animations[anim];
  if (!animFrames) return null;
  return animFrames.map((f) => f[category] || null);
}

/** Extract a single SWF item through the full pipeline. */
async function extractItem(swfPath, category, itemName, transforms) {
  const tempItemDir = path.join(TEMP_DIR, category, itemName);
  const wrappedSwf = path.join(tempItemDir, 'wrapped.swf');
  await mkdir(tempItemDir, { recursive: true });

  // Step 1: Wrap SWF (place sprites on stage, match body's stage bounds)
  let info;
  const wrapArgs = [swfPath, wrappedSwf];
  if (existsSync(BODY_SWF)) {
    wrapArgs.splice(0, 0, '--match-stage', BODY_SWF);
  }
  try {
    const out = run(SWF_WRAPPER, wrapArgs);
    info = JSON.parse(out);
  } catch (e) {
    console.error(`  SKIP ${category}/${itemName}: wrapper failed`);
    return null;
  }

  const isAnimated = info.max_frames > 1;
  const containerKey = CATEGORY_CONTAINERS[category] || category;

  // Step 2: Render frames with ruffle exporter
  const framesDir = path.join(tempItemDir, 'frames');
  await mkdir(framesDir, { recursive: true });

  if (isAnimated) {
    for (const [anim, { start, count, step }] of Object.entries(ANIMATIONS)) {
      const animDir = path.join(framesDir, anim);
      await mkdir(animDir, { recursive: true });

      const positions = getContainerPositions(transforms, containerKey, anim);

      let frameIdx = 0;
      for (let f = 0; f < count; f += step) {
        const frameNum = start + f;
        const rawFile = path.join(animDir, `${String(frameIdx).padStart(2, '0')}_raw.png`);
        try {
          run(RUFFLE_EXPORTER, [
            wrappedSwf, rawFile,
            '--skipframes', String(frameNum),
            '--frames', '1',
            '--scale', String(SCALE),
            '--silent',
          ], { stdio: 'pipe' });
        } catch {
          // Some frames may fail — skip
        }

        // Trim with position metadata
        if (existsSync(rawFile)) {
          const croppedFile = rawFile.replace('_raw.png', '.png');
          const metaFile = rawFile.replace('_raw.png', '.meta.json');
          const containerPos = positions?.[frameIdx] || null;

          try {
            const result = await trimWithMetadata(rawFile, croppedFile, containerPos);
            if (result) {
              await writeFile(metaFile, JSON.stringify({
                spriteSourceSize: result.spriteSourceSize,
              }));
            }
            await rm(rawFile);
          } catch {
            await rm(rawFile).catch(() => {});
          }
        }
        frameIdx++;
      }
    }
  } else {
    // Static items (wardrobe): render frame 0 for each animation state
    for (const [anim, { start }] of Object.entries(ANIMATIONS)) {
      const animDir = path.join(framesDir, anim);
      await mkdir(animDir, { recursive: true });

      const positions = getContainerPositions(transforms, containerKey, anim);

      // Static items use the same frame for all animation states, but need
      // per-frame container positions to match body pose
      let frameIdx = 0;
      const { count, step } = ANIMATIONS[anim];
      for (let f = 0; f < count; f += step) {
        const frameNum = start + f;
        const rawFile = path.join(animDir, `${String(frameIdx).padStart(2, '0')}_raw.png`);
        try {
          run(RUFFLE_EXPORTER, [
            wrappedSwf, rawFile,
            '--skipframes', String(frameNum),
            '--frames', '1',
            '--scale', String(SCALE),
            '--silent',
          ], { stdio: 'pipe' });
        } catch {
          // Skip on failure
        }

        if (existsSync(rawFile)) {
          const croppedFile = rawFile.replace('_raw.png', '.png');
          const metaFile = rawFile.replace('_raw.png', '.meta.json');
          const containerPos = positions?.[frameIdx] || null;

          try {
            const result = await trimWithMetadata(rawFile, croppedFile, containerPos);
            if (result) {
              await writeFile(metaFile, JSON.stringify({
                spriteSourceSize: result.spriteSourceSize,
              }));
            }
            await rm(rawFile);
          } catch {
            await rm(rawFile).catch(() => {});
          }
        }
        frameIdx++;
      }
    }
  }

  // Step 3: Pack into sprite sheet with trim metadata
  const outputCategoryDir = path.join(OUTPUT_DIR, category);
  const result = await packSpriteSheet(itemName, framesDir, outputCategoryDir, true);

  if (result) {
    console.log(`  ${category}/${itemName}: ${result.frames} frames, ${result.width}x${result.height}`);
  } else {
    console.log(`  ${category}/${itemName}: no renderable content`);
  }

  return result;
}

/** Extract the base avatar body (renders directly, no wrapper needed). */
async function extractBaseBody() {
  console.log('=== Base Avatar Body ===');
  if (!existsSync(BODY_SWF)) {
    console.error('Base avatar SWF not found');
    return;
  }

  const tempDir = path.join(TEMP_DIR, 'base');
  const framesDir = path.join(tempDir, 'frames');
  await mkdir(framesDir, { recursive: true });

  // Base avatar renders directly (sprites are already placed)
  for (const [anim, { start, count, step }] of Object.entries(ANIMATIONS)) {
    const animDir = path.join(framesDir, anim);
    await mkdir(animDir, { recursive: true });

    let frameIdx = 0;
    for (let f = 0; f < count; f += step) {
      const frameNum = start + f;
      const outFile = path.join(animDir, `${String(frameIdx).padStart(2, '0')}.png`);
      try {
        run(RUFFLE_EXPORTER, [
          BODY_SWF, outFile,
          '--skipframes', String(frameNum),
          '--frames', '1',
          '--scale', String(SCALE),
          '--silent',
        ], { stdio: 'pipe' });
      } catch {
        // Skip failures
      }
      frameIdx++;
    }
  }

  // Pack base body sprite sheet (no trim — body fills the full frame)
  const outputDir = path.join(OUTPUT_DIR, 'base');
  const result = await packSpriteSheet('body', framesDir, outputDir, false);
  if (result) {
    console.log(`  base/body: ${result.frames} frames, ${result.width}x${result.height}`);
  }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  // Validate tools exist
  if (!existsSync(SWF_WRAPPER)) {
    console.error(`swf-wrapper not found at ${SWF_WRAPPER}`);
    console.error('Build: cd src-tauri && cargo build --release --bin swf-wrapper --features swf-wrapper');
    process.exit(1);
  }
  if (!existsSync(RUFFLE_EXPORTER)) {
    console.error(`ruffle exporter not found at ${RUFFLE_EXPORTER}`);
    console.error('Build: cd ~/work/ruffle-rs/ruffle && cargo build --release --package=exporter');
    process.exit(1);
  }
  if (!existsSync(GLITCH_AVATARS)) {
    console.error(`glitch-avatars repo not found at ${GLITCH_AVATARS}`);
    process.exit(1);
  }

  // Load avatar transforms (per-frame container positions)
  let transforms = { animations: {} };
  if (existsSync(TRANSFORMS_PATH)) {
    transforms = JSON.parse(await readFile(TRANSFORMS_PATH, 'utf-8'));
    console.log('Loaded avatar transforms');
  } else {
    console.warn('Warning: avatar-transforms.json not found — components will not have position data');
  }

  await mkdir(TEMP_DIR, { recursive: true });
  await mkdir(OUTPUT_DIR, { recursive: true });

  const manifest = { categories: {}, defaults: {} };

  // Extract base body
  if (opts['base-only'] || !opts.item) {
    await extractBaseBody();
  }
  if (opts['base-only']) {
    await rm(TEMP_DIR, { recursive: true, force: true });
    return;
  }

  // Determine which categories to process
  let categories;
  if (opts.item) {
    // Single item: --item eyes/eyes_01
    const [cat, name] = opts.item.split('/');
    const type = VANITY_CATEGORIES.includes(cat) ? 'vanity' : 'wardrobe';
    const swfPath = path.join(GLITCH_AVATARS, type, cat, `${name}.swf`);
    if (!existsSync(swfPath)) {
      console.error(`SWF not found: ${swfPath}`);
      process.exit(1);
    }
    await extractItem(swfPath, cat, name, transforms);
    await rm(TEMP_DIR, { recursive: true, force: true });
    return;
  }

  if (opts.category) {
    categories = [opts.category];
  } else {
    categories = [...VANITY_CATEGORIES, ...WARDROBE_CATEGORIES];
  }

  // Process each category
  for (const cat of categories) {
    const type = VANITY_CATEGORIES.includes(cat) ? 'vanity' : 'wardrobe';
    const catDir = path.join(GLITCH_AVATARS, type, cat);
    if (!existsSync(catDir)) {
      console.log(`=== ${cat}: directory not found, skipping ===`);
      continue;
    }

    console.log(`=== ${cat} ===`);
    const swfFiles = (await readdir(catDir))
      .filter(f => f.endsWith('.swf'))
      .sort();

    const items = [];
    let processed = 0;

    for (const swfFile of swfFiles) {
      if (LIMIT > 0 && processed >= LIMIT) break;

      const itemName = path.basename(swfFile, '.swf');
      const swfPath = path.join(catDir, swfFile);
      const result = await extractItem(swfPath, cat, itemName, transforms);

      if (result) {
        items.push({
          id: itemName,
          name: itemName.replace(/_/g, ' '),
          sheet: `${cat}/${itemName}.json`,
        });
      }
      processed++;
    }

    if (items.length > 0) {
      manifest.categories[cat] = { items };
    }
  }

  // Set defaults — derive from extracted items when possible
  manifest.defaults = {
    eyes: manifest.categories.eyes?.items[0]?.id ?? null,
    ears: manifest.categories.ears?.items[0]?.id ?? null,
    nose: manifest.categories.nose?.items[0]?.id ?? null,
    mouth: manifest.categories.mouth?.items[0]?.id ?? null,
    hair: manifest.categories.hair?.items[0]?.id ?? null,
    skin_color: 'D4C159',
    hair_color: '4A3728',
    shirt: manifest.categories.shirt?.items[0]?.id ?? null,
    pants: manifest.categories.pants?.items[0]?.id ?? null,
    shoes: manifest.categories.shoes?.items[0]?.id ?? null,
  };

  // Write manifest
  const manifestPath = path.join(OUTPUT_DIR, 'manifest.json');
  await writeFile(manifestPath, JSON.stringify(manifest, null, 2));
  console.log(`\nManifest written to ${manifestPath}`);

  // Cleanup temp files
  await rm(TEMP_DIR, { recursive: true, force: true });
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
