# Crafting System Design

## Goal

Add a recipe-based crafting system so players can combine inventory items to
produce new items. Recipes may require tools (items that are needed but not
consumed). This gives harvested items purpose and creates the first real
gameplay loop beyond "walk around and pick things up."

## Non-Goals

- Item "use" / consumption for stat effects (no health/energy system yet)
- Timed crafting (duration_secs exists in data model but is ignored this pass)
- Station-based crafting (must be near a workbench, etc.)
- Recipe discovery / learning (all recipes visible from the start)
- Crafting animations or sound effects

## Architecture

Follows the project's sans-I/O pattern. Crafting is a pure function in Rust:
takes a recipe, inventory reference, and item_defs; returns success or a typed
error. The frontend is responsible for displaying recipe availability using the
inventory data it already receives each frame.

Recipe definitions are static data loaded once from JSON, like item_defs and
entity_defs. They cross the IPC boundary once (via `get_recipes` command) and
are cached client-side.

The authoritative craft execution happens server-side via `craft_recipe` IPC
command. The frontend availability check is a convenience for UI display only.

## Data Model

### RecipeDef (`assets/recipes.json` / `item/types.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeDef {
    #[serde(skip_deserializing)]
    pub id: String,
    pub name: String,
    pub description: String,
    pub inputs: Vec<RecipeItem>,
    pub tools: Vec<RecipeItem>,
    pub outputs: Vec<RecipeItem>,
    pub duration_secs: f64,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeItem {
    pub item: String,
    pub count: u32,
}
```

- `inputs`: Items consumed by crafting.
- `tools`: Items required in inventory but NOT consumed. A pot stays in your
  inventory after cooking.
- `outputs`: Items produced by crafting.
- `duration_secs`: Present in data, ignored by execution logic this pass.
  Allows adding timed crafting later without data migration.
- `category`: Informational grouping (e.g. "food", "material"). Not used by
  logic this pass, but available for future UI filtering.

### RecipeDefs type alias

```rust
pub type RecipeDefs = HashMap<String, RecipeDef>;
```

Loaded once at startup via `parse_recipe_defs()`, same pattern as item_defs
and entity_defs.

### New items (`assets/items.json`)

7 new items added:

| id          | name        | category | stack_limit | icon        |
|-------------|-------------|----------|-------------|-------------|
| cherry_pie  | Cherry Pie  | food     | 10          | cherry_pie  |
| bread       | Bread       | food     | 20          | bread       |
| steak       | Steak       | food     | 10          | steak       |
| butter      | Butter      | food     | 20          | butter      |
| bubble_wand | Bubble Wand | tool     | 1           | bubble_wand |
| plank       | Plank       | material | 50          | plank       |
| pot         | Pot         | tool     | 1           | pot         |

### Recipes (`assets/recipes.json`)

6 recipes:

```json
{
  "cherry_pie": {
    "name": "Cherry Pie",
    "description": "A delicious pie.",
    "inputs": [{ "item": "cherry", "count": 5 }, { "item": "grain", "count": 2 }],
    "tools": [{ "item": "pot", "count": 1 }],
    "outputs": [{ "item": "cherry_pie", "count": 1 }],
    "durationSecs": 10.0,
    "category": "food"
  },
  "bread": {
    "name": "Bread",
    "description": "Simple baked bread.",
    "inputs": [{ "item": "grain", "count": 4 }],
    "tools": [{ "item": "pot", "count": 1 }],
    "outputs": [{ "item": "bread", "count": 1 }],
    "durationSecs": 8.0,
    "category": "food"
  },
  "steak": {
    "name": "Steak",
    "description": "Grilled meat on a wood fire.",
    "inputs": [{ "item": "meat", "count": 2 }, { "item": "wood", "count": 1 }],
    "tools": [{ "item": "pot", "count": 1 }],
    "outputs": [{ "item": "steak", "count": 1 }],
    "durationSecs": 12.0,
    "category": "food"
  },
  "butter": {
    "name": "Butter",
    "description": "Churned from fresh milk.",
    "inputs": [{ "item": "milk", "count": 3 }],
    "tools": [{ "item": "pot", "count": 1 }],
    "outputs": [{ "item": "butter", "count": 1 }],
    "durationSecs": 6.0,
    "category": "food"
  },
  "bubble_wand": {
    "name": "Bubble Wand",
    "description": "A wand for blowing bubbles.",
    "inputs": [{ "item": "bubble", "count": 3 }, { "item": "wood", "count": 2 }],
    "tools": [],
    "outputs": [{ "item": "bubble_wand", "count": 1 }],
    "durationSecs": 5.0,
    "category": "tool"
  },
  "plank": {
    "name": "Plank",
    "description": "Processed lumber.",
    "inputs": [{ "item": "wood", "count": 3 }],
    "tools": [],
    "outputs": [{ "item": "plank", "count": 2 }],
    "durationSecs": 4.0,
    "category": "material"
  }
}
```

### Demo street ground items

A pot is placed as a WorldItem on the demo meadow street. This exercises the
ground-item pickup path (as opposed to entity harvest). The placement file is
`assets/streets/demo_meadow_items.json` (new file) or items are added to the
existing entity placement file if ground items share the same loader.

## Crafting Logic

### `craft()` function

New file: `src-tauri/src/item/crafting.rs`

Pure function with this signature:

```rust
pub fn craft(
    recipe: &RecipeDef,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
) -> Result<Vec<CraftOutput>, CraftError>
```

Steps:
1. **Validate tools**: For each tool in recipe.tools, check
   `inventory.count_item(tool.item) >= tool.count`. Fail with
   `CraftError::MissingTool` if not.
2. **Validate inputs**: For each input in recipe.inputs, check
   `inventory.count_item(input.item) >= input.count`. Fail with
   `CraftError::MissingInput` if not.
3. **Validate output room**: For each output in recipe.outputs, check
   `inventory.has_room_for_count(output.item, output.count, item_defs)`. Fail
   with `CraftError::NoRoom` if not. Uses count-aware room check to prevent
   silent item loss from multi-output recipes (e.g. plank x2).
4. **Consume inputs**: For each input, call
   `inventory.remove_item(input.item, input.count)` to remove the required
   count across inventory slots. Tools are NOT removed.
5. **Add outputs**: For each output, add to inventory.
6. **Return outputs**: Return `Vec<CraftOutput>` describing what was produced
   (for feedback display).

The operation is atomic: if any validation step fails, nothing is consumed.

### CraftError

```rust
pub enum CraftError {
    MissingInput { item: String, need: u32, have: u32 },
    MissingTool { item: String },
    NoRoom,
    UnknownRecipe,
}
```

### CraftOutput

```rust
pub struct CraftOutput {
    pub item_id: String,
    pub name: String,
    pub count: u32,
}
```

### Inventory additions

The `Inventory` struct needs two new methods:

```rust
pub fn count_item(&self, item_id: &str) -> u32
```

Returns total count of an item across all inventory slots. Used by craft
validation and by the frontend availability check pattern.

```rust
pub fn remove_item(&mut self, item_id: &str, mut count: u32)
```

Removes `count` items of the given item_id across inventory slots. Iterates
slots, removing from each until the total is satisfied. The existing
`remove(slot, count)` method operates on a single slot by index; this new
method wraps it to remove by item_id across multiple slots. Caller must
verify sufficient quantity exists first (via `count_item`).

### Recipe availability (for frontend)

A helper function computes which recipes are craftable given current inventory:

```rust
pub fn check_recipe_availability(
    recipe: &RecipeDef,
    inventory: &Inventory,
) -> RecipeAvailability
```

Returns:
```rust
pub struct RecipeAvailability {
    pub craftable: bool,
    pub inputs: Vec<IngredientStatus>,
    pub tools: Vec<IngredientStatus>,
}

pub struct IngredientStatus {
    pub item: String,
    pub need: u32,
    pub have: u32,
}
```

This function is NOT called from Rust per-frame. It exists so the frontend can
replicate the same logic in TypeScript for UI display. The TypeScript version
uses the `InventoryFrame` data that already arrives each frame.

## GameState Integration

### New fields on GameState

```rust
pub recipe_defs: RecipeDefs,
```

Loaded at startup alongside item_defs and entity_defs.

### craft_recipe method

```rust
pub fn craft_recipe(&mut self, recipe_id: &str) -> Result<Vec<CraftOutput>, CraftError> {
    let recipe = self.recipe_defs.get(recipe_id)
        .ok_or(CraftError::UnknownRecipe)?;
    let result = craft(recipe, &mut self.inventory, &self.item_defs)?;
    // Generate pickup feedback for each output
    for output in &result {
        self.pickup_feedback.push(PickupFeedback {
            id: self.next_feedback_id(),
            text: format!("+{} x{}", output.name, output.count),
            success: true,
            x: self.player.x,
            y: self.player.y,
            age_secs: 0.0,
        });
    }
    Ok(result)
}
```

### No tick loop changes

Crafting is instant and triggered by IPC command, not by the game tick. No
changes to `tick()`.

## IPC Commands

### get_recipes

```rust
#[tauri::command]
fn get_recipes(state: State<GameStateHandle>) -> Vec<RecipeDef>
```

Returns all recipe definitions. Called once by frontend on game start, cached
client-side. Recipes include the `id` field (set from the map key during
loading, same pattern as EntityDef).

### craft_recipe

```rust
#[tauri::command]
fn craft_recipe(recipe_id: String, state: State<GameStateHandle>) -> Result<(), String>
```

Executes crafting. On success, the next RenderFrame will contain the updated
inventory and any pickup feedback. On failure, returns error string for the
frontend to display.

## Frontend Changes

### Types (`src/lib/types.ts`)

```typescript
export interface RecipeDef {
  id: string;
  name: string;
  description: string;
  inputs: RecipeItem[];
  tools: RecipeItem[];
  outputs: RecipeItem[];
  category: string;
}

export interface RecipeItem {
  item: string;
  count: number;
}
```

### IPC (`src/lib/ipc.ts`)

```typescript
export async function getRecipes(): Promise<RecipeDef[]> { ... }
export async function craftRecipe(recipeId: string): Promise<void> { ... }
```

### InventoryPanel changes

The existing `InventoryPanel.svelte` gains a tabbed interface:

**Tab bar**: Items | Recipes

**Items tab**: Current behavior, unchanged.

**Recipes tab**:
- Lists all recipes, sorted: craftable first, then uncraftable
- Each row: recipe name, output icon placeholder, craft button
- Selecting a recipe shows detail panel:
  - Inputs with counts (green = have enough, red = need more)
  - Tools with status (green = have, red = missing)
  - Outputs
  - "Craft" button (disabled when uncraftable)
- After successful craft, inventory updates automatically via next RenderFrame

**Availability check (frontend)**:
The component computes availability locally using the `InventoryFrame` slots
data and the cached recipe list. For each recipe, it iterates inputs and tools,
summing item counts from inventory slots. This mirrors the Rust
`check_recipe_availability` logic but runs in TypeScript to avoid IPC overhead.

### App.svelte changes

Load recipes on game init (alongside street data):
```typescript
const recipes = await getRecipes();
```

Pass recipes to InventoryPanel as a prop.

### Accessibility

- Tab structure: `role="tablist"`, `role="tab"`, `role="tabpanel"`
- Tab switching: arrow keys, or click
- Recipe list: arrow key navigation, Enter to select
- Craft button: activates on Enter and Space (with `preventDefault` on Space)
- Missing ingredients announced via `aria-label` on each ingredient row
  (e.g. "Cherry: have 3, need 5")
- Focus management: switching tabs moves focus to first item in new tab panel

## Demo Street Item Placement

The pot ground item is placed in the demo meadow. Ground items currently spawn
at runtime (harvest overflow, player drop). To place a pot at street load time,
the street loading code needs to support initial ground items.

Approach: add a `groundItems` array to the existing street entity placement
JSON (`assets/streets/demo_meadow.json` or equivalent). The `load_street()`
method already parses this file for `Vec<WorldEntity>`; extend it to also
parse an optional `groundItems` array as `Vec<WorldItem>` and add them to
the street's ground items list at load time. No separate file needed.

## Testing Strategy

### Rust unit tests (crafting.rs)

- **Craft success**: Inputs consumed, tools remain, outputs added to inventory
- **Missing input**: Not enough of one ingredient → CraftError::MissingInput
  with correct have/need counts, inventory unchanged
- **Missing tool**: Has all ingredients but not the tool → CraftError::MissingTool,
  inventory unchanged
- **No room**: Inventory full → CraftError::NoRoom, nothing consumed
- **Multiple outputs**: Recipe producing 2 planks adds both
- **Tool not consumed**: Tool count unchanged after successful craft
- **Atomic failure**: Partial ingredients available, craft fails, nothing consumed
- **count_item**: Counts items across multiple inventory slots correctly

### Rust unit tests (loader.rs)

- **Recipe loading**: Parse bundled recipes.json, verify fields including tools
- **Recipe item validation**: Verify all recipe item references exist in item_defs
- **Update existing item count test**: The existing `parse_bundled_items_json`
  test asserts `defs.len() == 6`; must update to 13 after adding 7 new items

### Frontend tests

- **Recipe tab**: Tab switch renders recipe list
- **Availability sorting**: Craftable recipes sorted before uncraftable
- **Craft button state**: Disabled when missing ingredients
- **Accessibility**: Tab structure has correct ARIA roles

### Integration

Manual: `npm run tauri dev` — pick up pot from ground, harvest ingredients,
open inventory, switch to Recipes tab, craft cherry pie, verify it appears in
inventory with feedback text.

## Files Modified

### Rust
- `src-tauri/src/item/types.rs` — RecipeDef, RecipeItem, CraftError,
  CraftOutput, RecipeAvailability, IngredientStatus structs
- `src-tauri/src/item/crafting.rs` — **new**, craft() + check_recipe_availability()
- `src-tauri/src/item/inventory.rs` — add count_item() and remove_item() methods
- `src-tauri/src/item/loader.rs` — parse_recipe_defs()
- `src-tauri/src/item/mod.rs` — add crafting module
- `src-tauri/src/engine/state.rs` — add recipe_defs to GameState, craft_recipe
  method, extend load_street for ground items
- `src-tauri/src/lib.rs` — new IPC commands (get_recipes, craft_recipe),
  load recipes at startup

### Frontend
- `src/lib/types.ts` — RecipeDef, RecipeItem interfaces
- `src/lib/components/InventoryPanel.svelte` — tab UI, recipes tab, craft button
- `src/lib/ipc.ts` — getRecipes(), craftRecipe() wrappers
- `src/App.svelte` — load recipes on init, pass to InventoryPanel

### Data
- `assets/recipes.json` — **new**, 6 recipes
- `assets/items.json` — 7 new items (cherry_pie, bread, steak, butter,
  bubble_wand, plank, pot)
- `assets/streets/demo_meadow.json` (or equivalent) — add `groundItems` array
  with pot placement
