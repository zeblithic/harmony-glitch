import { Application, Container, FillGradient, Graphics, Text } from 'pixi.js';
import type { StreetData, RenderFrame, RemotePlayerFrame } from '../types';
import { SpriteManager } from './sprites';

interface ChatBubble {
  text: Text;
  targetHash: string;
  age: number;
}

export class GameRenderer {
  private static REMOTE_COLOR = 0x4488ff;
  private static CHAT_DURATION = 5.0;

  private static formatStreetName(raw: string): string {
    return raw
      .split('_')
      .map(w => w.charAt(0).toUpperCase() + w.slice(1))
      .join(' ');
  }

  app: Application;
  private parallaxContainer: Container;
  private worldContainer: Container;
  private uiContainer: Container;
  private layerContainers: Map<string, Container> = new Map();
  private remoteSprites: Map<string, Container> = new Map();
  private chatBubbles: ChatBubble[] = [];
  private entitySprites: Map<string, Container> = new Map();
  private groundItemSprites: Map<string, Container> = new Map();
  private promptText: Text | null = null;
  private feedbackTexts: { text: Text; feedbackId: number; startAge: number }[] = [];
  private lastFrameTime = 0;
  private platformGraphics: Graphics | null = null;
  private avatarContainer: Container | null = null;
  private bgGraphics: Graphics | null = null;
  private street: StreetData | null = null;
  private debugMode = false;
  private transitionContainer: Container;
  private transitionBg: Graphics | null = null;
  private streetNameText: Text | null = null;
  private lastTransitionGen = -1;
  private irisMask: Graphics | null = null;
  private decorationContainer: Container | null = null;
  private starGraphics: Graphics[] = [];
  private swirlGraphics: Graphics[] = [];
  private starPositions: { nx: number; ny: number }[] = [];
  private swirlPositions: { nx: number; ny: number }[] = [];
  private spriteManager: SpriteManager;

  constructor() {
    this.app = new Application();
    this.parallaxContainer = new Container();
    this.worldContainer = new Container();
    this.uiContainer = new Container();
    this.transitionContainer = new Container();
    this.transitionContainer.visible = false;
    this.spriteManager = new SpriteManager();
  }

  async init(canvas: HTMLCanvasElement): Promise<void> {
    await this.app.init({
      canvas,
      resizeTo: canvas.parentElement ?? undefined,
      background: '#1a1a2e',
      antialias: true,
    });

    this.app.stage.addChild(this.parallaxContainer);
    this.app.stage.addChild(this.worldContainer);
    this.app.stage.addChild(this.uiContainer);
    this.app.stage.addChild(this.transitionContainer);

    this.transitionBg = new Graphics();
    this.transitionContainer.addChild(this.transitionBg);

    this.irisMask = new Graphics();
    this.irisMask.renderable = false;
    this.transitionContainer.addChild(this.irisMask);

    this.decorationContainer = new Container();
    this.decorationContainer.mask = this.irisMask;
    this.transitionContainer.addChild(this.decorationContainer);

    this.streetNameText = new Text({
      text: '',
      style: { fontSize: 28, fill: 0xffffff, fontFamily: 'sans-serif' },
    });
    this.streetNameText.anchor.set(0.5, 0.5);
    this.streetNameText.visible = false;
    this.transitionContainer.addChild(this.streetNameText);

    this.app.renderer.on('resize', () => {
      if (this.street) this.drawBackground(this.street);
    });

    await this.spriteManager.init();
  }

  setDebugMode(enabled: boolean): void {
    this.debugMode = enabled;
    if (this.street) {
      this.drawPlatforms(this.street);
    }
  }

  private drawBackground(street: StreetData): void {
    if (!this.bgGraphics) return;
    this.bgGraphics.clear();
    const topColor = street.gradient ? parseInt(street.gradient.top, 16) : 0x87a8c9;
    const bottomColor = street.gradient ? parseInt(street.gradient.bottom, 16) : 0xffc400;
    const gradient = new FillGradient(0, 0, 0, this.app.screen.height);
    gradient.addColorStop(0, topColor);
    gradient.addColorStop(1, bottomColor);
    this.bgGraphics.rect(0, 0, this.app.screen.width, this.app.screen.height);
    this.bgGraphics.fill(gradient);
  }

  /**
   * Build the PixiJS scene graph from street data.
   *
   * Coordinate conversion: Glitch Y and screen Y both increase downward
   * from different origins. Glitch origin is at ground (Y=0), with negative
   * values going up (sky). Screen origin is at street.top. The conversion
   * is a pure translation: screenY = glitchY - street.top.
   *
   * Example (top=-800, bottom=0):
   *   Sky   (glitchY=-800) → screenY=0    (top of screen)
   *   Ground(glitchY=0)    → screenY=800  (bottom of screen)
   */
  async buildScene(street: StreetData): Promise<void> {
    this.street = street;
    await this.spriteManager.loadStreetAssets(street);
    this.parallaxContainer.removeChildren();
    this.worldContainer.removeChildren();
    this.layerContainers.clear();
    for (const [, sprite] of this.remoteSprites) {
      sprite.destroy();
    }
    this.remoteSprites.clear();
    for (const bubble of this.chatBubbles) {
      bubble.text.destroy();
    }
    this.chatBubbles = [];
    for (const [, sprite] of this.entitySprites) { sprite.destroy(); }
    this.entitySprites.clear();
    for (const [, sprite] of this.groundItemSprites) { sprite.destroy(); }
    this.groundItemSprites.clear();
    if (this.promptText) { this.promptText.destroy(); this.promptText = null; }
    for (const ft of this.feedbackTexts) { ft.text.destroy(); }
    this.feedbackTexts = [];

    // Build gradient background (redrawn on resize via drawBackground)
    this.bgGraphics = new Graphics();
    this.parallaxContainer.addChild(this.bgGraphics);
    this.drawBackground(street);

    // Build parallax layers
    for (const layer of street.layers) {
      const container = new Container();
      container.label = layer.name;

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

      if (layer.isMiddleground) {
        this.worldContainer.addChild(container);
      } else {
        this.parallaxContainer.addChild(container);
      }
      this.layerContainers.set(layer.name, container);
    }

    // Draw platforms (debug view or always-visible lines)
    this.platformGraphics = new Graphics();
    this.worldContainer.addChild(this.platformGraphics);
    this.drawPlatforms(street);

    // Create avatar (AnimatedSprite or fallback rectangle)
    this.avatarContainer = this.spriteManager.createAvatar();
    this.worldContainer.addChild(this.avatarContainer);

    // Create interaction prompt text (screen-fixed, in uiContainer)
    this.promptText = new Text({ text: '', style: { fontSize: 14, fill: 0xffffff } });
    this.promptText.anchor.set(0.5, 1);
    this.promptText.visible = false;
    this.uiContainer.addChild(this.promptText);
  }

  private drawPlatforms(street: StreetData): void {
    if (!this.platformGraphics) return;
    this.platformGraphics.clear();

    const platforms = street.layers.filter(l => l.isMiddleground).flatMap(l => l.platformLines);
    for (const platform of platforms) {
      const startScreenY = platform.start.y - street.top;
      const endScreenY = platform.end.y - street.top;
      const startScreenX = platform.start.x - street.left;
      const endScreenX = platform.end.x - street.left;

      this.platformGraphics.moveTo(startScreenX, startScreenY);
      this.platformGraphics.lineTo(endScreenX, endScreenY);
    }
    // Stroke all platform lines in one draw call
    if (platforms.length > 0) {
      this.platformGraphics.stroke({ color: this.debugMode ? 0x00ff00 : 0x6b5b3a, width: this.debugMode ? 2 : 4 });
    }

    // Draw walls in debug mode
    if (this.debugMode) {
      const walls = street.layers.filter(l => l.isMiddleground).flatMap(l => l.walls);
      for (const wall of walls) {
        const screenX = wall.x - street.left;
        const screenY = wall.y - street.top;
        this.platformGraphics.moveTo(screenX, screenY);
        this.platformGraphics.lineTo(screenX, screenY + wall.h);
      }
      if (walls.length > 0) {
        this.platformGraphics.stroke({ color: 0xff0000, width: 2 });
      }
    }
  }

  /**
   * Update the scene from a RenderFrame.
   */
  updateFrame(frame: RenderFrame): void {
    if (!this.street || !this.avatarContainer) return;

    const mg = this.street.layers.find(l => l.isMiddleground);
    const mgWidth = mg?.w ?? this.street.right - this.street.left;

    // Update avatar position — pure translation from Glitch to screen coords
    const avatarScreenX = frame.player.x - this.street.left;
    const avatarScreenY = frame.player.y - this.street.top;
    this.avatarContainer.x = avatarScreenX;
    this.avatarContainer.y = avatarScreenY;
    this.spriteManager.updateAvatar(this.avatarContainer, frame.player.animation, frame.player.facing);

    // Update camera — shift world container so the camera region is visible.
    // camera.y is the Glitch Y of the viewport's top edge.
    const camScreenX = frame.camera.x - this.street.left;
    const camScreenY = frame.camera.y - this.street.top;
    this.worldContainer.x = -camScreenX;
    this.worldContainer.y = -camScreenY;

    // Update parallax layers — horizontal scroll proportional to width ratio,
    // vertical scroll tracks camera 1:1 (backgrounds share middleground height).
    for (const layer of this.street.layers) {
      if (layer.isMiddleground) continue;
      const container = this.layerContainers.get(layer.name);
      if (!container) continue;

      const factor = mgWidth > 0 ? layer.w / mgWidth : 1.0;
      container.x = -camScreenX * factor;
      container.y = -camScreenY;
    }

    // Remote players — create/update/remove sprite lifecycle
    const remotePlayers = frame.remotePlayers ?? [];
    const seen = new Set<string>();
    for (const remote of remotePlayers) {
      seen.add(remote.addressHash);
      let sprite = this.remoteSprites.get(remote.addressHash);

      if (!sprite) {
        sprite = new Container();
        const body = new Graphics();
        body.rect(-15, -60, 30, 60);
        body.fill(GameRenderer.REMOTE_COLOR);
        sprite.addChild(body);

        const label = new Text({
          text: remote.displayName,
          style: { fontSize: 12, fill: 0xffffff, align: 'center' },
        });
        label.anchor.set(0.5, 1);
        label.y = -65;
        sprite.addChild(label);

        this.worldContainer.addChild(sprite);
        this.remoteSprites.set(remote.addressHash, sprite);
      }

      // Sync label text in case the peer's display name changed.
      const label = sprite.children[1] as Text;
      if (label && label.text !== remote.displayName) {
        label.text = remote.displayName;
      }

      sprite.x = remote.x - this.street.left;
      sprite.y = remote.y - this.street.top;
      sprite.scale.x = remote.facing === 'right' ? 1 : -1;
    }

    // Remove departed players
    for (const [hash, sprite] of this.remoteSprites) {
      if (!seen.has(hash)) {
        this.worldContainer.removeChild(sprite);
        sprite.destroy();
        this.remoteSprites.delete(hash);
      }
    }

    // Update chat bubbles with real elapsed time
    const now = performance.now();
    const dt = this.lastFrameTime ? (now - this.lastFrameTime) / 1000 : 1 / 60;
    this.lastFrameTime = now;

    // World entities — create/update/remove sprites (placeholder colored rectangles)
    const worldEntities = frame.worldEntities ?? [];
    const seenEntities = new Set<string>();
    for (const entity of worldEntities) {
      seenEntities.add(entity.id);
      let sprite = this.entitySprites.get(entity.id);
      if (!sprite) {
        sprite = this.spriteManager.createEntity(entity);
        this.worldContainer.addChild(sprite);
        this.entitySprites.set(entity.id, sprite);
      }
      sprite.x = entity.x - this.street.left;
      sprite.y = entity.y - this.street.top;

      // Opacity based on entity state
      if (entity.cooldownRemaining != null) {
        sprite.alpha = entity.depleted ? 0.25 : 0.5;
      } else {
        sprite.alpha = 1.0;
      }
    }
    for (const [id, sprite] of this.entitySprites) {
      if (!seenEntities.has(id)) {
        this.worldContainer.removeChild(sprite);
        sprite.destroy();
        this.entitySprites.delete(id);
      }
    }

    // Ground items — small sprites with bob animation
    const groundItems = frame.worldItems ?? [];
    const seenItems = new Set<string>();
    for (const item of groundItems) {
      seenItems.add(item.id);
      let sprite = this.groundItemSprites.get(item.id);
      if (!sprite) {
        sprite = this.spriteManager.createGroundItem(item);
        this.worldContainer.addChild(sprite);
        this.groundItemSprites.set(item.id, sprite);
      } else {
        const label = sprite.children[1] as Text;
        const expectedText = item.count > 1 ? `${item.name} x${item.count}` : item.name;
        if (label && label.text !== expectedText) {
          label.text = expectedText;
        }
      }
      sprite.x = item.x - this.street.left;
      sprite.y = item.y - this.street.top;
      sprite.y += Math.sin(performance.now() / 500) * 2;
    }
    for (const [id, sprite] of this.groundItemSprites) {
      if (!seenItems.has(id)) {
        this.worldContainer.removeChild(sprite);
        sprite.destroy();
        this.groundItemSprites.delete(id);
      }
    }

    // Interaction prompt (in uiContainer, screen-fixed)
    if (frame.interactionPrompt && this.promptText) {
      const p = frame.interactionPrompt;
      this.promptText.text = p.actionable
        ? `[E] ${p.verb} ${p.targetName}`
        : p.verb;
      const screenX = p.targetX - this.street.left + this.worldContainer.x;
      const screenY = p.targetY - this.street.top + this.worldContainer.y - 90;
      this.promptText.x = screenX;
      this.promptText.y = screenY;
      this.promptText.visible = true;
    } else if (this.promptText) {
      this.promptText.visible = false;
    }

    // Pickup feedback (floating text)
    const feedback = frame.pickupFeedback ?? [];
    this.feedbackTexts = this.feedbackTexts.filter((ft) => {
      if (ft.startAge >= 1.5) {
        this.uiContainer.removeChild(ft.text);
        ft.text.destroy();
        return false;
      }
      return true;
    });
    for (const fb of feedback) {
      if (fb.ageSecs < dt * 2) {
        const existing = this.feedbackTexts.find(
          (ft) => ft.feedbackId === fb.id
        );
        if (!existing) {
          const text = new Text({
            text: fb.text,
            style: { fontSize: 14, fill: fb.success ? 0x7ae87a : 0xe87a7a },
          });
          text.anchor.set(0.5, 1);
          this.uiContainer.addChild(text);
          this.feedbackTexts.push({ text, feedbackId: fb.id, startAge: 0 });
        }
      }
    }
    for (const ft of this.feedbackTexts) {
      ft.startAge += dt;
      ft.text.alpha = Math.max(0, 1 - ft.startAge / 1.5);
      const matchingFb = feedback.find((f) => f.id === ft.feedbackId);
      if (matchingFb) {
        const screenX = matchingFb.x - this.street.left + this.worldContainer.x;
        const screenY = matchingFb.y - this.street.top + this.worldContainer.y - 100 - ft.startAge * 30;
        ft.text.x = screenX;
        ft.text.y = screenY;
      }
    }

    this.updateChatBubbles(dt, remotePlayers);

    this.updateTransition(frame);
  }

  addChatBubble(addressHash: string, message: string): void {
    const bubble = new Text({
      text: message,
      style: {
        fontSize: 12,
        fill: 0xffffff,
        wordWrap: true,
        wordWrapWidth: 200,
      },
    });
    bubble.anchor.set(0.5, 1);
    this.worldContainer.addChild(bubble);
    this.chatBubbles.push({ text: bubble, targetHash: addressHash, age: 0 });
  }

  private updateChatBubbles(dt: number, remotePlayers: RemotePlayerFrame[]): void {
    this.chatBubbles = this.chatBubbles.filter((bubble) => {
      bubble.age += dt;
      if (bubble.age >= GameRenderer.CHAT_DURATION) {
        this.worldContainer.removeChild(bubble.text);
        bubble.text.destroy();
        return false;
      }
      const player = remotePlayers.find((p) => p.addressHash === bubble.targetHash);
      if (player && this.street) {
        bubble.text.x = player.x - this.street.left;
        bubble.text.y = player.y - this.street.top - 75;
      } else if (this.avatarContainer) {
        // Local player's bubble — position above local avatar.
        bubble.text.x = this.avatarContainer.x;
        bubble.text.y = this.avatarContainer.y - 75;
      }
      bubble.text.alpha = Math.min(1, GameRenderer.CHAT_DURATION - bubble.age);
      return true;
    });
  }

  private updateTransition(frame: RenderFrame): void {
    if (!frame.transition) {
      this.transitionContainer.visible = false;
      return;
    }

    this.transitionContainer.visible = true;
    const { progress, toStreet, generation } = frame.transition;
    const screenW = this.app.screen.width;
    const screenH = this.app.screen.height;
    const maxRadius = Math.hypot(screenW, screenH);

    // Update street name text and decorations on new transition
    if (generation !== this.lastTransitionGen) {
      this.lastTransitionGen = generation;
      if (this.streetNameText) {
        this.streetNameText.text = GameRenderer.formatStreetName(toStreet);
      }
      this.generateStarsAndSwirls();
    }

    // Compute iris radius: closing (0→0.5) then opening (0.5→1)
    let radius: number;
    let centerX: number;
    let centerY: number;

    if (progress <= 0.5) {
      // Closing: shrink from maxRadius to 0, centered on player
      radius = maxRadius * (1 - progress * 2);
      centerX = (this.avatarContainer?.x ?? 0) + this.worldContainer.x;
      centerY = (this.avatarContainer?.y ?? 0) + this.worldContainer.y;
    } else {
      // Opening: grow from 0 to maxRadius, centered on viewport
      radius = maxRadius * ((progress - 0.5) * 2);
      centerX = screenW / 2;
      centerY = screenH / 2;
    }

    // Draw background with iris hole (fill-only — .cut() operates on fills)
    if (this.transitionBg) {
      this.transitionBg.clear();
      this.transitionBg.rect(0, 0, screenW, screenH);
      this.transitionBg.fill({ color: 0x0d0d2b });

      if (radius > 0) {
        this.transitionBg.circle(centerX, centerY, radius);
        this.transitionBg.cut();
      }
    }

    // Update decoration mask (same iris shape — clips stars/swirls to dark region)
    if (this.irisMask) {
      this.irisMask.clear();
      this.irisMask.rect(0, 0, screenW, screenH);
      this.irisMask.fill({ color: 0xffffff });

      if (radius > 0) {
        this.irisMask.circle(centerX, centerY, radius);
        this.irisMask.cut();
      }
    }

    // Reposition stars/swirls from normalized coords (resize-safe)
    for (let i = 0; i < this.starGraphics.length; i++) {
      this.starGraphics[i].x = this.starPositions[i].nx * screenW;
      this.starGraphics[i].y = this.starPositions[i].ny * screenH;
    }
    for (let i = 0; i < this.swirlGraphics.length; i++) {
      this.swirlGraphics[i].x = this.swirlPositions[i].nx * screenW;
      this.swirlGraphics[i].y = this.swirlPositions[i].ny * screenH;
    }

    // Street name alpha
    if (this.streetNameText) {
      this.streetNameText.x = screenW / 2;
      this.streetNameText.y = screenH / 2;

      let alpha: number;
      if (progress < 0.48) {
        alpha = 0;
      } else if (progress < 0.52) {
        alpha = (progress - 0.48) / 0.04; // fade in over 0.48→0.52
      } else if (progress < 0.8) {
        alpha = 1;
      } else {
        alpha = 1 - (progress - 0.8) / 0.2; // fade out over 0.8→1.0
      }

      this.streetNameText.alpha = alpha;
      this.streetNameText.visible = alpha > 0;
    }
  }

  private generateStarsAndSwirls(): void {
    // Destroy old graphics (auto-removes from parent)
    for (const g of this.starGraphics) { g.destroy(); }
    for (const g of this.swirlGraphics) { g.destroy(); }
    this.starGraphics = [];
    this.swirlGraphics = [];

    const starCount = 25 + Math.floor(Math.random() * 11); // 25-35
    this.starPositions = Array.from({ length: starCount }, () => ({
      nx: Math.random(),
      ny: Math.random(),
    }));

    const swirlCount = 3 + Math.floor(Math.random() * 3); // 3-5
    this.swirlPositions = Array.from({ length: swirlCount }, () => ({
      nx: Math.random(),
      ny: Math.random(),
    }));

    // Stars: drawn once at local origin, repositioned per frame
    for (let i = 0; i < starCount; i++) {
      const g = new Graphics();
      const size = 2 + Math.random() * 4; // 2-6px
      const alpha = 0.2 + Math.random() * 0.3; // 0.2-0.5
      g.moveTo(-size, 0);
      g.lineTo(size, 0);
      g.moveTo(0, -size);
      g.lineTo(0, size);
      g.stroke({ color: 0xffffff, alpha, width: 1 });
      this.starGraphics.push(g);
      this.decorationContainer?.addChild(g);
    }

    // Swirls: quarter-circle arcs at local origin
    for (let i = 0; i < swirlCount; i++) {
      const g = new Graphics();
      const radius = 30 + Math.random() * 50; // 30-80px
      const alpha = 0.1 + Math.random() * 0.1; // 0.1-0.2
      const startAngle = Math.random() * Math.PI * 2;
      g.arc(0, 0, radius, startAngle, startAngle + Math.PI / 2);
      g.stroke({ color: 0xffffff, alpha, width: 1.5 });
      this.swirlGraphics.push(g);
      this.decorationContainer?.addChild(g);
    }
  }

  destroy(): void {
    for (const [, sprite] of this.remoteSprites) {
      sprite.destroy();
    }
    this.remoteSprites.clear();
    for (const bubble of this.chatBubbles) {
      bubble.text.destroy();
    }
    this.chatBubbles = [];
    for (const [, sprite] of this.entitySprites) { sprite.destroy(); }
    this.entitySprites.clear();
    for (const [, sprite] of this.groundItemSprites) { sprite.destroy(); }
    this.groundItemSprites.clear();
    if (this.promptText) { this.promptText.destroy(); this.promptText = null; }
    for (const ft of this.feedbackTexts) { ft.text.destroy(); }
    this.feedbackTexts = [];
    for (const g of this.starGraphics) { g.destroy(); }
    this.starGraphics = [];
    for (const g of this.swirlGraphics) { g.destroy(); }
    this.swirlGraphics = [];
    if (this.decorationContainer) { this.decorationContainer.mask = null; }
    if (this.irisMask) { this.irisMask.destroy(); this.irisMask = null; }
    if (this.decorationContainer) { this.decorationContainer.destroy(); this.decorationContainer = null; }
    if (this.transitionBg) { this.transitionBg.destroy(); this.transitionBg = null; }
    if (this.streetNameText) { this.streetNameText.destroy(); this.streetNameText = null; }
    this.transitionContainer.destroy();
    this.spriteManager.destroy();
    this.app.destroy(true);
  }
}
