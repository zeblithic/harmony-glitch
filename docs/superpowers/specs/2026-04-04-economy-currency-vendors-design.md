# Economy: Currency & Vendors

**Issue:** glitch-uqw
**Date:** 2026-04-04
**Status:** Approved

## Overview

Currants as the primary in-game currency, vendor NPC entities that buy and sell items, and a wallet HUD. Players earn currants by selling harvested/crafted items to vendors, and spend currants buying items they need. Every item has a `base_cost`; vendors sell at full price and buy back at a store-specific multiplier.

This is the economy foundation bead. Trading, auctions, imagination upgrades, energy/mood, XP/leveling, and other economy layers are tracked as separate beads that build on this.

## Architecture

Follows the existing Rust-owns-logic / PixiJS-renders / Svelte-does-UI split:

- **Rust:** Currency state, store catalog, buy/sell validation, transaction execution
- **Frontend:** Shop panel (Svelte), currant HUD display, transaction feedback
- **Data:** `stores.json` for vendor definitions, `base_cost` field on items in `items.json`

### Gameplay Loop

1. Player harvests items from entities (existing system)
2. Player walks to a vendor entity, interacts to open shop panel
3. Player sells items for currants (vendor pays `base_cost * buy_multiplier`)
4. Player buys items they need at full `base_cost`
5. Crafting is profitable — crafted goods sell for more than raw ingredient cost

## Data Model

### Currency

`currants: u64` added to `GameState` and `SaveState`. New players start with 50 currants. Backward-compatible — missing key in save file defaults to 50.

Exposed in `RenderFrame` so the frontend always has the current balance.

### Item Prices

`base_cost: Option<u32>` added to `ItemDef` in `items.json`. Items without a `base_cost` cannot be bought or sold.

| Item | Base Cost | Category | Notes |
|------|-----------|----------|-------|
| cherry | 3 | Food | Raw, easy to harvest |
| grain | 3 | Food | Raw, easy to harvest |
| meat | 5 | Food | Pig, 8s cooldown |
| milk | 4 | Food | Butterfly, single harvest |
| bubble | 2 | Material | Abundant (1-4 per harvest) |
| wood | 4 | Material | Moderate harvest |
| plank | 12 | Crafted | 3 wood = 12 raw cost |
| cherry_pie | 20 | Crafted | 5 cherry + 2 grain + pot |
| bread | 16 | Crafted | 4 grain + pot |
| steak | 22 | Crafted | 2 meat + 1 wood + pot |
| butter | 15 | Crafted | 3 milk + pot |
| bubble_wand | 18 | Crafted | 3 bubble + 2 wood |
| pot | 25 | Tool | Not consumed by crafting |

Crafting is always profitable: ingredient raw cost < crafted item sell price. The pot (25 currants) is the initial investment gate.

### Vendor Entity Definition

`store: Option<String>` added to `EntityDef`. An entity is a vendor if `store.is_some()`. The interaction system checks this and routes to vendor behavior instead of harvest behavior.

Entity definition example:

```json
{
    "vendor_grocery": {
        "name": "Grocery Vendor",
        "verb": "Shop",
        "yields": [],
        "cooldownSecs": 0,
        "maxHarvests": 0,
        "respawnSecs": 0,
        "spriteClass": "vendor",
        "interactRadius": 100,
        "store": "grocery"
    }
}
```

### Store Catalog (`assets/stores.json`)

```json
{
    "grocery": {
        "name": "Grocery Vendor",
        "buyMultiplier": 0.66,
        "inventory": ["cherry", "grain", "meat", "milk"]
    },
    "hardware": {
        "name": "Hardware Vendor",
        "buyMultiplier": 0.50,
        "inventory": ["wood", "plank", "pot", "bubble_wand"]
    }
}
```

- `inventory`: items the vendor sells at full `base_cost`
- `buyMultiplier`: fraction of `base_cost` the vendor pays when buying from the player
- Vendors buy **any item with a base_cost**, not just items in their inventory

### Rust Types

```rust
pub struct StoreDef {
    pub name: String,
    pub buy_multiplier: f64,
    pub inventory: Vec<String>,
}

pub struct StoreCatalog {
    pub stores: HashMap<String, StoreDef>,
}
```

Loaded at startup. Rust validates that every `store` ID referenced by entity defs exists in the catalog. Missing stores log a warning and the entity is not treated as a vendor.

## Interaction Flow

1. `proximity_scan` finds vendor entity within `interact_radius`
2. `build_prompt` returns prompt with verb "Shop", `entity_id` set
3. Player interacts → `execute_interaction` returns `InteractionType::Vendor { entity_id }`
4. Rust emits `AudioEvent::EntityInteract { entity_type: "vendor" }`
5. Frontend detects vendor interact, calls `get_store_state(entity_id)` IPC
6. Shop panel opens

Panel closes on interact-again or walking out of `interact_radius` (same 2-frame debounce as jukebox).

## IPC Commands (Tauri)

```rust
get_store_state(entity_id: String) -> StoreState
```

Returns store name, vendor inventory with buy prices, player's sellable inventory with sell prices, and current currant balance.

```rust
vendor_buy(entity_id: String, item_id: String, count: u32)
```

Player buys items from the vendor. Validates: proximity, item in vendor inventory, item has base_cost, player has enough currants, player has inventory room. Deducts currants, adds items to inventory. Returns updated currant balance.

```rust
vendor_sell(entity_id: String, item_id: String, count: u32)
```

Player sells items to the vendor. Validates: proximity, item has base_cost, player has enough of the item. Removes items from inventory, adds currants (`floor(base_cost * buy_multiplier)`, minimum 1). Returns updated currant balance.

### Proximity Validation

Rename `validate_jukebox_proximity` to `validate_entity_proximity` — it's now used by both jukeboxes and vendors. Update existing jukebox IPC calls to use the renamed function. Same 2D Euclidean distance check against `interact_radius`, default 60.0.

### Sell Price Calculation

```
sell_price = max(1, floor(base_cost * buy_multiplier))
```

Minimum sell price is 1 currant — no item is worthless. Rounding uses floor to keep the economy slightly sink-biased.

## Transaction Feedback

After a successful buy or sell, a floating text appears near the player using the existing `PickupFeedback` system:

- **Buy:** "-3 currants" (amber/gold color)
- **Sell:** "+2 currants" (green color)

This reuses the pickup feedback infrastructure. Add a `CurrencyFeedback` variant to the existing feedback system, carrying the currant amount and whether it's a gain or loss. The frontend renders it with the same floating text animation as item pickup, just with different colors.

## Shop Panel UI

`ShopPanel.svelte` — opens when player interacts with a vendor, closes on interact-again or walking out of `interact_radius`.

### Data

```typescript
interface StoreState {
    entityId: string;
    name: string;
    vendorInventory: StoreItem[];
    playerInventory: SellableItem[];
    currants: number;
}

interface StoreItem {
    itemId: string;
    name: string;
    baseCost: number;
}

interface SellableItem {
    itemId: string;
    name: string;
    count: number;
    sellPrice: number;
}
```

Initial state fetched via `get_store_state(entity_id)`. Panel refreshes after each buy/sell by re-fetching `get_store_state` — this is simpler than reactive updates and matches the jukebox panel's IPC-fetch pattern.

### Layout

```
+-- Grocery Vendor ------- 50c -- x -+
|                                      |
|  [ Buy ]  [ Sell ]                   |
|                                      |
|  Cherry                    3c  [Buy] |
|  Grain                     3c  [Buy] |
|  Meat                      5c  [Buy] |
|  Milk                      4c  [Buy] |
|                                      |
|  Click = 1 · Shift+click = stack     |
+--------------------------------------+
```

```
+-- Grocery Vendor ------ 74c --- x -+
|                                      |
|  [ Buy ]  [ Sell ]                   |
|                                      |
|  Cherry ×12            2c ea  [Sell] |
|  Cherry Pie ×2        13c ea  [Sell] |
|  Wood ×8               2c ea  [Sell] |
|  Pot ×1               16c ea  [Sell] |
|                                      |
|  Click = 1 · Shift+click = stack     |
+--------------------------------------+
```

- Two tabs: Buy and Sell
- Currant balance always visible in header
- Buy tab: vendor's curated inventory at `base_cost`, Buy button per item
- Sell tab: player's inventory items that have `base_cost`, with sell price and Sell button
- Click = buy/sell 1, Shift+click = buy/sell full stack (or as many as affordable/available)
- Buy button disabled when insufficient currants or inventory full
- Empty sell tab shows "No items to sell"

### Accessibility

- `<dialog>` with `aria-label="Shop: {name}"`
- Tab bar: `role="tablist"` with `role="tab"` buttons, `aria-selected`
- Item lists: `<ul>` with `<li>` per item, Buy/Sell as native `<button>` with `aria-label`
- Focus trapped within dialog while open, returns to game on close

## Currant Balance HUD

Small always-visible display showing current currant balance. Positioned in the game HUD area (implementation determines exact placement — likely top-right or near existing UI elements).

Updates reactively from `RenderFrame.currants`. Brief animation (number counting up/down) on change for satisfying feedback.

## Entity Placement

- `vendor_grocery` placed on `demo_meadow_entities.json` — same street as fruit_tree, chicken, bubble_tree
- `vendor_hardware` placed on `demo_heights_entities.json` — same street as wood_tree, pig

## Testing

### Rust Tests

- StoreDef/StoreCatalog deserialization round-trip
- ItemDef with base_cost parses correctly, without base_cost defaults to None
- Buy: deducts currants, adds item to inventory
- Buy: rejected when insufficient currants
- Buy: rejected when inventory full
- Buy: rejected when item not in vendor inventory
- Sell: adds currants, removes item from inventory
- Sell: rejected when player doesn't have the item
- Sell: price calculation rounds correctly (`floor(base_cost * multiplier)`)
- Sell: minimum price is 1 currant
- Sell: rejected when item has no base_cost
- Vendor interaction routes to vendor behavior (not harvest)
- Vendor IPC commands validate proximity
- SaveState round-trip with currants (backward-compatible default of 50)
- Unknown store ID in entity def logs warning, entity not treated as vendor

### Frontend Tests (Vitest)

- ShopPanel renders Buy tab with vendor inventory and prices
- ShopPanel renders Sell tab with player inventory and sell prices
- ShopPanel Buy/Sell buttons trigger correct IPC calls
- ShopPanel disables Buy when insufficient currants or inventory full
- ShopPanel shows empty state on sell tab with no sellable items
- Currant balance displays in HUD and updates reactively
- Currant balance displays in shop panel header

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Not enough currants to buy | IPC returns error, frontend shows feedback |
| Inventory full on buy | IPC returns error, frontend shows "Inventory full" |
| Item not in player inventory on sell | IPC rejects (race condition guard) |
| Item has no base_cost | Not shown in sell tab, excluded from vendor inventory validation |
| Unknown store ID on entity | Logged at street load, entity treated as non-vendor |
| Player out of interact_radius | IPC validates proximity, rejects |
| Sell price rounds to 0 | Minimum sell price of 1 currant |

## Out of Scope

- Imagination upgrades / haggling discounts (glitch-etl)
- Player-to-player trading (glitch-1d6)
- Auction house (glitch-ott)
- Energy/mood costs for actions (glitch-ajq)
- XP/leveling system (glitch-rqv)
- Skill-gated recipes (glitch-6z6)
- Giant favor/emblems (glitch-zp0)
- Vendor restock limits or dynamic pricing
- Quantity input dialog (click/shift-click is sufficient)
- Item tooltips in shop panel (just name + price for now)
- Premium/real-money currency (credits)

## Follow-Up Beads

- **glitch-ajq** — Energy & Mood metabolics (actions cost energy, food restores it)
- **glitch-1d6** — Player-to-player trading (escrow-based direct trades)
- **glitch-ott** — Auction house / marketplace (player-set prices, listing fees)
- **glitch-etl** — Imagination upgrade system (permanent bonuses, vendor haggling)
- **glitch-zp0** — Giant favor & emblem reputation
- **glitch-rqv** — XP & leveling system
- **glitch-6z6** — Skill tree & learning system
