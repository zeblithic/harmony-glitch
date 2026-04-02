#!/usr/bin/env node
/**
 * Sprite sheet packer — takes a directory of PNGs, packs them into a sprite
 * sheet, and outputs TexturePacker JSON Hash metadata.
 *
 * Modes:
 *   atlas (default) — general-purpose sprite atlas
 *   animation        — groups frames by name prefix into animation sequences
 */

import { parseArgs } from 'node:util';
import { readdir, mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import sharp from 'sharp';

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/**
 * Return the smallest power of two >= n.  Returns 1 for n <= 0.
 */
export function nextPowerOfTwo(n) {
  if (n <= 0) return 1;
  if (n <= 1) return 1;
  // Bit-twiddling trick
  let v = n - 1;
  v |= v >> 1;
  v |= v >> 2;
  v |= v >> 4;
  v |= v >> 8;
  v |= v >> 16;
  return v + 1;
}

/**
 * Shelf (row) packing algorithm.
 *
 * 1. Sort images by height descending.
 * 2. Try power-of-two sheet widths from 64 up to 16384.
 * 3. Pack left-to-right; when a row overflows, start a new shelf.
 * 4. Accept the first width where total height fits in a power-of-two.
 *
 * @param {Array<{path: string, name: string, width: number, height: number}>} images
 * @returns {{ frames: Array<{path: string, name: string, x: number, y: number, width: number, height: number}>, sheetWidth: number, sheetHeight: number }}
 */
export function shelfPack(images) {
  // Sort by height descending (stable)
  const sorted = [...images].sort((a, b) => b.height - a.height);

  for (let sheetWidth = 64; sheetWidth <= 16384; sheetWidth *= 2) {
    const frames = [];
    let cursorX = 0;
    let cursorY = 0;
    let rowHeight = 0;
    let fits = true;

    for (const img of sorted) {
      if (img.width > sheetWidth) {
        fits = false;
        break;
      }

      // Would this image overflow the current row?
      if (cursorX + img.width > sheetWidth) {
        // Start a new shelf
        cursorY += rowHeight;
        cursorX = 0;
        rowHeight = 0;
      }

      frames.push({
        path: img.path,
        name: img.name,
        x: cursorX,
        y: cursorY,
        width: img.width,
        height: img.height,
      });

      cursorX += img.width;
      rowHeight = Math.max(rowHeight, img.height);
    }

    if (!fits) continue;

    const totalHeight = cursorY + rowHeight;
    const sheetHeight = nextPowerOfTwo(totalHeight);

    if (sheetHeight <= sheetWidth * 4) {
      // Reasonable aspect ratio — accept
      return { frames, sheetWidth, sheetHeight };
    }
  }

  // Fallback: use 16384-wide sheet
  throw new Error('Images do not fit in a 16384-wide sprite sheet');
}

/**
 * Group frames into animation sequences.
 *
 * Frame names like `walk_0`, `walk_1` produce `{ walk: ["walk_0", "walk_1"] }`.
 * The prefix before the LAST underscore is the animation name.
 * Frames without an underscore-number suffix are skipped.
 * Within each group, sort numerically by the trailing number.
 */
export function groupAnimations(frames) {
  const groups = {};

  for (const frame of frames) {
    const lastUnderscore = frame.name.lastIndexOf('_');
    if (lastUnderscore === -1) continue;

    const suffix = frame.name.slice(lastUnderscore + 1);
    // Must be a non-empty numeric string
    if (!/^\d+$/.test(suffix)) continue;

    const prefix = frame.name.slice(0, lastUnderscore);
    if (!groups[prefix]) groups[prefix] = [];
    groups[prefix].push({ name: frame.name, index: Number(suffix) });
  }

  const animations = {};
  for (const [prefix, entries] of Object.entries(groups)) {
    entries.sort((a, b) => a.index - b.index);
    animations[prefix] = entries.map((e) => e.name);
  }

  return animations;
}

/**
 * Build TexturePacker JSON Hash metadata.
 */
export function buildJson(frames, name, sheetWidth, sheetHeight, animationMode) {
  const framesObj = {};
  for (const f of frames) {
    framesObj[f.name] = {
      frame: { x: f.x, y: f.y, w: f.width, h: f.height },
    };
  }

  const json = {
    frames: framesObj,
    meta: {
      image: `${name}.png`,
      format: 'RGBA8888',
      size: { w: sheetWidth, h: sheetHeight },
      scale: 1,
    },
  };

  if (animationMode) {
    json.animations = groupAnimations(frames);
  }

  return json;
}

// ---------------------------------------------------------------------------
// Image collection and metadata helpers
// ---------------------------------------------------------------------------

export async function collectImages(dir) {
  const results = [];
  const entries = await readdir(dir, { withFileTypes: true, recursive: true });
  for (const entry of entries) {
    if (!entry.isFile()) continue;
    const lower = entry.name.toLowerCase();
    const ext = lower.endsWith('.png') ? 'png' : lower.endsWith('.svg') ? 'svg' : null;
    if (ext) {
      const fullPath = path.join(entry.parentPath ?? entry.path, entry.name);
      const name = path.basename(entry.name, '.' + ext);
      results.push({ path: fullPath, name, ext });
    }
  }
  return results.sort((a, b) => a.path.localeCompare(b.path));
}

export async function readImageMeta(filePath, name, scale = 1) {
  try {
    const ext = filePath.toLowerCase().endsWith('.svg') ? 'svg' : 'png';
    if (ext === 'svg') {
      const meta = await sharp(filePath).metadata();
      const width = Math.round((meta.width ?? 0) * scale);
      const height = Math.round((meta.height ?? 0) * scale);
      if (width === 0 || height === 0) return null;
      const buffer = await sharp(filePath).resize({ width, height }).png().toBuffer();
      return { path: filePath, name, width, height, buffer };
    }
    const meta = await sharp(filePath).metadata();
    return { path: filePath, name, width: meta.width, height: meta.height };
  } catch (err) {
    console.warn(`WARN: skipped ${filePath} — ${err.message}`);
    return null;
  }
}

// ---------------------------------------------------------------------------
// CLI — only runs when executed directly
// ---------------------------------------------------------------------------

const isDirectRun =
  process.argv[1] &&
  import.meta.url.endsWith(process.argv[1].replace(/\\/g, '/'));

if (isDirectRun) {
  const { values } = parseArgs({
    options: {
      input: { type: 'string', short: 'i' },
      output: { type: 'string', short: 'o' },
      name: { type: 'string', short: 'n' },
      animation: { type: 'boolean', default: false },
      scale: { type: 'string', short: 's', default: '1' },
    },
    strict: true,
  });

  if (!values.input || !values.output || !values.name) {
    console.error('Usage: pack.mjs --input <dir> --output <dir> --name <name> [--animation] [--scale <n>]');
    process.exit(1);
  }

  const scale = parseFloat(values.scale ?? '1');
  if (!Number.isFinite(scale) || scale <= 0) {
    console.error(`Error: --scale must be a positive number, got: ${values.scale}`);
    process.exit(1);
  }
  await run(values.input, values.output, values.name, values.animation ?? false, scale);
}

async function run(inputDir, outputDir, name, animationMode, scale = 1) {
  // Collect PNGs and SVGs recursively
  const entries = await collectImages(inputDir);
  if (entries.length === 0) {
    console.error(`No PNG or SVG files found in ${inputDir}`);
    process.exit(1);
  }

  // Read metadata (skip corrupt/unreadable files; rasterize SVGs)
  const imageResults = await Promise.all(
    entries.map(({ path: filePath, name: imgName }) => readImageMeta(filePath, imgName, scale)),
  );
  const images = imageResults.filter(Boolean);

  // Warn on basename collisions (last one wins)
  const seen = new Map();
  for (const img of images) {
    if (seen.has(img.name)) {
      console.warn(`WARN: duplicate frame name "${img.name}" — ${img.path} overwrites ${seen.get(img.name)}`);
    }
    seen.set(img.name, img.path);
  }

  if (images.length === 0) {
    console.error('No valid PNG or SVG files could be read');
    process.exit(1);
  }

  // Pack
  const { frames, sheetWidth, sheetHeight } = shelfPack(images);

  // Composite — use pre-rasterized buffer for SVGs, path for PNGs
  const imageByName = new Map(images.map((i) => [i.name, i]));
  const composites = frames.map((f) => {
    const img = imageByName.get(f.name);
    const input = img?.buffer ?? f.path;
    return { input, left: f.x, top: f.y };
  });

  await mkdir(outputDir, { recursive: true });

  const outputPng = path.join(outputDir, `${name}.png`);
  const outputJson = path.join(outputDir, `${name}.json`);

  await sharp({
    create: {
      width: sheetWidth,
      height: sheetHeight,
      channels: 4,
      background: { r: 0, g: 0, b: 0, alpha: 0 },
    },
  })
    .composite(composites)
    .png()
    .toFile(outputPng);

  const json = buildJson(frames, name, sheetWidth, sheetHeight, animationMode);
  await writeFile(outputJson, JSON.stringify(json, null, 2) + '\n');

  console.log(`Wrote ${outputPng} (${sheetWidth}x${sheetHeight})`);
  console.log(`Wrote ${outputJson} (${frames.length} frames)`);
}
