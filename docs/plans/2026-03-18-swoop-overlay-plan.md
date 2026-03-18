# Swoop Visual Overlay Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the viewport-slide street transition with an iris wipe overlay, street name title card, and decorative star/swirl pattern.

**Architecture:** Pure frontend change — a new `transitionContainer` in the PixiJS scene graph draws an iris wipe (dark background with circular hole) driven by the existing `TransitionInfo.progress` value. Stars, swirls, and a street name text overlay complete the effect. No Rust changes.

**Tech Stack:** TypeScript, PixiJS v8 (`Graphics`, `Text`, `Container`)

**Spec:** `docs/plans/2026-03-17-swoop-overlay-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/lib/engine/renderer.ts` | **Modify** — add `transitionContainer`, iris wipe drawing, star/swirl generation, street name text, remove viewport-slide code, update `destroy()` |

Single file. No new files. No Rust changes. No type changes.

---

## Chunk 1: Implementation

### Task 1: Iris Wipe Overlay + Street Name Text

**Files:**
- Modify: `src/lib/engine/renderer.ts`

**Context for implementer:**

The `GameRenderer` class manages a PixiJS scene graph with three top-level containers: `parallaxContainer`, `worldContainer`, `uiContainer`. A `TransitionInfo` object arrives on each `RenderFrame` during a street transition with `progress` (0.0→1.0), `direction`, `toStreet` (human-readable name like `"demo_heights"`), and `generation` (monotonic counter).

Currently, transitions slide the world/parallax containers horizontally (lines 423-438 of renderer.ts). This task replaces that with an iris wipe overlay.

**PixiJS v8 key API:**
- `graphics.rect(x, y, w, h)` — adds rect to current path
- `graphics.fill({ color })` — fills current path
- `graphics.circle(cx, cy, r)` — adds circle to current path
- `graphics.cut()` — cuts current path shape from previously filled shape. Uses the stencil buffer internally, so the hole only removes from already-drawn pixels — safe even when the circle extends beyond the rect (the spec's `.cut()` warning is about the legacy `beginHole()` API, not v8's stencil-based `.cut()`)
- `graphics.clear()` — clears all drawn content
- `text.anchor.set(0.5, 0.5)` — centers text on its position
- `this.app.screen.width / height` — viewport size in CSS logical pixels

- [ ] **Step 1: Add new fields to `GameRenderer`**

Add these fields after the existing `private debugMode = false;` (line 30):

```typescript
  private transitionContainer: Container;
  private transitionBg: Graphics | null = null;
  private streetNameText: Text | null = null;
  private lastTransitionGen = -1;
```

Update the constructor (after line 36, before the closing `}`):

```typescript
    this.transitionContainer = new Container();
    this.transitionContainer.visible = false;
```

- [ ] **Step 2: Add `transitionContainer` to the stage in `init()`**

After line 49 (`this.app.stage.addChild(this.uiContainer);`), add:

```typescript
    this.app.stage.addChild(this.transitionContainer);
```

Then create the background graphics and street name text. After that new `addChild` line:

```typescript
    this.transitionBg = new Graphics();
    this.transitionContainer.addChild(this.transitionBg);

    this.streetNameText = new Text({
      text: '',
      style: { fontSize: 28, fill: 0xffffff, fontFamily: 'sans-serif' },
    });
    this.streetNameText.anchor.set(0.5, 0.5);
    this.streetNameText.visible = false;
    this.transitionContainer.addChild(this.streetNameText);
```

- [ ] **Step 3: Add the `formatStreetName` helper**

Add as a private static method on `GameRenderer` (after line 12, `private static CHAT_DURATION = 5.0;`):

```typescript
  private static formatStreetName(raw: string): string {
    return raw
      .split('_')
      .map(w => w.charAt(0).toUpperCase() + w.slice(1))
      .join(' ');
  }
```

- [ ] **Step 4: Add the `updateTransition` method**

Add as a private method after `updateChatBubbles()` (after line 476):

```typescript
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

    // Update street name text on new transition
    if (generation !== this.lastTransitionGen) {
      this.lastTransitionGen = generation;
      if (this.streetNameText) {
        this.streetNameText.text = GameRenderer.formatStreetName(toStreet);
      }
    }

    // Compute iris radius: closing (0→0.5) then opening (0.5→1)
    let radius: number;
    let centerX: number;
    let centerY: number;

    if (progress <= 0.5) {
      // Closing: shrink from maxRadius to 0, centered on player
      radius = maxRadius * (1 - progress * 2);
      centerX = (this.avatarGraphics?.x ?? 0) + this.worldContainer.x;
      centerY = (this.avatarGraphics?.y ?? 0) + this.worldContainer.y;
    } else {
      // Opening: grow from 0 to maxRadius, centered on viewport
      radius = maxRadius * ((progress - 0.5) * 2);
      centerX = screenW / 2;
      centerY = screenH / 2;
    }

    // Draw background with iris hole
    if (this.transitionBg) {
      this.transitionBg.clear();
      this.transitionBg.rect(0, 0, screenW, screenH);
      this.transitionBg.fill({ color: 0x0d0d2b });

      if (radius > 0) {
        this.transitionBg.circle(centerX, centerY, radius);
        this.transitionBg.cut();
      }
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
```

- [ ] **Step 5: Replace viewport-slide code with `updateTransition` call**

In `updateFrame()`, replace lines 422-438 (the swoop transition block including the comment):

```typescript
    this.updateChatBubbles(dt, remotePlayers);

    // Swoop transition — slide old street off-screen.
    // Only shift parallax layers here; the middleground is a child of worldContainer
    // and inherits its offset automatically.
    if (frame.transition) {
      const { progress, direction } = frame.transition;
      const viewportWidth = this.app.canvas.width;
      const offset = direction === 'right'
        ? -progress * viewportWidth
        : progress * viewportWidth;
      this.worldContainer.x += offset;
      for (const [name, container] of this.layerContainers) {
        const layer = this.street.layers.find(l => l.name === name);
        if (layer?.isMiddleground) continue;
        container.x += offset;
      }
    }
```

With:

```typescript
    this.updateChatBubbles(dt, remotePlayers);

    this.updateTransition(frame);
```

- [ ] **Step 6: Update `destroy()` method**

In the `destroy()` method, add cleanup before `this.app.destroy(true);` (before line 494):

```typescript
    if (this.transitionBg) { this.transitionBg.destroy(); this.transitionBg = null; }
    if (this.streetNameText) { this.streetNameText.destroy(); this.streetNameText = null; }
    this.transitionContainer.destroy({ children: true });
```

- [ ] **Step 7: Verify build**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: Build succeeds with no TypeScript errors.

- [ ] **Step 8: Manual smoke test**

Run: `npm run tauri dev`
Test:
1. Walk player into a signpost connection (edge of street)
2. Verify: iris circle closes centered on the player character
3. Verify: dark midnight blue background visible when iris is fully closed
4. Verify: street name appears as title case (e.g., "Demo Heights") centered on screen
5. Verify: iris opens from viewport center, revealing new street
6. Verify: street name fades out as iris opens fully
7. Verify: no visual artifacts after transition completes
8. Repeat walking in both directions (left and right)

- [ ] **Step 9: Commit**

```bash
git add src/lib/engine/renderer.ts
git commit -m "feat: iris wipe transition overlay with street name title card

Replace viewport-slide transition with iris wipe effect. Circle closes
centered on the player, opens from viewport center. Street name displayed
as a centered title card during the transition hold phase."
```

---

### Task 2: Star/Swirl Decorations

**Files:**
- Modify: `src/lib/engine/renderer.ts`

**Context for implementer:**

Task 1 added the iris wipe with a solid dark background. This task adds procedural star and swirl decorations to that background, making the transition feel polished rather than flat. Stars are small 4-pointed shapes, swirls are quarter-circle arcs. Positions are stored as normalized coordinates (0-1 range), scaled to viewport dimensions each frame via `g.x`/`g.y` (resize-safe). Graphics are recreated on each new transition (detected by generation counter change) since star sizes and swirl angles differ each time.

- [ ] **Step 1: Add star/swirl fields**

Add after the `private lastTransitionGen = -1;` field:

```typescript
  private starGraphics: Graphics[] = [];
  private swirlGraphics: Graphics[] = [];
  private starPositions: { nx: number; ny: number }[] = [];
  private swirlPositions: { nx: number; ny: number }[] = [];
```

- [ ] **Step 2: Add `generateStarsAndSwirls` method**

Add as a private method after `updateTransition()`:

```typescript
  private generateStarsAndSwirls(): void {
    const screenW = this.app.screen.width;
    const screenH = this.app.screen.height;

    // Remove old graphics
    for (const g of this.starGraphics) { this.transitionContainer.removeChild(g); g.destroy(); }
    for (const g of this.swirlGraphics) { this.transitionContainer.removeChild(g); g.destroy(); }
    this.starGraphics = [];
    this.swirlGraphics = [];

    // Generate normalized positions
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

    // Draw stars at local origin (positioned via g.x/g.y for resize safety)
    for (const pos of this.starPositions) {
      const g = new Graphics();
      const size = 2 + Math.random() * 4; // 2-6px
      const alpha = 0.2 + Math.random() * 0.3; // 0.2-0.5

      // Draw centered at local origin
      g.moveTo(-size, 0);
      g.lineTo(size, 0);
      g.moveTo(0, -size);
      g.lineTo(0, size);
      g.stroke({ color: 0xffffff, alpha, width: 1 });

      // Position from normalized coords (updated each frame in updateTransition)
      g.x = pos.nx * screenW;
      g.y = pos.ny * screenH;

      this.starGraphics.push(g);
      // Insert before streetNameText (last child) so text renders on top
      const insertIdx = this.transitionContainer.children.length - 1;
      this.transitionContainer.addChildAt(g, insertIdx);
    }

    // Draw swirls at local origin
    for (const pos of this.swirlPositions) {
      const g = new Graphics();
      const radius = 30 + Math.random() * 50; // 30-80px
      const alpha = 0.1 + Math.random() * 0.1; // 0.1-0.2
      const startAngle = Math.random() * Math.PI * 2;

      g.arc(0, 0, radius, startAngle, startAngle + Math.PI / 2);
      g.stroke({ color: 0xffffff, alpha, width: 1.5 });

      g.x = pos.nx * screenW;
      g.y = pos.ny * screenH;

      this.swirlGraphics.push(g);
      const insertIdx = this.transitionContainer.children.length - 1;
      this.transitionContainer.addChildAt(g, insertIdx);
    }
  }
```

- [ ] **Step 3: Wire stars/swirls into `updateTransition`**

In `updateTransition()`, inside the `if (generation !== this.lastTransitionGen)` block, add the generate call after the `streetNameText.text` update:

```typescript
    // Update street name text and decorations on new transition
    if (generation !== this.lastTransitionGen) {
      this.lastTransitionGen = generation;
      if (this.streetNameText) {
        this.streetNameText.text = GameRenderer.formatStreetName(toStreet);
      }
      this.generateStarsAndSwirls();
    }
```

Then, after the iris hole drawing block (after the `if (this.transitionBg)` block) and before the street name alpha block, add star/swirl repositioning for resize safety:

```typescript
    // Reposition stars/swirls from normalized coords (resize-safe)
    for (let i = 0; i < this.starGraphics.length; i++) {
      this.starGraphics[i].x = this.starPositions[i].nx * screenW;
      this.starGraphics[i].y = this.starPositions[i].ny * screenH;
    }
    for (let i = 0; i < this.swirlGraphics.length; i++) {
      this.swirlGraphics[i].x = this.swirlPositions[i].nx * screenW;
      this.swirlGraphics[i].y = this.swirlPositions[i].ny * screenH;
    }
```

- [ ] **Step 4: Clean up stars/swirls in `destroy()`**

Add before the existing `this.transitionBg` cleanup line:

```typescript
    for (const g of this.starGraphics) { g.destroy(); }
    this.starGraphics = [];
    for (const g of this.swirlGraphics) { g.destroy(); }
    this.swirlGraphics = [];
```

- [ ] **Step 5: Verify build**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch && npm run build`
Expected: Build succeeds.

- [ ] **Step 6: Manual smoke test**

Run: `npm run tauri dev`
Test:
1. Trigger a street transition — verify stars (small crossed lines) and swirls (arcs) are visible during the fully-closed phase
2. Trigger a second transition — verify the pattern is different (re-randomized)
3. Verify decorations don't obscure the street name text (text should render on top)
4. Verify no visual artifacts after transition completes

- [ ] **Step 7: Verify Rust tests still pass**

Run: `cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri && cargo test --workspace`
Expected: 168 tests passing (no Rust changes, but verify no accidental modifications).

- [ ] **Step 8: Commit**

```bash
git add src/lib/engine/renderer.ts
git commit -m "feat: add star and swirl decorations to iris wipe overlay

Procedural stars (25-35 crossed lines) and swirls (3-5 quarter-circle
arcs) scattered across the dark background during transitions. Positions
re-randomized per transition via generation counter."
```
