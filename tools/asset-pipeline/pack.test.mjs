import { describe, it, expect } from 'vitest';
import {
  nextPowerOfTwo,
  shelfPack,
  buildJson,
  groupAnimations,
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
