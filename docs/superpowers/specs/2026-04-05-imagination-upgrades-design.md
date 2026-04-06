# Imagination (iMG) Upgrade System

**Issue:** harmony-glitch-etl
**Date:** 2026-04-05
**Status:** Approved

## Overview

Imagination (iMG) is a secondary currency earned from productive actions (harvesting, crafting) and spent on permanent player upgrades. It creates the long-term progression loop — players always have something to work toward beyond immediate survival (energy) and commerce (currants).

This bead introduces the iMG currency, two upgrade paths (Energy Tank and Vendor Haggling), and the HUD/panel UI. Future beads add more earning sources and upgrade paths.

## Architecture

Follows the existing Rust-owns-logic / PixiJS-renders / Svelte-does-UI split:

- **Rust:** iMG earning logic, upgrade definitions, purchase validation/execution, haggling discount application
- **Frontend:** iMG HUD pill (Svelte), upgrade panel overlay (Svelte), purple feedback rendering (PixiJS)
- **Data:** Upgrade definitions as Rust consts (game mechanics, not content)

### Progression Loop

1. Player harvests items from entities → earns iMG (1x base_cost per item produced)
2. Player crafts items → earns iMG (2x base_cost per item crafted)
3. Player clicks iMG HUD to open upgrade panel
4. Player spends iMG on Energy Tank (more max energy) or Vendor Haggling (cheaper vendor prices)
5. Upgrades make harvesting/crafting sessions longer (more energy) and food cheaper (haggling)
6. Longer sessions + cheaper food → more harvesting/crafting → more iMG → more upgrades

## Data Model

### Imagination Currency

`imagination: u64` added to `GameState`, `SaveState`, and `RenderFrame`. New players start at 0. Backward-compatible — missing key in save file defaults to 0.

### Player Upgrades

```
pub struct PlayerUpgrades {
    pub energy_tank_tier: u8,  // 0–4, default 0
    pub haggling_tier: u8,     // 0–4, default 0
}
```

Added to `GameState`, `SaveState`, and `RenderFrame`. Backward-compatible — missing key defaults to `PlayerUpgrades { energy_tank_tier: 0, haggling_tier: 0 }`.

### Upgrade Definitions

Rust const arrays in `imagination.rs`. Not data files — these are core game mechanics.

**Energy Tank:**

| Tier | Cost (iMG) | max_energy | Delta |
|------|-----------|-----------|-------|
| 1 | 100 | 650 | +50 |
| 2 | 200 | 725 | +75 |
| 3 | 400 | 825 | +100 |
| 4 | 800 | 950 | +125 |

**Vendor Haggling:**

| Tier | Cost (iMG) | Discount |
|------|-----------|----------|
| 1 | 100 | 5% |
| 2 | 200 | 10% |
| 3 | 400 | 15% |
| 4 | 800 | 20% |

### iMG Earning Constants

```
HARVEST_IMG_MULTIPLIER: u64 = 1
CRAFT_IMG_MULTIPLIER: u64 = 2
```

### Save State

`imagination: u64` and `upgrades: PlayerUpgrades` added to `SaveState` with `#[serde(default)]`. Backward-compatible with existing saves.

## iMG Earning Mechanics

### Earning from Harvest

After the yield loop in `execute_interaction`, compute iMG from all produced items:

```
for yield_entry in &def.yields {
    let count = rng.gen_range(min..=max);  // production count
    // ... existing add/overflow logic ...
    // iMG earned from total produced, NOT just what fit in inventory
    img_earned += base_cost * count * HARVEST_IMG_MULTIPLIER;
}
```

Items without `base_cost` earn 0 iMG. A player with a full inventory still earns iMG from harvesting — the currency comes from the act of production, not from items entering inventory.

`execute_interaction` gains an `imagination: &mut u64` parameter (same pattern as the `energy: &mut f64` parameter). The iMG computation happens inside the function alongside the yield loop, and the caller passes `&mut self.imagination`.

### Earning from Craft

After successful `craft()` in `GameState::craft_recipe`, compute iMG from all outputs:

```
for output in &result {
    img_earned += base_cost * output.count * CRAFT_IMG_MULTIPLIER;
}
```

The 2x crafting multiplier makes crafting the most iMG-efficient activity, reinforcing the crafting loop alongside the existing energy advantage of crafted foods.

### Earning Examples

| Action | Items | base_cost | Multiplier | iMG Earned |
|--------|-------|-----------|-----------|-----------|
| Harvest 3 cherries | cherry x3 | 3 | 1x | 9 |
| Harvest 1 meat | meat x1 | 5 | 1x | 5 |
| Craft 1 bread | bread x1 | 16 | 2x | 32 |
| Craft 2 planks | plank x2 | 12 | 2x | 48 |
| Harvest bubbles | bubble x2 | 2 | 1x | 4 |

### Earning Function

Pure function `earn_imagination(items: &[(item_id, count)], item_defs: &ItemDefs, multiplier: u64) -> u64` that takes produced items and returns iMG amount. The caller (state.rs) adds it to `self.imagination` via `saturating_add`.

## Upgrade Mechanics

### Upgrade Definitions

```rust
pub struct UpgradeTier {
    pub cost: u64,
    pub effect_value: f64,
}

pub struct UpgradePath {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub tiers: [UpgradeTier; 4],
}
```

### Purchase Function

`buy_upgrade(upgrade_id, imagination, upgrades) -> Result<UpgradeEffect, String>`:

Validates:
- `upgrade_id` is a known upgrade path
- Current tier < 4 (not maxed)
- Player has enough iMG for next tier's cost

On success:
- Deducts iMG cost from `imagination`
- Increments the tier
- Returns the upgrade effect (energy delta or discount value) so the caller can apply it

### Energy Tank Effect

On purchase: `max_energy += delta` and `energy += delta`. Both increase so the player feels the upgrade immediately — the bar fills up rather than looking emptier after upgrading.

### Haggling Effect

The `buy()` function in `vendor.rs` gets a new `haggling_discount: f64` parameter (0.0 to 0.20). Discounted price per item:

```
max(1, floor(base_cost as f64 * (1.0 - haggling_discount)) as u32)
```

Minimum price of 1 currant per item — players can't get free items. The caller looks up the discount from `upgrades.haggling_tier` before calling `buy()`.

### IPC Command

`buy_upgrade(upgrade_id: String)` — validates, deducts iMG, applies effect, returns updated state (`{ imagination: u64, upgrades: PlayerUpgradesFrame, energy: f64, maxEnergy: f64 }`). Returns error string on failure.

## Feedback

### PickupFeedback Color Extension

Add optional `color: Option<String>` to `PickupFeedback` (Rust) and `color?: string` to the TypeScript interface. When present, the PixiJS renderer uses it as the fill color instead of the success-based green/red default. When absent, existing behavior is unchanged.

### iMG Earning Feedback

After harvest or craft, emit a `PickupFeedback` with:
- `text: "+9 iMG"` (or whatever amount earned)
- `success: true`
- `color: Some("#c084fc".to_string())` (purple)
- Position: same as the item feedback (entity position for harvest, player position for craft)

Only emitted when iMG earned > 0 (skip for items without base_cost).

### Upgrade Purchase Feedback

After successful upgrade purchase, emit at player position:
- `"+50 max energy!"` with `color: Some("#4ade80".to_string())` (green) for energy tank
- `"Haggling → 10%"` with `color: Some("#fbbf24".to_string())` (amber) for haggling

## Imagination HUD

### Layout

- **Top-right, below currant pill:** Purple pill with `✦` icon and numeric iMG value
- Same semi-transparent dark pill styling as CurrantHud (`rgba(26,26,46,0.85)`)
- `role="status"` live region for accessibility
- `pointer-events: auto` — clickable (unlike currant/energy HUDs)
- Click opens the upgrade panel
- Updates reactively from `RenderFrame.imagination`

### Styling

- Text color: `#c084fc` (purple/violet)
- Icon: `✦` (four-pointed star)
- Format: `✦ 156 iMG`
- Hover: subtle border highlight to indicate clickability

## Upgrade Panel

### Layout

Overlay panel anchored top-right, near the iMG HUD pill.

- `role="dialog"` with `aria-label="Imagination Upgrades"`
- Focus trapped while open
- Closes on Escape or click-outside
- Header: `✦ Imagination` title + current iMG balance
- Two upgrade cards:
  - Card header: icon + upgrade name + "Tier N / 4"
  - Current effect value (e.g., "Max energy: 725")
  - Progress dots (filled for purchased tiers, empty for remaining)
  - Next tier preview (e.g., "Next: max energy → 825")
  - Buy button with iMG cost
- Buy button disabled (grayed) when insufficient iMG
- Buy button replaced with "MAX" badge when tier is 4
- Same dark semi-transparent styling as other game panels

### Data Flow

`RenderFrame.imagination` and `RenderFrame.upgrades` drive the panel reactively. After a purchase IPC call succeeds, the next RenderFrame reflects the updated state.

## Testing

### Rust Tests

**imagination.rs:**
- `earn_from_harvest`: correct iMG for items with base_cost (1x multiplier)
- `earn_from_harvest_no_base_cost`: items without base_cost earn 0 iMG
- `earn_from_craft`: correct iMG for crafted items (2x multiplier)
- `earn_from_craft_multiple_outputs`: sums iMG across all outputs
- `buy_upgrade_success`: deducts iMG, increments tier
- `buy_upgrade_insufficient_img`: rejected, state unchanged
- `buy_upgrade_already_maxed`: rejected at tier 4
- `buy_upgrade_energy_tank_applies_delta`: max_energy and energy both increase by correct delta
- `buy_upgrade_haggling_returns_discount`: correct discount for each tier
- `upgrade_tier_defaults_to_zero`: PlayerUpgrades default is tier 0 for both paths

**vendor.rs:**
- `buy_with_haggling_discount`: price reduced by correct percentage
- `buy_with_haggling_minimum_price`: discounted price never below 1
- `buy_with_zero_haggling`: no discount at tier 0 (unchanged behavior)

**state.rs:**
- `harvest_earns_imagination`: iMG increases after successful harvest
- `craft_earns_imagination`: iMG increases after successful craft
- `imagination_in_save_state_default`: missing field defaults to 0
- `imagination_round_trip`: save/restore preserves imagination + upgrade tiers
- `render_frame_includes_imagination`: imagination + upgrades in RenderFrame

**interaction.rs:**
- `harvest_earns_img_even_when_inventory_full`: iMG earned from full production count, not just items that fit

### Frontend Tests (Vitest)

**ImaginationHud:**
- Renders iMG amount with correct value
- Has `role="status"`
- Clickable — calls onOpen callback

**UpgradePanel:**
- Shows both upgrade paths with correct tier/effect
- Buy button enabled when sufficient iMG
- Buy button disabled when insufficient iMG
- Shows "MAX" when tier is 4
- Calls `buyUpgrade` IPC on click
- Has `role="dialog"` with aria-label
- Closes on Escape key

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Buy upgrade with insufficient iMG | IPC returns error, button already disabled in UI |
| Buy upgrade already maxed | IPC returns error, button replaced with "MAX" badge |
| Unknown upgrade_id in IPC | IPC returns error string |
| Harvest items with no base_cost | 0 iMG earned, no feedback emitted |
| iMG overflow (u64::MAX) | Saturating add — `imagination.saturating_add(earned)` |
| Save file missing imagination field | Defaults to 0 |
| Save file missing upgrades field | Defaults to tier 0 for both paths |
| Haggling discount on 1-currant item | Minimum price of 1, can't get free items |
| Vendor interaction unchanged at tier 0 | 0.0 discount, existing behavior preserved |
| Energy tank upgrade at full energy | Both energy and max_energy increase by delta |

## Out of Scope

- Mood metabolics (harmony-glitch-rj7)
- Inventory expansion upgrade (future upgrade path)
- Additional earning sources (eating, trading, vendor transactions)
- Skill tree / learning system (harmony-glitch-6z6)
- iMG earning from exploration / walking / quests
- Upgrade animations / particle effects
- Upgrade card "discovery" or unlock mechanics
- Diminishing iMG returns on repeat actions
- iMG spending on anything other than upgrades (no iMG-to-currant conversion)
- Multiplayer iMG sync (Phase B concern)
- Drinks that restore energy (same energy_value field works later)

## Follow-Up Beads

- **harmony-glitch-rj7** — Mood metabolics (shares HUD left-side pattern, mood sources from social systems)
- **harmony-glitch-6z6** — Skill tree (could be gated by iMG or separate XP)
- **harmony-glitch-etl follow-ons** — Inventory expansion tier, more earning sources (eating, trading), more upgrade paths, stimulant items
