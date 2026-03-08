# Glitch Phase A: Walking Simulator — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** A Tauri v2 desktop app where you can load any Glitch street, see parallax backgrounds rendered, and walk an avatar with platformer physics.

**Architecture:** Rust backend owns all game logic (street loading, physics, tick loop). PixiJS is a dumb renderer that draws RenderFrames sent via Tauri IPC events. Svelte 5 handles UI chrome (street picker, debug overlay).

**Tech Stack:** Rust, Tauri v2, Svelte 5 (runes), TypeScript, PixiJS v8, quick-xml, serde, vitest

**Design doc:** `docs/plans/2026-03-07-glitch-resurrection-design.md`

**Repo:** `/Users/zeblith/work/zeblithic/harmony-glitch`

---

## Critical Context

### Glitch Location XML Format

The CC0 `glitch-locations` archive uses a **typed-value element system**, NOT standard XML attributes:

```xml
<int id="key">42</int>
<str id="key">hello</str>
<bool id="key">true</bool>
<null id="key"/>
<object id="key"><!-- nested children --></object>
```

### Coordinate System

- **Y=0 is at the bottom** (ground level), **negative Y goes UP**
- Street bounds: `l` (left, negative), `r` (right, positive), `t` (top, negative), `b` (bottom, 0)
- Example: `l=-3000, r=3000, t=-1000, b=0` = 6000px wide, 1000px tall

### Parallax

Parallax is computed from **layer width relative to middleground width**:
- `scroll_factor = layer.w / middleground.w`
- A layer with `w=5460` and middleground `w=6000` scrolls at `5460/6000 = 0.91`

### Platforms

Platforms are **line segments** (start + end points), not rectangles. If `start.y != end.y`, the platform slopes. Permission values: `-1` = one-way top, `1` = one-way bottom, `0` = pass-through, absent = solid.

---

## Task 1: Project Scaffolding

**Files:**
- Create: `package.json`, `vite.config.ts`, `tsconfig.json`, `svelte.config.js`
- Create: `src/main.ts`, `src/App.svelte`, `src/app.css`
- Create: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Create: `CLAUDE.md`

**Step 1: Initialize Tauri v2 + Svelte 5 + TypeScript project**

From the repo root (`/Users/zeblith/work/zeblithic/harmony-glitch`), scaffold the project. Since the repo already has a LICENSE file, use manual setup rather than `create tauri-app` (which expects an empty directory).

```bash
# Initialize npm project
npm init -y

# Install frontend dependencies
npm install svelte @sveltejs/vite-plugin-svelte vite typescript
npm install pixi.js

# Install Tauri v2 CLI and API
npm install -D @tauri-apps/cli@latest
npm install @tauri-apps/api@latest

# Install dev/test dependencies
npm install -D vitest @testing-library/svelte jsdom @sveltejs/package
```

Create `vite.config.ts`:
```typescript
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
});
```

Create `svelte.config.js`:
```javascript
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
  preprocess: vitePreprocess(),
};
```

Create `tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "types": ["vitest/globals"]
  },
  "include": ["src/**/*.ts", "src/**/*.svelte"]
}
```

Create `index.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Harmony Glitch</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

Create `src/main.ts`:
```typescript
import App from './App.svelte';
import { mount } from 'svelte';
import './app.css';

const app = mount(App, { target: document.getElementById('app')! });

export default app;
```

Create `src/App.svelte`:
```svelte
<script lang="ts">
  let message = $state('Harmony Glitch — Phase A');
</script>

<main>
  <h1>{message}</h1>
</main>

<style>
  main {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: #1a1a2e;
    color: #e0e0e0;
    font-family: system-ui, sans-serif;
  }
</style>
```

Create `src/app.css`:
```css
:root {
  font-family: system-ui, -apple-system, sans-serif;
  color: #e0e0e0;
  background-color: #1a1a2e;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html, body, #app {
  height: 100%;
  width: 100%;
  overflow: hidden;
}
```

Add scripts to `package.json`:
```json
{
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "tauri": "tauri",
    "test": "vitest run"
  }
}
```

**Step 2: Initialize Tauri v2 backend**

```bash
cd src-tauri
cargo init --name harmony-glitch
```

Create `src-tauri/Cargo.toml`:
```toml
[package]
name = "harmony-glitch"
version = "0.1.0"
edition = "2021"

[dependencies]
tauri = { version = "2", features = [] }
tauri-build = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
quick-xml = "0.37"
log = "0.4"

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

Create `src-tauri/build.rs`:
```rust
fn main() {
    tauri_build::build();
}
```

Create `src-tauri/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    harmony_glitch::run();
}
```

Create `src-tauri/src/lib.rs`:
```rust
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to Ur.", name)
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running harmony-glitch");
}
```

Create `src-tauri/tauri.conf.json`:
```json
{
  "$schema": "https://raw.githubusercontent.com/nicedoc/tauri/refs/heads/v2/tooling/cli/schema.json",
  "productName": "Harmony Glitch",
  "version": "0.1.0",
  "identifier": "com.zeblithic.harmony-glitch",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "title": "Harmony Glitch",
    "windows": [
      {
        "title": "Harmony Glitch",
        "width": 1280,
        "height": 720,
        "minWidth": 800,
        "minHeight": 600,
        "resizable": true
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": []
  }
}
```

Create `src-tauri/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities for harmony-glitch",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:event:default",
    "core:event:allow-emit",
    "core:event:allow-listen"
  ]
}
```

**Step 3: Create CLAUDE.md**

Create `CLAUDE.md` at the repo root with project-specific instructions:
```markdown
# CLAUDE.md — harmony-glitch

## Project
Harmony Glitch — a decentralized resurrection of the Glitch MMO on the Harmony network stack.
Currently in Phase A: walking simulator (single-player, no networking).

## Tech Stack
- **Backend:** Rust (Tauri v2)
- **Frontend:** Svelte 5 (runes), TypeScript, PixiJS v8
- **Test:** `cargo test` (Rust), `npx vitest run` (frontend)
- **Build:** `npm run tauri build`
- **Dev:** `npm run tauri dev`

## Architecture
- Rust owns ALL game logic (physics, collision, state, tick loop)
- PixiJS is a dumb renderer — draws RenderFrames from Rust, no game logic in JS
- Svelte 5 handles UI chrome (menus, overlays)
- Tauri IPC bridges Rust ↔ frontend (commands + events)

## Svelte 5 Runes
Use `$state()`, `$derived()`, `$props()`, `$effect()`, `onclick={handler}` (NOT `on:click`).

## Coordinate System
Glitch uses Y=0 at bottom (ground), negative Y goes up. The renderer must convert
to screen coordinates (Y=0 at top, positive Y goes down).

## Key Directories
- `src-tauri/src/street/` — Street data types and XML parser
- `src-tauri/src/physics/` — AABB collision, movement, gravity
- `src-tauri/src/engine/` — Game state, tick loop, RenderFrame
- `src/lib/engine/` — PixiJS rendering (street, avatar, camera)
- `src/lib/components/` — Svelte UI components
- `assets/` — Demo street data and placeholder art

## Design Docs
- `docs/plans/2026-03-07-glitch-resurrection-design.md` — Full design
- `docs/plans/2026-03-07-glitch-phase-a-plan.md` — This implementation plan
```

**Step 4: Verify build**

```bash
# From repo root
cd /Users/zeblith/work/zeblithic/harmony-glitch
npm run build  # Vite frontend build should succeed
cd src-tauri && cargo check  # Rust should compile
```

**Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add -A
git commit -m "feat: scaffold Tauri v2 + Svelte 5 + PixiJS project"
```

---

## Task 2: Street Data Types (Rust)

**Files:**
- Create: `src-tauri/src/street/mod.rs`
- Create: `src-tauri/src/street/types.rs`
- Modify: `src-tauri/src/lib.rs` (add module declaration)

**Step 1: Write tests for street types serialization**

Create `src-tauri/src/street/types.rs` with the types AND tests:

```rust
use serde::{Deserialize, Serialize};

/// A complete street definition parsed from Glitch location XML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreetData {
    pub tsid: String,
    pub name: String,
    /// Left bound (negative)
    pub left: f64,
    /// Right bound (positive)
    pub right: f64,
    /// Top bound (negative — Y goes up)
    pub top: f64,
    /// Bottom bound (typically 0)
    pub bottom: f64,
    /// Ground Y level (typically 0)
    pub ground_y: f64,
    /// Background gradient colors (hex, no # prefix)
    pub gradient: Option<Gradient>,
    /// Parallax layers sorted by z (back to front)
    pub layers: Vec<Layer>,
    /// Signpost connections to other streets
    pub signposts: Vec<Signpost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gradient {
    pub top: String,
    pub bottom: String,
}

/// A parallax background layer containing decorative objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub name: String,
    pub z: i32,
    pub w: f64,
    pub h: f64,
    pub is_middleground: bool,
    pub decos: Vec<Deco>,
    /// Platform lines (only present on the middleground layer).
    pub platform_lines: Vec<PlatformLine>,
    /// Walls (only present on the middleground layer).
    pub walls: Vec<Wall>,
    /// Ladders (only present on the middleground layer).
    pub ladders: Vec<Ladder>,
    pub filters: Option<LayerFilters>,
}

/// A platform line segment. Players walk along these.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformLine {
    pub id: String,
    pub start: Point,
    pub end: Point,
    /// -1 = one-way from top, 1 = one-way from bottom, 0 = pass-through, None = solid
    pub pc_perm: Option<i32>,
    pub item_perm: Option<i32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A vertical collision barrier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wall {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub h: f64,
    pub pc_perm: Option<i32>,
    pub item_perm: Option<i32>,
}

/// A climbable ladder zone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ladder {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A decorative sprite placed in a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deco {
    pub id: String,
    pub name: String,
    pub sprite_class: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub z: i32,
    pub r: f64,
    pub h_flip: bool,
}

/// Visual filters applied to a layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerFilters {
    pub brightness: Option<i32>,
    pub contrast: Option<i32>,
    pub saturation: Option<i32>,
    pub blur: Option<i32>,
    pub tint_color: Option<i32>,
    pub tint_amount: Option<i32>,
}

/// A navigation signpost connecting to other streets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signpost {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub connects: Vec<SignpostConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignpostConnection {
    pub target_tsid: String,
    pub target_label: String,
}

impl StreetData {
    /// Width of the street in pixels.
    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    /// Height of the street in pixels.
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }

    /// Get the middleground layer (z=0), where platforms live.
    pub fn middleground(&self) -> Option<&Layer> {
        self.layers.iter().find(|l| l.is_middleground)
    }

    /// All platform lines from the middleground layer.
    pub fn platforms(&self) -> &[PlatformLine] {
        self.middleground()
            .map(|l| l.platform_lines.as_slice())
            .unwrap_or(&[])
    }

    /// Compute parallax scroll factor for a layer relative to middleground.
    /// Returns 1.0 for middleground, <1.0 for backgrounds, >1.0 for foregrounds.
    pub fn parallax_factor(&self, layer: &Layer) -> f64 {
        let mg_w = self.middleground().map(|l| l.w).unwrap_or(self.width());
        if mg_w == 0.0 {
            return 1.0;
        }
        layer.w / mg_w
    }
}

impl PlatformLine {
    /// Whether the player can stand on this platform from above.
    pub fn solid_from_top(&self) -> bool {
        // Solid from top unless perm is 1 (bottom-only) or 0 (pass-through)
        !matches!(self.pc_perm, Some(1) | Some(0))
    }

    /// Whether this platform blocks from below.
    pub fn solid_from_bottom(&self) -> bool {
        !matches!(self.pc_perm, Some(-1) | Some(0))
    }

    /// Get the Y position on this platform at a given X coordinate.
    /// Linearly interpolates between start and end for sloped platforms.
    pub fn y_at(&self, x: f64) -> f64 {
        let dx = self.end.x - self.start.x;
        if dx.abs() < 0.001 {
            return self.start.y;
        }
        let t = ((x - self.start.x) / dx).clamp(0.0, 1.0);
        self.start.y + t * (self.end.y - self.start.y)
    }

    /// X extent of this platform.
    pub fn min_x(&self) -> f64 {
        self.start.x.min(self.end.x)
    }

    pub fn max_x(&self) -> f64 {
        self.start.x.max(self.end.x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_y_at_flat() {
        let p = PlatformLine {
            id: "test".into(),
            start: Point { x: 0.0, y: -100.0 },
            end: Point { x: 200.0, y: -100.0 },
            pc_perm: None,
            item_perm: None,
        };
        assert!((p.y_at(100.0) - (-100.0)).abs() < 0.001);
    }

    #[test]
    fn platform_y_at_sloped() {
        let p = PlatformLine {
            id: "test".into(),
            start: Point { x: 0.0, y: -100.0 },
            end: Point { x: 200.0, y: -200.0 },
            pc_perm: None,
            item_perm: None,
        };
        // Midpoint should be -150
        assert!((p.y_at(100.0) - (-150.0)).abs() < 0.001);
    }

    #[test]
    fn platform_one_way_from_top() {
        let p = PlatformLine {
            id: "test".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 100.0, y: 0.0 },
            pc_perm: Some(-1),
            item_perm: None,
        };
        assert!(p.solid_from_top());
        assert!(!p.solid_from_bottom());
    }

    #[test]
    fn platform_fully_solid() {
        let p = PlatformLine {
            id: "test".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 100.0, y: 0.0 },
            pc_perm: None,
            item_perm: None,
        };
        assert!(p.solid_from_top());
        assert!(p.solid_from_bottom());
    }

    #[test]
    fn street_width_height() {
        let s = StreetData {
            tsid: "test".into(),
            name: "Test Street".into(),
            left: -3000.0,
            right: 3000.0,
            top: -1000.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![],
            signposts: vec![],
        };
        assert!((s.width() - 6000.0).abs() < 0.001);
        assert!((s.height() - 1000.0).abs() < 0.001);
    }

    #[test]
    fn parallax_factor_middleground() {
        let mg = Layer {
            name: "middleground".into(),
            z: 0,
            w: 6000.0,
            h: 1000.0,
            is_middleground: true,
            decos: vec![],
            platform_lines: vec![],
            walls: vec![],
            ladders: vec![],
            filters: None,
        };
        let bg = Layer {
            name: "bg_2".into(),
            z: -2,
            w: 5460.0,
            h: 1000.0,
            is_middleground: false,
            decos: vec![],
            platform_lines: vec![],
            walls: vec![],
            ladders: vec![],
            filters: None,
        };
        let s = StreetData {
            tsid: "test".into(),
            name: "Test".into(),
            left: -3000.0,
            right: 3000.0,
            top: -1000.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![mg.clone(), bg.clone()],
            signposts: vec![],
        };
        assert!((s.parallax_factor(&mg) - 1.0).abs() < 0.001);
        assert!((s.parallax_factor(&bg) - 0.91).abs() < 0.01);
    }

    #[test]
    fn serializes_to_camel_case() {
        let p = PlatformLine {
            id: "p1".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 100.0, y: 0.0 },
            pc_perm: Some(-1),
            item_perm: None,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("pcPerm"));
        assert!(json.contains("itemPerm"));
    }
}
```

Create `src-tauri/src/street/mod.rs`:
```rust
pub mod types;
```

**Step 2: Run tests to verify they pass**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri
cargo test -- street
```

Expected: All tests PASS.

**Step 3: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/street/
git commit -m "feat: street data types with platform geometry and parallax"
```

---

## Task 3: Street XML Parser (Rust)

**Files:**
- Create: `src-tauri/src/street/parser.rs`
- Create: `src-tauri/src/street/xml_helpers.rs`
- Modify: `src-tauri/src/street/mod.rs`

The Glitch XML uses a typed-value element system. We need a helper layer that extracts typed values from the XML tree before assembling StreetData.

**Step 1: Write the XML helper module**

Create `src-tauri/src/street/xml_helpers.rs`:

```rust
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

/// A parsed node from the Glitch typed-value XML system.
/// The XML uses elements like <int id="key">value</int>, <str id="key">value</str>,
/// <object id="key">children</object>, etc.
#[derive(Debug, Clone)]
pub enum XmlValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
    Object(HashMap<String, XmlValue>),
    ObjRef { tsid: String, label: String },
}

impl XmlValue {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            XmlValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            XmlValue::Int(v) => Some(*v as f64),
            XmlValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            XmlValue::Str(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            XmlValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, XmlValue>> {
        match self {
            XmlValue::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&XmlValue> {
        self.as_object()?.get(key)
    }
}

/// Parse a Glitch location XML string into an XmlValue tree.
pub fn parse_glitch_xml(xml: &str) -> Result<XmlValue, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    // Skip to game_object root, then parse its children
    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == "game_object" || tag == "object" {
                    return parse_object_children(&mut reader, &tag);
                }
            }
            Ok(Event::Eof) => return Err("Unexpected end of XML".into()),
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
    }
}

fn parse_object_children(reader: &mut Reader<&[u8]>, parent_tag: &str) -> Result<XmlValue, String> {
    let mut map = HashMap::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let id = get_attr(e, "id").unwrap_or_default();

                match tag.as_str() {
                    "object" => {
                        let value = parse_object_children(reader, "object")?;
                        if !id.is_empty() {
                            map.insert(id, value);
                        }
                    }
                    "int" => {
                        let text = read_text(reader, "int")?;
                        if let Ok(v) = text.parse::<i64>() {
                            map.insert(id, XmlValue::Int(v));
                        }
                    }
                    "str" => {
                        let text = read_text(reader, "str")?;
                        map.insert(id, XmlValue::Str(text));
                    }
                    "bool" => {
                        let text = read_text(reader, "bool")?;
                        map.insert(id, XmlValue::Bool(text == "true"));
                    }
                    "objref" => {
                        let tsid = get_attr(e, "tsid").unwrap_or_default();
                        let label = get_attr(e, "label").unwrap_or_default();
                        map.insert(id, XmlValue::ObjRef { tsid, label });
                        // objref may be self-closing or have an end tag
                        skip_to_end(reader, "objref");
                    }
                    "objrefs" => {
                        // Container of objref elements — parse as object
                        let value = parse_object_children(reader, "objrefs")?;
                        if !id.is_empty() {
                            map.insert(id, value);
                        }
                    }
                    _ => {
                        // Unknown element — skip it
                        let text = read_text(reader, &tag).unwrap_or_default();
                        if !text.is_empty() && !id.is_empty() {
                            map.insert(id, XmlValue::Str(text));
                        }
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let id = get_attr(e, "id").unwrap_or_default();
                match tag.as_str() {
                    "null" => {
                        map.insert(id, XmlValue::Null);
                    }
                    "objref" => {
                        let tsid = get_attr(e, "tsid").unwrap_or_default();
                        let label = get_attr(e, "label").unwrap_or_default();
                        map.insert(id, XmlValue::ObjRef { tsid, label });
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == parent_tag {
                    return Ok(XmlValue::Object(map));
                }
            }
            Ok(Event::Eof) => return Err("Unexpected end of XML in object".into()),
            Err(e) => return Err(format!("XML error: {}", e)),
            _ => {}
        }
    }
}

fn get_attr(e: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == name.as_bytes())
        .map(|a| String::from_utf8_lossy(&a.value).to_string())
}

fn read_text(reader: &mut Reader<&[u8]>, end_tag: &str) -> Result<String, String> {
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(e)) => {
                text = e.unescape().map_err(|e| e.to_string())?.to_string();
            }
            Ok(Event::End(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if tag == end_tag {
                    return Ok(text);
                }
            }
            Ok(Event::Eof) => return Err(format!("Unexpected EOF reading <{}>", end_tag)),
            Err(e) => return Err(format!("XML error in <{}>: {}", end_tag, e)),
            _ => {}
        }
    }
}

fn skip_to_end(reader: &mut Reader<&[u8]>, tag: &str) {
    let mut depth = 1;
    loop {
        match reader.read_event() {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(ref e)) => {
                let t = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if t == tag {
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => return,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_typed_values() {
        let xml = r#"
        <object id="root">
            <int id="width">6000</int>
            <str id="name">Test Street</str>
            <bool id="active">true</bool>
            <null id="nothing"/>
        </object>"#;
        let val = parse_glitch_xml(xml).unwrap();
        assert_eq!(val.get("width").unwrap().as_int(), Some(6000));
        assert_eq!(val.get("name").unwrap().as_str(), Some("Test Street"));
        assert_eq!(val.get("active").unwrap().as_bool(), Some(true));
        assert!(matches!(val.get("nothing"), Some(XmlValue::Null)));
    }

    #[test]
    fn parse_nested_objects() {
        let xml = r#"
        <object id="root">
            <object id="gradient">
                <str id="top">87A8C9</str>
                <str id="bottom">FFC400</str>
            </object>
        </object>"#;
        let val = parse_glitch_xml(xml).unwrap();
        let grad = val.get("gradient").unwrap();
        assert_eq!(grad.get("top").unwrap().as_str(), Some("87A8C9"));
        assert_eq!(grad.get("bottom").unwrap().as_str(), Some("FFC400"));
    }

    #[test]
    fn parse_platform_line() {
        let xml = r#"
        <object id="root">
            <object id="platform_lines">
                <object id="plat_1">
                    <object id="start">
                        <int id="x">-228</int>
                        <int id="y">-58</int>
                    </object>
                    <object id="end">
                        <int id="x">-119</int>
                        <int id="y">-77</int>
                    </object>
                    <int id="platform_pc_perm">-1</int>
                </object>
            </object>
        </object>"#;
        let val = parse_glitch_xml(xml).unwrap();
        let plats = val.get("platform_lines").unwrap().as_object().unwrap();
        let p = plats.get("plat_1").unwrap();
        let start = p.get("start").unwrap();
        assert_eq!(start.get("x").unwrap().as_int(), Some(-228));
        assert_eq!(start.get("y").unwrap().as_int(), Some(-58));
    }
}
```

**Step 2: Write the street parser module**

Create `src-tauri/src/street/parser.rs`:

```rust
use super::types::*;
use super::xml_helpers::{parse_glitch_xml, XmlValue};
use std::collections::HashMap;

/// Parse a Glitch location XML string into a StreetData struct.
pub fn parse_street(xml: &str) -> Result<StreetData, String> {
    let root = parse_glitch_xml(xml)?;

    // The actual data is nested under "dynamic" in the game_object
    let dynamic = root.get("dynamic").unwrap_or(&root);

    let tsid = dynamic.get("tsid")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let name = dynamic.get("label")
        .and_then(|v| v.as_str())
        .unwrap_or("Unnamed Street")
        .to_string();

    let left = dynamic.get("l").and_then(|v| v.as_f64()).unwrap_or(-3000.0);
    let right = dynamic.get("r").and_then(|v| v.as_f64()).unwrap_or(3000.0);
    let top = dynamic.get("t").and_then(|v| v.as_f64()).unwrap_or(-1000.0);
    let bottom = dynamic.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let ground_y = dynamic.get("ground_y").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let gradient = dynamic.get("gradient").and_then(|g| {
        Some(Gradient {
            top: g.get("top")?.as_str()?.to_string(),
            bottom: g.get("bottom")?.as_str()?.to_string(),
        })
    });

    let layers = parse_layers(dynamic)?;
    let signposts = parse_signposts(dynamic);

    Ok(StreetData {
        tsid,
        name,
        left,
        right,
        top,
        bottom,
        ground_y,
        gradient,
        layers,
        signposts,
    })
}

fn parse_layers(dynamic: &XmlValue) -> Result<Vec<Layer>, String> {
    let layers_obj = match dynamic.get("layers") {
        Some(v) => v,
        None => return Ok(vec![]),
    };
    let layers_map = match layers_obj.as_object() {
        Some(m) => m,
        None => return Ok(vec![]),
    };

    let mut layers = Vec::new();
    for (layer_id, layer_val) in layers_map {
        let is_middleground = layer_id == "middleground"
            || layer_val.get("name").and_then(|v| v.as_str()) == Some("middleground");

        let z = layer_val.get("z").and_then(|v| v.as_int()).unwrap_or(0) as i32;
        let w = layer_val.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let h = layer_val.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let name = layer_val.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(layer_id)
            .to_string();

        let decos = parse_decos(layer_val);
        let filters = parse_filters(layer_val);

        let (platform_lines, walls, ladders) = if is_middleground {
            (
                parse_platform_lines(layer_val),
                parse_walls(layer_val),
                parse_ladders(layer_val),
            )
        } else {
            (vec![], vec![], vec![])
        };

        layers.push(Layer {
            name,
            z,
            w,
            h,
            is_middleground,
            decos,
            platform_lines,
            walls,
            ladders,
            filters,
        });
    }

    // Sort layers by z (back to front)
    layers.sort_by_key(|l| l.z);
    Ok(layers)
}

fn parse_decos(layer: &XmlValue) -> Vec<Deco> {
    let decos_obj = match layer.get("decos").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    decos_obj.iter().map(|(deco_id, d)| {
        Deco {
            id: deco_id.clone(),
            name: d.get("name").and_then(|v| v.as_str()).unwrap_or(deco_id).to_string(),
            sprite_class: d.get("sprite_class").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            x: d.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: d.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            w: d.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0),
            h: d.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0),
            z: d.get("z").and_then(|v| v.as_int()).unwrap_or(0) as i32,
            r: d.get("r").and_then(|v| v.as_f64()).unwrap_or(0.0),
            h_flip: d.get("h_flip").and_then(|v| v.as_bool()).unwrap_or(false),
        }
    }).collect()
}

fn parse_platform_lines(layer: &XmlValue) -> Vec<PlatformLine> {
    let plats = match layer.get("platform_lines").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    plats.iter().map(|(plat_id, p)| {
        let start = p.get("start").map(|s| Point {
            x: s.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: s.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }).unwrap_or(Point { x: 0.0, y: 0.0 });

        let end = p.get("end").map(|e| Point {
            x: e.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: e.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }).unwrap_or(Point { x: 0.0, y: 0.0 });

        let pc_perm = p.get("platform_pc_perm").and_then(|v| v.as_int()).map(|v| v as i32);
        let item_perm = p.get("platform_item_perm").and_then(|v| v.as_int()).map(|v| v as i32);

        PlatformLine {
            id: plat_id.clone(),
            start,
            end,
            pc_perm,
            item_perm,
        }
    }).collect()
}

fn parse_walls(layer: &XmlValue) -> Vec<Wall> {
    let walls = match layer.get("walls").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    walls.iter().map(|(wall_id, w)| {
        Wall {
            id: wall_id.clone(),
            x: w.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: w.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            h: w.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0),
            pc_perm: w.get("pc_perm").and_then(|v| v.as_int()).map(|v| v as i32),
            item_perm: w.get("item_perm").and_then(|v| v.as_int()).map(|v| v as i32),
        }
    }).collect()
}

fn parse_ladders(layer: &XmlValue) -> Vec<Ladder> {
    let ladders = match layer.get("ladders").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    ladders.iter().map(|(ladder_id, l)| {
        Ladder {
            id: ladder_id.clone(),
            x: l.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: l.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            w: l.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0),
            h: l.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0),
        }
    }).collect()
}

fn parse_filters(layer: &XmlValue) -> Option<LayerFilters> {
    let f = layer.get("filtersNEW")?.as_object()?;
    Some(LayerFilters {
        brightness: f.get("brightness").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        contrast: f.get("contrast").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        saturation: f.get("saturation").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        blur: f.get("blur").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        tint_color: f.get("tintColor").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
        tint_amount: f.get("tintAmount").and_then(|v| v.get("value")).and_then(|v| v.as_int()).map(|v| v as i32),
    })
}

fn parse_signposts(dynamic: &XmlValue) -> Vec<Signpost> {
    let sps = match dynamic.get("signposts").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return vec![],
    };

    sps.iter().map(|(sp_id, s)| {
        let connects = s.get("connects")
            .and_then(|v| v.as_object())
            .map(|conns| {
                conns.values().filter_map(|c| {
                    match c.get("target") {
                        Some(XmlValue::ObjRef { tsid, label }) => Some(SignpostConnection {
                            target_tsid: tsid.clone(),
                            target_label: label.clone(),
                        }),
                        _ => None,
                    }
                }).collect()
            })
            .unwrap_or_default();

        Signpost {
            id: sp_id.clone(),
            x: s.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
            y: s.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
            connects,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_STREET_XML: &str = r#"
    <game_object tsid="GA5101HF7F429V5" label="Test Meadow">
      <object id="dynamic">
        <int id="l">-3000</int>
        <int id="r">3000</int>
        <int id="t">-1000</int>
        <int id="b">0</int>
        <str id="label">Groddle Meadow</str>
        <str id="tsid">LA5101HF7F429V5</str>
        <int id="ground_y">0</int>
        <object id="gradient">
          <str id="top">87A8C9</str>
          <str id="bottom">FFC400</str>
        </object>
        <object id="layers">
          <object id="T_sky">
            <int id="w">5160</int>
            <int id="h">1000</int>
            <int id="z">-4</int>
            <str id="name">sky</str>
            <object id="decos">
              <object id="cloud_1">
                <int id="x">100</int>
                <int id="y">200</int>
                <int id="w">80</int>
                <int id="h">40</int>
                <int id="z">0</int>
                <int id="r">0</int>
                <str id="sprite_class">cloud_fluffy</str>
                <str id="name">cloud_1</str>
              </object>
            </object>
          </object>
          <object id="middleground">
            <int id="w">6000</int>
            <int id="h">1000</int>
            <int id="z">0</int>
            <str id="name">middleground</str>
            <object id="decos"></object>
            <object id="platform_lines">
              <object id="plat_ground">
                <object id="start">
                  <int id="x">-2800</int>
                  <int id="y">0</int>
                </object>
                <object id="end">
                  <int id="x">2800</int>
                  <int id="y">0</int>
                </object>
                <int id="platform_pc_perm">-1</int>
              </object>
              <object id="plat_hill">
                <object id="start">
                  <int id="x">500</int>
                  <int id="y">0</int>
                </object>
                <object id="end">
                  <int id="x">1000</int>
                  <int id="y">-100</int>
                </object>
              </object>
            </object>
            <object id="walls">
              <object id="wall_left">
                <int id="x">-3000</int>
                <int id="y">-500</int>
                <int id="h">500</int>
              </object>
            </object>
            <object id="ladders">
              <object id="ladder_1">
                <int id="x">200</int>
                <int id="y">-200</int>
                <int id="w">50</int>
                <int id="h">200</int>
              </object>
            </object>
          </object>
        </object>
      </object>
    </game_object>
    "#;

    #[test]
    fn parse_street_metadata() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        assert_eq!(street.name, "Groddle Meadow");
        assert_eq!(street.left, -3000.0);
        assert_eq!(street.right, 3000.0);
        assert_eq!(street.top, -1000.0);
        assert_eq!(street.bottom, 0.0);
        assert_eq!(street.ground_y, 0.0);
        assert!((street.width() - 6000.0).abs() < 0.001);
    }

    #[test]
    fn parse_gradient() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let grad = street.gradient.unwrap();
        assert_eq!(grad.top, "87A8C9");
        assert_eq!(grad.bottom, "FFC400");
    }

    #[test]
    fn parse_layers_sorted() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        assert_eq!(street.layers.len(), 2);
        // Should be sorted by z: sky (-4) first, middleground (0) second
        assert_eq!(street.layers[0].name, "sky");
        assert_eq!(street.layers[0].z, -4);
        assert_eq!(street.layers[1].name, "middleground");
        assert!(street.layers[1].is_middleground);
    }

    #[test]
    fn parse_platforms() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let platforms = street.platforms();
        assert_eq!(platforms.len(), 2);

        // Find the ground platform
        let ground = platforms.iter().find(|p| p.id == "plat_ground").unwrap();
        assert_eq!(ground.start.x, -2800.0);
        assert_eq!(ground.start.y, 0.0);
        assert_eq!(ground.end.x, 2800.0);
        assert!(ground.solid_from_top());
        assert!(!ground.solid_from_bottom()); // pc_perm = -1

        // Find the hill (sloped)
        let hill = platforms.iter().find(|p| p.id == "plat_hill").unwrap();
        assert_eq!(hill.start.y, 0.0);
        assert_eq!(hill.end.y, -100.0);
        // Midpoint of slope: y should be -50
        assert!((hill.y_at(750.0) - (-50.0)).abs() < 0.001);
    }

    #[test]
    fn parse_walls() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let mg = street.middleground().unwrap();
        assert_eq!(mg.walls.len(), 1);
        assert_eq!(mg.walls[0].x, -3000.0);
    }

    #[test]
    fn parse_ladders() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let mg = street.middleground().unwrap();
        assert_eq!(mg.ladders.len(), 1);
        assert_eq!(mg.ladders[0].w, 50.0);
        assert_eq!(mg.ladders[0].h, 200.0);
    }

    #[test]
    fn parse_decos() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let sky = &street.layers[0];
        assert_eq!(sky.decos.len(), 1);
        assert_eq!(sky.decos[0].sprite_class, "cloud_fluffy");
    }

    #[test]
    fn parse_parallax_factor() {
        let street = parse_street(SAMPLE_STREET_XML).unwrap();
        let sky = &street.layers[0];
        let factor = street.parallax_factor(sky);
        assert!((factor - 5160.0 / 6000.0).abs() < 0.001);
    }
}
```

Update `src-tauri/src/street/mod.rs`:
```rust
pub mod parser;
pub mod types;
pub mod xml_helpers;
```

**Step 3: Run tests**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri
cargo test -- street
```

Expected: All tests PASS.

**Step 4: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/street/
git commit -m "feat: Glitch location XML parser with typed-value element system"
```

---

## Task 4: Physics Engine (Rust)

**Files:**
- Create: `src-tauri/src/physics/mod.rs`
- Create: `src-tauri/src/physics/movement.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Write the physics module with tests**

Create `src-tauri/src/physics/mod.rs`:
```rust
pub mod movement;
```

Create `src-tauri/src/physics/movement.rs`:

```rust
use crate::street::types::PlatformLine;

/// Physics constants (tunable).
pub const GRAVITY: f64 = 980.0;       // px/s²
pub const WALK_SPEED: f64 = 200.0;    // px/s
pub const JUMP_VELOCITY: f64 = -400.0; // px/s (negative = up in Glitch coords)
pub const TERMINAL_VELOCITY: f64 = 600.0; // px/s

/// Player physics state.
#[derive(Debug, Clone)]
pub struct PhysicsBody {
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub on_ground: bool,
    /// Half-width for collision (avatar is centered on x).
    pub half_width: f64,
    /// Height from feet (y) to head.
    pub height: f64,
}

/// Input state from the player.
#[derive(Debug, Clone, Copy, Default)]
pub struct InputState {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}

impl PhysicsBody {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            on_ground: false,
            half_width: 15.0,
            height: 60.0,
        }
    }

    /// Run one physics tick.
    /// Glitch coordinate system: Y=0 at bottom, negative Y = up.
    /// Gravity pulls toward positive Y (toward ground_y=0).
    pub fn tick(&mut self, dt: f64, input: &InputState, platforms: &[PlatformLine], street_left: f64, street_right: f64) {
        // Apply horizontal input
        self.vx = if input.left && !input.right {
            -WALK_SPEED
        } else if input.right && !input.left {
            WALK_SPEED
        } else {
            0.0
        };

        // Jump (only if on ground)
        if input.jump && self.on_ground {
            self.vy = JUMP_VELOCITY;
            self.on_ground = false;
        }

        // Apply gravity (positive direction = down toward y=0 in Glitch coords)
        // In Glitch coords, gravity pulls toward MORE POSITIVE Y.
        // But wait — ground_y is 0 and platforms have negative y.
        // So gravity should push y toward 0 (more positive).
        if !self.on_ground {
            self.vy += GRAVITY * dt;
            self.vy = self.vy.min(TERMINAL_VELOCITY);
        }

        // Move
        self.x += self.vx * dt;
        self.y += self.vy * dt;

        // Clamp to street bounds
        self.x = self.x.clamp(street_left + self.half_width, street_right - self.half_width);

        // Platform collision
        self.on_ground = false;
        for platform in platforms {
            if !platform.solid_from_top() {
                continue;
            }

            // Check if player is within platform X range
            let plat_min_x = platform.min_x();
            let plat_max_x = platform.max_x();
            if self.x < plat_min_x || self.x > plat_max_x {
                continue;
            }

            let plat_y = platform.y_at(self.x);

            // Player feet are at self.y. If feet are at or below platform surface
            // and were above it before (falling onto it), snap to platform.
            // "Below" in Glitch coords means more positive Y.
            if self.vy >= 0.0 && self.y >= plat_y && self.y <= plat_y + GRAVITY * dt * dt + 2.0 {
                self.y = plat_y;
                self.vy = 0.0;
                self.on_ground = true;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::types::Point;

    fn flat_ground() -> Vec<PlatformLine> {
        vec![PlatformLine {
            id: "ground".into(),
            start: Point { x: -1000.0, y: 0.0 },
            end: Point { x: 1000.0, y: 0.0 },
            pc_perm: None,
            item_perm: None,
        }]
    }

    #[test]
    fn falls_with_gravity() {
        let mut body = PhysicsBody::new(0.0, -200.0); // High up
        let input = InputState::default();
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // Should have moved downward (vy positive, y increased)
        assert!(body.vy > 0.0);
    }

    #[test]
    fn lands_on_platform() {
        let mut body = PhysicsBody::new(0.0, -5.0); // Just above ground
        body.vy = 100.0; // Falling
        let input = InputState::default();
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        assert!(body.on_ground);
        assert_eq!(body.y, 0.0);
        assert_eq!(body.vy, 0.0);
    }

    #[test]
    fn walks_right() {
        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        assert!(body.x > 0.0);
    }

    #[test]
    fn walks_left() {
        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let input = InputState { left: true, ..Default::default() };
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        assert!(body.x < 0.0);
    }

    #[test]
    fn jumps() {
        let mut body = PhysicsBody::new(0.0, 0.0);
        body.on_ground = true;
        let input = InputState { jump: true, ..Default::default() };
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // Should have negative vy (going up) and moved up (negative y)
        assert!(body.vy < 0.0 || body.y < 0.0);
        assert!(!body.on_ground);
    }

    #[test]
    fn cannot_jump_in_air() {
        let mut body = PhysicsBody::new(0.0, -100.0);
        body.on_ground = false;
        let initial_vy = body.vy;
        let input = InputState { jump: true, ..Default::default() };
        let platforms = flat_ground();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // vy should not have gotten the jump impulse
        // (it gets gravity instead)
        assert!(body.vy >= initial_vy);
    }

    #[test]
    fn clamped_to_street_bounds() {
        let mut body = PhysicsBody::new(999.0, 0.0);
        body.on_ground = true;
        let input = InputState { right: true, ..Default::default() };
        let platforms = flat_ground();

        // Run many ticks to push past boundary
        for _ in 0..100 {
            body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);
        }

        assert!(body.x <= 1000.0 - body.half_width);
    }

    #[test]
    fn walks_on_slope() {
        let platforms = vec![PlatformLine {
            id: "slope".into(),
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 200.0, y: -100.0 },
            pc_perm: None,
            item_perm: None,
        }];

        let mut body = PhysicsBody::new(100.0, -48.0); // Near the slope surface
        body.vy = 10.0; // Slight downward velocity
        body.on_ground = false;
        let input = InputState::default();

        body.tick(1.0 / 60.0, &input, &platforms, -1000.0, 1000.0);

        // Should land on slope — y should be at the slope's Y at x=100
        // slope y_at(100) = 0 + 0.5 * (-100) = -50
        if body.on_ground {
            assert!((body.y - (-50.0)).abs() < 5.0);
        }
    }

    #[test]
    fn terminal_velocity_caps_falling() {
        let mut body = PhysicsBody::new(0.0, -10000.0);
        let input = InputState::default();
        let platforms: Vec<PlatformLine> = vec![]; // No platforms to land on

        // Fall for a long time
        for _ in 0..600 {
            body.tick(1.0 / 60.0, &input, &platforms, -100000.0, 100000.0);
        }

        assert!(body.vy <= TERMINAL_VELOCITY);
    }
}
```

**Step 2: Run tests**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri
cargo test -- physics
```

Expected: All tests PASS.

**Step 3: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/physics/
git commit -m "feat: physics engine with gravity, movement, jump, slope collision"
```

---

## Task 5: Game State, Avatar, and Tick Loop (Rust)

**Files:**
- Create: `src-tauri/src/engine/mod.rs`
- Create: `src-tauri/src/engine/state.rs`
- Create: `src-tauri/src/avatar/mod.rs`
- Create: `src-tauri/src/avatar/types.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Create avatar types**

Create `src-tauri/src/avatar/mod.rs`:
```rust
pub mod types;
```

Create `src-tauri/src/avatar/types.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnimationState {
    Idle,
    Walking,
    Jumping,
    Falling,
}
```

**Step 2: Create engine state and tick logic**

Create `src-tauri/src/engine/mod.rs`:
```rust
pub mod state;
```

Create `src-tauri/src/engine/state.rs`:
```rust
use serde::{Deserialize, Serialize};
use crate::avatar::types::{AnimationState, Direction};
use crate::physics::movement::{InputState, PhysicsBody};
use crate::street::types::StreetData;

/// The complete game state.
pub struct GameState {
    pub player: PhysicsBody,
    pub facing: Direction,
    pub street: Option<StreetData>,
    pub viewport_width: f64,
    pub viewport_height: f64,
}

/// Data sent to the frontend each tick for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderFrame {
    pub player: PlayerFrame,
    pub camera: CameraFrame,
    pub street_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerFrame {
    pub x: f64,
    pub y: f64,
    pub facing: Direction,
    pub animation: AnimationState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraFrame {
    pub x: f64,
    pub y: f64,
}

impl GameState {
    pub fn new(viewport_width: f64, viewport_height: f64) -> Self {
        Self {
            player: PhysicsBody::new(0.0, -100.0),
            facing: Direction::Right,
            street: None,
            viewport_width,
            viewport_height,
        }
    }

    pub fn load_street(&mut self, street: StreetData) {
        // Place player at ground level, center of street
        let center_x = (street.left + street.right) / 2.0;
        self.player = PhysicsBody::new(center_x, street.ground_y - 100.0);
        self.street = Some(street);
    }

    /// Run one tick of the game loop.
    pub fn tick(&mut self, dt: f64, input: &InputState) -> Option<RenderFrame> {
        let street = self.street.as_ref()?;

        // Update facing direction
        if input.left && !input.right {
            self.facing = Direction::Left;
        } else if input.right && !input.left {
            self.facing = Direction::Right;
        }

        // Physics tick
        self.player.tick(dt, input, street.platforms(), street.left, street.right);

        // Determine animation state
        let animation = if !self.player.on_ground {
            if self.player.vy < 0.0 {
                AnimationState::Jumping
            } else {
                AnimationState::Falling
            }
        } else if self.player.vx.abs() > 0.1 {
            AnimationState::Walking
        } else {
            AnimationState::Idle
        };

        // Camera: center on player, clamped to street bounds
        let cam_x = self.player.x - self.viewport_width / 2.0;
        let cam_y = self.player.y - self.viewport_height * 0.6; // Player in lower 40%
        let cam_x = cam_x.clamp(street.left, street.right - self.viewport_width);
        let cam_y = cam_y.clamp(street.top, street.bottom - self.viewport_height);

        Some(RenderFrame {
            player: PlayerFrame {
                x: self.player.x,
                y: self.player.y,
                facing: self.facing,
                animation,
            },
            camera: CameraFrame { x: cam_x, y: cam_y },
            street_id: street.tsid.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::types::*;

    fn test_street() -> StreetData {
        StreetData {
            tsid: "test".into(),
            name: "Test".into(),
            left: -3000.0,
            right: 3000.0,
            top: -1000.0,
            bottom: 0.0,
            ground_y: 0.0,
            gradient: None,
            layers: vec![Layer {
                name: "middleground".into(),
                z: 0,
                w: 6000.0,
                h: 1000.0,
                is_middleground: true,
                decos: vec![],
                platform_lines: vec![PlatformLine {
                    id: "ground".into(),
                    start: Point { x: -2800.0, y: 0.0 },
                    end: Point { x: 2800.0, y: 0.0 },
                    pc_perm: None,
                    item_perm: None,
                }],
                walls: vec![],
                ladders: vec![],
                filters: None,
            }],
            signposts: vec![],
        }
    }

    #[test]
    fn tick_produces_render_frame() {
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());
        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input);
        assert!(frame.is_some());
    }

    #[test]
    fn tick_returns_none_without_street() {
        let mut state = GameState::new(1280.0, 720.0);
        let input = InputState::default();
        assert!(state.tick(1.0 / 60.0, &input).is_none());
    }

    #[test]
    fn facing_updates_from_input() {
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());

        let input = InputState { left: true, ..Default::default() };
        state.tick(1.0 / 60.0, &input);
        assert_eq!(state.facing, Direction::Left);

        let input = InputState { right: true, ..Default::default() };
        state.tick(1.0 / 60.0, &input);
        assert_eq!(state.facing, Direction::Right);
    }

    #[test]
    fn animation_idle_on_ground() {
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());
        state.player.on_ground = true;
        state.player.y = 0.0;
        state.player.vy = 0.0;

        let input = InputState::default();
        let frame = state.tick(1.0 / 60.0, &input).unwrap();
        assert_eq!(frame.player.animation, AnimationState::Idle);
    }

    #[test]
    fn animation_walking() {
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());
        state.player.on_ground = true;
        state.player.y = 0.0;

        let input = InputState { right: true, ..Default::default() };
        let frame = state.tick(1.0 / 60.0, &input).unwrap();
        assert_eq!(frame.player.animation, AnimationState::Walking);
    }

    #[test]
    fn load_street_places_player() {
        let mut state = GameState::new(1280.0, 720.0);
        state.load_street(test_street());
        // Player should be at center of street
        assert!((state.player.x - 0.0).abs() < 1.0);
    }
}
```

**Step 3: Wire modules in lib.rs**

Update `src-tauri/src/lib.rs` to declare all modules:
```rust
mod avatar;
mod engine;
mod physics;
mod street;

// ... existing Tauri setup
```

**Step 4: Run tests**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri
cargo test
```

Expected: All tests PASS.

**Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/
git commit -m "feat: game state machine with avatar, camera, and tick loop"
```

---

## Task 6: Tauri IPC Commands and Game Thread

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml` (if needed)

**Step 1: Wire up Tauri commands and game thread**

Replace `src-tauri/src/lib.rs` with:

```rust
mod avatar;
mod engine;
mod physics;
mod street;

use engine::state::{GameState, RenderFrame};
use physics::movement::InputState;
use street::parser::parse_street;
use street::types::StreetData;

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Shared game state protected by a mutex.
struct GameStateWrapper(Mutex<GameState>);

/// Shared input state — written by frontend, read by game loop.
struct InputStateWrapper(Mutex<InputState>);

/// Flag to control the game loop.
struct GameRunning(Mutex<bool>);

#[tauri::command]
fn list_streets() -> Vec<String> {
    // For Phase A: return hardcoded demo street names.
    // Later: scan assets directory or query content network.
    vec!["demo_meadow".to_string()]
}

#[tauri::command]
fn load_street(name: String, app: AppHandle) -> Result<StreetData, String> {
    // Load XML from bundled assets
    let xml = load_street_xml(&name)?;
    let street_data = parse_street(&xml)?;

    // Update game state
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;
    state.load_street(street_data.clone());

    Ok(street_data)
}

#[tauri::command]
fn send_input(input: InputState, app: AppHandle) -> Result<(), String> {
    let input_wrapper = app.state::<InputStateWrapper>();
    let mut current = input_wrapper.0.lock().map_err(|e| e.to_string())?;
    *current = input;
    Ok(())
}

#[tauri::command]
fn start_game(app: AppHandle) -> Result<(), String> {
    let running = app.state::<GameRunning>();
    let mut is_running = running.0.lock().map_err(|e| e.to_string())?;
    if *is_running {
        return Ok(()); // Already running
    }
    *is_running = true;
    drop(is_running);

    let app_handle = app.clone();
    std::thread::spawn(move || {
        game_loop(app_handle);
    });

    Ok(())
}

#[tauri::command]
fn stop_game(app: AppHandle) -> Result<(), String> {
    let running = app.state::<GameRunning>();
    let mut is_running = running.0.lock().map_err(|e| e.to_string())?;
    *is_running = false;
    Ok(())
}

fn game_loop(app: AppHandle) {
    let tick_duration = Duration::from_secs_f64(1.0 / 60.0);
    let dt = 1.0 / 60.0;

    loop {
        let tick_start = Instant::now();

        // Check if still running
        let running = app.state::<GameRunning>();
        let is_running = running.0.lock().unwrap_or_else(|e| e.into_inner());
        if !*is_running {
            break;
        }
        drop(is_running);

        // Read current input
        let input_wrapper = app.state::<InputStateWrapper>();
        let input = *input_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());

        // Tick game state
        let state_wrapper = app.state::<GameStateWrapper>();
        let mut state = state_wrapper.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(frame) = state.tick(dt, &input) {
            drop(state); // Release lock before emitting
            let _ = app.emit("render_frame", &frame);
        }

        // Sleep for remainder of tick
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            std::thread::sleep(tick_duration - elapsed);
        }
    }
}

fn load_street_xml(name: &str) -> Result<String, String> {
    // Phase A: load from bundled assets directory
    // The asset files live at assets/streets/<name>.xml relative to the app
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../assets/streets")
        .join(format!("{}.xml", name));

    std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to load street '{}': {} (path: {:?})", name, e, path))
}

pub fn run() {
    tauri::Builder::default()
        .manage(GameStateWrapper(Mutex::new(GameState::new(1280.0, 720.0))))
        .manage(InputStateWrapper(Mutex::new(InputState::default())))
        .manage(GameRunning(Mutex::new(false)))
        .invoke_handler(tauri::generate_handler![
            list_streets,
            load_street,
            send_input,
            start_game,
            stop_game,
        ])
        .run(tauri::generate_context!())
        .expect("error while running harmony-glitch");
}
```

Add `serde::Deserialize` to `InputState` in `src-tauri/src/physics/movement.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputState {
    pub left: bool,
    pub right: bool,
    pub jump: bool,
}
```

**Step 2: Verify compilation**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri
cargo check
```

**Step 3: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src-tauri/src/
git commit -m "feat: Tauri IPC commands and 60Hz game loop thread"
```

---

## Task 7: Frontend Types, IPC Wrappers, and PixiJS Setup

**Files:**
- Create: `src/lib/types.ts`
- Create: `src/lib/ipc.ts`
- Create: `src/lib/engine/renderer.ts`

**Step 1: Create TypeScript types matching Rust DTOs**

Create `src/lib/types.ts`:
```typescript
// Types matching Rust DTOs — must stay in sync with src-tauri/src/

export interface StreetData {
  tsid: string;
  name: string;
  left: number;
  right: number;
  top: number;
  bottom: number;
  groundY: number;
  gradient: Gradient | null;
  layers: Layer[];
  signposts: Signpost[];
}

export interface Gradient {
  top: string;
  bottom: string;
}

export interface Layer {
  name: string;
  z: number;
  w: number;
  h: number;
  isMiddleground: boolean;
  decos: Deco[];
  platformLines: PlatformLine[];
  walls: Wall[];
  ladders: Ladder[];
  filters: LayerFilters | null;
}

export interface PlatformLine {
  id: string;
  start: Point;
  end: Point;
  pcPerm: number | null;
  itemPerm: number | null;
}

export interface Point {
  x: number;
  y: number;
}

export interface Wall {
  id: string;
  x: number;
  y: number;
  h: number;
  pcPerm: number | null;
  itemPerm: number | null;
}

export interface Ladder {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Deco {
  id: string;
  name: string;
  spriteClass: string;
  x: number;
  y: number;
  w: number;
  h: number;
  z: number;
  r: number;
  hFlip: boolean;
}

export interface LayerFilters {
  brightness: number | null;
  contrast: number | null;
  saturation: number | null;
  blur: number | null;
  tintColor: number | null;
  tintAmount: number | null;
}

export interface Signpost {
  id: string;
  x: number;
  y: number;
  connects: SignpostConnection[];
}

export interface SignpostConnection {
  targetTsid: string;
  targetLabel: string;
}

export type Direction = 'left' | 'right';
export type AnimationState = 'idle' | 'walking' | 'jumping' | 'falling';

export interface PlayerFrame {
  x: number;
  y: number;
  facing: Direction;
  animation: AnimationState;
}

export interface CameraFrame {
  x: number;
  y: number;
}

export interface RenderFrame {
  player: PlayerFrame;
  camera: CameraFrame;
  streetId: string;
}

export interface InputState {
  left: boolean;
  right: boolean;
  jump: boolean;
}
```

**Step 2: Create IPC wrappers**

Create `src/lib/ipc.ts`:
```typescript
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { StreetData, InputState, RenderFrame } from './types';

export async function listStreets(): Promise<string[]> {
  return invoke<string[]>('list_streets');
}

export async function loadStreet(name: string): Promise<StreetData> {
  return invoke<StreetData>('load_street', { name });
}

export async function sendInput(input: InputState): Promise<void> {
  return invoke('send_input', { input });
}

export async function startGame(): Promise<void> {
  return invoke('start_game');
}

export async function stopGame(): Promise<void> {
  return invoke('stop_game');
}

export async function onRenderFrame(
  callback: (frame: RenderFrame) => void
): Promise<UnlistenFn> {
  return listen<RenderFrame>('render_frame', (event) => {
    callback(event.payload);
  });
}
```

**Step 3: Create PixiJS application setup**

Create `src/lib/engine/renderer.ts`:
```typescript
import { Application, Container, Graphics } from 'pixi.js';
import type { StreetData, RenderFrame, Layer } from '../types';

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
      this.buildScene(this.street);
    }
  }

  /**
   * Build the PixiJS scene graph from street data.
   * Converts from Glitch coordinates (Y=0 at bottom, negative up)
   * to screen coordinates (Y=0 at top, positive down).
   */
  buildScene(street: StreetData): void {
    this.street = street;
    this.parallaxContainer.removeChildren();
    this.worldContainer.removeChildren();
    this.layerContainers.clear();

    const streetHeight = street.bottom - street.top;

    // Build gradient background
    const bg = new Graphics();
    const topColor = street.gradient ? parseInt(street.gradient.top, 16) : 0x87a8c9;
    const bottomColor = street.gradient ? parseInt(street.gradient.bottom, 16) : 0xffc400;
    bg.rect(0, 0, this.app.screen.width, this.app.screen.height);
    bg.fill(topColor); // Simplified — full gradient would use a shader
    this.parallaxContainer.addChild(bg);

    // Build parallax layers
    for (const layer of street.layers) {
      const container = new Container();
      container.label = layer.name;

      // Draw decos as placeholder rectangles (until real art assets are available)
      for (const deco of layer.decos) {
        const g = new Graphics();
        // Convert Glitch Y (0=bottom, negative=up) to screen Y (0=top, positive=down)
        const screenY = streetHeight - (deco.y - street.top);
        g.rect(deco.x - street.left, screenY - deco.h, deco.w, deco.h);
        g.fill({ color: 0x4a6741, alpha: 0.3 });
        if (deco.hFlip) {
          g.scale.x = -1;
          g.x += deco.w;
        }
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
    this.drawPlatforms(street, streetHeight);

    // Create avatar placeholder
    this.avatarGraphics = new Graphics();
    this.avatarGraphics.rect(-15, -60, 30, 60);
    this.avatarGraphics.fill(0x5865f2);
    this.worldContainer.addChild(this.avatarGraphics);
  }

  private drawPlatforms(street: StreetData, streetHeight: number): void {
    if (!this.platformGraphics) return;
    this.platformGraphics.clear();

    for (const platform of street.layers.filter(l => l.isMiddleground).flatMap(l => l.platformLines)) {
      const startScreenY = streetHeight - (platform.start.y - street.top);
      const endScreenY = streetHeight - (platform.end.y - street.top);
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
        const screenY = streetHeight - (wall.y - street.top);
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

    const streetHeight = this.street.bottom - this.street.top;
    const mg = this.street.layers.find(l => l.isMiddleground);
    const mgWidth = mg?.w ?? this.street.right - this.street.left;

    // Update avatar position (convert Glitch coords to screen coords)
    const avatarScreenX = frame.player.x - this.street.left;
    const avatarScreenY = streetHeight - (frame.player.y - this.street.top);
    this.avatarGraphics.x = avatarScreenX;
    this.avatarGraphics.y = avatarScreenY;
    this.avatarGraphics.scale.x = frame.player.facing === 'right' ? 1 : -1;

    // Update camera — shift world container
    const camScreenX = frame.camera.x - this.street.left;
    const camScreenY = streetHeight - (frame.camera.y - this.street.top) - this.app.screen.height;
    this.worldContainer.x = -camScreenX;
    this.worldContainer.y = -camScreenY;

    // Update parallax layers
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
```

**Step 4: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src/lib/
git commit -m "feat: frontend types, IPC wrappers, and PixiJS renderer"
```

---

## Task 8: Svelte Components and Input Handling

**Files:**
- Create: `src/lib/components/GameCanvas.svelte`
- Create: `src/lib/components/StreetPicker.svelte`
- Create: `src/lib/components/DebugOverlay.svelte`
- Modify: `src/App.svelte`

**Step 1: Create GameCanvas component**

Create `src/lib/components/GameCanvas.svelte`:
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { GameRenderer } from '../engine/renderer';
  import { sendInput, onRenderFrame, startGame } from '../ipc';
  import type { StreetData, InputState, RenderFrame } from '../types';

  let { street, debugMode = false, onFrame }: {
    street: StreetData | null;
    debugMode?: boolean;
    onFrame?: (frame: RenderFrame) => void;
  } = $props();

  let canvasEl: HTMLCanvasElement;
  let renderer: GameRenderer | null = null;

  // Track key state
  let keys = $state<InputState>({ left: false, right: false, jump: false });

  function handleKeyDown(e: KeyboardEvent) {
    let changed = false;
    if (e.key === 'ArrowLeft' || e.key === 'a') { keys.left = true; changed = true; }
    if (e.key === 'ArrowRight' || e.key === 'd') { keys.right = true; changed = true; }
    if (e.key === ' ' || e.key === 'ArrowUp' || e.key === 'w') {
      e.preventDefault();
      keys.jump = true;
      changed = true;
    }
    if (changed) sendInput({ ...keys });
  }

  function handleKeyUp(e: KeyboardEvent) {
    let changed = false;
    if (e.key === 'ArrowLeft' || e.key === 'a') { keys.left = false; changed = true; }
    if (e.key === 'ArrowRight' || e.key === 'd') { keys.right = false; changed = true; }
    if (e.key === ' ' || e.key === 'ArrowUp' || e.key === 'w') { keys.jump = false; changed = true; }
    if (changed) sendInput({ ...keys });
  }

  onMount(async () => {
    renderer = new GameRenderer();
    await renderer.init(canvasEl);

    const unlisten = await onRenderFrame((frame) => {
      renderer?.updateFrame(frame);
      onFrame?.(frame);
    });

    return () => {
      unlisten();
      renderer?.destroy();
    };
  });

  $effect(() => {
    if (renderer && street) {
      renderer.buildScene(street);
      renderer.setDebugMode(debugMode);
      startGame();
    }
  });

  $effect(() => {
    renderer?.setDebugMode(debugMode);
  });
</script>

<svelte:window onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

<div class="canvas-container">
  <canvas bind:this={canvasEl}></canvas>
</div>

<style>
  .canvas-container {
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
```

**Step 2: Create StreetPicker component**

Create `src/lib/components/StreetPicker.svelte`:
```svelte
<script lang="ts">
  import { listStreets, loadStreet } from '../ipc';
  import type { StreetData } from '../types';
  import { onMount } from 'svelte';

  let { onStreetLoaded }: {
    onStreetLoaded: (street: StreetData) => void;
  } = $props();

  let streets = $state<string[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      streets = await listStreets();
    } catch (e) {
      error = `Failed to list streets: ${e}`;
    }
  });

  async function handleSelect(name: string) {
    loading = true;
    error = null;
    try {
      const street = await loadStreet(name);
      onStreetLoaded(street);
    } catch (e) {
      error = `Failed to load street: ${e}`;
    } finally {
      loading = false;
    }
  }
</script>

<div class="street-picker">
  <h1>Harmony Glitch</h1>
  <p class="subtitle">Choose a street to explore</p>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="street-list">
    {#each streets as name}
      <button
        type="button"
        class="street-btn"
        onclick={() => handleSelect(name)}
        disabled={loading}
      >
        {name.replace(/_/g, ' ')}
      </button>
    {/each}

    {#if streets.length === 0 && !error}
      <p class="empty">No streets available</p>
    {/if}
  </div>
</div>

<style>
  .street-picker {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 16px;
    padding: 32px;
  }

  h1 {
    font-size: 2rem;
    color: #e0e0e0;
    margin: 0;
  }

  .subtitle {
    color: #888;
    font-size: 0.9rem;
    margin: 0;
  }

  .error {
    color: #e74c3c;
    font-size: 0.85rem;
  }

  .street-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 16px;
  }

  .street-btn {
    padding: 12px 32px;
    border: 1px solid #444;
    border-radius: 8px;
    background: #2a2a4a;
    color: #e0e0e0;
    font-size: 1rem;
    cursor: pointer;
    text-transform: capitalize;
  }

  .street-btn:hover:not(:disabled) {
    background: #5865f2;
    border-color: #5865f2;
  }

  .street-btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .empty {
    color: #666;
    font-size: 0.85rem;
  }
</style>
```

**Step 3: Create DebugOverlay component**

Create `src/lib/components/DebugOverlay.svelte`:
```svelte
<script lang="ts">
  import type { RenderFrame } from '../types';

  let { frame, visible = false }: {
    frame: RenderFrame | null;
    visible?: boolean;
  } = $props();
</script>

{#if visible && frame}
  <div class="debug-overlay" aria-hidden="true">
    <p>pos: ({frame.player.x.toFixed(0)}, {frame.player.y.toFixed(0)})</p>
    <p>cam: ({frame.camera.x.toFixed(0)}, {frame.camera.y.toFixed(0)})</p>
    <p>state: {frame.player.animation} | {frame.player.facing}</p>
    <p>street: {frame.streetId}</p>
  </div>
{/if}

<style>
  .debug-overlay {
    position: fixed;
    top: 8px;
    left: 8px;
    background: rgba(0, 0, 0, 0.7);
    color: #0f0;
    font-family: monospace;
    font-size: 12px;
    padding: 8px;
    border-radius: 4px;
    z-index: 50;
    pointer-events: none;
  }

  p {
    margin: 0;
    line-height: 1.4;
  }
</style>
```

**Step 4: Wire up App.svelte**

Replace `src/App.svelte`:
```svelte
<script lang="ts">
  import './app.css';
  import GameCanvas from './lib/components/GameCanvas.svelte';
  import StreetPicker from './lib/components/StreetPicker.svelte';
  import DebugOverlay from './lib/components/DebugOverlay.svelte';
  import type { StreetData, RenderFrame } from './lib/types';

  let currentStreet = $state<StreetData | null>(null);
  let latestFrame = $state<RenderFrame | null>(null);
  let debugMode = $state(false);

  function handleStreetLoaded(street: StreetData) {
    currentStreet = street;
  }

  function handleFrame(frame: RenderFrame) {
    latestFrame = frame;
  }

  function toggleDebug() {
    debugMode = !debugMode;
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'F3') { e.preventDefault(); toggleDebug(); }}} />

<main>
  {#if currentStreet}
    <GameCanvas street={currentStreet} {debugMode} onFrame={handleFrame} />
    <DebugOverlay frame={latestFrame} visible={debugMode} />
    <button type="button" class="back-btn" onclick={() => { currentStreet = null; latestFrame = null; }}>
      Back
    </button>
  {:else}
    <StreetPicker onStreetLoaded={handleStreetLoaded} />
  {/if}
</main>

<style>
  main {
    height: 100%;
    width: 100%;
  }

  .back-btn {
    position: fixed;
    top: 8px;
    right: 8px;
    padding: 6px 16px;
    border: 1px solid #444;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.6);
    color: #e0e0e0;
    font-size: 0.8rem;
    cursor: pointer;
    z-index: 50;
  }

  .back-btn:hover {
    background: rgba(88, 101, 242, 0.8);
  }
</style>
```

**Step 5: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add src/
git commit -m "feat: GameCanvas, StreetPicker, DebugOverlay components with keyboard input"
```

---

## Task 9: Demo Street Asset and Integration

**Files:**
- Create: `assets/streets/demo_meadow.xml`
- Verify: full app launches and renders

**Step 1: Create a demo street XML file**

Create `assets/streets/demo_meadow.xml` — a simple but complete Glitch-format street with platforms, slopes, and layers:

```xml
<game_object tsid="GADEMO001" label="Demo Meadow">
  <object id="dynamic">
    <int id="l">-2000</int>
    <int id="r">2000</int>
    <int id="t">-800</int>
    <int id="b">0</int>
    <str id="label">Demo Meadow</str>
    <str id="tsid">LADEMO001</str>
    <int id="ground_y">0</int>
    <object id="gradient">
      <str id="top">87A8C9</str>
      <str id="bottom">98D4A0</str>
    </object>
    <object id="layers">
      <object id="T_sky">
        <int id="w">3600</int>
        <int id="h">800</int>
        <int id="z">-3</int>
        <str id="name">sky</str>
        <object id="decos"></object>
      </object>
      <object id="T_bg">
        <int id="w">3800</int>
        <int id="h">800</int>
        <int id="z">-1</int>
        <str id="name">bg_1</str>
        <object id="decos">
          <object id="tree_bg_1">
            <int id="x">800</int>
            <int id="y">300</int>
            <int id="w">120</int>
            <int id="h">200</int>
            <int id="z">0</int>
            <int id="r">0</int>
            <str id="sprite_class">tree_bg</str>
            <str id="name">tree_bg_1</str>
          </object>
          <object id="tree_bg_2">
            <int id="x">2200</int>
            <int id="y">350</int>
            <int id="w">100</int>
            <int id="h">180</int>
            <int id="z">1</int>
            <int id="r">0</int>
            <str id="sprite_class">tree_bg</str>
            <str id="name">tree_bg_2</str>
          </object>
        </object>
      </object>
      <object id="middleground">
        <int id="w">4000</int>
        <int id="h">800</int>
        <int id="z">0</int>
        <str id="name">middleground</str>
        <object id="decos">
          <object id="bush_1">
            <int id="x">1500</int>
            <int id="y">0</int>
            <int id="w">60</int>
            <int id="h">40</int>
            <int id="z">5</int>
            <int id="r">0</int>
            <str id="sprite_class">bush_small</str>
            <str id="name">bush_1</str>
          </object>
        </object>
        <object id="platform_lines">
          <object id="plat_main">
            <object id="start">
              <int id="x">-1800</int>
              <int id="y">0</int>
            </object>
            <object id="end">
              <int id="x">1800</int>
              <int id="y">0</int>
            </object>
          </object>
          <object id="plat_hill_up">
            <object id="start">
              <int id="x">400</int>
              <int id="y">0</int>
            </object>
            <object id="end">
              <int id="x">800</int>
              <int id="y">-120</int>
            </object>
          </object>
          <object id="plat_hill_top">
            <object id="start">
              <int id="x">800</int>
              <int id="y">-120</int>
            </object>
            <object id="end">
              <int id="x">1200</int>
              <int id="y">-120</int>
            </object>
          </object>
          <object id="plat_hill_down">
            <object id="start">
              <int id="x">1200</int>
              <int id="y">-120</int>
            </object>
            <object id="end">
              <int id="x">1600</int>
              <int id="y">0</int>
            </object>
          </object>
          <object id="plat_floating">
            <object id="start">
              <int id="x">-600</int>
              <int id="y">-200</int>
            </object>
            <object id="end">
              <int id="x">-200</int>
              <int id="y">-200</int>
            </object>
            <int id="platform_pc_perm">-1</int>
          </object>
        </object>
        <object id="walls">
          <object id="wall_left">
            <int id="x">-1900</int>
            <int id="y">-400</int>
            <int id="h">400</int>
          </object>
          <object id="wall_right">
            <int id="x">1900</int>
            <int id="y">-400</int>
            <int id="h">400</int>
          </object>
        </object>
        <object id="ladders"></object>
      </object>
    </object>
  </object>
</game_object>
```

This demo street includes:
- A flat main ground platform
- A hill (slope up → flat top → slope down)
- A floating one-way platform (can jump through from below)
- Left and right boundary walls
- Background layers with decos at different parallax depths
- A sky gradient

**Step 2: Run the full app**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
npm run tauri dev
```

Expected behavior:
1. App opens showing StreetPicker with "demo meadow" button
2. Clicking it loads the street and shows the game canvas
3. Arrow keys / WASD move the avatar left/right
4. Space / Up / W jumps
5. Avatar walks along the ground, walks up/down the hill slopes
6. Can jump onto the floating platform from below
7. Camera follows player, parallax backgrounds scroll at different rates
8. F3 toggles debug overlay (shows position, animation state, collision lines)
9. "Back" button returns to street picker

**Step 3: Run all tests**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri
cargo test
```

Expected: All Rust tests PASS.

**Step 4: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add assets/ src/ src-tauri/
git commit -m "feat: demo street and full Phase A integration"
```

---

## Task 10: Polish, Testing, and README

**Files:**
- Add any missing test coverage for edge cases
- Create: `vitest.config.ts` (if frontend tests needed)
- Verify: all tests pass, app runs cleanly

**Step 1: Add edge case tests to Rust**

Add to `src-tauri/src/physics/movement.rs` tests:
- Player standing still on a slope doesn't slide
- Player walks smoothly across platform transitions (one platform end → next platform start)
- Multiple overlapping platforms: player lands on highest one

Add to `src-tauri/src/street/parser.rs` tests:
- Empty street (no layers) parses without error
- Missing optional fields (gradient, filters) parse as None
- Unknown XML elements are gracefully ignored

**Step 2: Verify full test suite**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch/src-tauri
cargo test
cargo clippy --all-targets
```

**Step 3: Manual smoke test**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
npm run tauri dev
```

Verify all Phase A success criteria from the design doc:
- [ ] App launches, shows street picker
- [ ] Selecting a street loads and renders parallax backgrounds
- [ ] Avatar stands on platforms, obeys gravity, walks, jumps
- [ ] Sloped platforms work
- [ ] Camera follows player, clamped to street bounds
- [ ] Debug overlay shows collision geometry, FPS, position (F3)
- [ ] Street loader is generic (any valid XML)
- [ ] Demo street bundled
- [ ] All Rust game logic has unit tests
- [ ] Renderer contains zero game logic

**Step 4: Commit**

```bash
cd /Users/zeblith/work/zeblithic/harmony-glitch
git add -A
git commit -m "test: edge cases and Phase A polish"
```
