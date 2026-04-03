# SWF Vector Rendering — Convert Vector-Only Glitch Art to SVG

**Issue:** harmony-glitch-2bp
**Date:** 2026-04-02
**Status:** Approved

## Overview

Extend the existing `extract-swf` tool to handle the ~73% of Glitch item SWFs that are pure vector art (DefineShape tags with no embedded bitmaps). The tool auto-detects: SWFs with bitmaps produce PNGs (existing behavior), vector-only SWFs produce SVGs. The packer (`pack.mjs`) gains SVG input support, rasterizing them via `sharp` before packing into the existing PNG atlas. The game side is unchanged — everything becomes PNG textures in the atlas.

## Corpus Analysis

From the 2,651 SWFs in `glitch-items`:

| Category | Count | Percentage |
|----------|-------|------------|
| Bitmap SWFs (existing extractor handles) | 725 | 27% |
| Vector-only SWFs (this bead) | 1,926 | 73% |
| Single-frame SWFs | 1,847 | 70% |
| Multi-frame SWFs | 804 | 30% |

Fill styles across all vector shapes:

| Fill Type | Count | SVG Equivalent |
|-----------|-------|----------------|
| Solid Color | 73,460 | `fill="rgba(r,g,b,a)"` |
| LinearGradient | 28,495 | `<linearGradient>` |
| RadialGradient | 10,746 | `<radialGradient>` |
| Bitmap | 3,075 | Skipped (transparent fallback) |
| FocalGradient | 97 | `<radialGradient>` with focal point |

## SWF Shape → SVG Conversion

### The SWF Shape Model

SWF shapes use a dual-fill edge list, fundamentally different from SVG's closed-path model. Each edge in a SWF shape has a `fill_style_0` (left side) and `fill_style_1` (right side). The renderer builds closed sub-paths by grouping edges that share the same fill.

The `swf` crate's `ShapeRecord` enum provides:
- `StyleChange` — sets current `fill_style_0`, `fill_style_1`, `line_style`, and optionally moves the pen
- `StraightEdge { delta }` — line segment
- `CurvedEdge { control_delta, anchor_delta }` — quadratic bezier curve

### Conversion Algorithm

1. **Walk edges** — Iterate `ShapeRecord`s, tracking pen position and active fill0/fill1/line style. Each edge is stored as `(start, end, edge_type)` with its associated fill IDs.

2. **Group by fill** — Collect all edges that reference each fill ID. For `fill_style_1` edges, reverse the edge direction (fill1 is the right-side fill, so reversing makes it a left-side fill like fill0).

3. **Handle new_styles** — A `StyleChange` record may carry `new_styles`, which replaces the active fill/line style tables mid-shape. When this occurs, subsequent fill/line IDs reference the new tables. The walker must track the current style tables and resolve fill IDs accordingly.

4. **Connect into closed paths** — For each fill group, chain edges end-to-start into closed sub-paths using endpoint matching. Edges whose start point matches the previous edge's end point are connected. When no match exists, close the current sub-path and start a new one.

5. **Emit SVG paths** — Each closed sub-path becomes an SVG `<path>` element with `d` attribute using `M`, `L`, `Q`, and `Z` commands. Fill styles become SVG fill attributes.

6. **Emit line paths** — Edges grouped by line style become `<path>` elements with `stroke` and `stroke-width` attributes, rendered after fills (painter's order).

### Fill Style Mapping

**Solid Color:** Direct mapping to SVG `fill` attribute. SWF colors are RGBA; SVG uses `fill="rgb(r,g,b)"` with `fill-opacity` for alpha.

**Linear Gradient:** SWF defines gradients in a normalized space ([-16384, 16384] twips, i.e., [-819.2, 819.2] pixels) mapped to world space via a 2x3 affine matrix. The SVG equivalent:
- `<linearGradient>` with `gradientUnits="userSpaceOnUse"`
- `x1="-819.2" y1="0" x2="819.2" y2="0"` (horizontal gradient in gradient space)
- `gradientTransform="matrix(a,b,c,d,tx,ty)"` from the SWF matrix (converted from twips to pixels)
- `<stop>` elements from the gradient records, with `offset` derived from `ratio / 255`

**Radial Gradient:** Same matrix approach, using `<radialGradient>` with `cx="0" cy="0" r="819.2"`.

**Focal Gradient:** `<radialGradient>` with `fx` and `fy` offset by the focal point value.

**Bitmap Fill:** Skipped for this bead. Rendered as `fill="none"` (transparent). Logged as warning.

### Line Style Mapping

SWF `LineStyle` maps to SVG stroke attributes:
- `width` (in twips) → `stroke-width` (twips / 20 = pixels)
- `fill_style` (always Color for Glitch items) → `stroke` color
- Cap style → `stroke-linecap` (round/butt/square)
- Join style → `stroke-linejoin` (round/bevel/miter) with `stroke-miterlimit`

### Frame Selection

For multi-frame SWFs, only the first frame is extracted. All `DefineShape` and `PlaceObject` tags up to the first `ShowFrame` tag are processed. `PlaceObject` tags provide the transformation matrix for each shape instance, emitted as `<g transform="matrix(...)">` wrappers in the SVG.

### SVG Output Structure

```xml
<svg xmlns="http://www.w3.org/2000/svg"
     viewBox="0 0 42.6 42.5"
     width="42.6" height="42.5">
  <defs>
    <linearGradient id="g1" gradientUnits="userSpaceOnUse"
                    x1="-819.2" y1="0" x2="819.2" y2="0"
                    gradientTransform="matrix(...)">
      <stop offset="0" stop-color="#ff0000"/>
      <stop offset="1" stop-color="#880000"/>
    </linearGradient>
  </defs>
  <g transform="matrix(a,b,c,d,tx,ty)">
    <path d="M5,0 L37,0 Q42,0 42,5 L42,37 Q42,42 37,42 L5,42 Q0,42 0,37 L0,5 Q0,0 5,0 Z"
          fill="url(#g1)"/>
  </g>
  <g transform="matrix(...)">
    <path d="M10,20 L20,10 L30,20 Z" fill="#7d6630"/>
  </g>
</svg>
```

The `viewBox` is derived from `header.stage_size()` converted from twips to pixels (divide by 20).

## Extract-SWF Changes

### Auto-Detection

The `extract_largest_bitmap()` function is replaced by a higher-level `process_swf()` function:

```
process_swf(swf_data) → ExtractResult::Bitmap(ExtractedBitmap) | ExtractResult::Svg(String)
```

Logic:
1. Parse and scan all tags
2. If any bitmap tags exist (`DefineBitsLossless`, `DefineBitsJpeg2`, `DefineBitsJpeg3`): extract largest bitmap → PNG (existing path)
3. If no bitmap tags but DefineShape tags exist: convert first frame → SVG (new path)
4. If neither: skip

### Output

The output directory contains mixed file types:
```
extracted/items/food/
  apple.svg        ← vector-only SWF
  cherry.svg       ← vector-only SWF
  fried_egg.png    ← had embedded bitmap
```

### Summary Output

Updated to report both extraction types:
```
Extracted 725 bitmaps + 1926 SVGs / 2651 items (0 skipped)
```

### No New Dependencies

SVG output is string formatting — no additional Rust crates needed. The existing `swf`, `png`, `flate2`, `clap`, `jpeg-decoder` dependencies remain unchanged.

## Packer Changes

### SVG Input Support

`pack.mjs` extends its input scanning from `**/*.png` to `**/*.{png,svg}`.

### SVG Rasterization

Before packing, SVG files are rasterized to PNG buffers via `sharp`:

```javascript
const buffer = await sharp(svgPath)
  .resize({ width: Math.round(width * scale) })
  .png()
  .toBuffer();
```

A new `--scale` CLI flag (default: `1`) controls the rasterization resolution. At `--scale 2`, a 42x42px SVG becomes 84x84px in the atlas.

### Pipeline Flow

```
SVG file → sharp(svg).png() → in-memory buffer → shelf packer
PNG file → sharp(png)       → in-memory buffer → shelf packer (unchanged)
```

### No Output Format Changes

The atlas JSON remains TexturePacker JSON Hash. The atlas image remains PNG. The game side is completely unchanged.

## npm Scripts

The existing `pipeline-items` script works unchanged — `extract-items` now produces both PNGs and SVGs, and `pack-items` handles both.

```json
{
  "extract-items": "cargo run --manifest-path src-tauri/Cargo.toml --bin extract-swf --features extract-swf -- --source ${GLITCH_ART_PATH:-$HOME/work/tinyspeck/glitch-items} --output tools/asset-pipeline/extracted/items",
  "pack-items": "node tools/asset-pipeline/pack.mjs --input tools/asset-pipeline/extracted/items --output assets/sprites/items --name items",
  "pipeline-items": "npm run extract-items && npm run pack-items"
}
```

## Testing

### Rust Tests (cargo test)

- Shape-to-SVG conversion for a simple shape with solid color fill
- Gradient fill conversion (linear + radial) produces valid SVG gradient defs
- Multi-shape SWF with PlaceObject transforms emits correct `<g transform="...">`
- Auto-detection: SWF with bitmaps → PNG, vector-only → SVG
- Edge connection algorithm handles multiple disjoint sub-paths within one fill
- SVG viewBox matches SWF stage dimensions (twips / 20)

### Packer Tests (vitest)

- SVG inputs are rasterized and packed alongside PNGs
- `--scale` flag controls output resolution of SVG-sourced frames
- Mixed PNG + SVG input produces correct atlas JSON with all frames
- SVG-only input works (no PNGs)

### Manual Verification

- Run full pipeline on `glitch-items/food/` — verify SVG output renders recognizably
- Compare atlas with mixed PNG + SVG sources against the bitmap-only atlas
- Load in game — vector-derived items display correctly alongside bitmap-derived items

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Shape has bitmap fill | Skip that fill (transparent), log warning |
| Shape has no edges (empty) | Skip silently |
| PlaceObject references undefined shape | Skip, log warning |
| SVG rasterization fails in sharp | Skip that image, log warning, continue packing |
| SWF has zero frames (no ShowFrame) | Treat all shapes as frame 1 |
| SWF parse failure | Log warning, skip (existing behavior) |

## Out of Scope

- Multi-frame animation extraction (follow-up bead)
- Bitmap fills in vector shapes
- SWF filters/blend modes (drop shadow, blur, etc.)
- Runtime SVG rendering in the game engine
- Location/avatar/overlay SWF extraction
- Clipping masks (DefineClip)
- Texture compression (WebP, AVIF)
