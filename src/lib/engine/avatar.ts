import { Assets, AnimatedSprite, Container, Graphics } from 'pixi.js';
import type { Spritesheet, Texture } from 'pixi.js';
import type { AnimationState, AvatarAppearance, Direction } from '../types';

const ANIMATION_SPEEDS: Record<AnimationState, number> = {
  idle: 0.08,
  walking: 0.15,
  jumping: 0.1,
  falling: 0.1,
};

/**
 * Z-order for avatar layers (back to front).
 * Each entry maps a slot name to the category path used for asset loading.
 */
const LAYER_ORDER: { slot: string; category: string }[] = [
  { slot: 'body', category: 'base' },
  { slot: 'shoes', category: 'shoes' },
  { slot: 'pants', category: 'pants' },
  { slot: 'shirt', category: 'shirt' },
  { slot: 'dress', category: 'dress' },
  { slot: 'skirt', category: 'skirt' },
  { slot: 'coat', category: 'coat' },
  { slot: 'eyes', category: 'eyes' },
  { slot: 'nose', category: 'nose' },
  { slot: 'mouth', category: 'mouth' },
  { slot: 'ears', category: 'ears' },
  { slot: 'hair', category: 'hair' },
  { slot: 'hat', category: 'hat' },
  { slot: 'bracelet', category: 'bracelet' },
];

/** Slots that receive skin color tinting. */
const SKIN_TINT_SLOTS = new Set(['body']);

/** Slots that receive hair color tinting. */
const HAIR_TINT_SLOTS = new Set(['hair']);

/**
 * Display scale for the avatar. The base body sprite sheet is rendered at 8x
 * (544×1013 per frame). This scale brings it to a reasonable in-world size.
 * The physics body is 30×60, so ~90px tall gives 1.5x the collision box.
 */
const DISPLAY_SCALE = 90 / 1013;

/**
 * Layered avatar compositor for PixiJS.
 *
 * Loads per-slot sprite sheets, composites them into a Container with correct
 * z-order, applies color tinting, and synchronizes animation across all layers.
 *
 * NOTE: Currently only the body layer renders correctly. Equipment and vanity
 * layers require extraction pipeline changes to produce sprite sheets in a
 * shared coordinate space (consistent stage size + trim data). The layer
 * infrastructure is fully wired — once the pipeline is fixed, layers will
 * composite automatically.
 */
export class AvatarCompositor {
  private container: Container;
  private layers: Map<string, AnimatedSprite> = new Map();
  private sheets: Map<string, Spritesheet> = new Map();
  private appearance: AvatarAppearance | null = null;
  private currentAnimation: AnimationState | null = null;

  constructor() {
    this.container = new Container();
    this.container.scale.set(DISPLAY_SCALE);
  }

  getContainer(): Container {
    return this.container;
  }

  /**
   * Apply an avatar appearance, loading/unloading sprite sheets as needed.
   * Diffs against the current appearance to minimize asset loading.
   */
  async applyAppearance(appearance: AvatarAppearance): Promise<void> {
    const prev = this.appearance;
    this.appearance = appearance;

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
    // Check for removed slots
    for (const [slot] of oldSlots) {
      if (!newSlots.has(slot)) {
        changed.push(slot);
      }
    }

    if (changed.length === 0 && prev) return;

    // Load/unload changed slots
    await Promise.all(changed.map(async (slot) => {
      // Remove old layer
      const oldSprite = this.layers.get(slot);
      if (oldSprite) {
        oldSprite.stop();
        oldSprite.destroy();
        this.layers.delete(slot);
      }
      const oldSheet = this.sheets.get(slot);
      if (oldSheet) {
        this.sheets.delete(slot);
      }

      // Load new layer
      const newId = newSlots.get(slot);
      if (!newId) return;

      const entry = LAYER_ORDER.find(l => l.slot === slot);
      if (!entry) return;

      const sheetPath = slot === 'body'
        ? `sprites/avatar/${entry.category}/body.json`
        : `sprites/avatar/${entry.category}/${newId}.json`;

      try {
        const sheet: Spritesheet = await Assets.load(sheetPath);
        this.sheets.set(slot, sheet);

        const anim = this.currentAnimation ?? 'idle';
        const textures = sheet.animations[anim];
        if (!textures || textures.length === 0) return;

        const sprite = new AnimatedSprite({
          textures,
          animationSpeed: ANIMATION_SPEEDS[anim],
          loop: true,
        });
        sprite.anchor.set(0.5, 1);
        sprite.play();

        this.layers.set(slot, sprite);
      } catch {
        // Sheet not found — skip this layer
      }
    }));

    // Apply tints
    this.applyTints(appearance);

    // Rebuild container children in z-order
    this.rebuildChildren();
  }

  /**
   * Sync all layers to the current animation state and facing direction.
   */
  updateAnimation(animation: AnimationState, facing: Direction): void {
    this.container.scale.x = (facing === 'right' ? 1 : -1) * DISPLAY_SCALE;

    if (animation === this.currentAnimation) return;
    this.currentAnimation = animation;

    for (const [slot, sprite] of this.layers) {
      const sheet = this.sheets.get(slot);
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
    this.appearance = null;
    this.currentAnimation = null;
  }

  /**
   * Build a map of slot→itemId from an appearance.
   * Body is always present. Vanity slots are always present.
   * Wardrobe slots are optional (may be null).
   */
  private buildSlotMap(appearance: AvatarAppearance): Map<string, string> {
    const slots = new Map<string, string>();
    // Body is always present
    slots.set('body', 'body');
    // Vanity — always present
    slots.set('eyes', appearance.eyes);
    slots.set('ears', appearance.ears);
    slots.set('nose', appearance.nose);
    slots.set('mouth', appearance.mouth);
    slots.set('hair', appearance.hair);
    // Wardrobe — optional
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

  private applyTints(appearance: AvatarAppearance): void {
    const skinTint = parseInt(appearance.skinColor, 16);
    const hairTint = parseInt(appearance.hairColor, 16);

    for (const [slot, sprite] of this.layers) {
      if (SKIN_TINT_SLOTS.has(slot)) {
        sprite.tint = skinTint;
      } else if (HAIR_TINT_SLOTS.has(slot)) {
        sprite.tint = hairTint;
      }
    }
  }

  /**
   * Clear and re-add children in LAYER_ORDER z-order.
   * Also adds a fallback rectangle if no layers loaded.
   */
  private rebuildChildren(): void {
    this.container.removeChildren();

    let hasLayers = false;
    for (const { slot } of LAYER_ORDER) {
      const sprite = this.layers.get(slot);
      if (sprite) {
        this.container.addChild(sprite);
        hasLayers = true;
      }
    }

    // Fallback: blue rectangle if nothing loaded
    if (!hasLayers) {
      const g = new Graphics();
      g.rect(-15, -60, 30, 60);
      g.fill(0x5865f2);
      this.container.addChild(g);
      // Reset scale for fallback (it's already in world units)
      this.container.scale.set(1);
    }
  }
}
