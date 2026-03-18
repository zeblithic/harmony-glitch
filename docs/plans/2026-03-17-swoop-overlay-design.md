# Swoop Visual Overlay — Design Spec

**Bead:** glitch-uya
**Date:** 2026-03-17
**Status:** Approved

## Problem

Street transitions work mechanically (the Rust state machine drives progress 0.0→1.0 correctly), but the visual effect is just a viewport offset — the world containers slide horizontally. There is no screen wipe, no decorative overlay during the transition, and no indication of what street the player is arriving at.

## Goals

- Replace the viewport-slide with an iris wipe (shrinking/expanding circle) overlay
- Show the destination street name as a centered title card during the transition
- Add a decorative star/swirl pattern behind the iris to make the transition feel polished

## Non-Goals

- Loading indicator while stalled at progress 0.9 (stall is sub-second for local assets)
- Animated star/swirl pattern (static is sufficient, avoids per-frame draw cost)
- Rust-side changes (`TransitionInfo` already provides everything needed)
- Automated testing (PixiJS rendering is not testable in jsdom)

## Naming Note

The Rust struct is `TransitionFrame` (state.rs), which serializes to camelCase and is consumed on the TypeScript side as `TransitionInfo` (types.ts). This spec uses `TransitionInfo` when referring to the frontend interface and `TransitionFrame` when referring to the Rust struct.

## Iris Wipe Effect

### Overview

A new `transitionContainer` is added to the PixiJS scene graph above all other containers. It draws a full-screen dark background, with a circular mask revealing the game underneath. The circle radius is driven by `TransitionInfo.progress`.

### Scene Graph

```
app.stage
  ├── parallaxContainer     (existing)
  ├── worldContainer        (existing)
  ├── uiContainer           (existing)
  └── transitionContainer   (NEW — on top of everything)
       ├── backgroundGraphics   (full-screen dark fill)
       ├── star Graphics × ~30  (scattered decorations)
       ├── swirl Graphics × ~4  (subtle arcs)
       └── streetNameText       (centered Text)
```

### Iris Animation

The iris maps 1:1 to `TransitionInfo.progress` (0.0→1.0). No separate timer.

- **Closing phase** (progress 0.0 → 0.5): circle radius shrinks from `maxRadius` to 0
  - `maxRadius = Math.hypot(screen.width, screen.height)` (viewport diagonal, using `this.app.screen` for CSS logical pixels)
  - Circle centered on player's **screen position**: `(avatarGraphics.x + worldContainer.x, avatarGraphics.y + worldContainer.y)`
- **Opening phase** (progress 0.5 → 1.0): circle radius grows from 0 to `maxRadius`
  - Circle centered on **viewport center**: `(screen.width / 2, screen.height / 2)`

The center shifts from player to viewport center because the player hasn't been repositioned on the new street during the opening phase — viewport center is a neutral anchor.

At progress=0.5 the radius is exactly 0. When `radius <= 0`, the mask is not applied (or set to an empty shape), so the full background is visible with no hole — this is the desired fully-closed state.

### Masking Technique

PixiJS v8's `.cut()` method fails when the hole extends outside the parent shape, which happens when the iris center is near a viewport edge. Instead, use a **mask on the transitionContainer** with an inverted approach:

1. `backgroundGraphics` draws a full-screen rect in dark midnight blue (`0x0d0d2b`) — no hole
2. A separate `irisMask` Graphics object draws a circle at the computed center/radius
3. `irisMask` is set as the `mask` property on the `transitionContainer`
4. Since PixiJS masks show content *inside* the mask shape, we need the inverse. The solution: set the mask on a **clear layer** that sits between the game and the overlay, OR use the `inverseMask` approach:
   - Set `transitionContainer.mask = irisMask` with `irisMask.isMask = true`
   - To invert: draw the `irisMask` as a full-screen rect, then `.cut()` a circle from it (the cut stays within the rect bounds since the rect is the same size as the viewport, and the circle is always smaller than maxRadius which equals the viewport diagonal — the cut may extend slightly outside corners but this is acceptable since the corners are covered by the rect)
   - Alternative: draw `backgroundGraphics` itself using even-odd fill rule — single `beginPath()`, draw outer rect clockwise, draw inner circle counter-clockwise, fill with even-odd rule. This avoids `.cut()` entirely.

The implementer should use whichever PixiJS v8 technique produces correct results. The key constraint is: the dark background must be visible everywhere **except** inside the iris circle.

### Background Fill

Dark midnight blue (`0x0d0d2b`). The background `Graphics` is redrawn each frame during transitions (only the circle position and radius change). When `radius <= 0`, the full viewport is filled with no hole.

### Replacing Viewport Slide

The existing viewport-slide code (renderer.ts lines 423-438) is **removed**. The iris wipe fully covers the screen during the transition, so sliding the world containers is unnecessary — the game world is hidden behind the overlay.

## Street Name Title Card

A PixiJS `Text` object centered on screen, displaying the destination street name from `TransitionInfo.toStreet`.

### Text Formatting

The raw street name (e.g. `"demo_heights"`) is converted to title case with underscores replaced by spaces → `"Demo Heights"`. White text, ~28px, centered horizontally and vertically on the viewport (using `this.app.screen.width/height`). No background panel.

### Timing

| Progress | Street Name State |
|----------|------------------|
| 0.0 → 0.48 | Hidden (alpha=0, iris closing) |
| 0.48 → 0.52 | Fades in (alpha ramps 0→1) |
| 0.52 → 0.8 | Visible at full alpha (iris opening) |
| 0.8 → 1.0 | Fades out (alpha ramps 1→0) |

### Lifecycle

The `Text` object is created once (in `buildScene` or `init`) and reused across transitions. Its `text`, `visible`, and `alpha` properties are updated each frame.

## Star/Swirl Pattern

Procedural decorations on the iris background.

### Stars

- ~25-35 small 4-pointed shapes (crossed lines or tiny diamonds)
- White fill with low alpha (0.2-0.5)
- Sizes varying from 2-6px
- Positions stored as **normalized coordinates** (0-1 range), scaled to viewport dimensions at draw time — ensures correct coverage after window resize

### Swirls

- 3-5 subtle arcs (quarter-circle strokes)
- White stroke with very low alpha (0.1-0.2)
- Larger radius (30-80px)
- Positions stored as normalized coordinates

### Regeneration

Star/swirl positions are re-randomized when a new transition starts, detected by comparing the current `TransitionInfo.generation` against a stored `lastTransitionGeneration`. This ensures each transition has a slightly different pattern. The `Graphics` objects are created once and reused — only their positions are updated.

### Performance

Static `Graphics` objects with no per-frame updates (positions set once at transition start). PixiJS batches them efficiently. The only per-frame work during a transition is:
1. Redrawing `backgroundGraphics` (one rect with circle mask)
2. Updating `streetNameText` alpha

## Renderer Integration

### updateFrame() Changes

Remove the existing viewport-slide code. Add a new `updateTransition(frame)` method called at the end of `updateFrame()`:

1. If `frame.transition` is null/undefined: `transitionContainer.visible = false`, return
2. `transitionContainer.visible = true`
3. If `frame.transition.generation !== lastTransitionGeneration`: re-randomize star/swirl positions, update `streetNameText.text`, store new generation
4. Compute iris radius from progress:
   - Closing (0.0→0.5): `radius = maxRadius * (1 - progress * 2)`
   - Opening (0.5→1.0): `radius = maxRadius * ((progress - 0.5) * 2)`
5. When `radius <= 0`: no hole, full background visible
6. Compute iris center:
   - Closing: player screen position (`avatarGraphics.x + worldContainer.x`, `avatarGraphics.y + worldContainer.y`)
   - Opening: viewport center (`this.app.screen.width / 2`, `this.app.screen.height / 2`)
7. Redraw background with iris hole at computed center/radius
8. Update `streetNameText` alpha per timing table above

### buildScene() Changes

Create `transitionContainer` and children (background, stars, swirls, text) if not already created. Set `transitionContainer.visible = false`.

### destroy() Changes

Destroy `transitionContainer` and all its children (backgroundGraphics, star/swirl Graphics, streetNameText) following the same explicit cleanup pattern as existing sprite/text teardown.

## Testing

Manual testing only (PixiJS rendering not testable in jsdom):

- Iris closes centered on player, opens from viewport center
- Street name appears in title case during hold phase, fades out
- Star/swirl pattern visible during fully-closed phase
- Star/swirl pattern changes between transitions (generation-based re-randomization)
- No leftover visual state after transition completes
- Both left and right directions work identically
- Window resize during transition doesn't break overlay coverage
- Existing Rust transition tests (8 tests in transition.rs) unaffected

## Files Modified

| File | Change |
|------|--------|
| `src/lib/engine/renderer.ts` | Add `transitionContainer`, star/swirl generation, `updateTransition()` method, `destroy()` cleanup, remove viewport-slide code |

No new files. No Rust changes. No type changes.
