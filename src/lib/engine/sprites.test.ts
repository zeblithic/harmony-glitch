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
        facing: 'right' as const,
      };
      const result = manager.createEntity(entity);
      expect(result).toBeDefined();
    });

    it('returns a Container for non-tree entities', () => {
      const entity = {
        id: 'e2', entityType: 'npc', name: 'Chicken',
        spriteClass: 'npc_chicken', x: 200, y: 0,
        cooldownRemaining: null, depleted: false,
        facing: 'right' as const,
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

  describe('fallback upgrade path', () => {
    it('hasEntityTexture returns true after async load resolves', async () => {
      const { Assets } = await import('pixi.js');
      const mockTexture = { width: 60, height: 80 };
      vi.mocked(Assets.load).mockResolvedValueOnce(mockTexture);

      const entity = {
        id: 'e1', entityType: 'tree', name: 'Fruit Tree',
        spriteClass: 'tree_fruit', x: 0, y: 0,
        cooldownRemaining: null, depleted: false,
        facing: 'right' as const,
      };
      manager.createEntity(entity);
      expect(manager.hasEntityTexture('tree_fruit')).toBe(false);

      await new Promise((r) => setTimeout(r, 0));
      expect(manager.hasEntityTexture('tree_fruit')).toBe(true);
    });

    it('hasItemTexture returns true after async load resolves', async () => {
      const { Assets } = await import('pixi.js');
      const mockTexture = { width: 16, height: 16 };
      vi.mocked(Assets.load).mockResolvedValueOnce(mockTexture);

      const item = {
        id: 'i1', itemId: 'cherry', name: 'Cherry',
        icon: 'cherry', count: 1, x: 0, y: 0,
      };
      manager.createGroundItem(item);
      expect(manager.hasItemTexture('cherry')).toBe(false);

      await new Promise((r) => setTimeout(r, 0));
      expect(manager.hasItemTexture('cherry')).toBe(true);
    });
  });

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

  describe('entity → items atlas fallback', () => {
    it('resolves entity lookup via matching items atlas frame', async () => {
      const { Assets } = await import('pixi.js');
      const chickenTex = { width: 256, height: 256, label: 'chicken' };
      const mockAtlas = { textures: { npc_chicken: chickenTex } };
      vi.mocked(Assets.load).mockImplementation(async (path: string) => {
        if (path === 'sprites/items/items.json') return mockAtlas;
        throw new Error('not found');
      });
      await manager.loadAtlas('items', 'sprites/items/items.json');

      expect(manager.hasEntityTexture('npc_chicken')).toBe(false);
      const entity = {
        id: 'e1', entityType: 'npc', name: 'Chicken',
        spriteClass: 'npc_chicken', x: 0, y: 0,
        cooldownRemaining: null, depleted: false,
        facing: 'right' as const,
      };
      manager.createEntity(entity);
      // Resolution is synchronous — no microtask flush needed
      expect(manager.hasEntityTexture('npc_chicken')).toBe(true);
    });

    it('resolves aliased entity sprite_class via items atlas', async () => {
      const { Assets } = await import('pixi.js');
      const pigTex = { width: 256, height: 256, label: 'piggy' };
      const mockAtlas = { textures: { npc_piggy: pigTex } };
      vi.mocked(Assets.load).mockImplementation(async (path: string) => {
        if (path === 'sprites/items/items.json') return mockAtlas;
        throw new Error('not found');
      });
      await manager.loadAtlas('items', 'sprites/items/items.json');

      const entity = {
        id: 'e1', entityType: 'npc', name: 'Pig',
        spriteClass: 'npc_pig', x: 0, y: 0,
        cooldownRemaining: null, depleted: false,
        facing: 'right' as const,
      };
      manager.createEntity(entity);
      expect(manager.hasEntityTexture('npc_pig')).toBe(true);
    });
  });

  describe('missing texture dedup', () => {
    it('logs missing entity texture only once per spriteClass', async () => {
      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const entity = {
        id: 'e1', entityType: 'npc', name: 'Chicken',
        spriteClass: 'npc_chicken', x: 0, y: 0,
        cooldownRemaining: null, depleted: false,
        facing: 'right' as const,
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
