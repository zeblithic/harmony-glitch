# Phase C Vertical Slice: Items & Interaction — Design Document

## Overview

This document specifies the first vertical slice of Phase C for harmony-glitch: an item and interaction system that lets players interact with world entities (trees, chickens, pigs, butterflies) to receive items, manage an inventory, and drop items back into the world.

**Goal:** "I can interact with the world." — the smallest system that captures Glitch's core loop of approaching a world entity, performing an action, and receiving items.

**Scope boundaries:**
- Pickup + use on world objects (interact with entity → receive item)
- World entities are static props (no state, no movement, no depletion)
- Item definitions are JSON-driven (bundled files)
- Multiplayer: local-only, no sync between peers
- Inventory: bounded slots (16) with stacking

**Not in scope:** Crafting, item use verbs, NPC AI/movement, entity state/cooldowns, multiplayer item sync, persistence via harmony-content.

## Data Model

### Item Definitions (`assets/items.json`)

An item *type* is a template — "cherry" is a type, a stack of 12 cherries is an instance.

```json
{
  "cherry": {
    "name": "Cherry",
    "description": "A plump, juicy cherry from a Fruit Tree.",
    "category": "food",
    "stackLimit": 50,
    "icon": "cherry"
  },
  "grain": {
    "name": "Grain",
    "description": "Squeezed from a chicken. Don't ask.",
    "category": "food",
    "stackLimit": 50,
    "icon": "grain"
  }
}
```

Fields:
- `name` — display name
- `description` — tooltip text
- `category` — informational (food/material/tool), no gameplay effect yet
- `stackLimit` — max items per inventory slot
- `icon` — maps to a sprite (placeholder for now)

Initial items: cherry, grain, meat, milk, bubble, wood.

### Entity Definitions (`assets/entities.json`)

An entity *type* defines an interactable world object and what it yields.

```json
{
  "fruit_tree": {
    "name": "Fruit Tree",
    "verb": "Harvest",
    "yields": [{ "item": "cherry", "min": 1, "max": 3 }],
    "cooldownSecs": 0,
    "spriteClass": "tree_fruit",
    "interactRadius": 80
  },
  "chicken": {
    "name": "Chicken",
    "verb": "Squeeze",
    "yields": [{ "item": "grain", "min": 1, "max": 2 }],
    "cooldownSecs": 0,
    "spriteClass": "npc_chicken",
    "interactRadius": 60
  }
}
```

Fields:
- `name` — display name
- `verb` — action label shown to the player ("Squeeze", "Harvest", etc.)
- `yields` — array of `{ item, min, max }` defining what items and how many
- `cooldownSecs` — 0 for static props (field exists for future stateful entities)
- `spriteClass` — maps to a sprite
- `interactRadius` — pixels, how close the player must be to interact

Initial entities: fruit_tree, chicken, pig, butterfly, bubble_tree.

### Rust Types

```rust
/// Item type definition (loaded from JSON at startup)
struct ItemDef {
    id: String,
    name: String,
    description: String,
    category: String,
    stack_limit: u32,
    icon: String,
}

/// A stack of items in inventory
struct ItemStack {
    item_id: String,
    count: u32,
}

/// Player inventory — fixed-size array of optional item stacks
struct Inventory {
    slots: Vec<Option<ItemStack>>,
    capacity: usize,
}

/// Entity type definition (loaded from JSON at startup)
struct EntityDef {
    id: String,
    name: String,
    verb: String,
    yields: Vec<YieldEntry>,
    cooldown_secs: f64,
    sprite_class: String,
    interact_radius: f64,
}

struct YieldEntry {
    item: String,
    min: u32,
    max: u32,
}

/// An entity instance placed in the world (per-street)
struct WorldEntity {
    id: String,
    entity_type: String,
    x: f64,
    y: f64,
}

/// An item sitting on the ground (runtime-created)
struct WorldItem {
    id: String,
    item_id: String,
    count: u32,
    x: f64,
    y: f64,
}
```

Type aliases for definition lookups:
- `ItemDefs = HashMap<String, ItemDef>`
- `EntityDefs = HashMap<String, EntityDef>`

### Separation of Concerns

- **Definitions** (ItemDef, EntityDef) are loaded once at startup, immutable, global.
- **Placements** (WorldEntity) are per-street, loaded with the street.
- **Runtime state** (Inventory, WorldItem) is mutable, created by game actions.

Item types are global (a cherry is a cherry everywhere). Entity types are global (a fruit_tree works the same everywhere). Only entity placements are per-street.

## Entity Placement

### Per-Street Entity Files

Entity positions are defined in JSON files alongside street XMLs:

```
assets/streets/
  demo_meadow.xml               # existing — geometry
  demo_meadow_entities.json     # NEW — entity placements
  demo_heights.xml              # existing
  demo_heights_entities.json    # NEW
```

```json
[
  { "id": "tree_1", "type": "fruit_tree", "x": -800, "y": -2 },
  { "id": "tree_2", "type": "fruit_tree", "x": 1200, "y": -2 },
  { "id": "chicken_1", "type": "chicken", "x": 200, "y": -2 },
  { "id": "butterfly_1", "type": "butterfly", "x": -400, "y": -80 }
]
```

Entity files are `include_str!`-embedded like street XMLs to keep the binary self-contained. A new `load_entity_placement(name)` function mirrors the existing `load_street_xml(name)` with its own `include_str!` match arms mapping street names/TSIDs to entity JSON files.

### Loading Flow

When `load_street(name)` is called:
1. Parse street XML → `StreetData` (existing, via `load_street_xml`)
2. Parse entity placement JSON → `Vec<WorldEntity>` (new, via `load_entity_placement`)
3. Clear previous `world_items` and `world_entities`
4. Store new entities in `GameState`

## Interaction System

### Input Extension

`InputState` gains an `interact` field:

```rust
struct InputState {
    left: bool,
    right: bool,
    jump: bool,
    interact: bool,  // mapped to 'E' key
}
```

**IPC boundary note:** `InputState` is an IPC DTO — it is serialized by the frontend and deserialized by Rust's `send_input` command. Adding `interact` requires updating:
- The TypeScript `InputState` interface in `types.ts` to add `interact: boolean`
- The reactive key state in `GameCanvas.svelte` to include `interact`
- The keydown/keyup handlers in `GameCanvas.svelte` to map 'E' → `interact`
- The chat-focus key reset logic to also clear `interact`

These are listed under Modified Files but called out here because `InputState` crosses the IPC boundary.

### Rising Edge Detection

Holding 'E' must not spam interactions. `GameState` tracks `prev_interact: bool` and triggers only on `!prev && current`. Unlike `prev_jump` which lives in `PhysicsBody` (physics-scoped), `prev_interact` lives in `GameState` because interaction is game logic, not physics.

### Per-Tick Processing

Each tick, two new steps run between physics and camera:

**Step 1 — Proximity scan:** Linear scan over world entities and world items, computing distance to player. Find the nearest interactable within range. Entities use their `interact_radius` from the definition. Ground items use a fixed pickup radius (60px). At equal distance, entities take priority over ground items.

**Step 2 — Interaction execution:** If interact was just pressed (rising edge) and there is a nearest interactable:

- **Entity:** Roll yield quantity (uniform random in `min..=max` for each yield entry using the RNG passed to `tick()`), attempt to add each to inventory. If inventory full, excess items spawn as WorldItems at the entity's position.
- **Ground item:** Attempt to add the item stack to inventory. If partial fit (e.g., room for 12 out of 30), pick up what fits, reduce the ground item's count.

**RNG:** Following the sans-I/O pattern ("the caller provides RNG"), `tick()` gains an `rng: &mut impl Rng` parameter. The game loop thread passes its existing `ThreadRng`. This keeps `GameState` free of runtime coupling and makes interaction tests deterministic with a seeded RNG.

### Interaction Prompt

When something is in range, produce an `InteractionPrompt`:

```rust
struct InteractionPrompt {
    verb: String,          // "Squeeze", "Harvest", "Pick up"
    target_name: String,   // "Chicken", "Cherry x3"
    target_x: f64,
    target_y: f64,
}
```

Frontend renders as `[E] {verb} {name}` floating above the target.

## Inventory Operations

### Core API

```rust
impl Inventory {
    /// Try to add items. Returns the count that couldn't fit.
    fn add(&mut self, item_id: &str, count: u32, defs: &ItemDefs) -> u32;

    /// Remove items from a specific slot. Returns actual count removed.
    fn remove(&mut self, slot: usize, count: u32) -> u32;

    /// Drop entire stack from slot — returns what was there.
    fn drop_item(&mut self, slot: usize) -> Option<ItemStack>;

    /// Check if any room exists for this item type.
    fn has_room_for(&self, item_id: &str, defs: &ItemDefs) -> bool;
}
```

**Add logic:** First stack onto existing slots with the same `item_id` (up to `stack_limit`). Then fill empty slots. Return overflow.

**Capacity:** 16 slots.

### Drop via Tauri Command

Dropping items is a UI action (no proximity requirement, no timing sensitivity), so it uses a Tauri command rather than flowing through InputState:

```rust
#[tauri::command]
fn drop_item(slot: usize, app: AppHandle) -> Result<(), String>;
```

Acquires `GameState` lock, calls `inventory.drop_item(slot)`, spawns a `WorldItem` at the player's feet.

## RenderFrame Extension

```rust
pub struct RenderFrame {
    pub player: PlayerFrame,
    pub camera: CameraFrame,
    pub street_id: String,
    pub remote_players: Vec<RemotePlayerFrame>,
    // NEW
    pub inventory: InventoryFrame,
    pub world_entities: Vec<WorldEntityFrame>,
    pub world_items: Vec<WorldItemFrame>,
    pub interaction_prompt: Option<InteractionPrompt>,
    pub pickup_feedback: Vec<PickupFeedback>,
}

struct InventoryFrame {
    slots: Vec<Option<ItemStackFrame>>,
    capacity: usize,
}

struct ItemStackFrame {
    item_id: String,
    name: String,
    icon: String,
    count: u32,
    stack_limit: u32,
}

struct WorldEntityFrame {
    id: String,
    entity_type: String,
    name: String,
    sprite_class: String,
    x: f64,
    y: f64,
}

struct WorldItemFrame {
    id: String,
    item_id: String,
    name: String,
    icon: String,
    count: u32,
    x: f64,
    y: f64,
}

struct PickupFeedback {
    text: String,       // "+Cherry x3" or "Inventory full!"
    success: bool,      // green vs red
    x: f64,
    y: f64,
    age_secs: f64,      // for fade animation
}
```

Item definition fields (name, icon, stack_limit) are denormalized into frames so the frontend needs no lookups.

### PickupFeedback Lifecycle

`PickupFeedback` instances are stored in `GameState.pickup_feedback: Vec<PickupFeedback>`. They are:
- **Created** during interaction execution (step 4 of tick) — one per item type yielded, or one "Inventory full!" on failure
- **Aged** each tick: `age_secs += dt`
- **Removed** when `age_secs > 1.5`
- **Rendered** in the RenderFrame with their current `age_secs` so the frontend can compute opacity (`1.0 - age_secs / 1.5`)

## Frontend Design

### Inventory Panel (`InventoryPanel.svelte`)

- **Toggle:** 'I' key opens/closes, Escape closes
- **Layout:** Right-side panel, game continues underneath
- **Grid:** 4 columns × 4 rows = 16 slots
- **Selection:** Click slot to select, shows item details below (name, description, count/stackLimit)
- **Drop:** "Drop" button on selected item → calls `dropItem` IPC command
- **Accessibility:** Arrow key navigation between slots, Enter to select, D to drop. Panel has `role="dialog"`, slots have `role="gridcell"`, proper aria-labels.

### World Rendering (PixiJS)

- **Entities:** Sprites in WorldContainer at z=0 (middleground), depth-sorted by y-position alongside decos. Placeholder colored rectangles for this slice.
- **Ground items:** Small sprites on platform surface. Gentle vertical bob (2-3px sine wave). Count badge if > 1.
- **Interaction prompt:** In UIContainer (screen-fixed). Position derived from target entity's world coords through camera transform. `[E] {verb} {name}` in semi-transparent dark background.
- **Pickup feedback:** Floating text that rises and fades over ~1 second. Green for success, red for "Inventory full!".

### Sprite Assets

Placeholder sprites for this slice — colored shapes with labels. The sprite system maps `icon` → sprite. Real Glitch CC0 sprites integrated later.

## GameState & Game Loop Integration

### GameState Extension

```rust
pub struct GameState {
    pub player: PhysicsBody,
    pub facing: Direction,
    pub street: Option<StreetData>,
    pub viewport_width: f64,
    pub viewport_height: f64,
    // NEW
    pub inventory: Inventory,
    pub world_entities: Vec<WorldEntity>,
    pub world_items: Vec<WorldItem>,
    pub item_defs: ItemDefs,
    pub entity_defs: EntityDefs,
    pub prev_interact: bool,
    pub next_item_id: u64,
    pub pickup_feedback: Vec<PickupFeedback>,
}
```

**Initialization:** `item_defs` and `entity_defs` are loaded from JSON before `GameState` is created. `GameState::new()` gains `item_defs: ItemDefs, entity_defs: EntityDefs` parameters. In `lib.rs`, the JSON loading moves before the `.manage(GameStateWrapper(...))` call so defs are available at construction time. All other new fields initialize to empty/default (`Inventory::new(16)`, empty vecs, `false`, `0`).

### Tick Loop Extension

```
1. Update facing                    (existing)
2. Physics tick                     (existing)
3. Proximity scan                   NEW
4. Interaction execution            NEW
5. Determine animation              (existing)
6. Camera                           (existing)
7. Build RenderFrame                (existing, extended with item data)
```

### Tick Signature Change

`GameState::tick()` gains an RNG parameter to support yield rolling:

```rust
pub fn tick(&mut self, dt: f64, input: &InputState, rng: &mut impl Rng) -> Option<RenderFrame>
```

The game loop thread passes its existing `ThreadRng`.

### Tauri Setup

- `item_defs` and `entity_defs` loaded from bundled JSON before `GameState` creation, passed to `GameState::new()`
- Single new command: `drop_item(slot)`
- No new managed state wrappers — everything in `GameState`
- The `drop_item` command blocks on the `GameState` mutex. This is acceptable — the game loop holds the lock for at most ~16ms (one tick), so the command blocks briefly at worst.

## File Changes

### New Files

| File | Purpose |
|------|---------|
| `assets/items.json` | 6 item type definitions |
| `assets/entities.json` | 5 entity type definitions |
| `assets/streets/demo_meadow_entities.json` | Entity placements for demo_meadow |
| `assets/streets/demo_heights_entities.json` | Entity placements for demo_heights |
| `src-tauri/src/item/mod.rs` | Module root |
| `src-tauri/src/item/types.rs` | ItemDef, ItemStack, Inventory, WorldItem, WorldEntity, EntityDef |
| `src-tauri/src/item/inventory.rs` | Inventory operations |
| `src-tauri/src/item/interaction.rs` | Proximity scan, interaction execution |
| `src-tauri/src/item/loader.rs` | JSON parsing for definitions and placements |
| `src/lib/components/InventoryPanel.svelte` | Side panel inventory UI |

### Modified Files

| File | Change |
|------|--------|
| `src-tauri/src/lib.rs` | Add `mod item`, load defs at startup, add `drop_item` command |
| `src-tauri/src/engine/state.rs` | Extend GameState, tick, RenderFrame |
| `src-tauri/src/physics/movement.rs` | Add `interact: bool` to InputState |
| `src/lib/types.ts` | Add inventory/entity/item/interaction TS types |
| `src/lib/ipc.ts` | Add `dropItem` IPC wrapper |
| `src/lib/engine/renderer.ts` | Render entities, ground items, prompts, feedback |
| `src/lib/components/GameCanvas.svelte` | Handle 'E' and 'I' keys, pass data |
| `src/App.svelte` | Mount InventoryPanel, wire toggle |

### Unchanged

- `src-tauri/src/network/` — no multiplayer item sync
- `src-tauri/src/street/parser.rs` — entity placement is separate JSON
- `src-tauri/src/physics/` — no physics changes beyond InputState

## Future Work (out of scope, informed by design)

- **Entity state & cooldowns:** `cooldownSecs` field exists, just ignored. Add depletion/regeneration later.
- **Entity movement:** Chickens, pigs wandering on platforms. Requires pathfinding.
- **Crafting:** Combine items via recipes. harmony-compute WASM runtime for recipe execution.
- **Item use verbs:** "Eat Cherry" to restore energy, etc.
- **Multiplayer sync:** Broadcast interaction events via Zenoh, consensus on shared items.
- **Persistence:** Snapshot inventory/world state to harmony-content CIDs.
- **Glitch-style bags:** Expand side panel into multiple expandable bag containers.
- **Real sprites:** Replace placeholders with converted Glitch CC0 assets.
