# Sprite System — Design Spec

**Bead:** glitch-q6p
**Date:** 2026-03-18
**Status:** Approved

## Problem

Every visual element in the game renders as a colored rectangle — blue for the avatar, green for decos, gold for entities, gold circles for ground items. The data pipeline already provides `spriteClass` (entities, decos) and `icon` (items) keys, plus `AnimationState` (idle/walking/jumping/falling) and `facing` direction. The frontend needs to map these keys to actual textures.

## Goals

- Replace placeholder rectangles with real sprite rendering (PixiJS `Sprite` and `AnimatedSprite`)
- Support animated avatar with state-driven animation (idle, walking, jumping, falling)
- Load TexturePacker JSON Hash sprite sheets for animated entities
- Load individual PNG textures for static assets (decos, items, entities without animation)
- Fall back gracefully to current colored rectangles when textures are missing
- Log missing sprite classes on street load for asset pipeline tracking
- Zero visual regression — game looks identical until art assets are added

## Non-Goals

- Converting Flash SWF assets to PNG (that's `glitch-4nn` asset pipeline bead)
- Skeletal animation or spine-based animation
- Dynamic texture loading from network/CAS
- NPC animation states beyond static sprites (that's `glitch-4pe` NPC bead)
- Remote player animation — `RemotePlayerFrame` lacks `animation` field; adding it requires Rust changes (future multiplayer bead)
- Sprite editor or runtime asset hot-reload

## Asset Source Context

The original Glitch art (~18,000+ files across 8 repos at `~/work/tinyspeck/glitch-*`) is entirely Adobe Flash SWF/FLA format. No pre-made sprite sheets or animation metadata exist outside of Flash. The `glitch-hq-android` repo has 294 PNG drawables (item icons). Converting SWF → PNG sprite sheets is the asset pipeline bead's responsibility. This bead builds the rendering system with a small set of placeholder art and validates the architecture works.

## Architecture

### New File: `src/lib/engine/sprites.ts`

A `SpriteManager` class that owns texture loading, sprite creation, and fallback logic. The renderer delegates to it instead of drawing Graphics directly.

```typescript
class SpriteManager {
  async init(): Promise<void>
  async loadStreetAssets(street: StreetData): Promise<void>
  createAvatar(): Container
  createDeco(deco: Deco): Container
  createEntity(entity: WorldEntityFrame): Container
  createGroundItem(item: WorldItemFrame): Container
  updateAvatar(container: Container, animation: AnimationState, facing: Direction): void
  hasTexture(spriteClass: string): boolean
  destroy(): void
}
```

### Modified File: `src/lib/engine/renderer.ts`

- Import and hold a `SpriteManager` instance
- Replace `new Graphics()` placeholder drawing with `spriteManager.create*()` calls
- Add `spriteManager.updateAvatar()` call in `updateFrame()` for animation state changes
- Existing lifecycle management (Maps for entities/items/remoteSprites, create/update/destroy loops) stays exactly the same — only the creation step changes
- `avatarGraphics: Graphics` widens to `avatarContainer: Container` to hold either `AnimatedSprite` or fallback `Graphics`. This field is referenced in `updateFrame()`, `updateTransition()` (iris center calculation), and `updateChatBubbles()` — all references must be updated to the new name.

### No Rust Changes

The Rust side already provides everything needed: `spriteClass`, `icon`, `AnimationState`, `Direction`. No type changes or new IPC commands required.

## Sprite Sheet Format

TexturePacker JSON Hash — native PixiJS v8 `Spritesheet` support.

### Avatar Spritesheet

`assets/sprites/avatar/avatar.json` + `avatar.png`:

```json
{
  "frames": {
    "idle_0": { "frame": {"x":0, "y":0, "w":30, "h":60} },
    "idle_1": { "frame": {"x":30, "y":0, "w":30, "h":60} },
    "walk_0": { "frame": {"x":0, "y":60, "w":30, "h":60} },
    "walk_1": { "frame": {"x":30, "y":60, "w":30, "h":60} },
    "walk_2": { "frame": {"x":60, "y":60, "w":30, "h":60} },
    "walk_3": { "frame": {"x":90, "y":60, "w":30, "h":60} },
    "jump_0": { "frame": {"x":0, "y":120, "w":30, "h":60} },
    "fall_0": { "frame": {"x":30, "y":120, "w":30, "h":60} }
  },
  "animations": {
    "idle": ["idle_0", "idle_1"],
    "walking": ["walk_0", "walk_1", "walk_2", "walk_3"],
    "jumping": ["jump_0"],
    "falling": ["fall_0"]
  },
  "meta": {
    "image": "avatar.png",
    "size": {"w": 256, "h": 256},
    "scale": 1
  }
}
```

- The `animations` key is part of the TexturePacker JSON Hash spec. PixiJS v8's `Spritesheet` parses it natively and exposes `spritesheet.animations['walking']` as an array of `Texture` objects ready for `AnimatedSprite`.
- Animation names match `AnimationState` values exactly (`idle`, `walking`, `jumping`, `falling`) — no mapping layer needed.
- Direction (left/right) handled by `scale.x = -1` on the container — frames only face right.
- `AnimatedSprite.animationSpeed` controls playback rate (~0.15 for walk, ~0.08 for idle).

### Static Assets

Individual PNG files in category subdirectories:

- `assets/sprites/decos/<spriteClass>.png` — background decorations
- `assets/sprites/entities/<spriteClass>.png` — world entities (trees, NPCs)
- `assets/sprites/items/<icon>.png` — ground item icons

Loaded individually via PixiJS `Assets.load()`. No atlas needed — static assets are loaded once per street, PixiJS batches draw calls regardless.

## Animation State Machine

The avatar `AnimatedSprite` tracks a `currentAnimation` string. On each frame, `updateAvatar()` compares `frame.player.animation` against `currentAnimation`:

- **Same state:** Do nothing. Animation continues looping.
- **Different state:** Swap textures to the new animation group via `animatedSprite.textures = spritesheet.animations[newState]`, set `animationSpeed`, call `play()`. Store new `currentAnimation`.

Animation speeds:
- `idle`: 0.08 (slow breathing/sway)
- `walking`: 0.15 (brisk step cycle)
- `jumping`: 0.1 (single frame)
- `falling`: 0.1 (single frame)

All states call `play()` uniformly — single-frame animations display their sole frame regardless of speed, so no special-casing is needed.

Facing direction: `container.scale.x = facing === 'right' ? 1 : -1`. Applied to the parent container so the `AnimatedSprite` anchor stays correct.

## Fallback System

When `SpriteManager` looks up a `spriteClass` or `icon` and no texture exists:

| Creator Method | Fallback | Matches Current |
|---------------|----------|-----------------|
| `createAvatar()` | Blue rect 30×60 (`0x5865f2`) | Yes |
| `createDeco(deco)` | Green rect (`0x4a6741`, alpha 0.3) sized to `deco.w × deco.h` | Yes |
| `createEntity(entity)` | If `spriteClass.startsWith('tree')`: forest green rect 60×80 (`0x2d8a4e`); otherwise: tan rect 30×30 (`0xc4a35a`). Both include text label below. | Yes |
| `createGroundItem(item)` | Gold circle r=8 (`0xe8c170`) + text label | Yes |

Zero visual regression — the game looks identical for any sprite class that doesn't have art yet.

### Missing Asset Logging

`loadStreetAssets()` pre-loads textures for the street's decos (available in `StreetData`). Entity and item textures are loaded lazily on first `createEntity()`/`createGroundItem()` call — these are dynamic world state not known at street-load time. A missing-texture warning is logged once per sprite class on first miss (deduplicated via a `Set<string>` of already-warned keys). On street load, deco misses are logged as a grouped warning:

```
[SpriteManager] Missing deco textures for street "demo_meadow":
  cloud_fluffy, bush_small, rock_mossy
```

Entity and item misses are logged individually on first encounter:
```
[SpriteManager] Missing entity texture: npc_butterfly
[SpriteManager] Missing item texture: grain
```

Each sprite class is warned about only once (deduplicated). This provides a clear audit trail without per-frame spam.

## Texture Caching

- Avatar spritesheet loaded once at `init()` time (always needed)
- Individual textures cached in a `Map<string, Texture>` inside `SpriteManager`
- Cache persists across street transitions — decos shared between streets don't reload
- `loadStreetAssets()` skips textures already in cache
- Cache cleared in `SpriteManager.destroy()`

## Deco Sprite Sizing

Decos in the street XML have explicit `w` (width) and `h` (height) dimensions. When a deco texture loads:

1. Create a `Sprite` from the texture
2. Set anchor to `(0.5, 1)` (center-bottom) — this gives correct pivot for both rotation and horizontal flip
3. Scale the sprite to match the deco's `w`/`h`: `sprite.width = deco.w; sprite.height = deco.h`
4. Position at `(deco.x - street.left + deco.w / 2, deco.y - street.top)` — offset by half-width to compensate for center anchor
5. Apply `hFlip` via `scale.x = -1` — with center-bottom anchor, this flips in-place (no additional x-offset needed, unlike the current Graphics code which uses `g.x += deco.w` to compensate for a left-edge pivot)
6. Apply rotation via `sprite.rotation = deco.r`

The fallback `Graphics` path continues using the current approach (rect drawn at local origin with manual x-offset for hFlip) to avoid changing existing behavior.

## Scene Graph Changes

```
app.stage
  ├── parallaxContainer              (unchanged)
  │   ├── bgGraphics                 (unchanged)
  │   └── layer containers
  │       └── Sprite OR fallback Graphics   (was: always Graphics)
  ├── worldContainer                 (unchanged)
  │   ├── middleground layer         (unchanged)
  │   ├── platformGraphics           (unchanged)
  │   ├── avatarContainer            (was: avatarGraphics)
  │   │   └── AnimatedSprite OR fallback Graphics
  │   ├── entitySprites Map          (unchanged structure)
  │   │   └── Container { Sprite OR fallback + Text }
  │   ├── groundItemSprites Map      (unchanged structure)
  │   │   └── Container { Sprite OR fallback + Text }
  │   └── remoteSprites Map          (unchanged structure)
  │       └── Container { fallback Graphics + Text }  (no animation data available)
  ├── uiContainer                    (unchanged)
  └── transitionContainer            (unchanged)
```

## Placeholder Art

Minimum viable set to validate all code paths:

| File | Type | Purpose |
|------|------|---------|
| `assets/sprites/avatar/avatar.png` + `.json` | Spritesheet (8-12 frames) | AnimatedSprite, animation switching, facing flip |
| `assets/sprites/entities/tree_fruit.png` | Individual (~60×80px) | Entity sprite rendering |
| `assets/sprites/items/cherry.png` | Individual (~16×16px) | Ground item sprite rendering |
| `assets/sprites/decos/tree_bg.png` | Individual (~120×200px) | Deco sprite rendering with scaling |

Art source: AI-generated simple pixel art or hand-drawn geometric shapes. These are throwaway — replaced by real Glitch art when the asset pipeline (`glitch-4nn`) delivers converted sprite sheets. Dropping in a new PNG automatically replaces the placeholder with zero code changes.

## Testing

### Automated (vitest)

`SpriteManager` logic can be unit-tested in jsdom for:
- `formatStreetName()` helper (already exists, shared)
- Missing texture logging (mock `Assets.load` to reject)
- `hasTexture()` cache behavior
- Animation state switching logic (given mock textures)

PixiJS rendering itself is not testable in jsdom — validated manually.

### Manual

- Walk around demo street — avatar animates (walk cycle plays, idle when stopped)
- Jump — animation switches to jump frame, then fall frame
- Face left/right — sprite flips correctly
- Entities with art show sprites; entities without art show fallback rectangles
- Ground items with art show sprites; others show fallback circles
- Decos with art show sprites scaled to correct size; others show fallback rects
- Street transition — new street loads correct assets
- Console shows grouped missing texture warning on street load
- Window resize doesn't break sprite positioning
- Existing Rust tests unaffected (no Rust changes)

## Files Modified

| File | Change |
|------|--------|
| `src/lib/engine/sprites.ts` | **Create** — SpriteManager class |
| `src/lib/engine/renderer.ts` | **Modify** — delegate to SpriteManager, widen avatar type |
| `assets/sprites/avatar/avatar.json` | **Create** — TexturePacker spritesheet metadata |
| `assets/sprites/avatar/avatar.png` | **Create** — placeholder avatar sprite sheet |
| `assets/sprites/entities/tree_fruit.png` | **Create** — placeholder entity sprite |
| `assets/sprites/items/cherry.png` | **Create** — placeholder item sprite |
| `assets/sprites/decos/tree_bg.png` | **Create** — placeholder deco sprite |

No Rust changes. No type changes.
