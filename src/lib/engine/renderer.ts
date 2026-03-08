import { Application, Container, FillGradient, Graphics, Text } from 'pixi.js';
import type { StreetData, RenderFrame } from '../types';

export class GameRenderer {
  private static REMOTE_COLOR = 0x4488ff;

  app: Application;
  private parallaxContainer: Container;
  private worldContainer: Container;
  private uiContainer: Container;
  private layerContainers: Map<string, Container> = new Map();
  private remoteSprites: Map<string, Container> = new Map();
  private platformGraphics: Graphics | null = null;
  private avatarGraphics: Graphics | null = null;
  private bgGraphics: Graphics | null = null;
  private street: StreetData | null = null;
  private debugMode = false;

  constructor() {
    this.app = new Application();
    this.parallaxContainer = new Container();
    this.worldContainer = new Container();
    this.uiContainer = new Container();
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

    this.app.renderer.on('resize', () => {
      if (this.street) this.drawBackground(this.street);
    });
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
  buildScene(street: StreetData): void {
    this.street = street;
    this.parallaxContainer.removeChildren();
    this.worldContainer.removeChildren();
    this.layerContainers.clear();
    for (const [, sprite] of this.remoteSprites) {
      sprite.destroy();
    }
    this.remoteSprites.clear();

    // Build gradient background (redrawn on resize via drawBackground)
    this.bgGraphics = new Graphics();
    this.parallaxContainer.addChild(this.bgGraphics);
    this.drawBackground(street);

    // Build parallax layers
    for (const layer of street.layers) {
      const container = new Container();
      container.label = layer.name;

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

    // Create avatar placeholder
    this.avatarGraphics = new Graphics();
    this.avatarGraphics.rect(-15, -60, 30, 60);
    this.avatarGraphics.fill(0x5865f2);
    this.worldContainer.addChild(this.avatarGraphics);
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
    if (!this.street || !this.avatarGraphics) return;

    const mg = this.street.layers.find(l => l.isMiddleground);
    const mgWidth = mg?.w ?? this.street.right - this.street.left;

    // Update avatar position — pure translation from Glitch to screen coords
    const avatarScreenX = frame.player.x - this.street.left;
    const avatarScreenY = frame.player.y - this.street.top;
    this.avatarGraphics.x = avatarScreenX;
    this.avatarGraphics.y = avatarScreenY;
    this.avatarGraphics.scale.x = frame.player.facing === 'right' ? 1 : -1;

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

    // Swoop transition — slide old street off-screen
    if (frame.transition) {
      const { progress, direction } = frame.transition;
      const viewportWidth = this.app.canvas.width;
      const offset = direction === 'right'
        ? -progress * viewportWidth
        : progress * viewportWidth;
      this.worldContainer.x += offset;
      for (const [, container] of this.layerContainers) {
        container.x += offset;
      }
    }
  }

  destroy(): void {
    for (const [, sprite] of this.remoteSprites) {
      sprite.destroy();
    }
    this.remoteSprites.clear();
    this.app.destroy(true);
  }
}
