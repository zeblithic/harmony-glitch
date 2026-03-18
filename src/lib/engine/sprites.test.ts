import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock pixi.js before importing SpriteManager
vi.mock('pixi.js', () => {
  const mockTexture = { width: 30, height: 60, label: 'mock' };
  const mockSpritesheet = {
    animations: {
      idle: [mockTexture],
      walking: [mockTexture, mockTexture],
      jumping: [mockTexture],
      falling: [mockTexture],
    },
    parse: vi.fn(),
  };
  return {
    Assets: {
      load: vi.fn().mockRejectedValue(new Error('not found')),
    },
    Spritesheet: vi.fn(function () { return mockSpritesheet; }),
    AnimatedSprite: vi.fn(function () {
      return {
        anchor: { set: vi.fn() },
        textures: [],
        animationSpeed: 0,
        loop: true,
        play: vi.fn(),
        gotoAndStop: vi.fn(),
        destroy: vi.fn(),
      };
    }),
    Sprite: vi.fn(function () {
      return {
        anchor: { set: vi.fn() },
        width: 0,
        height: 0,
        scale: { x: 1 },
        rotation: 0,
        destroy: vi.fn(),
      };
    }),
    Container: vi.fn(function () {
      return {
        addChild: vi.fn(),
        children: [],
        destroy: vi.fn(),
        scale: { x: 1 },
      };
    }),
    Graphics: vi.fn(function () {
      return {
        rect: vi.fn().mockReturnThis(),
        circle: vi.fn().mockReturnThis(),
        fill: vi.fn().mockReturnThis(),
        moveTo: vi.fn().mockReturnThis(),
        lineTo: vi.fn().mockReturnThis(),
        stroke: vi.fn().mockReturnThis(),
        x: 0,
        y: 0,
        scale: { x: 1 },
        rotation: 0,
        destroy: vi.fn(),
      };
    }),
    Text: vi.fn(function () {
      return {
        anchor: { set: vi.fn() },
        text: '',
        x: 0,
        y: 0,
        destroy: vi.fn(),
      };
    }),
    Texture: { EMPTY: { width: 0, height: 0 } },
  };
});

import { SpriteManager } from './sprites';

describe('SpriteManager', () => {
  let manager: SpriteManager;

  beforeEach(() => {
    manager = new SpriteManager();
    vi.clearAllMocks();
  });

  describe('hasTexture', () => {
    it('returns false for uncached sprite class', () => {
      expect(manager.hasTexture('nonexistent')).toBe(false);
    });
  });

  describe('createDeco fallback', () => {
    it('returns a Container when no texture is loaded', () => {
      const deco = {
        id: 'd1', name: 'tree', spriteClass: 'tree_bg',
        x: 100, y: -200, w: 120, h: 200, z: 0, r: 0, hFlip: false,
      };
      const result = manager.createDeco(deco);
      expect(result).toBeDefined();
    });
  });

  describe('createEntity fallback', () => {
    it('returns a Container for tree entities', () => {
      const entity = {
        id: 'e1', entityType: 'tree', name: 'Fruit Tree',
        spriteClass: 'tree_fruit', x: 100, y: 0,
        cooldownRemaining: null, depleted: false,
      };
      const result = manager.createEntity(entity);
      expect(result).toBeDefined();
    });

    it('returns a Container for non-tree entities', () => {
      const entity = {
        id: 'e2', entityType: 'npc', name: 'Chicken',
        spriteClass: 'npc_chicken', x: 200, y: 0,
        cooldownRemaining: null, depleted: false,
      };
      const result = manager.createEntity(entity);
      expect(result).toBeDefined();
    });
  });

  describe('createGroundItem fallback', () => {
    it('returns a Container when no texture is loaded', () => {
      const item = {
        id: 'i1', itemId: 'cherry', name: 'Cherry',
        icon: 'cherry', count: 1, x: 100, y: 0,
      };
      const result = manager.createGroundItem(item);
      expect(result).toBeDefined();
    });
  });

  describe('createAvatar fallback', () => {
    it('returns a Container when no spritesheet is loaded', () => {
      const result = manager.createAvatar();
      expect(result).toBeDefined();
    });
  });

  describe('missing texture dedup', () => {
    it('logs missing entity texture only once per spriteClass', async () => {
      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const entity = {
        id: 'e1', entityType: 'npc', name: 'Chicken',
        spriteClass: 'npc_chicken', x: 0, y: 0,
        cooldownRemaining: null, depleted: false,
      };
      manager.createEntity(entity);
      manager.createEntity({ ...entity, id: 'e2' });
      // Flush microtasks so the async .catch() warning fires
      await new Promise((r) => setTimeout(r, 0));
      const chickenWarnings = consoleSpy.mock.calls.filter(
        (args) => String(args[0]).includes('npc_chicken')
      );
      expect(chickenWarnings.length).toBe(1);
      consoleSpy.mockRestore();
    });
  });
});
