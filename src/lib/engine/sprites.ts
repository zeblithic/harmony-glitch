import {
  Assets,
  Container,
  Graphics,
  Sprite,
  Text,
} from 'pixi.js';
import type { Texture } from 'pixi.js';
import type {
  Deco,
  StreetData,
  WorldEntityFrame,
  WorldItemFrame,
} from '../types';

export class SpriteManager {
  private textureCache: Map<string, Texture> = new Map();
  private warnedMissing: Set<string> = new Set();

  async init(): Promise<void> {
    // Load atlases if they exist — individual PNGs still work as fallback
    await Promise.all([
      this.loadAtlas('items', 'sprites/items/items.json'),
      this.loadAtlas('entities', 'sprites/entities/entities.json'),
    ]);
  }

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

  async loadStreetAssets(street: StreetData): Promise<void> {
    const decoClasses = new Set<string>();
    for (const layer of street.layers) {
      for (const deco of layer.decos) {
        decoClasses.add(deco.spriteClass);
      }
    }

    const missing: string[] = [];
    const toLoad = [...decoClasses].filter(c => !this.textureCache.has(`deco:${c}`));
    await Promise.all(toLoad.map(async (spriteClass) => {
      try {
        const texture = await Assets.load(`sprites/decos/${spriteClass}.png`);
        this.textureCache.set(`deco:${spriteClass}`, texture);
      } catch {
        missing.push(spriteClass);
      }
    }));

    if (missing.length > 0) {
      console.warn(
        `[SpriteManager] Missing deco textures for street "${street.name}":\n  ${missing.sort().join(', ')}`
      );
    }
  }

  /** Check if a deco spriteClass has a cached texture (used for deco positioning logic). */
  hasTexture(spriteClass: string): boolean {
    return this.textureCache.has(`deco:${spriteClass}`);
  }

  /** Check if an entity texture has been async-loaded into cache. */
  hasEntityTexture(spriteClass: string): boolean {
    return this.textureCache.has(`entity:${spriteClass}`);
  }

  /** Check if an item texture has been async-loaded into cache. */
  hasItemTexture(icon: string): boolean {
    return this.textureCache.has(`item:${icon}`);
  }

  createDeco(deco: Deco): Container {
    const texture = this.textureCache.get(`deco:${deco.spriteClass}`);
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

    // Fallback: green rect
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
      const w = isTree ? 60 : 30;
      const h = isTree ? 80 : 30;
      const sprite = new Sprite(texture);
      sprite.anchor.set(0.5, 1);
      sprite.width = w;
      sprite.height = h;
      container.addChild(sprite);

      const label = new Text({
        text: entity.name,
        style: { fontSize: 10, fill: 0xffffff, align: 'center' },
      });
      label.anchor.set(0.5, 1);
      label.y = -h - 4;
      container.addChild(label);
      return container;
    }

    // Fallback: colored rect (label marks it for upgrade once texture loads)
    container.label = 'fallback';
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

    // Fallback: gold circle (label marks it for upgrade once texture loads)
    container.label = 'fallback';
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
    const cacheKey = `entity:${spriteClass}`;
    if (this.textureCache.has(cacheKey)) {
      return this.textureCache.get(cacheKey)!;
    }
    // Fire-and-forget async load — returns null now, cached for next encounter
    if (!this.warnedMissing.has(cacheKey)) {
      this.warnedMissing.add(cacheKey);
      Assets.load(`sprites/entities/${spriteClass}.png`)
        .then((texture: Texture) => { this.textureCache.set(cacheKey, texture); })
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
    this.warnedMissing.clear();
  }
}
