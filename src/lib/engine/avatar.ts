import { Assets, AnimatedSprite, Container, Graphics } from 'pixi.js';
import type { Spritesheet } from 'pixi.js';
import type { AnimationState, AvatarAppearance, AvatarManifest, Direction } from '../types';

const AVATAR_ASSET_BASE = '/assets/sprites/avatar';

const ANIMATION_SPEEDS: Record<AnimationState, number> = {
  idle: 0.08,
  walking: 0.15,
  jumping: 0.1,
  falling: 0.1,
};

/**
 * Z-order for avatar layers (back to front).
 * Wardrobe slots have sub-part entries for multi-part rendering — offside
 * limbs behind the body, close limbs in front. Entries with a `part` field
 * use the sprite sheet at `{category}/{itemId}.{part}.json`. Entries without
 * `part` use `{category}/{itemId}.json` (vanity items, single-sprite fallback).
 */
const LAYER_ORDER: { slot: string; category: string; part?: string }[] = [
  { slot: 'body', category: 'base' },
  // Offside layers (behind body)
  { slot: 'shoes', category: 'shoes', part: 'bootFootOffside' },
  { slot: 'shoes', category: 'shoes', part: 'bootUpperOffside' },
  { slot: 'shoes', category: 'shoes', part: 'shoeUpperOffside' },
  { slot: 'shoes', category: 'shoes', part: 'shoeToeOffside' },
  { slot: 'shoes', category: 'shoes', part: 'shoeOffside' },
  { slot: 'pants', category: 'pants', part: 'pantsFootOffside' },
  { slot: 'pants', category: 'pants', part: 'pantsLegLowerOffside' },
  { slot: 'pants', category: 'pants', part: 'pantsLegUpperOffside' },
  { slot: 'shirt', category: 'shirt', part: 'sleeveUpperOffside' },
  { slot: 'shirt', category: 'shirt', part: 'sleeveLowerOffside' },
  { slot: 'dress', category: 'dress', part: 'dressSleeveUpperOffside' },
  { slot: 'dress', category: 'dress', part: 'dressSleeveLowerOffside' },
  { slot: 'dress', category: 'dress', part: 'dressOffside' },
  { slot: 'coat', category: 'coat', part: 'coatSleeveUpperOffside' },
  { slot: 'coat', category: 'coat', part: 'coatSleeveLowerOffside' },
  { slot: 'coat', category: 'coat', part: 'coatOffside' },
  // Main torso layers
  { slot: 'pants', category: 'pants', part: 'pantsTop' },
  { slot: 'shirt', category: 'shirt', part: 'shirtTorso' },
  { slot: 'dress', category: 'dress', part: 'dress' },
  { slot: 'skirt', category: 'skirt', part: 'skirt' },
  { slot: 'coat', category: 'coat', part: 'coatClose' },
  // Single-sprite fallback for items without parts (loads {itemId}.json)
  { slot: 'shoes', category: 'shoes' },
  { slot: 'pants', category: 'pants' },
  { slot: 'shirt', category: 'shirt' },
  { slot: 'dress', category: 'dress' },
  { slot: 'skirt', category: 'skirt' },
  { slot: 'coat', category: 'coat' },
  // Close layers (in front of body)
  { slot: 'pants', category: 'pants', part: 'pantsLegUpperClose' },
  { slot: 'pants', category: 'pants', part: 'pantsLegLowerClose' },
  { slot: 'pants', category: 'pants', part: 'pantsFootClose' },
  { slot: 'shirt', category: 'shirt', part: 'sleeveUpperClose' },
  { slot: 'shirt', category: 'shirt', part: 'sleeveLowerClose' },
  { slot: 'dress', category: 'dress', part: 'dressSleeveUpperClose' },
  { slot: 'dress', category: 'dress', part: 'dressSleeveLowerClose' },
  { slot: 'coat', category: 'coat', part: 'coatSleeveUpperClose' },
  { slot: 'coat', category: 'coat', part: 'coatSleeveLowerClose' },
  { slot: 'shoes', category: 'shoes', part: 'shoeUpperClose' },
  { slot: 'shoes', category: 'shoes', part: 'shoeToeClose' },
  { slot: 'shoes', category: 'shoes', part: 'shoeClose' },
  { slot: 'shoes', category: 'shoes', part: 'bootUpperClose' },
  { slot: 'shoes', category: 'shoes', part: 'bootFootClose' },
  // Face and head
  { slot: 'eyes', category: 'eyes' },
  { slot: 'nose', category: 'nose' },
  { slot: 'mouth', category: 'mouth' },
  { slot: 'ears', category: 'ears' },
  { slot: 'hair', category: 'hair' },
  { slot: 'hat', category: 'hat', part: 'sideHat' },
  { slot: 'hat', category: 'hat', part: 'sideHeaddressClose' },
  { slot: 'hat', category: 'hat' },
  { slot: 'bracelet', category: 'bracelet', part: 'braceletClose' },
  { slot: 'bracelet', category: 'bracelet' },
];

/** Slots that receive skin color tinting. */
const SKIN_TINT_SLOTS = new Set(['body']);

/** Slots that receive hair color tinting. */
const HAIR_TINT_SLOTS = new Set(['hair']);

/**
 * Display scale for the avatar. The base body sprite sheet is rendered at 8x
 * (544×1013 per frame). ~240px tall is roughly 4× the 30×60 collision box,
 * which gives readable clothing detail without dwarfing the physics shape.
 */
const DISPLAY_SCALE = 240 / 1013;

/** Fade-in duration for newly loaded layers (in seconds at 60fps). */
const FADE_IN_RATE = 1 / 10; // ~10 frames = ~167ms at 60fps

/**
 * Layered avatar compositor for PixiJS.
 *
 * Loads per-slot sprite sheets, composites them into a Container with correct
 * z-order, applies color tinting, and synchronizes animation across all layers.
 *
 * Wardrobe items may have multiple sub-parts (e.g., shoes → shoeClose +
 * shoeOffside). The manifest's `parts` array determines which sub-sprite
 * sheets to load. Items without `parts` load a single sprite sheet.
 */
export class AvatarCompositor {
  private container: Container;
  private layers: Map<string, AnimatedSprite> = new Map();
  private sheets: Map<string, Spritesheet> = new Map();
  private appearance: AvatarAppearance | null = null;
  private currentAnimation: AnimationState | null = null;
  private manifest: AvatarManifest | null = null;
  private fadingIn: Set<string> = new Set();
  private debugOverlayEnabled = false;
  private debugGraphics: Graphics | null = null;

  constructor() {
    this.container = new Container();
    this.container.scale.set(DISPLAY_SCALE);
  }

  getContainer(): Container {
    return this.container;
  }

  /**
   * Toggle a debug overlay that visualizes the canonical 544×1013 avatar
   * canvas, the anchor point, and each layer's orig+trim-derived expected
   * bounds. If a sprite's rendered pixels don't align with its green bounds
   * box, PixiJS isn't honoring the trim metadata.
   */
  setDebugOverlay(enabled: boolean): void {
    this.debugOverlayEnabled = enabled;
    this.rebuildChildren();
  }

  /**
   * Apply an avatar appearance, loading/unloading sprite sheets as needed.
   * Diffs against the current appearance to minimize asset loading.
   */
  async applyAppearance(appearance: AvatarAppearance): Promise<void> {
    const prev = this.appearance;
    this.appearance = appearance;

    // Load manifest on first use
    if (!this.manifest) {
      try {
        const resp = await fetch(`${AVATAR_ASSET_BASE}/manifest.json`);
        if (resp.ok) this.manifest = await resp.json();
      } catch { /* manifest unavailable */ }
    }

    // Build slot→itemId maps for diffing
    const newSlots = this.buildSlotMap(appearance);
    const oldSlots = prev ? this.buildSlotMap(prev) : new Map<string, string>();

    // Determine which slots changed
    const changed: string[] = [];
    for (const [slot, newId] of newSlots) {
      if (oldSlots.get(slot) !== newId) {
        changed.push(slot);
      }
    }
    for (const [slot] of oldSlots) {
      if (!newSlots.has(slot)) {
        changed.push(slot);
      }
    }

    if (changed.length === 0 && prev) return;

    // Load/unload changed slots
    await Promise.all(changed.map(async (slot) => {
      // Remove old layers for this slot (including sub-parts)
      this.removeSlotLayers(slot);

      const newId = newSlots.get(slot);
      if (!newId) return;

      // Determine which sprite sheets to load for this slot
      const sheetPaths = this.resolveSheetPaths(slot, newId);

      for (const { key, path: sheetPath } of sheetPaths) {
        try {
          // cachePrefix includes itemId so swapping items within a slot
          // (e.g. shirt A → shirt B) doesn't reuse the previous item's
          // frame-key namespace and collide in the Pixi Assets cache.
          const sheet: Spritesheet = await Assets.load({
            src: sheetPath,
            data: { cachePrefix: `${key}.${newId}.` },
          });
          this.sheets.set(key, sheet);

          const anim = this.currentAnimation ?? 'idle';
          const textures = sheet.animations[anim];
          if (!textures || textures.length === 0) continue;

          const sprite = new AnimatedSprite({
            textures,
            animationSpeed: ANIMATION_SPEEDS[anim],
            loop: true,
          });
          sprite.anchor.set(0.5, 1);
          sprite.alpha = 0;
          sprite.play();

          this.layers.set(key, sprite);
          this.fadingIn.add(key);
        } catch {
          // Sheet not found — skip this layer
        }
      }
    }));

    this.applyTints(appearance);
    this.rebuildChildren();
  }

  /**
   * Sync all layers to the current animation state and facing direction.
   */
  updateAnimation(animation: AnimationState, facing: Direction): void {
    this.container.scale.x = (facing === 'right' ? 1 : -1) * DISPLAY_SCALE;

    // Tick fade-in for newly loaded layers
    if (this.fadingIn.size > 0) {
      for (const key of this.fadingIn) {
        const sprite = this.layers.get(key);
        if (!sprite) { this.fadingIn.delete(key); continue; }
        sprite.alpha = Math.min(1, sprite.alpha + FADE_IN_RATE);
        if (sprite.alpha >= 1) this.fadingIn.delete(key);
      }
    }

    if (animation === this.currentAnimation) return;
    this.currentAnimation = animation;

    for (const [key, sprite] of this.layers) {
      const sheet = this.sheets.get(key);
      if (!sheet) continue;

      const textures = sheet.animations[animation];
      if (textures && textures.length > 0) {
        sprite.textures = textures;
        sprite.animationSpeed = ANIMATION_SPEEDS[animation];
        sprite.play();
      }
    }
  }

  destroy(): void {
    for (const [, sprite] of this.layers) {
      sprite.stop();
      sprite.destroy();
    }
    this.layers.clear();
    this.sheets.clear();
    this.fadingIn.clear();
    this.appearance = null;
    this.currentAnimation = null;
  }

  /**
   * Build a map of slot→itemId from an appearance.
   */
  private buildSlotMap(appearance: AvatarAppearance): Map<string, string> {
    const slots = new Map<string, string>();
    slots.set('body', 'body');
    slots.set('eyes', appearance.eyes);
    slots.set('ears', appearance.ears);
    slots.set('nose', appearance.nose);
    slots.set('mouth', appearance.mouth);
    slots.set('hair', appearance.hair);
    if (appearance.hat) slots.set('hat', appearance.hat);
    if (appearance.coat) slots.set('coat', appearance.coat);
    if (appearance.shirt) slots.set('shirt', appearance.shirt);
    if (appearance.pants) slots.set('pants', appearance.pants);
    if (appearance.dress) slots.set('dress', appearance.dress);
    if (appearance.skirt) slots.set('skirt', appearance.skirt);
    if (appearance.shoes) slots.set('shoes', appearance.shoes);
    if (appearance.bracelet) slots.set('bracelet', appearance.bracelet);
    return slots;
  }

  /**
   * Resolve which sprite sheet(s) to load for a slot.
   * Multi-part items (with manifest `parts`) load one sheet per part.
   * Single-part items load one sheet.
   */
  private resolveSheetPaths(slot: string, itemId: string): { key: string; path: string }[] {
    if (slot === 'body') {
      return [{ key: 'body', path: `${AVATAR_ASSET_BASE}/base/body.json` }];
    }

    const category = LAYER_ORDER.find(l => l.slot === slot)?.category ?? slot;

    // Check manifest for parts
    const manifestItem = this.manifest?.categories[category]?.items?.find(
      (i: { id: string }) => i.id === itemId,
    );
    const parts = manifestItem?.parts;

    if (parts && parts.length > 0) {
      // Multi-part: one sheet per sub-sprite
      return parts.map((part: string) => ({
        key: `${slot}.${part}`,
        path: `${AVATAR_ASSET_BASE}/${category}/${itemId}.${part}.json`,
      }));
    }

    // Single-part (vanity items, or wardrobe fallback)
    return [{ key: slot, path: `${AVATAR_ASSET_BASE}/${category}/${itemId}.json` }];
  }

  /**
   * Remove all layers and sheets for a slot (including sub-parts).
   */
  private removeSlotLayers(slot: string): void {
    const keysToRemove: string[] = [];
    for (const key of this.layers.keys()) {
      if (key === slot || key.startsWith(`${slot}.`)) {
        keysToRemove.push(key);
      }
    }
    for (const key of keysToRemove) {
      const sprite = this.layers.get(key);
      if (sprite) {
        sprite.stop();
        sprite.destroy();
        this.layers.delete(key);
      }
      this.sheets.delete(key);
      this.fadingIn.delete(key);
    }
  }

  private applyTints(appearance: AvatarAppearance): void {
    const skinTint = parseInt(appearance.skinColor, 16);
    const hairTint = parseInt(appearance.hairColor, 16);
    const resolvedSkin = Number.isNaN(skinTint) ? 0xffffff : skinTint;
    const resolvedHair = Number.isNaN(hairTint) ? 0xffffff : hairTint;

    for (const [key, sprite] of this.layers) {
      const slot = key.split('.')[0];
      if (SKIN_TINT_SLOTS.has(slot)) {
        sprite.tint = resolvedSkin;
      } else if (HAIR_TINT_SLOTS.has(slot)) {
        sprite.tint = resolvedHair;
      }
    }
  }

  /**
   * Clear and re-add children in LAYER_ORDER z-order.
   * For each LAYER_ORDER entry, the layer key is either `slot.part` (multi-part)
   * or `slot` (single-part). Only one of these will have a loaded layer — the
   * multi-part entries are skipped for single-part items and vice versa.
   */
  private rebuildChildren(): void {
    // Explicitly destroy the prior overlay Graphics — removeChildren()
    // detaches but doesn't free GPU resources. Toggling the overlay
    // repeatedly during dev would otherwise leak Pixi objects.
    if (this.debugGraphics) {
      this.debugGraphics.destroy();
      this.debugGraphics = null;
    }
    this.container.removeChildren();

    let hasLayers = false;
    for (const { slot, part } of LAYER_ORDER) {
      const key = part ? `${slot}.${part}` : slot;
      const sprite = this.layers.get(key);
      if (sprite) {
        this.container.addChild(sprite);
        hasLayers = true;
      }
    }

    if (!hasLayers) {
      const g = new Graphics();
      g.rect(-15, -60, 30, 60);
      g.fill(0x5865f2);
      this.container.addChild(g);
      this.container.scale.set(1);
      return;
    }

    this.container.scale.set(DISPLAY_SCALE);

    if (this.debugOverlayEnabled) {
      this.renderDebugOverlay();
    }
  }

  /**
   * Draw a diagnostic overlay on top of the composited avatar. Strokes are
   * sized in container-local units; the container scales everything down by
   * DISPLAY_SCALE (~0.089) on the way to the screen.
   */
  private renderDebugOverlay(): void {
    const g = new Graphics();

    g.rect(-272, -1013, 544, 1013);
    g.stroke({ color: 0xff0000, width: 20 });

    g.moveTo(-120, 0).lineTo(120, 0);
    g.moveTo(0, -120).lineTo(0, 120);
    g.stroke({ color: 0xffff00, width: 12 });

    for (const sprite of this.layers.values()) {
      const tex = sprite.texture;
      if (!tex.orig) continue;
      const origW = tex.orig.width;
      const origH = tex.orig.height;
      const tx = tex.trim?.x ?? 0;
      const ty = tex.trim?.y ?? 0;
      const tw = tex.trim?.width ?? origW;
      const th = tex.trim?.height ?? origH;
      g.rect(tx - origW / 2, ty - origH, tw, th);
      g.stroke({ color: 0x00ff00, width: 8 });
    }

    this.debugGraphics = g;
    this.container.addChild(g);
  }
}
