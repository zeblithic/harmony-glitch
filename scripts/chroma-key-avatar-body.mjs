#!/usr/bin/env node
/**
 * Key out the teal SWF-stage background from assets/sprites/avatar/base/body.png.
 *
 * Ruffle renders the base-avatar SWF against the SWF's declared background
 * color (approximately RGB 102,153,153). The wardrobe-item path in
 * tools/avatar-pipeline/extract.mjs runs through a wrapper that forces a
 * white stage + a white-to-alpha key, but the body path renders the SWF
 * directly and ships the opaque teal through. This script post-processes the
 * produced body.png to replace teal with alpha=0, sampling the bg color from
 * the top-left pixel rather than hardcoding it.
 *
 * Runs in place. Idempotent — re-running on a keyed PNG does nothing (its
 * top-left is already transparent, so no pixels match).
 *
 * Usage: node scripts/chroma-key-avatar-body.mjs
 */

import sharp from 'sharp';
import { existsSync } from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const REPO_ROOT = path.dirname(path.dirname(__filename));
const BODY_PNG = path.join(REPO_ROOT, 'assets/sprites/avatar/base/body.png');
const TOLERANCE = 8;

if (!existsSync(BODY_PNG)) {
  console.error(`File not found: ${path.relative(REPO_ROOT, BODY_PNG)}`);
  console.error('Run ./scripts/fetch-avatar-assets.sh first (or regenerate via tools/avatar-pipeline/extract.mjs).');
  process.exit(1);
}

const { data, info } = await sharp(BODY_PNG)
  .ensureAlpha()
  .raw()
  .toBuffer({ resolveWithObject: true });

const buf = Buffer.from(data);
const bgR = buf[0];
const bgG = buf[1];
const bgB = buf[2];
const bgA = buf[3];

if (bgA === 0) {
  console.log(`Top-left of ${path.relative(REPO_ROOT, BODY_PNG)} is already transparent; nothing to do.`);
  process.exit(0);
}

console.log(`Background color sampled: rgb(${bgR},${bgG},${bgB}) @ alpha ${bgA}. Keying to transparent (tolerance ±${TOLERANCE})...`);

let keyed = 0;
for (let i = 0; i < buf.length; i += 4) {
  if (
    Math.abs(buf[i]     - bgR) <= TOLERANCE &&
    Math.abs(buf[i + 1] - bgG) <= TOLERANCE &&
    Math.abs(buf[i + 2] - bgB) <= TOLERANCE
  ) {
    buf[i + 3] = 0;
    keyed++;
  }
}

await sharp(buf, { raw: { width: info.width, height: info.height, channels: 4 } })
  .png()
  .toFile(BODY_PNG);

const total = buf.length / 4;
console.log(`Keyed ${keyed.toLocaleString()} / ${total.toLocaleString()} pixels (${(100 * keyed / total).toFixed(1)}%) to alpha 0.`);
