# Asset Pipeline Tooling — SWF Extraction & Sprite Sheet Packing

**Issue:** glitch-4nn
**Date:** 2026-04-02
**Status:** Approved

## Overview

Build-time tooling to convert Glitch source art (SWF files) into game-ready sprite sheets. Two composable tools: a Rust SWF bitmap extractor and a Node.js sprite sheet packer. The extractor handles the simple single-frame case (item icons); animated SWFs are deferred. Output is TexturePacker JSON Hash format, consumed natively by the existing PixiJS `SpriteManager`.

## Source Material

The original Glitch art lives in `~/work/tinyspeck/glitch-*` (CC0-licensed repos from Tiny Speck):

| Repo | Contents | Files | Format |
|------|----------|-------|--------|
| `glitch-items` | Item icons (food, tools, seeds, etc.) | ~5,400 | SWF/FLA |
| `glitch-locations` | Street backgrounds, deco art | ~11,400 | SWF/FLA |
| `glitch-avatars` | Avatar body parts, wardrobe | ~1,400 | SWF/FLA |
| `glitch-overlays` | UI overlays, effects | ~260 | FLA |
| `glitch-objects` | World object definitions | ~94 | XML |
| `glitch-hq-android` | Android app (294 PNG drawables) | ~500 | PNG/Java |

Item SWFs are CWS-compressed (zlib), SWF version 9, typically 4-20KB. They contain embedded bitmap tags (`DefineBitsLossless2` for RGBA, `DefineBitsJPEG` for JPEG) with the item artwork. This bead targets item SWF extraction only.

## Tool 1: SWF Bitmap Extractor (Rust)

### Architecture

A standalone Rust binary target in `src-tauri/` (not part of the Tauri app). Uses the `swf` crate from the Ruffle project to parse SWF files and extract embedded bitmaps.

### Binary Target

New `[[bin]]` entry in `src-tauri/Cargo.toml`:

```toml
[[bin]]
name = "extract-swf"
path = "src/bin/extract_swf.rs"
```

### Dependencies

Added to `src-tauri/Cargo.toml`:

- `swf` — SWF parsing (from Ruffle; handles CWS/ZWS decompression)
- `png` — PNG encoding
- `clap` — CLI argument parsing

### Behavior

- Takes `--source <dir>` and `--output <dir>` arguments
- Recursively walks the source directory for `.swf` files
- For each SWF: parses it, finds bitmap tags (`DefineBitsLossless`/`DefineBitsLossless2`/`DefineBitsJPEG2`/`DefineBitsJPEG3`), decodes the largest bitmap (by pixel area), writes it as PNG
- Preserves directory structure: `glitch-items/food/apple/apple.swf` → `output/food/apple.png`
- Skips SWFs that fail to parse or contain no bitmaps (logs warning to stderr)
- Reports summary on completion: `Extracted 193/200 items (7 skipped)`

### Why Largest Bitmap

Item SWFs may contain multiple bitmaps — the main artwork plus small UI elements (icons, chrome). The largest bitmap by pixel area is the main artwork. This heuristic is simple and correct for the item SWF case.

### Invocation

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin extract-swf -- \
  --source ~/work/tinyspeck/glitch-items \
  --output tools/asset-pipeline/extracted/items
```

## Tool 2: Sprite Sheet Packer (Node.js)

### Architecture

A Node.js CLI script at `tools/asset-pipeline/pack.mjs`. Uses `sharp` for image reading, compositing, and writing.

### Modes

**Atlas mode** (default): Packs multiple individual PNGs into a single sprite sheet atlas. For static assets like item icons.

```
Input: cherry.png, apple.png, grain.png
Output: items.png + items.json (TexturePacker JSON Hash atlas)
```

**Animation mode** (`--animation`): Takes numbered frames and packs them into a sprite sheet with an `animations` block. For animated entities. Frame filenames follow `<animation>_<index>.png` convention — the prefix before the last `_` becomes the animation name, the suffix is the frame index. Frames are sorted numerically within each animation group.

```
Input: idle_0.png, idle_1.png, walk_0.png, walk_1.png, walk_2.png, walk_3.png
Output: entity.png + entity.json (with animations.idle and animations.walking arrays)
```

### Packing Algorithm

Shelf/row packing: sort images by height descending, pack left-to-right into rows. Start a new row when the current row is full. Output dimensions are the smallest power-of-two that fits all images. Simple, deterministic, good enough for the asset sizes involved.

### Output Format

TexturePacker JSON Hash — native PixiJS v8 `Spritesheet` support:

```json
{
  "frames": {
    "apple": { "frame": {"x": 0, "y": 0, "w": 64, "h": 64} },
    "cherry": { "frame": {"x": 64, "y": 0, "w": 16, "h": 16} }
  },
  "meta": {
    "image": "items.png",
    "format": "RGBA8888",
    "size": { "w": 256, "h": 256 },
    "scale": 1
  }
}
```

In animation mode, adds an `animations` block:

```json
{
  "frames": { ... },
  "animations": {
    "walking": ["walk_0", "walk_1", "walk_2", "walk_3"]
  },
  "meta": { ... }
}
```

### Dependencies

- `sharp` — image reading, compositing, PNG writing

### Invocation

```bash
node tools/asset-pipeline/pack.mjs \
  --input tools/asset-pipeline/extracted/items/food \
  --output assets/sprites/items \
  --name food-items
```

Animation mode:
```bash
node tools/asset-pipeline/pack.mjs \
  --input frames/walk/ \
  --output assets/sprites/avatar \
  --name avatar \
  --animation
```

## Pipeline Orchestration

### npm Scripts

```json
{
  "extract-items": "cargo run --manifest-path src-tauri/Cargo.toml --bin extract-swf -- --source ${GLITCH_ART_PATH:-$HOME/work/tinyspeck/glitch-items} --output tools/asset-pipeline/extracted/items",
  "pack-items": "node tools/asset-pipeline/pack.mjs --input tools/asset-pipeline/extracted/items --output assets/sprites/items --name items",
  "pipeline-items": "npm run extract-items && npm run pack-items"
}
```

### Source Path Configuration

The extractor needs to find the Glitch art repos, which vary per machine. Uses `GLITCH_ART_PATH` environment variable with default `$HOME/work/tinyspeck/glitch-items`.

### Directory Layout

```
tools/asset-pipeline/
  pack.mjs              # Sprite sheet packer
  extracted/            # .gitignored — intermediate PNGs
```

`extracted/` is ephemeral build output. Final sprite sheets in `assets/sprites/` are committed.

## SpriteManager Integration

### Atlas Loading

`SpriteManager` gains the ability to load atlas sprite sheets in addition to individual textures. PixiJS handles this natively: `Assets.load('sprites/items/items.json')` parses the spritesheet and registers each frame as a named `Texture`.

### Changes

- On `init()`, attempt to load known atlas files (e.g., `sprites/items/items.json`). If the atlas exists, all frames become available.
- `createEntity()` and `createGroundItem()` check the texture cache (which now includes atlas frames) before attempting individual PNG loads.
- Individual PNGs continue to work as fallback. Atlas-packed sprites and individual files coexist — you can pack some items while others remain as individual PNGs.

### No Rust Changes

The Rust side sends `spriteClass` and `icon` strings. The frontend resolves them to textures. The Rust side doesn't know or care about sprite sheets vs. individual images.

## Error Handling

| Scenario | Behavior |
|----------|----------|
| SWF parse failure | Log warning with filename, skip, continue |
| SWF has no bitmaps | Log warning, skip |
| Source directory doesn't exist | Exit with clear error message |
| Output directory doesn't exist | Create it |
| `sharp` fails on a PNG | Log warning, skip that image, continue packing others |
| Atlas JSON fails to load in SpriteManager | Fall back to individual PNGs (existing behavior) |
| `GLITCH_ART_PATH` not set, default path missing | Exit with message explaining how to set it |

## Testing

### Rust Extractor (cargo test)

- Parses a known small SWF fixture and extracts a bitmap
- Output PNG dimensions match embedded bitmap dimensions
- Handles corrupt/truncated SWF gracefully (logs warning, skips)
- Handles SWF with no bitmap tags (logs warning, skips)

### Node.js Packer (vitest)

- Packs 2-3 small PNGs into an atlas, verifies output JSON matches TexturePacker schema
- Frame coordinates don't overlap
- Output image dimensions are power-of-two
- Animation mode groups numbered frames correctly (`walk_0`, `walk_1` → `animations.walking`)
- Handles single-image input (degenerate case)

### SpriteManager (vitest)

- Atlas loading makes frames available as textures
- Individual PNGs still work when no atlas exists
- Atlas frames and individual PNGs coexist

### Manual Verification

- Run full pipeline on `glitch-items/food/` — verify extracted PNGs look correct
- Load generated atlas in game — items display with real Glitch art
- Items without atlas entries still show placeholder/individual PNGs

## Out of Scope

- Animated SWF extraction (multi-frame timelines)
- Avatar SWF extraction (composited body parts)
- Location/deco SWF extraction (complex scene graphs)
- Automated re-pack on source changes (file watcher)
- Texture compression (WebP, AVIF, basis)
- Max atlas size limits or multi-atlas splitting
- CAS integration for decentralized asset distribution
