# Energy Metabolics

**Issue:** harmony-glitch-ajq
**Date:** 2026-04-05
**Status:** Approved

## Overview

Energy as a player metabolic stat that creates the core gameplay pressure loop: actions cost energy, food restores it, vendors sell food for currants. Players must eat to keep playing — harvesting is blocked at zero energy, but movement and vendor interactions remain available so players are never truly stuck.

This is the first metabolics bead. Mood (harmony-glitch-rj7) follows later when social systems exist to provide meaningful mood sources. Energy establishes the tick-based decay pattern, HUD placement, and item consumption mechanic that mood will reuse.

## Architecture

Follows the existing Rust-owns-logic / PixiJS-renders / Svelte-does-UI split:

- **Rust:** Energy state, tick-based decay, harvest energy gate, eat validation/execution
- **Frontend:** Energy HUD bar (Svelte), inventory "Use" button for food items, feedback
- **Data:** `energy_value` field on items in `items.json`

### Gameplay Loop

1. Player harvests items from entities (costs energy)
2. Energy passively decays over time (gentle drain)
3. Player eats food from inventory to restore energy
4. At zero energy, harvesting is blocked ("Too tired")
5. Player can always walk to vendor, buy food, and eat — never stuck
6. Crafted foods restore disproportionately more energy — reinforces crafting profitability

## Data Model

### Energy State

`energy: f64` and `max_energy: f64` added to `GameState`. New players start at 600.0/600.0. Backward-compatible — missing key in save file defaults to 600.0.

Exposed in `RenderFrame` as `energy: f64` and `max_energy: f64` so the frontend always has the current values for the HUD bar.

### Energy on Items

`energy_value: Option<u32>` added to `ItemDef` in `items.json`. Items with an energy value are "usable" (edible). Items without it cannot be eaten.

| Item | Base Cost | Energy Value | Category |
|------|-----------|-------------|----------|
| cherry | 3 | 12 | Food (raw) |
| grain | 3 | 10 | Food (raw) |
| meat | 5 | 20 | Food (raw) |
| milk | 4 | 15 | Food (raw) |
| bread | 16 | 80 | Food (crafted) |
| cherry_pie | 20 | 100 | Food (crafted) |
| steak | 22 | 90 | Food (crafted) |
| butter | 15 | 60 | Food (crafted) |
| bubble | 2 | — | Material |
| wood | 4 | — | Material |
| plank | 12 | — | Crafted material |
| pot | 25 | — | Tool |
| bubble_wand | 18 | — | Crafted tool |

### Energy Constants

```
MAX_ENERGY: 600.0
DEFAULT_ENERGY: 600.0
PASSIVE_DECAY_RATE: 0.1 per second (~6/min, ~100 min idle to empty)
HARVEST_ENERGY_COST: 5.0 per harvest action
```

Passive decay is gentle — about 100 minutes from full to empty while idling. The real drain comes from active play (harvesting costs 5 energy per action).

### Save State

`energy: f64` added to `SaveState` with `#[serde(default = "default_energy")]` returning 600.0. Backward-compatible with existing saves.

## Energy Mechanics

### Passive Decay

Every tick in `state.rs`:
```
energy = max(0.0, energy - PASSIVE_DECAY_RATE * dt)
```

Simple linear drain at 0.1/sec. No acceleration, no thresholds — just a gentle constant trickle.

### Action Costs

When a harvest interaction executes, before yielding items:
1. Check `energy >= HARVEST_ENERGY_COST`
2. If yes: deduct energy, proceed with harvest normally
3. If no: reject interaction, return `InteractionType::Rejected` — frontend shows "Too tired" feedback

The interaction prompt still appears at 0 energy (player sees what they'd interact with), but executing it fails.

Crafting energy costs are out of scope — no crafting action to hook into yet.

### Eating (Energy Restoration)

New IPC command: `eat_item(item_id: String)`

Validates:
- Player has the item in inventory
- Item has `energy_value` defined
- Player energy < max_energy (can't eat at full — don't waste food)

On success:
- Removes 1 of the item from inventory
- Adds `energy_value` to energy, capped at `max_energy`
- Emits pickup feedback: "+80 energy" (green, reusing existing `PickupFeedback` system)
- Returns updated energy and max_energy

On "already full": returns error, frontend shows "Already full" feedback.

### Zero Energy State

At 0 energy:
- Harvesting is blocked ("Too tired")
- Movement still works (player can walk to a vendor to buy food)
- Vendor interactions still work (can buy/sell)
- Inventory still works (can eat food they already have)

The player is never truly stuck — they can always walk to a vendor, buy food, and eat it.

## Inventory "Use" Button

For items with `energy_value`, a "Use" button appears in the inventory panel alongside the item.

### Behavior

- "Use" button visible only on food items (items with `energy_value`)
- Click: eat 1, restoring energy
- Button disabled when energy is full (feedback: "Already full")
- After eating, inventory re-renders with updated count (item disappears if last one eaten)

### Data Flow

`energy_value: Option<u32>` added to inventory slot data in `RenderFrame`. The frontend uses this to decide which items show a "Use" button.

### IPC

`eat_item(item_id: String)` — no entity_id or proximity check needed since this is a self-action from inventory.

Returns `{ energy: f64, max_energy: f64 }` on success, or error string on failure.

## Energy HUD

### Layout

- **Top-left:** Energy bar — green bar with `⚡` icon and numeric value
- **Top-right:** Currant balance (existing CurrantHud, unchanged)

This separates intrinsic resources (energy, future mood) on the left from currencies (currants, future imagination) on the right. Future metabolic stats stack below energy on the left.

### Bar Details

- Green fill bar representing `energy / max_energy`
- Numeric value displayed (e.g., "432")
- Same semi-transparent dark pill styling as CurrantHud (`rgba(26,26,46,0.85)`)
- `role="status"` live region for accessibility
- Color shifts toward amber/red when energy below 150 (25%) as visual warning
- `pointer-events: none` — doesn't intercept game clicks
- Updates reactively from `RenderFrame.energy` and `RenderFrame.max_energy`

## Interaction Changes & Feedback

### Harvest Energy Gate

1. `build_prompt` still shows entity prompt normally at 0 energy (player sees what's there)
2. `execute_interaction` checks `energy >= HARVEST_ENERGY_COST` before proceeding
3. If insufficient: returns `InteractionType::Rejected`, "Too tired" floating text (amber)
4. If sufficient: deducts energy, proceeds with normal harvest

### Eat Feedback

After successful eat via IPC:
- Floating text: "+80 energy" (green, reusing `PickupFeedback` system)
- Energy bar updates reactively from `RenderFrame`

### Low Energy Warning

When energy drops below 150 (25%), the energy bar shifts to amber/red. No popup — the color change is the warning.

## Testing

### Rust Tests

- Energy decay: `energy -= decay_rate * dt` clamps at 0, never negative
- Energy decay: doesn't decay below 0
- Harvest with sufficient energy: deducts energy, yields items
- Harvest with zero energy: returns Rejected, energy unchanged, no items yielded
- Eat item: restores energy, removes item from inventory
- Eat item: capped at max_energy (no overheal)
- Eat item: rejected when energy already full
- Eat item: rejected when item has no energy_value
- Eat item: rejected when item not in inventory
- Eat IPC: returns updated energy/max_energy
- SaveState: energy round-trip serialization
- SaveState: missing energy defaults to 600 (backward compat)
- ItemDef: parses energy_value, defaults to None when missing
- RenderFrame: includes energy and max_energy

### Frontend Tests (Vitest)

- EnergyHud renders bar with correct fill percentage
- EnergyHud shows numeric energy value
- EnergyHud bar turns amber/red at low energy (below 150)
- EnergyHud has accessible role="status"
- Inventory panel shows "Use" button for food items
- Inventory panel hides "Use" button for non-food items
- Inventory "Use" button triggers eat_item IPC
- Inventory "Use" button disabled when energy is full

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Harvest at 0 energy | Interaction rejected, "Too tired" feedback |
| Eat item not in inventory | IPC returns error, frontend shows feedback |
| Eat item with no energy_value | IPC returns error |
| Eat at full energy | IPC returns error, "Already full" feedback |
| Energy overflows max on eat | Capped at max_energy |
| Energy decays below 0 | Clamped at 0.0 |
| Save file missing energy field | Defaults to 600.0 |
| Vendor interaction at 0 energy | Allowed — player can still buy food |
| Movement at 0 energy | Allowed — player can walk to vendor |

## Out of Scope

- Mood metabolics (harmony-glitch-rj7)
- Crafting energy costs (no crafting action to hook into yet)
- Movement/sprinting energy costs (no sprint mechanic yet)
- Stimulant items / no-no powder (future fun)
- Death / Hell / Naraka system (zero energy = blocked, not dead)
- Energy upgrades via imagination (harmony-glitch-etl)
- Drinks that restore energy (can add to items.json later — same `energy_value` field works)
- Energy bar animations (counting up/down) — simple reactive update for now
- Food quality tiers / diminishing returns on repeat eating

## Follow-Up Beads

- **harmony-glitch-rj7** — Mood metabolics (social-driven, shares decay/HUD pattern)
- **harmony-glitch-etl** — Imagination upgrade system (energy capacity upgrades, decay rate modifiers)
- **harmony-glitch-ajq** follow-ons — Crafting energy costs, sprint energy drain, stimulants
