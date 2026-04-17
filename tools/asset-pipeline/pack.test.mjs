import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { writeFile, mkdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import sharp from 'sharp';
import {
  nextPowerOfTwo,
  shelfPack,
  buildJson,
  groupAnimations,
  collectImages,
  readImageMeta,
  readImageMetaBatched,
  batchPromises,
} from './pack.mjs';

// ---------------------------------------------------------------------------
// nextPowerOfTwo
// ---------------------------------------------------------------------------

describe('nextPowerOfTwo', () => {
  it('returns 1 for 0', () => {
    expect(nextPowerOfTwo(0)).toBe(1);
  });

  it('returns 1 for negative numbers', () => {
    expect(nextPowerOfTwo(-5)).toBe(1);
  });

  it('returns exact value for powers of two', () => {
    expect(nextPowerOfTwo(64)).toBe(64);
    expect(nextPowerOfTwo(256)).toBe(256);
    expect(nextPowerOfTwo(1024)).toBe(1024);
  });

  it('rounds up to next power of two', () => {
    expect(nextPowerOfTwo(65)).toBe(128);
    expect(nextPowerOfTwo(100)).toBe(128);
    expect(nextPowerOfTwo(300)).toBe(512);
  });
});

// ---------------------------------------------------------------------------
// shelfPack
// ---------------------------------------------------------------------------

describe('shelfPack', () => {
  it('packs into power-of-two dimensions', () => {
    const images = [
      { path: 'a.png', name: 'a', width: 32, height: 32 },
      { path: 'b.png', name: 'b', width: 32, height: 32 },
      { path: 'c.png', name: 'c', width: 32, height: 32 },
    ];

    const { sheetWidth, sheetHeight } = shelfPack(images);

    // Both dimensions must be powers of two
    expect(sheetWidth & (sheetWidth - 1)).toBe(0);
    expect(sheetHeight & (sheetHeight - 1)).toBe(0);
  });

  it('produces no overlapping frames', () => {
    const images = [
      { path: 'a.png', name: 'a', width: 50, height: 80 },
      { path: 'b.png', name: 'b', width: 60, height: 40 },
      { path: 'c.png', name: 'c', width: 30, height: 70 },
      { path: 'd.png', name: 'd', width: 45, height: 45 },
      { path: 'e.png', name: 'e', width: 20, height: 90 },
    ];

    const { frames } = shelfPack(images);

    // Check all pairs for overlap
    for (let i = 0; i < frames.length; i++) {
      for (let j = i + 1; j < frames.length; j++) {
        const a = frames[i];
        const b = frames[j];
        const overlaps =
          a.x < b.x + b.width &&
          a.x + a.width > b.x &&
          a.y < b.y + b.height &&
          a.y + a.height > b.y;
        expect(overlaps, `${a.name} overlaps ${b.name}`).toBe(false);
      }
    }
  });

  it('handles a single image', () => {
    const images = [{ path: 'solo.png', name: 'solo', width: 100, height: 100 }];
    const { frames, sheetWidth, sheetHeight } = shelfPack(images);

    expect(frames).toHaveLength(1);
    expect(frames[0].x).toBe(0);
    expect(frames[0].y).toBe(0);
    expect(sheetWidth).toBeGreaterThanOrEqual(100);
    expect(sheetHeight).toBeGreaterThanOrEqual(100);
  });
});

// ---------------------------------------------------------------------------
// buildJson
// ---------------------------------------------------------------------------

describe('buildJson', () => {
  const frames = [
    { path: 'a.png', name: 'apple', x: 0, y: 0, width: 64, height: 64 },
    { path: 'b.png', name: 'banana', x: 64, y: 0, width: 32, height: 32 },
  ];

  it('produces correct TexturePacker JSON Hash format', () => {
    const json = buildJson(frames, 'fruits', 128, 64, false);

    // Frames
    expect(json.frames.apple).toEqual({
      frame: { x: 0, y: 0, w: 64, h: 64 },
    });
    expect(json.frames.banana).toEqual({
      frame: { x: 64, y: 0, w: 32, h: 32 },
    });

    // Meta
    expect(json.meta.image).toBe('fruits.png');
    expect(json.meta.format).toBe('RGBA8888');
    expect(json.meta.size).toEqual({ w: 128, h: 64 });
    expect(json.meta.scale).toBe(1);
  });

  it('does not include animations block when animationMode is false', () => {
    const json = buildJson(frames, 'fruits', 128, 64, false);
    expect(json.animations).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// groupAnimations
// ---------------------------------------------------------------------------

describe('groupAnimations', () => {
  it('groups frames by name prefix', () => {
    const frames = [
      { name: 'walk_0' },
      { name: 'walk_1' },
      { name: 'walk_2' },
      { name: 'idle_0' },
      { name: 'idle_1' },
    ];

    const groups = groupAnimations(frames);

    expect(groups.walk).toEqual(['walk_0', 'walk_1', 'walk_2']);
    expect(groups.idle).toEqual(['idle_0', 'idle_1']);
  });

  it('sorts numerically within groups', () => {
    const frames = [
      { name: 'run_2' },
      { name: 'run_0' },
      { name: 'run_1' },
    ];

    const groups = groupAnimations(frames);
    expect(groups.run).toEqual(['run_0', 'run_1', 'run_2']);
  });

  it('skips frames without underscore-number suffix', () => {
    const frames = [
      { name: 'background' },
      { name: 'walk_0' },
      { name: 'icon_large' },
    ];

    const groups = groupAnimations(frames);
    expect(groups.walk).toEqual(['walk_0']);
    expect(groups.background).toBeUndefined();
    expect(groups.icon).toBeUndefined();
  });

  it('buildJson includes animations when animationMode is true', () => {
    const animFrames = [
      { path: 'a.png', name: 'walk_0', x: 0, y: 0, width: 32, height: 32 },
      { path: 'b.png', name: 'walk_1', x: 32, y: 0, width: 32, height: 32 },
      { path: 'c.png', name: 'idle_0', x: 64, y: 0, width: 32, height: 32 },
    ];

    const json = buildJson(animFrames, 'chars', 128, 32, true);

    expect(json.animations).toBeDefined();
    expect(json.animations.walk).toEqual(['walk_0', 'walk_1']);
    expect(json.animations.idle).toEqual(['idle_0']);
  });
});

// ---------------------------------------------------------------------------
// SVG support
// ---------------------------------------------------------------------------

describe('SVG support', () => {
  let dir;

  beforeEach(async () => {
    dir = join(tmpdir(), `pack-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
    await mkdir(dir, { recursive: true });
  });

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('collectImages finds both PNG and SVG files', async () => {
    const svgContent =
      '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>';
    await writeFile(join(dir, 'sprite.svg'), svgContent);
    await sharp({
      create: { width: 10, height: 10, channels: 4, background: { r: 255, g: 0, b: 0, alpha: 1 } },
    })
      .png()
      .toFile(join(dir, 'icon.png'));

    const results = await collectImages(dir);

    expect(results).toHaveLength(2);
    const names = results.map((r) => r.name).sort();
    expect(names).toEqual(['icon', 'sprite']);
    const exts = results.map((r) => r.ext).sort();
    expect(exts).toEqual(['png', 'svg']);
  });

  it('readImageMeta handles SVG files', async () => {
    const svgContent =
      '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>';
    const svgPath = join(dir, 'sprite.svg');
    await writeFile(svgPath, svgContent);

    const meta = await readImageMeta(svgPath, 'sprite');

    expect(meta).not.toBeNull();
    expect(meta.name).toBe('sprite');
    expect(meta.width).toBe(10);
    expect(meta.height).toBe(10);
    expect(meta.buffer).toBeInstanceOf(Buffer);
  });

  it('readImageMeta applies scale factor to SVGs', async () => {
    const svgContent =
      '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red"/></svg>';
    const svgPath = join(dir, 'sprite.svg');
    await writeFile(svgPath, svgContent);

    const meta = await readImageMeta(svgPath, 'sprite', 2);

    expect(meta).not.toBeNull();
    expect(meta.width).toBe(20);
    expect(meta.height).toBe(20);
    expect(meta.buffer).toBeInstanceOf(Buffer);
  });

  it('readImageMeta clamps oversized SVGs to maxSize', async () => {
    // 800x400 SVG — wider than tall, so width should clamp to 256 and height
    // should be proportional (128) when maxSize=256 with "inside" fit.
    const svgContent =
      '<svg xmlns="http://www.w3.org/2000/svg" width="800" height="400"><rect width="800" height="400" fill="red"/></svg>';
    const svgPath = join(dir, 'huge.svg');
    await writeFile(svgPath, svgContent);

    const meta = await readImageMeta(svgPath, 'huge', 1, 256);

    expect(meta).not.toBeNull();
    expect(meta.width).toBe(256);
    expect(meta.height).toBe(128);
  });

  it('readImageMeta leaves small PNGs untouched when under maxSize', async () => {
    const pngPath = join(dir, 'small.png');
    await sharp({
      create: { width: 50, height: 50, channels: 4, background: { r: 255, g: 0, b: 0, alpha: 1 } },
    }).png().toFile(pngPath);

    const meta = await readImageMeta(pngPath, 'small', 1, 256);

    expect(meta).not.toBeNull();
    expect(meta.width).toBe(50);
    expect(meta.height).toBe(50);
    // Small PNGs are read by path, not buffer — buffer should be absent.
    expect(meta.buffer).toBeUndefined();
  });

  it('readImageMeta resizes oversized PNGs to maxSize', async () => {
    const pngPath = join(dir, 'big.png');
    await sharp({
      create: { width: 512, height: 300, channels: 4, background: { r: 0, g: 255, b: 0, alpha: 1 } },
    }).png().toFile(pngPath);

    const meta = await readImageMeta(pngPath, 'big', 1, 256);

    expect(meta).not.toBeNull();
    expect(meta.width).toBe(256);
    // 300 * (256/512) = 150
    expect(meta.height).toBe(150);
    expect(meta.buffer).toBeInstanceOf(Buffer);
  });

  it('readImageMeta returns numeric width/height on every non-null path', async () => {
    // Regression: the maxSize PNG path previously returned meta.width /
    // meta.height directly, which could be undefined and propagate NaN
    // into shelfPack. Verify every return shape has numeric dimensions.
    const pngPath = join(dir, 'small.png');
    await sharp({
      create: { width: 30, height: 30, channels: 4, background: { r: 0, g: 0, b: 255, alpha: 1 } },
    }).png().toFile(pngPath);

    const withoutMax = await readImageMeta(pngPath, 'small', 1);
    const withMaxUnder = await readImageMeta(pngPath, 'small', 1, 256);

    for (const result of [withoutMax, withMaxUnder]) {
      expect(typeof result.width).toBe('number');
      expect(typeof result.height).toBe('number');
      expect(result.width).toBeGreaterThan(0);
      expect(result.height).toBeGreaterThan(0);
    }
  });

});

// ---------------------------------------------------------------------------
// batching helpers
// ---------------------------------------------------------------------------

describe('batching helpers', () => {
  let dir;

  beforeEach(async () => {
    dir = join(tmpdir(), `pack-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
    await mkdir(dir, { recursive: true });
  });

  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it('readImageMetaBatched returns all entries in order', async () => {
    const entries = [];
    for (let i = 0; i < 10; i++) {
      const p = join(dir, `f${i}.png`);
      await sharp({
        create: { width: 10 + i, height: 10 + i, channels: 4, background: { r: 0, g: 0, b: 0, alpha: 0 } },
      }).png().toFile(p);
      entries.push({ path: p, name: `f${i}` });
    }

    const results = await readImageMetaBatched(entries, 1, null, 3);

    expect(results).toHaveLength(10);
    for (let i = 0; i < 10; i++) {
      expect(results[i].name).toBe(`f${i}`);
      expect(results[i].width).toBe(10 + i);
    }
  });

  it('batchPromises caps concurrent in-flight calls at batchSize', async () => {
    // Instrument the processor so we can observe concurrency directly — this
    // is the actual invariant the refactor exists to uphold. A regression back
    // to unbounded Promise.all would drive peak up to items.length.
    let inFlight = 0;
    let peak = 0;
    const processor = async (n) => {
      inFlight++;
      peak = Math.max(peak, inFlight);
      // Hold the promise long enough for all queued items to start if they're
      // going to, so an unbounded implementation would show peak ≈ items.length.
      await new Promise((resolve) => setTimeout(resolve, 5));
      inFlight--;
      return n * 2;
    };

    const items = Array.from({ length: 20 }, (_, i) => i);
    const results = await batchPromises(items, processor, 3);

    expect(peak).toBeLessThanOrEqual(3);
    expect(results).toHaveLength(20);
    // Order preserved, values transformed.
    expect(results).toEqual(items.map((n) => n * 2));
  });

  it('batchPromises rejects invalid batchSize', async () => {
    const noop = async (x) => x;
    await expect(batchPromises([1], noop, 0)).rejects.toThrow(RangeError);
    await expect(batchPromises([1], noop, -1)).rejects.toThrow(RangeError);
    await expect(batchPromises([1], noop, 1.5)).rejects.toThrow(RangeError);
    await expect(batchPromises([1], noop, NaN)).rejects.toThrow(RangeError);
  });
});
