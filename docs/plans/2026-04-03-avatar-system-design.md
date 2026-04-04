# Avatar System Design — harmony-glitch

**Date:** 2026-04-03
**Bead:** harmony-glitch-m5j
**Status:** Design

## Overview

The original Glitch avatar was a sophisticated "paper doll" composition engine:
536 wardrobe items across 8 categories, 131 vanity (facial) features across 5
categories, parametric facial scaling, arbitrary hex color tinting for skin and
hair, and 13+ animation states synchronized across all layers. Assets were pure
Flash vector (SWF). The base body was gender-neutral — gender expression came
entirely through wardrobe choices.

harmony-glitch currently renders the player as a single 30x60 placeholder
AnimatedSprite inside a PixiJS Container. This design replaces that with a
layered composition system that faithfully reproduces the original's
expressiveness while working in a modern PixiJS + Rust stack.

## Goals

1. Players can customize face, hair, skin color, and clothing before entering the world
2. Avatar renders as layered sprite composition in PixiJS (no Flash)
3. Appearance state lives in Rust GameState for persistence and future multiplayer sync
4. Asset pipeline converts original SWF art to sprite sheets
5. Customization UI feels immediate — changes preview in real-time

## Non-Goals (for now)

- Server-authoritative wardrobe validation (Phase C)
- Clothing acquisition/economy (depends on economy system)
- Avatar scale potions / temporary transformations
- Back-facing view (only side-facing needed for Phase A platformer)

---

## 1. Asset Pipeline

### 1.1 Source Assets

The `glitch-avatars` repo contains 672 SWF files:

| Category | Count | Examples |
|----------|-------|---------|
| Hat | 120 | aviator_hat, bowler_hat, deadmau5_hat |
| Coat | 97 | leather_coat, steampunk_coat |
| Shirt | 97 | hawaiian_shirt, pirate_shirt |
| Pants | 73 | cowboy_pants, cargo_pants |
| Dress | 58 | toga_male, toga_female |
| Shoes | 60 | campers, cross_strap_sandals |
| Skirt | 29 | scotland_kilt, carnival_tail |
| Bracelet | 2 | bracelet variants |
| Eyes | 29 | eyes_01–12, eyes_alien, eyes_robot |
| Ears | 8 | ears_0001–0009 |
| Nose | 24 | nose_0001+ |
| Mouth | 21 | mouth_01+ |
| Hair | 49 | pigtails, mullet, loopy_bun |
| Base body | 2 | Avatar.swf, Avatar2011.swf |

Plus `inc_data_clothing.js` (900+ items with color tint definitions) and
`inc_data_faces.js` (131 face features with asset paths).

### 1.2 Extraction Strategy

Each wardrobe/vanity SWF contains a single MovieClip with animation frames
matching the base avatar's timeline labels (idle1-4, walk1x, walk2x, jumpUp,
jumpOver, etc.). Since the assets are vector, we need to rasterize them.

**Approach: Per-item sprite sheet generation**

For each SWF file:
1. Parse with the existing `extract-swf` tool (already handles SWF decompression)
2. Render each animation frame at a target resolution (e.g., 128px tall per frame)
3. Pack frames into a sprite sheet PNG + JSON manifest
4. Store in `assets/sprites/avatar/{category}/{item_name}.json`

The base avatar body needs the same treatment but with ~30 body-part layers
extracted separately so we can tint skin independently.

**Base body extraction (special case):**

The base Avatar2011.swf contains nested MovieClips for each body part:
- `skull`, `torso`, `arm_upper_close/offside`, `arm_lower_close/offside`,
  `hand_close/offside`, `leg_upper_close/offside`, `leg_lower_close/offside`,
  `foot_close/offside` (14 front parts)
- Plus `ear_close` for ear positioning

Each body part needs its own sprite sheet so we can apply independent skin
color tinting. For the initial implementation, we can simplify: render the
**entire base body as a single sheet** with a neutral skin tone, and apply
PixiJS `ColorMatrixFilter` for skin tinting (matching the original's
`ColorMatrix.colorize()` approach).

### 1.3 Simplified Phase A Approach

Full extraction of all 672 SWFs is a significant pipeline project. For Phase A:

1. **Hand-pick a starter set:** ~20-30 items per wardrobe category, all vanity items
2. **Pre-render at 2x resolution** for crisp display on HiDPI
3. **4 animation states** (idle, walk, jump, fall) — map from original labels:
   - idle → idle1
   - walk → walk1x
   - jump → jumpUp
   - fall → jumpOver (or a static falling pose)
4. **Side-facing only** (no back view needed for 2D platformer)

Target file structure:
```
assets/sprites/avatar/
├── base/
│   ├── body.json          # Base body sprite sheet (all frames)
│   └── body.png
├── eyes/
│   ├── eyes_01.json
│   ├── eyes_01.png
│   └── ...
├── hair/
│   ├── pigtails.json
│   ├── pigtails.png
│   └── ...
├── hat/
│   ├── aviator_hat.json
│   ├── aviator_hat.png
│   └── ...
├── coat/
├── shirt/
├── pants/
├── dress/
├── skirt/
├── shoes/
└── manifest.json          # Index of all available items per category
```

### 1.4 Manifest Format

```json
{
  "categories": {
    "eyes": {
      "items": [
        { "id": "eyes_01", "name": "Classic", "sheet": "eyes/eyes_01.json" },
        { "id": "eyes_alien", "name": "Alien", "sheet": "eyes/eyes_alien.json" }
      ]
    },
    "hair": {
      "items": [
        { "id": "pigtails", "name": "Pigtails", "sheet": "hair/pigtails.json" }
      ]
    }
  },
  "defaults": {
    "eyes": "eyes_01",
    "ears": "ears_0001",
    "nose": "nose_0001",
    "mouth": "mouth_01",
    "hair": "hair_01",
    "skin_color": "D4C159",
    "hair_color": "4A3728",
    "shirt": "hawaiian_shirt",
    "pants": "cargo_pants",
    "shoes": "campers"
  }
}
```

---

## 2. Data Model

### 2.1 Rust Types

```rust
// src-tauri/src/avatar/types.rs

/// Complete avatar appearance — stored in GameState, sent in PlayerFrame,
/// persisted to disk, and eventually broadcast to peers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarAppearance {
    // Vanity (face)
    pub eyes: String,        // e.g. "eyes_01"
    pub ears: String,        // e.g. "ears_0001"
    pub nose: String,        // e.g. "nose_0001"
    pub mouth: String,       // e.g. "mouth_01"
    pub hair: String,        // e.g. "pigtails"

    // Colors (hex strings, no # prefix)
    pub skin_color: String,  // e.g. "D4C159"
    pub hair_color: String,  // e.g. "4A3728"

    // Vanity scaling (original ranges preserved)
    pub eye_scale: f64,      // 0.65–1.2, default 0.9
    pub eye_height: f64,     // -5–4, default 1
    pub eye_dist: f64,       // -3–2, default -1
    pub ears_scale: f64,     // 0.6–1.2, default 1
    pub ears_height: f64,    // -4–1, default 0
    pub nose_scale: f64,     // 0.65–1.45, default 1
    pub nose_height: f64,    // -5–6, default 0
    pub mouth_scale: f64,    // 0.75–1.45, default 1
    pub mouth_height: f64,   // -3–4, default 0

    // Wardrobe (None = slot empty)
    pub hat: Option<String>,
    pub coat: Option<String>,
    pub shirt: Option<String>,
    pub pants: Option<String>,
    pub dress: Option<String>,
    pub skirt: Option<String>,
    pub shoes: Option<String>,
    pub bracelet: Option<String>,
}

impl Default for AvatarAppearance {
    fn default() -> Self {
        Self {
            eyes: "eyes_01".into(),
            ears: "ears_0001".into(),
            nose: "nose_0001".into(),
            mouth: "mouth_01".into(),
            hair: "hair_01".into(),
            skin_color: "D4C159".into(),
            hair_color: "4A3728".into(),
            eye_scale: 0.9, eye_height: 1.0, eye_dist: -1.0,
            ears_scale: 1.0, ears_height: 0.0,
            nose_scale: 1.0, nose_height: 0.0,
            mouth_scale: 1.0, mouth_height: 0.0,
            hat: None,
            coat: None,
            shirt: Some("hawaiian_shirt".into()),
            pants: Some("cargo_pants".into()),
            dress: None,
            skirt: None,
            shoes: Some("campers".into()),
            bracelet: None,
        }
    }
}
```

### 2.2 Integration into GameState

```rust
// In engine/state.rs — add to GameState:
pub struct GameState {
    pub player: PhysicsBody,
    pub avatar: AvatarAppearance,  // NEW
    // ... existing fields ...
}

// In PlayerFrame (sent each tick):
pub struct PlayerFrame {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub facing: Direction,
    pub animation: AnimationState,
    pub on_ground: bool,
    // Avatar appearance only sent on change (not every frame):
    // Frontend caches it and only updates layers when it changes.
}
```

Avatar appearance doesn't need to be in every RenderFrame — it changes
infrequently. Instead, send it:
- Once on game start (via a new `get_avatar` IPC command)
- On change (via a `set_avatar` IPC command that returns the new state)

### 2.3 New IPC Commands

```rust
#[tauri::command]
fn get_avatar(state: State<GameState>) -> AvatarAppearance { ... }

#[tauri::command]
fn set_avatar(state: State<GameState>, appearance: AvatarAppearance) -> AvatarAppearance { ... }

#[tauri::command]
fn get_avatar_manifest() -> AvatarManifest { ... }
```

---

## 3. Rendering Architecture

### 3.1 Layered Container Composition

Replace the current single `AnimatedSprite` with a multi-layer `Container`:

```
avatarContainer (Container)
├── layer_body (AnimatedSprite — base body, tinted for skin_color)
├── layer_shoes (AnimatedSprite — if equipped)
├── layer_pants (AnimatedSprite — if equipped)
├── layer_shirt (AnimatedSprite — if equipped)
├── layer_dress (AnimatedSprite — if equipped, replaces shirt+pants visually)
├── layer_skirt (AnimatedSprite — if equipped)
├── layer_coat (AnimatedSprite — if equipped)
├── layer_eyes (AnimatedSprite — positioned by eye_height, scaled by eye_scale)
├── layer_nose (AnimatedSprite — positioned by nose_height, scaled by nose_scale)
├── layer_mouth (AnimatedSprite — positioned by mouth_height, scaled by mouth_scale)
├── layer_ears (AnimatedSprite — positioned by ears_height, scaled by ears_scale)
├── layer_hair (AnimatedSprite — tinted for hair_color)
├── layer_hat (AnimatedSprite — if equipped)
└── layer_bracelet (AnimatedSprite — if equipped)
```

**Key principles:**
- All layers share the same anchor point (0.5, 1.0 = center-bottom)
- All layers use the same animation state and frame timing
- `container.scale.x = -1` for left-facing flips ALL children at once
- Layers added/removed dynamically as equipment changes
- Vanity layers repositioned via `sprite.y += appearance.nose_height * SCALE_FACTOR`
- Color tinting: `sprite.tint = parseInt(appearance.skin_color, 16)`

### 3.2 AvatarCompositor Class

New class in `src/lib/engine/avatar.ts`:

```typescript
export class AvatarCompositor {
  private container: Container;
  private layers: Map<string, AnimatedSprite> = new Map();
  private appearance: AvatarAppearance;
  private sheets: Map<string, Spritesheet> = new Map();

  constructor() {
    this.container = new Container();
  }

  /** Load sprite sheets for current appearance */
  async applyAppearance(appearance: AvatarAppearance): Promise<void> {
    // Diff against current appearance — only reload changed layers
    // For each changed slot: load sheet, create AnimatedSprite, add to container
    // Apply tints, scales, offsets
  }

  /** Sync all layers to the current animation state */
  updateAnimation(animation: AnimationState, facing: Direction): void {
    this.container.scale.x = facing === 'right' ? 1 : -1;
    for (const [name, sprite] of this.layers) {
      const textures = this.sheets.get(name)?.animations[animation];
      if (textures && sprite.textures !== textures) {
        sprite.textures = textures;
        sprite.animationSpeed = ANIMATION_SPEEDS[animation];
        sprite.play();
      }
    }
  }

  getContainer(): Container { return this.container; }
}
```

### 3.3 Color Tinting

PixiJS supports two tinting approaches:

**Simple tint** (for skin/hair — covers most cases):
```typescript
bodySprite.tint = parseInt(appearance.skin_color, 16);
hairSprite.tint = parseInt(appearance.hair_color, 16);
```

**ColorMatrixFilter** (for clothing with multi-color tints, matching original):
```typescript
import { ColorMatrixFilter } from 'pixi.js';
const filter = new ColorMatrixFilter();
// The original used: tintColor, brightness, saturation, contrast, tintAmount
// PixiJS ColorMatrixFilter supports: brightness(), contrast(), saturate(), tint()
sprite.filters = [filter];
```

For Phase A, simple tint is sufficient. Multi-color clothing tinting (the
original's `color_1`/`color_2` regions) is a Phase B enhancement.

### 3.4 Facial Feature Positioning

Vanity items (eyes, nose, mouth, ears) need parametric positioning:

```typescript
// Scale factor converts original parameter ranges to pixel offsets
const PX_PER_UNIT = 2; // Tune based on rendered sprite resolution

function positionVanityLayer(
  sprite: AnimatedSprite,
  scale: number,
  height: number,
  dist?: number  // Only for eyes
): void {
  sprite.scale.set(scale, scale);
  sprite.y = -(height * PX_PER_UNIT); // Negative because higher = more negative Y
  if (dist !== undefined) {
    sprite.x = dist * PX_PER_UNIT;
  }
}
```

---

## 4. Customization UI

### 4.1 AvatarEditor Component

New Svelte component: `src/lib/components/AvatarEditor.svelte`

**Layout:**
```
┌─────────────────────────────────────────────────┐
│  ┌─────────────┐  ┌─────────────────────────┐   │
│  │             │  │  [Face] [Hair] [Clothes] │   │
│  │   PREVIEW   │  ├─────────────────────────┤   │
│  │  (live      │  │                         │   │
│  │   rendered  │  │  Item grid (scrollable) │   │
│  │   avatar)   │  │  ┌───┐ ┌───┐ ┌───┐     │   │
│  │             │  │  │   │ │   │ │   │     │   │
│  │             │  │  └───┘ └───┘ └───┘     │   │
│  │             │  │  ┌───┐ ┌───┐ ┌───┐     │   │
│  │             │  │  │   │ │   │ │   │     │   │
│  │             │  │  └───┘ └───┘ └───┘     │   │
│  └─────────────┘  ├─────────────────────────┤   │
│                   │  Color picker (skin/hair)│   │
│  [Randomize]      │  ○○○○○○○○  ○○○○○○○○    │   │
│                   │  Sliders (scale/height)  │   │
│  [Save] [Cancel]  │  eye_scale ═══════●═══  │   │
│                   └─────────────────────────┘   │
└─────────────────────────────────────────────────┘
```

**Tabs:**
- **Face** — Eyes, ears, nose, mouth sub-categories with scaling sliders
- **Hair** — Hair styles + hair color picker
- **Body** — Skin color picker
- **Clothes** — Hat, coat, shirt, pants/skirt/dress, shoes sub-categories

**Interaction:**
- Clicking an item in the grid immediately previews it on the avatar
- Color changes preview in real-time via the AvatarCompositor
- Sliders update facial feature positioning live
- "Randomize" picks random valid values for all slots
- "Save" calls `set_avatar` IPC command
- "Cancel" reverts to saved appearance

### 4.2 Color Picker

Rather than arbitrary hex input, offer a curated palette inspired by the original
defaults, plus a "custom" option:

```typescript
const SKIN_PALETTE = [
  'F5D6B8', 'D4C159', 'C68642', '8D5524', '4A2912',
  'FFE0BD', 'FFCD94', 'F5C68C', 'E0AC69', 'C68642',
  'A16E3B', '7B4B2A', '5C3317', '3B1F0B', 'FFB6C1',
  'E8B4B8', 'B8D4E3', 'A8E6CF', 'FFD93D',
];

const HAIR_PALETTE = [
  '000000', '4A3728', '8B4513', 'B87333', 'DAA520',
  'FFD700', 'F5F5DC', 'FF4500', 'FF69B4', '9370DB',
  '00CED1', '32CD32', '4169E1', 'FF1493', 'C0C0C0',
];
```

---

## 5. Persistence

Avatar appearance persists alongside the existing game save:

```rust
// In existing SavedState (or equivalent):
pub struct SavedState {
    pub street_id: String,
    pub player_x: f64,
    pub player_y: f64,
    pub inventory: Vec<InventorySlot>,
    pub avatar: AvatarAppearance,  // NEW
}
```

On game start:
1. Load saved state (includes avatar appearance)
2. If no save exists, show AvatarEditor for initial customization
3. Avatar editor also accessible from pause/settings menu

---

## 6. Future: Multiplayer Avatar Sync (Phase B)

When networking is wired in Phase B, avatar appearance becomes part of the
player announcement:

```rust
// In network/types.rs — PlayerAnnounce message:
pub struct PlayerAnnounce {
    pub display_name: String,
    pub avatar: AvatarAppearance,  // Full appearance sent once on connect
}

// In RemotePlayerFrame (sent each tick — no appearance, just position):
pub struct RemotePlayerFrame {
    pub peer_id: String,
    pub display_name: String,
    pub x: f64,
    pub y: f64,
    pub facing: Direction,
    pub animation: AnimationState,
    // appearance cached client-side from PlayerAnnounce
}
```

This is bandwidth-efficient: full appearance (~500 bytes) sent once on
connection, then only position/animation (~40 bytes) each tick.

---

## 7. Implementation Plan

### Phase 1: Asset extraction (prerequisite)
- Extend `extract-swf` to handle avatar SWFs specifically:
  - Render animation frames at target resolution
  - Generate sprite sheet + JSON manifest per item
- Extract base body sprite sheet
- Extract starter set: all 131 vanity items + ~20 per wardrobe category
- Create `assets/sprites/avatar/manifest.json`

### Phase 2: Rust data model
- Add `AvatarAppearance` to `avatar/types.rs`
- Add `avatar` field to `GameState`
- Add `get_avatar`, `set_avatar`, `get_avatar_manifest` IPC commands
- Add avatar to persistence (save/load)

### Phase 3: PixiJS AvatarCompositor
- Implement `AvatarCompositor` class
- Replace current placeholder avatar with layered composition
- Wire up `updateAnimation` to existing `updateAvatar` call path
- Implement skin/hair color tinting
- Implement facial feature positioning (scale, height, dist)

### Phase 4: Customization UI
- Build `AvatarEditor.svelte` with category tabs and item grid
- Color palette pickers for skin and hair
- Scaling sliders for facial features
- Live preview via AvatarCompositor
- Randomize button
- Wire save/cancel to IPC

### Phase 5: Polish
- First-run flow: show AvatarEditor before entering world
- Access from pause menu
- Smooth transitions when changing items
- Accessibility: keyboard navigation, screen reader labels

---

## 8. Open Questions

1. **SWF extraction fidelity** — The existing `extract-swf` handles bitmaps and
   simple vectors. Avatar SWFs use nested MovieClips with timeline animations.
   May need to extend the tool or use an external SWF renderer (e.g., `ruffle`)
   to rasterize frames at target resolution.

2. **Body part separation** — Do we need individual body-part sprite sheets for
   accurate skin tinting, or can we tint the whole body sprite and overlay
   clothing? The latter is simpler; the former is more faithful. Recommend
   starting with whole-body tinting and iterating if needed.

3. **Animation frame count** — Original had 13+ animation labels with multiple
   frames each. For Phase A we only need 4 states (idle, walk, jump, fall).
   How many frames per state? Original walk had 8+ frames; we should aim for
   at least 6-8 walk frames for smooth animation.

4. **Dress vs. shirt+pants exclusivity** — In the original, dress and
   shirt+pants were somewhat mutually exclusive visually (dress covers the
   torso and legs). Do we enforce this in the UI, or let players layer freely?

5. **Asset licensing** — glitch-avatars is CC0. Clothing data in
   `inc_data_clothing.js` references specific asset IDs. Verify all referenced
   SWFs exist in the CC0 repo before including.
