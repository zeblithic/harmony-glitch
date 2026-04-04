#!/usr/bin/env node
/**
 * Avatar extraction pipeline — extracts Glitch avatar SWF art into
 * PixiJS-compatible sprite sheets.
 *
 * Pipeline: SWF → swf-wrapper → ruffle exporter → crop → sprite sheet + manifest
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
import { readdir, mkdir, writeFile, rm, readFile } from 'node:fs/promises';
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Run a command and return stdout. */
function run(cmd, args, opts = {}) {
  return execFileSync(cmd, args, { encoding: 'utf-8', maxBuffer: 10 * 1024 * 1024, ...opts });
}

/** Auto-crop a PNG to its non-transparent content bounds, with padding. */
async function autoCrop(inputPath, outputPath, padding = 2) {
  const img = sharp(inputPath);
  const { width, height } = await img.metadata();

  // Use sharp's trim to remove uniform borders
  // We trim the white (or near-white) background + transparent areas
  const trimmed = await sharp(inputPath)
    .trim({ background: '#ffffff', threshold: 20 })
    .extend({ top: padding, bottom: padding, left: padding, right: padding, background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .png()
    .toBuffer();

  const meta = await sharp(trimmed).metadata();
  await sharp(trimmed).toFile(outputPath);
  return { width: meta.width, height: meta.height };
}

/** Build a PixiJS sprite sheet JSON from a set of animation frame PNGs. */
async function packSpriteSheet(itemName, framesDir, outputDir) {
  const frames = {};
  const animations = {};
  const framePngs = [];

  // Collect all frames organized by animation state
  for (const anim of Object.keys(ANIMATIONS)) {
    const animDir = path.join(framesDir, anim);
    if (!existsSync(animDir)) continue;

    const files = (await readdir(animDir))
      .filter(f => f.endsWith('.png'))
      .sort();

    if (files.length === 0) continue;

    animations[anim] = [];
    for (const file of files) {
      const frameName = `${anim}_${path.basename(file, '.png')}`;
      animations[anim].push(frameName);
      framePngs.push({ name: frameName, path: path.join(animDir, file) });
    }
  }

  if (framePngs.length === 0) return null;

  // Get dimensions of all frames
  const frameMeta = await Promise.all(
    framePngs.map(async (f) => {
      const meta = await sharp(f.path).metadata();
      return { ...f, width: meta.width, height: meta.height };
    })
  );

  // Simple horizontal strip packing (all frames in a row)
  const maxH = Math.max(...frameMeta.map(f => f.height));
  let totalW = 0;
  for (const f of frameMeta) {
    f.x = totalW;
    f.y = 0;
    frames[f.name] = {
      frame: { x: f.x, y: f.y, w: f.width, h: f.height },
    };
    totalW += f.width;
  }

  // Composite all frames into a single sprite sheet
  const composites = await Promise.all(
    frameMeta.map(async (f) => ({
      input: await sharp(f.path).resize({ height: maxH, fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } }).png().toBuffer(),
      left: f.x,
      top: 0,
    }))
  );

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

/** Extract a single SWF item through the full pipeline. */
async function extractItem(swfPath, category, itemName) {
  const tempItemDir = path.join(TEMP_DIR, category, itemName);
  const wrappedSwf = path.join(tempItemDir, 'wrapped.swf');
  await mkdir(tempItemDir, { recursive: true });

  // Step 1: Wrap SWF (place sprites on stage)
  let info;
  try {
    const out = run(SWF_WRAPPER, [swfPath, wrappedSwf]);
    info = JSON.parse(out);
  } catch (e) {
    console.error(`  SKIP ${category}/${itemName}: wrapper failed`);
    return null;
  }

  const isAnimated = info.max_frames > 1;

  // Step 2: Render frames with ruffle exporter
  const framesDir = path.join(tempItemDir, 'frames');
  await mkdir(framesDir, { recursive: true });

  if (isAnimated) {
    // Animated items (vanity): render specific frames per animation state
    for (const [anim, { start, count, step }] of Object.entries(ANIMATIONS)) {
      const animDir = path.join(framesDir, anim);
      await mkdir(animDir, { recursive: true });

      let frameIdx = 0;
      for (let f = 0; f < count; f += step) {
        const frameNum = start + f;
        const outFile = path.join(animDir, `${String(frameIdx).padStart(2, '0')}_raw.png`);
        try {
          run(RUFFLE_EXPORTER, [
            wrappedSwf, outFile,
            '--skipframes', String(frameNum),
            '--frames', '1',
            '--scale', String(SCALE),
            '--silent',
          ], { stdio: 'pipe' });
        } catch {
          // Some frames may fail — skip
        }
        frameIdx++;
      }
    }
  } else {
    // Static items (wardrobe): all body parts at a single pose
    // Render frame 0 for each animation state (static items look the same at every frame)
    for (const anim of Object.keys(ANIMATIONS)) {
      const animDir = path.join(framesDir, anim);
      await mkdir(animDir, { recursive: true });
      const outFile = path.join(animDir, '00_raw.png');
      try {
        run(RUFFLE_EXPORTER, [
          wrappedSwf, outFile,
          '--frames', '1',
          '--scale', String(SCALE),
          '--silent',
        ], { stdio: 'pipe' });
      } catch {
        // Skip on failure
      }
    }
  }

  // Step 3: Auto-crop each rendered frame
  for (const anim of Object.keys(ANIMATIONS)) {
    const animDir = path.join(framesDir, anim);
    if (!existsSync(animDir)) continue;

    const rawFiles = (await readdir(animDir)).filter(f => f.endsWith('_raw.png'));
    for (const rawFile of rawFiles) {
      const croppedFile = rawFile.replace('_raw.png', '.png');
      try {
        await autoCrop(
          path.join(animDir, rawFile),
          path.join(animDir, croppedFile),
        );
        // Remove raw file
        await rm(path.join(animDir, rawFile));
      } catch {
        // Crop failed — likely all-white image (no content)
        await rm(path.join(animDir, rawFile)).catch(() => {});
      }
    }
  }

  // Step 4: Pack into sprite sheet
  const outputCategoryDir = path.join(OUTPUT_DIR, category);
  const result = await packSpriteSheet(itemName, framesDir, outputCategoryDir);

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
  const baseSwf = path.join(GLITCH_AVATARS, 'base_avatar/Avatar.swf');
  if (!existsSync(baseSwf)) {
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
          baseSwf, outFile,
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

  // Pack base body sprite sheet (no crop needed — base avatar has tight bounds)
  const outputDir = path.join(OUTPUT_DIR, 'base');
  const result = await packSpriteSheet('body', framesDir, outputDir);
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
    await extractItem(swfPath, cat, name);
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
      const result = await extractItem(swfPath, cat, itemName);

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

  // Set defaults
  manifest.defaults = {
    eyes: 'eyes_01', ears: 'ears_0001', nose: 'nose_0001',
    mouth: 'mouth_01', hair: 'hair_01',
    skin_color: 'D4C159', hair_color: '4A3728',
    shirt: 'hawaiian_shirt', pants: 'cargo_pants', shoes: 'campers',
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
