# Sprite System Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace placeholder colored rectangles with PixiJS Sprite/AnimatedSprite rendering, driven by a new SpriteManager class that loads TexturePacker sprite sheets and individual PNG textures with graceful fallback.

**Architecture:** New `SpriteManager` class (`src/lib/engine/sprites.ts`) owns all texture loading, sprite creation, and fallback logic. The existing `GameRenderer` delegates sprite creation to it instead of drawing Graphics directly. Zero visual regression — every missing texture falls back to the current colored rectangle.

**Tech Stack:** TypeScript, PixiJS v8 (`Sprite`, `AnimatedSprite`, `Spritesheet`, `Assets`, `Texture`, `Container`, `Graphics`), vitest

**Spec:** `docs/plans/2026-03-18-sprite-system-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/lib/engine/sprites.ts` | **Create** — SpriteManager class: texture loading, caching, sprite/fallback creation, animation state management, missing-asset logging |
| `src/lib/engine/sprites.test.ts` | **Create** — Unit tests for SpriteManager (texture caching, fallback logic, animation switching, missing-asset dedup) |
| `src/lib/engine/renderer.ts` | **Modify** — Import SpriteManager, delegate sprite creation, rename `avatarGraphics` → `avatarContainer`, add `updateAvatar()` call |
| `assets/sprites/avatar/avatar.json` | **Create** — TexturePacker JSON Hash spritesheet metadata for avatar |
| `assets/sprites/avatar/avatar.png` | **Create** — Placeholder avatar sprite sheet (programmatic, generated in test or by script) |
| `assets/sprites/entities/tree_fruit.png` | **Create** — Placeholder entity texture |
| `assets/sprites/items/cherry.png` | **Create** — Placeholder item texture |
| `assets/sprites/decos/tree_bg.png` | **Create** — Placeholder deco texture |

---

## Chunk 1: SpriteManager Core

### Task 1: SpriteManager — Texture Cache & Fallback Creators

**Files:**
- Create: `src/lib/engine/sprites.ts`
- Create: `src/lib/engine/sprites.test.ts`

**Context for implementer:**

The `SpriteManager` class manages texture loading, sprite creation, and fallback rendering. It uses PixiJS v8's `Assets.load()` for individual PNGs and `Spritesheet` for animated sprite sheets. When textures are missing, it returns the exact same colored rectangles the renderer currently draws.

Key PixiJS v8 imports (all from `'pixi.js'`):
- `Assets` — static asset loader: `await Assets.load('path/to/file.png')` returns `Texture`; `await Assets.load('path/to/sheet.json')` returns `Spritesheet`
- `Spritesheet` — parsed atlas: `spritesheet.animations['walking']` returns `Texture[]`
- `AnimatedSprite` — frame-based animation: `new AnimatedSprite({ textures, animationSpeed, loop: true })`
- `Sprite` — single texture: `new Sprite(texture)`
- `Container`, `Graphics`, `Text` — scene graph nodes

Types from `'../types'`:
- `StreetData` — has `layers[].decos[]` with `spriteClass`, `w`, `h`, `x`, `y`, `r`, `hFlip`
- `Deco` — `{ spriteClass, x, y, w, h, z, r, hFlip }`
- `WorldEntityFrame` — `{ id, spriteClass, name, x, y, cooldownRemaining, depleted }`
- `WorldItemFrame` — `{ id, itemId, name, icon, count, x, y }`
- `AnimationState` — `'idle' | 'walking' | 'jumping' | 'falling'`
- `Direction` — `'left' | 'right'`

- [ ] **Step 1: Write failing tests for SpriteManager**

Create `src/lib/engine/sprites.test.ts`:

```typescript
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
    Spritesheet: vi.fn(() => mockSpritesheet),
    AnimatedSprite: vi.fn(() => ({
      anchor: { set: vi.fn() },
      textures: [],
      animationSpeed: 0,
      loop: true,
      play: vi.fn(),
      gotoAndStop: vi.fn(),
      destroy: vi.fn(),
    })),
    Sprite: vi.fn(() => ({
      anchor: { set: vi.fn() },
      width: 0,
      height: 0,
      destroy: vi.fn(),
    })),
    Container: vi.fn(() => ({
      addChild: vi.fn(),
      children: [],
      destroy: vi.fn(),
      scale: { x: 1 },
    })),
    Graphics: vi.fn(() => ({
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
    })),
    Text: vi.fn(() => ({
      anchor: { set: vi.fn() },
      text: '',
      x: 0,
      y: 0,
      destroy: vi.fn(),
    })),
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
    it('logs missing entity texture only once per spriteClass', () => {
      const consoleSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
      const entity = {
        id: 'e1', entityType: 'npc', name: 'Chicken',
        spriteClass: 'npc_chicken', x: 0, y: 0,
        cooldownRemaining: null, depleted: false,
      };
      manager.createEntity(entity);
      manager.createEntity({ ...entity, id: 'e2' });
      const chickenWarnings = consoleSpy.mock.calls.filter(
        (args) => String(args[0]).includes('npc_chicken')
      );
      expect(chickenWarnings.length).toBe(1);
      consoleSpy.mockRestore();
    });
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npx vitest run src/lib/engine/sprites.test.ts`
Expected: FAIL — module `./sprites` not found.

- [ ] **Step 3: Implement SpriteManager**

Create `src/lib/engine/sprites.ts`:

```typescript
import {
  Assets,
  AnimatedSprite,
  Container,
  Graphics,
  Sprite,
  Text,
  Texture,
} from 'pixi.js';
import type { Spritesheet } from 'pixi.js';
import type {
  AnimationState,
  Deco,
  Direction,
  StreetData,
  WorldEntityFrame,
  WorldItemFrame,
} from '../types';

const ANIMATION_SPEEDS: Record<AnimationState, number> = {
  idle: 0.08,
  walking: 0.15,
  jumping: 0.1,
  falling: 0.1,
};

export class SpriteManager {
  private textureCache: Map<string, Texture> = new Map();
  private avatarSheet: Spritesheet | null = null;
  private warnedMissing: Set<string> = new Set();
  private currentAvatarAnimation: AnimationState | null = null;
  private avatarAnimatedSprite: AnimatedSprite | null = null;

  async init(): Promise<void> {
    try {
      this.avatarSheet = await Assets.load('sprites/avatar/avatar.json');
    } catch {
      console.warn('[SpriteManager] Avatar spritesheet not found, using fallback');
    }
  }

  async loadStreetAssets(street: StreetData): Promise<void> {
    const decoClasses = new Set<string>();
    for (const layer of street.layers) {
      for (const deco of layer.decos) {
        decoClasses.add(deco.spriteClass);
      }
    }

    const missing: string[] = [];
    for (const spriteClass of decoClasses) {
      if (this.textureCache.has(spriteClass)) continue;
      try {
        const texture = await Assets.load(`sprites/decos/${spriteClass}.png`);
        this.textureCache.set(spriteClass, texture);
      } catch {
        missing.push(spriteClass);
      }
    }

    if (missing.length > 0) {
      console.warn(
        `[SpriteManager] Missing deco textures for street "${street.name}":\n  ${missing.join(', ')}`
      );
    }
  }

  hasTexture(spriteClass: string): boolean {
    return this.textureCache.has(spriteClass);
  }

  createAvatar(): Container {
    const container = new Container();

    if (this.avatarSheet) {
      const idleTextures = this.avatarSheet.animations['idle'];
      if (idleTextures) {
        const animated = new AnimatedSprite({
          textures: idleTextures,
          animationSpeed: ANIMATION_SPEEDS.idle,
          loop: true,
        });
        animated.anchor.set(0.5, 1);
        animated.play();
        container.addChild(animated);
        this.avatarAnimatedSprite = animated;
        this.currentAvatarAnimation = 'idle';
        return container;
      }
    }

    // Fallback: blue rect 30x60, matching renderer.ts line 188-190
    const g = new Graphics();
    g.rect(-15, -60, 30, 60);
    g.fill(0x5865f2);
    container.addChild(g);
    this.avatarAnimatedSprite = null;
    this.currentAvatarAnimation = null;
    return container;
  }

  updateAvatar(container: Container, animation: AnimationState, facing: Direction): void {
    container.scale.x = facing === 'right' ? 1 : -1;

    if (!this.avatarAnimatedSprite || !this.avatarSheet) return;
    if (animation === this.currentAvatarAnimation) return;

    const textures = this.avatarSheet.animations[animation];
    if (textures) {
      this.avatarAnimatedSprite.textures = textures;
      this.avatarAnimatedSprite.animationSpeed = ANIMATION_SPEEDS[animation];
      this.avatarAnimatedSprite.play();
      this.currentAvatarAnimation = animation;
    }
  }

  createDeco(deco: Deco): Container {
    const texture = this.textureCache.get(deco.spriteClass);
    if (texture) {
      const sprite = new Sprite(texture);
      sprite.anchor.set(0.5, 1);
      sprite.width = deco.w;
      sprite.height = deco.h;
      if (deco.hFlip) {
        sprite.scale.x *= -1;
      }
      sprite.rotation = deco.r;
      return sprite;
    }

    // Fallback: green rect matching renderer.ts lines 160-171
    const g = new Graphics();
    g.rect(0, -deco.h, deco.w, deco.h);
    g.fill({ color: 0x4a6741, alpha: 0.3 });
    if (deco.hFlip) {
      g.scale.x = -1;
    }
    g.rotation = deco.r;
    return g;
  }

  createEntity(entity: WorldEntityFrame): Container {
    const texture = this.tryLoadEntityTexture(entity.spriteClass);
    const container = new Container();

    if (texture) {
      const isTree = entity.spriteClass.startsWith('tree');
      const sprite = new Sprite(texture);
      sprite.anchor.set(0.5, 1);
      sprite.width = isTree ? 60 : 30;
      sprite.height = isTree ? 80 : 30;
      container.addChild(sprite);

      const label = new Text({
        text: entity.name,
        style: { fontSize: 10, fill: 0xffffff, align: 'center' },
      });
      label.anchor.set(0.5, 1);
      label.y = -(isTree ? 80 : 30) - 4;
      container.addChild(label);
      return container;
    }

    // Fallback: colored rect matching renderer.ts lines 327-342
    const body = new Graphics();
    const isTree = entity.spriteClass.startsWith('tree');
    const color = isTree ? 0x2d8a4e : 0xc4a35a;
    const w = isTree ? 60 : 30;
    const h = isTree ? 80 : 30;
    body.rect(-w / 2, -h, w, h);
    body.fill({ color, alpha: 1.0 });
    container.addChild(body);

    const label = new Text({
      text: entity.name,
      style: { fontSize: 10, fill: 0xffffff, align: 'center' },
    });
    label.anchor.set(0.5, 1);
    label.y = -h - 4;
    container.addChild(label);

    return container;
  }

  createGroundItem(item: WorldItemFrame): Container {
    const texture = this.tryLoadItemTexture(item.icon);
    const container = new Container();

    if (texture) {
      const sprite = new Sprite(texture);
      sprite.anchor.set(0.5, 1);
      sprite.width = 16;
      sprite.height = 16;
      container.addChild(sprite);

      const label = new Text({
        text: item.count > 1 ? `${item.name} x${item.count}` : item.name,
        style: { fontSize: 9, fill: 0xffffff, align: 'center' },
      });
      label.anchor.set(0.5, 1);
      label.y = -18;
      container.addChild(label);
      return container;
    }

    // Fallback: gold circle matching renderer.ts lines 373-384
    const body = new Graphics();
    body.circle(0, -8, 8);
    body.fill({ color: 0xe8c170, alpha: 0.9 });
    container.addChild(body);

    const label = new Text({
      text: item.count > 1 ? `${item.name} x${item.count}` : item.name,
      style: { fontSize: 9, fill: 0xffffff, align: 'center' },
    });
    label.anchor.set(0.5, 1);
    label.y = -18;
    container.addChild(label);

    return container;
  }

  private tryLoadEntityTexture(spriteClass: string): Texture | null {
    if (this.textureCache.has(spriteClass)) {
      return this.textureCache.get(spriteClass)!;
    }
    // Fire-and-forget async load — returns null now, cached for next encounter
    if (!this.warnedMissing.has(`entity:${spriteClass}`)) {
      this.warnedMissing.add(`entity:${spriteClass}`);
      Assets.load(`sprites/entities/${spriteClass}.png`)
        .then((texture: Texture) => { this.textureCache.set(spriteClass, texture); })
        .catch(() => {
          console.warn(`[SpriteManager] Missing entity texture: ${spriteClass}`);
        });
    }
    return null;
  }

  private tryLoadItemTexture(icon: string): Texture | null {
    const cacheKey = `item:${icon}`;
    if (this.textureCache.has(cacheKey)) {
      return this.textureCache.get(cacheKey)!;
    }
    // Fire-and-forget async load — returns null now, cached for next encounter
    if (!this.warnedMissing.has(cacheKey)) {
      this.warnedMissing.add(cacheKey);
      Assets.load(`sprites/items/${icon}.png`)
        .then((texture: Texture) => { this.textureCache.set(cacheKey, texture); })
        .catch(() => {
          console.warn(`[SpriteManager] Missing item texture: ${icon}`);
        });
    }
    return null;
  }

  destroy(): void {
    this.textureCache.clear();
    this.avatarSheet = null;
    this.avatarAnimatedSprite = null;
    this.currentAvatarAnimation = null;
    this.warnedMissing.clear();
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npx vitest run src/lib/engine/sprites.test.ts`
Expected: All 6 tests pass.

- [ ] **Step 5: Verify full build**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: Build succeeds with no TypeScript errors.

- [ ] **Step 6: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src/lib/engine/sprites.ts src/lib/engine/sprites.test.ts
git commit -m "feat: add SpriteManager with texture loading and fallback creators

SpriteManager loads TexturePacker JSON Hash spritesheets for animated
entities and individual PNGs for static assets. Falls back to colored
rectangles matching current renderer output when textures are missing.
Deduplicates missing-asset warnings."
```

---

## Chunk 2: Renderer Integration

### Task 2: Wire SpriteManager into GameRenderer

**Files:**
- Modify: `src/lib/engine/renderer.ts`

**Context for implementer:**

The `GameRenderer` class (670 lines) manages the PixiJS scene graph. Currently it draws all visual elements as colored `Graphics` rectangles directly. This task replaces those direct `Graphics` calls with delegated `spriteManager.create*()` calls, and adds `spriteManager.updateAvatar()` for animation state switching.

Key changes:
1. Import `SpriteManager`, add it as a field
2. Call `spriteManager.init()` in `init()`
3. Call `spriteManager.loadStreetAssets(street)` in `buildScene()`
4. Replace avatar Graphics with `spriteManager.createAvatar()`
5. Replace deco Graphics with `spriteManager.createDeco(deco)`
6. Replace entity Graphics with `spriteManager.createEntity(entity)`
7. Replace ground item Graphics with `spriteManager.createGroundItem(item)`
8. Rename `avatarGraphics` → `avatarContainer` (type widens from `Graphics` to `Container`)
9. Add `spriteManager.updateAvatar()` call in `updateFrame()`
10. Add `spriteManager.destroy()` in `destroy()`

The field `avatarGraphics` is referenced in:
- `updateFrame()` — line 238 (null check), lines 246-248 (position, scale)
- `updateTransition()` — lines 531-532 (iris center calculation)
- `updateChatBubbles()` — lines 492-495 (local player bubble positioning)
- `buildScene()` — lines 188-191 (creation)
- `destroy()` — not currently explicitly destroyed (it's a child of worldContainer)

All occurrences of `this.avatarGraphics` must be renamed to `this.avatarContainer`.

- [ ] **Step 1: Add SpriteManager import and field**

In `renderer.ts`, update the import line (line 1) from:
```typescript
import { Application, Container, FillGradient, Graphics, Text } from 'pixi.js';
```
To:
```typescript
import { Application, Container, FillGradient, Graphics, Text } from 'pixi.js';
import { SpriteManager } from './sprites';
```

Add field after `private swirlPositions` (line 47):
```typescript
  private spriteManager: SpriteManager;
```

In the constructor (after line 55, `this.transitionContainer.visible = false;`), add:
```typescript
    this.spriteManager = new SpriteManager();
```

- [ ] **Step 2: Initialize SpriteManager in `init()`**

In the `init()` method, add after the resize handler (after line 92, before the closing `}`):
```typescript
    await this.spriteManager.init();
```

- [ ] **Step 3: Rename `avatarGraphics` to `avatarContainer`**

Change the field declaration (line 34) from:
```typescript
  private avatarGraphics: Graphics | null = null;
```
To:
```typescript
  private avatarContainer: Container | null = null;
```

Then rename ALL occurrences of `this.avatarGraphics` to `this.avatarContainer` throughout the file. There are references in:
- `buildScene()` — lines 188-191
- `updateFrame()` — lines 238, 246-248
- `updateTransition()` — lines 531-532
- `updateChatBubbles()` — lines 492-495

- [ ] **Step 4: Replace deco creation in `buildScene()`**

Replace the deco drawing loop (lines 157-172 of `buildScene()`):

```typescript
      // Draw decos as placeholder rectangles (until real art assets are available).
      // Rect drawn at local origin so g.rotation pivots around the deco's anchor.
      for (const deco of layer.decos) {
        const g = new Graphics();
        const screenY = deco.y - street.top;
        g.rect(0, -deco.h, deco.w, deco.h);
        g.fill({ color: 0x4a6741, alpha: 0.3 });
        g.x = deco.x - street.left;
        g.y = screenY;
        if (deco.hFlip) {
          g.scale.x = -1;
          g.x += deco.w;
        }
        g.rotation = deco.r;
        container.addChild(g);
      }
```

With:

```typescript
      for (const deco of layer.decos) {
        const decoDisplay = this.spriteManager.createDeco(deco);
        const screenY = deco.y - street.top;
        if (this.spriteManager.hasTexture(deco.spriteClass)) {
          // Sprite with center-bottom anchor: offset x by half-width
          decoDisplay.x = deco.x - street.left + deco.w / 2;
          decoDisplay.y = screenY;
        } else {
          // Fallback Graphics: positioned same as original code
          decoDisplay.x = deco.x - street.left;
          decoDisplay.y = screenY;
          if (deco.hFlip) {
            decoDisplay.x += deco.w;
          }
        }
        container.addChild(decoDisplay);
      }
```

- [ ] **Step 5: Replace avatar creation in `buildScene()`**

Replace lines 187-191:

```typescript
    // Create avatar placeholder
    this.avatarGraphics = new Graphics();
    this.avatarGraphics.rect(-15, -60, 30, 60);
    this.avatarGraphics.fill(0x5865f2);
    this.worldContainer.addChild(this.avatarGraphics);
```

With:

```typescript
    // Create avatar (AnimatedSprite or fallback rectangle)
    this.avatarContainer = this.spriteManager.createAvatar();
    this.worldContainer.addChild(this.avatarContainer);
```

- [ ] **Step 6: Make `buildScene` async and await `loadStreetAssets`**

Change the `buildScene` method signature (line 126) from:
```typescript
  buildScene(street: StreetData): void {
```
To:
```typescript
  async buildScene(street: StreetData): Promise<void> {
```

Add at the very beginning of `buildScene()`, after `this.street = street;` (line 127):

```typescript
    // Pre-load deco textures so createDeco() can use them below
    await this.spriteManager.loadStreetAssets(street);
```

The caller is `src/lib/components/GameCanvas.svelte` line 74: `r.buildScene(street)`. This is inside an `onMount` async callback, so change it to `await r.buildScene(street)`. The `startGame()` call on line 75 must happen after `buildScene` completes, so the `await` is both correct and necessary.

- [ ] **Step 7: Replace entity creation in `updateFrame()`**

Replace the entity creation block (lines 326-342):

```typescript
      if (!sprite) {
        sprite = new Container();
        const body = new Graphics();
        const color = entity.spriteClass.startsWith('tree') ? 0x2d8a4e : 0xc4a35a;
        const w = entity.spriteClass.startsWith('tree') ? 60 : 30;
        const h = entity.spriteClass.startsWith('tree') ? 80 : 30;
        body.rect(-w / 2, -h, w, h);
        body.fill({ color, alpha: 1.0 });
        sprite.addChild(body);

        const label = new Text({
          text: entity.name,
          style: { fontSize: 10, fill: 0xffffff, align: 'center' },
        });
        label.anchor.set(0.5, 1);
        label.y = -h - 4;
        sprite.addChild(label);

        this.worldContainer.addChild(sprite);
        this.entitySprites.set(entity.id, sprite);
      }
```

With:

```typescript
      if (!sprite) {
        sprite = this.spriteManager.createEntity(entity);
        this.worldContainer.addChild(sprite);
        this.entitySprites.set(entity.id, sprite);
      }
```

- [ ] **Step 8: Replace ground item creation in `updateFrame()`**

Replace the ground item creation block (lines 371-384):

```typescript
      if (!sprite) {
        sprite = new Container();
        const body = new Graphics();
        body.circle(0, -8, 8);
        body.fill({ color: 0xe8c170, alpha: 0.9 });
        sprite.addChild(body);

        const label = new Text({
          text: item.count > 1 ? `${item.name} x${item.count}` : item.name,
          style: { fontSize: 9, fill: 0xffffff, align: 'center' },
        });
        label.anchor.set(0.5, 1);
        label.y = -18;
        sprite.addChild(label);

        this.worldContainer.addChild(sprite);
        this.groundItemSprites.set(item.id, sprite);
      }
```

With:

```typescript
      if (!sprite) {
        sprite = this.spriteManager.createGroundItem(item);
        this.worldContainer.addChild(sprite);
        this.groundItemSprites.set(item.id, sprite);
      }
```

- [ ] **Step 9: Add avatar animation update in `updateFrame()`**

After the avatar position/facing update (after line 248, `this.avatarContainer.scale.x = ...`), add:

```typescript
    this.spriteManager.updateAvatar(this.avatarContainer, frame.player.animation, frame.player.facing);
```

And remove the `scale.x` line from `updateFrame()` since `updateAvatar()` now handles facing:

Replace:
```typescript
    this.avatarContainer.x = avatarScreenX;
    this.avatarContainer.y = avatarScreenY;
    this.avatarContainer.scale.x = frame.player.facing === 'right' ? 1 : -1;
```

With:
```typescript
    this.avatarContainer.x = avatarScreenX;
    this.avatarContainer.y = avatarScreenY;
    this.spriteManager.updateAvatar(this.avatarContainer, frame.player.animation, frame.player.facing);
```

- [ ] **Step 10: Add SpriteManager cleanup in `destroy()`**

In the `destroy()` method, add before `this.app.destroy(true);` (before line 667):

```typescript
    this.spriteManager.destroy();
```

- [ ] **Step 11: Verify build**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: Build succeeds with no TypeScript errors.

- [ ] **Step 12: Run all tests**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npx vitest run`
Expected: All tests pass (SpriteManager unit tests + any existing tests).

- [ ] **Step 13: Verify Rust tests unaffected**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test --workspace`
Expected: All Rust tests pass (no Rust changes made).

- [ ] **Step 14: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src/lib/engine/renderer.ts
git commit -m "feat: wire SpriteManager into GameRenderer

Replace direct Graphics drawing with spriteManager.create*() calls for
decos, entities, ground items, and avatar. Add animation state switching
via updateAvatar(). Rename avatarGraphics to avatarContainer. Zero
visual regression — all missing textures fall back to current
colored rectangles."
```

---

### Task 3: Placeholder Art Assets

**Files:**
- Create: `assets/sprites/avatar/avatar.json`
- Create: `assets/sprites/avatar/avatar.png`
- Create: `assets/sprites/entities/tree_fruit.png`
- Create: `assets/sprites/items/cherry.png`
- Create: `assets/sprites/decos/tree_bg.png`

**Context for implementer:**

Create minimal placeholder art to validate every sprite code path. These are throwaway assets — simple geometric shapes or AI-generated pixel art. The avatar needs a multi-frame spritesheet; the rest are single-image PNGs.

The avatar spritesheet uses TexturePacker JSON Hash format. PixiJS v8 loads it via `Assets.load('sprites/avatar/avatar.json')` which auto-discovers the `avatar.png` referenced in the JSON `meta.image` field. The JSON and PNG must be in the same directory.

Asset paths are relative to the Vite public directory. In a Tauri app, static assets served from `assets/` are available at the root URL path. The `sprites/` prefix in code maps to `assets/sprites/` on disk.

For the avatar PNG: create a simple sprite sheet image with distinct colored rectangles for each animation frame (e.g., different shades of blue for walk frames, green for idle, yellow for jump, red for fall). Each frame is 30×60px. The atlas is 256×256px.

For other PNGs: simple colored shapes at the specified dimensions.

**Note:** Binary PNG files cannot be created with the Write tool. Use a programmatic approach — either a Node.js script that generates them, or find a minimal PNG creation method.

- [ ] **Step 1: Create avatar spritesheet JSON**

Create `assets/sprites/avatar/avatar.json`:

```json
{
  "frames": {
    "idle_0": { "frame": {"x": 0, "y": 0, "w": 30, "h": 60} },
    "idle_1": { "frame": {"x": 30, "y": 0, "w": 30, "h": 60} },
    "walk_0": { "frame": {"x": 0, "y": 60, "w": 30, "h": 60} },
    "walk_1": { "frame": {"x": 30, "y": 60, "w": 30, "h": 60} },
    "walk_2": { "frame": {"x": 60, "y": 60, "w": 30, "h": 60} },
    "walk_3": { "frame": {"x": 90, "y": 60, "w": 30, "h": 60} },
    "jump_0": { "frame": {"x": 0, "y": 120, "w": 30, "h": 60} },
    "fall_0": { "frame": {"x": 30, "y": 120, "w": 30, "h": 60} }
  },
  "animations": {
    "idle": ["idle_0", "idle_1"],
    "walking": ["walk_0", "walk_1", "walk_2", "walk_3"],
    "jumping": ["jump_0"],
    "falling": ["fall_0"]
  },
  "meta": {
    "image": "avatar.png",
    "size": { "w": 256, "h": 256 },
    "scale": 1
  }
}
```

- [ ] **Step 2: Generate placeholder PNG files**

Create a Node.js script `scripts/generate-placeholders.mjs` that generates all placeholder PNGs using the `canvas` npm package (or write raw PNG bytes). Run it once to create the files, then delete the script.

Alternative: Use ImageMagick if available:
```bash
# Create asset directories
mkdir -p assets/sprites/avatar assets/sprites/entities assets/sprites/items assets/sprites/decos

# Avatar: 256x256 atlas with colored frames
# (Use whatever tool is available to create a simple colored-rectangle spritesheet)
convert -size 256x256 xc:transparent \
  -fill '#5865f2' -draw 'rectangle 0,0 29,59' \
  -fill '#4455e0' -draw 'rectangle 30,0 59,59' \
  -fill '#6070ff' -draw 'rectangle 0,60 29,119' \
  -fill '#5060ee' -draw 'rectangle 30,60 59,119' \
  -fill '#7080ff' -draw 'rectangle 60,60 89,119' \
  -fill '#4050dd' -draw 'rectangle 90,60 119,119' \
  -fill '#88cc44' -draw 'rectangle 0,120 29,179' \
  -fill '#cc4444' -draw 'rectangle 30,120 59,179' \
  assets/sprites/avatar/avatar.png

# Entity: 60x80 green tree silhouette
convert -size 60x80 xc:'#2d8a4e' assets/sprites/entities/tree_fruit.png

# Item: 16x16 red cherry circle
convert -size 16x16 xc:transparent -fill '#e84040' -draw 'circle 8,8 8,1' assets/sprites/items/cherry.png

# Deco: 120x200 semi-transparent green tree
convert -size 120x200 xc:'rgba(74,103,65,0.6)' assets/sprites/decos/tree_bg.png
```

If ImageMagick is not available, use Python with Pillow, or any other method to create simple colored PNG files. The exact method is flexible — what matters is that the PNGs exist at the right paths with approximately the right dimensions.

- [ ] **Step 3: Verify assets load in build**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: Build succeeds. Assets in `assets/sprites/` are copied to the build output.

- [ ] **Step 4: Manual smoke test**

Run: `npm run tauri dev`
Test:
1. Walk player around — verify avatar uses AnimatedSprite (distinct colored frames per animation state) instead of solid blue rect
2. Stop moving — verify idle animation plays (alternating frames)
3. Jump — verify jump frame shows, then fall frame
4. Face left/right — verify sprite flips
5. Look at `tree_bg` decos — verify they show the placeholder sprite scaled to deco dimensions (decos are pre-loaded via `await loadStreetAssets`, so they work on first visit)
6. Look at `tree_fruit` entities — on first frame they show fallback rectangle (lazy async load in progress), but on subsequent encounters (e.g. after street transition and back) they show the placeholder sprite. Verify the async load fires by checking console for no `Missing entity texture: tree_fruit` error.
7. Look at `cherry` ground items — same lazy behavior as entities. First frame shows fallback, cached for next encounter.
8. Decos/entities without matching art still show colored rectangles (no regression)
9. Console shows `[SpriteManager] Missing deco textures...` warning with list of unmatched deco classes
10. Street transition works correctly (iris wipe unaffected)

- [ ] **Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add assets/sprites/ scripts/generate-placeholders.mjs
git commit -m "feat: add placeholder sprite assets for sprite system validation

TexturePacker JSON Hash spritesheet for avatar (8 frames: idle, walk,
jump, fall). Individual PNG placeholders for tree_fruit entity, cherry
item, and tree_bg deco. All are colored geometric shapes to be replaced
by real Glitch art via the asset pipeline."
```

If the generation script was deleted, just `git add assets/sprites/`.
