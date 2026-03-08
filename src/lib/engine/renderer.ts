import { Application, Container, FillGradient, Graphics } from 'pixi.js';
import type { StreetData, RenderFrame } from '../types';

export class GameRenderer {
  app: Application;
  private parallaxContainer: Container;
  private worldContainer: Container;
  private uiContainer: Container;
  private layerContainers: Map<string, Container> = new Map();
  private platformGraphics: Graphics | null = null;
  private avatarGraphics: Graphics | null = null;
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
  }

  setDebugMode(enabled: boolean): void {
    this.debugMode = enabled;
    if (this.street) {
      this.drawPlatforms(this.street);
    }
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

    // Build gradient background
    const bg = new Graphics();
    const topColor = street.gradient ? parseInt(street.gradient.top, 16) : 0x87a8c9;
    const bottomColor = street.gradient ? parseInt(street.gradient.bottom, 16) : 0xffc400;
    const gradient = new FillGradient(0, 0, 0, this.app.screen.height);
    gradient.addColorStop(0, topColor);
    gradient.addColorStop(1, bottomColor);
    bg.rect(0, 0, this.app.screen.width, this.app.screen.height);
    bg.fill(gradient);
    this.parallaxContainer.addChild(bg);

    // Build parallax layers
    for (const layer of street.layers) {
      const container = new Container();
      container.label = layer.name;

      // Draw decos as placeholder rectangles (until real art assets are available)
      for (const deco of layer.decos) {
        const g = new Graphics();
        const screenY = deco.y - street.top;
        g.rect(deco.x - street.left, screenY - deco.h, deco.w, deco.h);
        g.fill({ color: 0x4a6741, alpha: 0.3 });
        if (deco.hFlip) {
          g.scale.x = -1;
          // scale.x=-1 mirrors around g.x=0, so we offset to keep the
          // rect visually anchored at its original screen position.
          g.x = 2 * (deco.x - street.left) + deco.w;
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

    for (const platform of street.layers.filter(l => l.isMiddleground).flatMap(l => l.platformLines)) {
      const startScreenY = platform.start.y - street.top;
      const endScreenY = platform.end.y - street.top;
      const startScreenX = platform.start.x - street.left;
      const endScreenX = platform.end.x - street.left;

      // Draw platform line
      this.platformGraphics.moveTo(startScreenX, startScreenY);
      this.platformGraphics.lineTo(endScreenX, endScreenY);
      this.platformGraphics.stroke({ color: this.debugMode ? 0x00ff00 : 0x6b5b3a, width: this.debugMode ? 2 : 4 });
    }

    // Draw walls in debug mode
    if (this.debugMode) {
      for (const wall of street.layers.filter(l => l.isMiddleground).flatMap(l => l.walls)) {
        const screenX = wall.x - street.left;
        const screenY = wall.y - street.top;
        // Wall extends h pixels downward (toward ground/positive Y)
        this.platformGraphics.moveTo(screenX, screenY);
        this.platformGraphics.lineTo(screenX, screenY + wall.h);
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

    // Update parallax layers — scroll proportionally to the camera offset.
    for (const layer of this.street.layers) {
      if (layer.isMiddleground) continue;
      const container = this.layerContainers.get(layer.name);
      if (!container) continue;

      const factor = layer.w / mgWidth;
      container.x = -camScreenX * factor;
      container.y = -camScreenY * factor;
    }
  }

  destroy(): void {
    this.app.destroy(true);
  }
}
