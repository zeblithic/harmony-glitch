# Imagination (iMG) Upgrade System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add imagination (iMG) as a secondary currency earned from harvesting and crafting, spent on permanent upgrades (energy tank expansion and vendor haggling discounts).

**Architecture:** Rust owns all iMG state and logic (earning, spending, upgrade definitions). Svelte handles the iMG HUD pill and upgrade panel overlay. PickupFeedback gets an optional color field for purple iMG feedback. Vendor buy function gains a haggling discount parameter.

**Tech Stack:** Rust (Tauri v2), Svelte 5 (runes), TypeScript, PixiJS v8

**Spec:** `docs/superpowers/specs/2026-04-05-imagination-upgrades-design.md`

---

## File Structure

### New Files
- `src-tauri/src/item/imagination.rs` — Upgrade definitions, earn/buy logic, PlayerUpgrades type
- `src/lib/components/ImaginationHud.svelte` — iMG pill HUD (clickable)
- `src/lib/components/ImaginationHud.test.ts` — Vitest tests for HUD
- `src/lib/components/UpgradePanel.svelte` — Upgrade panel overlay
- `src/lib/components/UpgradePanel.test.ts` — Vitest tests for panel

### Modified Files
- `src-tauri/src/item/mod.rs` — Add `pub mod imagination;`
- `src-tauri/src/item/types.rs` — Add optional `color` field to `PickupFeedback`
- `src-tauri/src/item/interaction.rs` — Add `imagination: &mut u64` param, earn iMG from harvest yields
- `src-tauri/src/item/vendor.rs` — Add `haggling_discount: f64` param to `buy()`
- `src-tauri/src/engine/state.rs` — Add iMG + upgrades to GameState/SaveState/RenderFrame, earn iMG from craft, wire up haggling
- `src-tauri/src/lib.rs` — Add `buy_upgrade` IPC command, pass haggling to vendor_buy
- `src/lib/types.ts` — Add iMG + upgrades to RenderFrame, color to PickupFeedback, BuyUpgradeResult type
- `src/lib/ipc.ts` — Add `buyUpgrade()` function
- `src/lib/engine/renderer.ts` — Use optional color field for feedback text
- `src/App.svelte` — Add ImaginationHud + UpgradePanel, pass props

---

### Task 1: Imagination Module — Types, Upgrade Definitions, and Earn/Buy Logic

**Files:**
- Create: `src-tauri/src/item/imagination.rs`
- Modify: `src-tauri/src/item/mod.rs`

- [ ] **Step 1: Write the failing tests for iMG earning**

Create `src-tauri/src/item/imagination.rs`:

```rust
use crate::item::types::ItemDefs;
use serde::{Deserialize, Serialize};

/// Multiplier applied to base_cost when earning iMG from harvesting.
const HARVEST_IMG_MULTIPLIER: u64 = 1;
/// Multiplier applied to base_cost when earning iMG from crafting.
const CRAFT_IMG_MULTIPLIER: u64 = 2;

/// Per-player upgrade state. Each field is the current tier (0 = no upgrade, 4 = max).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerUpgrades {
    #[serde(default)]
    pub energy_tank_tier: u8,
    #[serde(default)]
    pub haggling_tier: u8,
}

/// A single tier within an upgrade path.
pub struct UpgradeTier {
    pub cost: u64,
    pub effect_value: f64,
}

/// An upgrade path with 4 tiers.
pub struct UpgradePath {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub tiers: [UpgradeTier; 4],
}

/// Effect returned after a successful upgrade purchase.
pub enum UpgradeEffect {
    /// Energy tank: delta to add to both max_energy and energy.
    EnergyTankDelta(f64),
    /// Haggling: new discount percentage (e.g. 0.10 for 10%).
    HagglingDiscount(f64),
}

pub const ENERGY_TANK: UpgradePath = UpgradePath {
    id: "energy_tank",
    name: "Energy Tank",
    description: "Increases maximum energy capacity",
    tiers: [
        UpgradeTier { cost: 100, effect_value: 50.0 },   // max_energy → 650
        UpgradeTier { cost: 200, effect_value: 75.0 },   // max_energy → 725
        UpgradeTier { cost: 400, effect_value: 100.0 },  // max_energy → 825
        UpgradeTier { cost: 800, effect_value: 125.0 },  // max_energy → 950
    ],
};

pub const HAGGLING: UpgradePath = UpgradePath {
    id: "haggling",
    name: "Vendor Haggling",
    description: "Reduces vendor buy prices",
    tiers: [
        UpgradeTier { cost: 100, effect_value: 0.05 },  // 5% discount
        UpgradeTier { cost: 200, effect_value: 0.10 },  // 10% discount
        UpgradeTier { cost: 400, effect_value: 0.15 },  // 15% discount
        UpgradeTier { cost: 800, effect_value: 0.20 },  // 20% discount
    ],
};

/// Compute iMG earned from producing items.
///
/// `items` is a list of (item_id, count) pairs representing what was produced.
/// `multiplier` is HARVEST_IMG_MULTIPLIER or CRAFT_IMG_MULTIPLIER.
pub fn earn_imagination(
    items: &[(&str, u32)],
    item_defs: &ItemDefs,
    multiplier: u64,
) -> u64 {
    let mut total: u64 = 0;
    for (item_id, count) in items {
        if let Some(def) = item_defs.get(*item_id) {
            if let Some(base_cost) = def.base_cost {
                total = total.saturating_add(
                    (base_cost as u64)
                        .saturating_mul(*count as u64)
                        .saturating_mul(multiplier),
                );
            }
        }
    }
    total
}

/// Convenience wrapper for harvest earning (1x multiplier).
pub fn earn_from_harvest(items: &[(&str, u32)], item_defs: &ItemDefs) -> u64 {
    earn_imagination(items, item_defs, HARVEST_IMG_MULTIPLIER)
}

/// Convenience wrapper for craft earning (2x multiplier).
pub fn earn_from_craft(items: &[(&str, u32)], item_defs: &ItemDefs) -> u64 {
    earn_imagination(items, item_defs, CRAFT_IMG_MULTIPLIER)
}

/// Look up the current haggling discount for a given tier (0-4).
/// Tier 0 returns 0.0 (no discount).
pub fn haggling_discount(tier: u8) -> f64 {
    if tier == 0 || tier > 4 {
        0.0
    } else {
        HAGGLING.tiers[(tier - 1) as usize].effect_value
    }
}

/// Attempt to purchase the next tier of an upgrade.
///
/// Returns the upgrade effect on success, or an error string on failure.
pub fn buy_upgrade(
    upgrade_id: &str,
    imagination: &mut u64,
    upgrades: &mut PlayerUpgrades,
) -> Result<UpgradeEffect, String> {
    match upgrade_id {
        "energy_tank" => {
            let tier = upgrades.energy_tank_tier;
            if tier >= 4 {
                return Err("Energy Tank is already at max tier".to_string());
            }
            let next = &ENERGY_TANK.tiers[tier as usize];
            if *imagination < next.cost {
                return Err(format!(
                    "Not enough iMG (need {}, have {})",
                    next.cost, *imagination
                ));
            }
            *imagination -= next.cost;
            upgrades.energy_tank_tier += 1;
            Ok(UpgradeEffect::EnergyTankDelta(next.effect_value))
        }
        "haggling" => {
            let tier = upgrades.haggling_tier;
            if tier >= 4 {
                return Err("Vendor Haggling is already at max tier".to_string());
            }
            let next = &HAGGLING.tiers[tier as usize];
            if *imagination < next.cost {
                return Err(format!(
                    "Not enough iMG (need {}, have {})",
                    next.cost, *imagination
                ));
            }
            *imagination -= next.cost;
            upgrades.haggling_tier += 1;
            Ok(UpgradeEffect::HagglingDiscount(next.effect_value))
        }
        _ => Err(format!("Unknown upgrade: '{upgrade_id}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;
    use std::collections::HashMap;

    fn test_item_defs() -> ItemDefs {
        let mut defs = HashMap::new();
        defs.insert(
            "cherry".to_string(),
            ItemDef {
                id: "cherry".to_string(),
                name: "Cherry".to_string(),
                description: "A cherry.".to_string(),
                category: "food".to_string(),
                stack_limit: 50,
                icon: "cherry".to_string(),
                base_cost: Some(3),
                energy_value: Some(12),
            },
        );
        defs.insert(
            "bread".to_string(),
            ItemDef {
                id: "bread".to_string(),
                name: "Bread".to_string(),
                description: "A loaf of bread.".to_string(),
                category: "food".to_string(),
                stack_limit: 50,
                icon: "bread".to_string(),
                base_cost: Some(16),
                energy_value: Some(80),
            },
        );
        defs.insert(
            "pot".to_string(),
            ItemDef {
                id: "pot".to_string(),
                name: "Pot".to_string(),
                description: "A cooking pot.".to_string(),
                category: "tool".to_string(),
                stack_limit: 1,
                icon: "pot".to_string(),
                base_cost: None,
                energy_value: None,
            },
        );
        defs
    }

    #[test]
    fn earn_from_harvest_uses_base_cost() {
        let defs = test_item_defs();
        // 3 cherries at base_cost=3, 1x multiplier = 9
        let earned = earn_from_harvest(&[("cherry", 3)], &defs);
        assert_eq!(earned, 9);
    }

    #[test]
    fn earn_from_harvest_no_base_cost_earns_zero() {
        let defs = test_item_defs();
        // pot has no base_cost
        let earned = earn_from_harvest(&[("pot", 1)], &defs);
        assert_eq!(earned, 0);
    }

    #[test]
    fn earn_from_craft_uses_double_multiplier() {
        let defs = test_item_defs();
        // 1 bread at base_cost=16, 2x multiplier = 32
        let earned = earn_from_craft(&[("bread", 1)], &defs);
        assert_eq!(earned, 32);
    }

    #[test]
    fn earn_from_craft_multiple_outputs() {
        let defs = test_item_defs();
        // 2 cherries (3*2*2=12) + 1 bread (16*1*2=32) = 44
        let earned = earn_from_craft(&[("cherry", 2), ("bread", 1)], &defs);
        assert_eq!(earned, 44);
    }

    #[test]
    fn earn_ignores_unknown_items() {
        let defs = test_item_defs();
        let earned = earn_from_harvest(&[("nonexistent", 5)], &defs);
        assert_eq!(earned, 0);
    }

    #[test]
    fn buy_upgrade_energy_tank_success() {
        let mut img = 500;
        let mut upgrades = PlayerUpgrades::default();
        let result = buy_upgrade("energy_tank", &mut img, &mut upgrades);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), UpgradeEffect::EnergyTankDelta(d) if (d - 50.0).abs() < 0.01));
        assert_eq!(img, 400); // 500 - 100
        assert_eq!(upgrades.energy_tank_tier, 1);
    }

    #[test]
    fn buy_upgrade_insufficient_img() {
        let mut img = 50;
        let mut upgrades = PlayerUpgrades::default();
        let result = buy_upgrade("energy_tank", &mut img, &mut upgrades);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Not enough iMG"));
        assert_eq!(img, 50); // unchanged
        assert_eq!(upgrades.energy_tank_tier, 0); // unchanged
    }

    #[test]
    fn buy_upgrade_already_maxed() {
        let mut img = 10000;
        let mut upgrades = PlayerUpgrades {
            energy_tank_tier: 4,
            haggling_tier: 0,
        };
        let result = buy_upgrade("energy_tank", &mut img, &mut upgrades);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("max tier"));
        assert_eq!(img, 10000); // unchanged
    }

    #[test]
    fn buy_upgrade_haggling_success() {
        let mut img = 300;
        let mut upgrades = PlayerUpgrades::default();
        // Buy tier 1
        let result = buy_upgrade("haggling", &mut img, &mut upgrades);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), UpgradeEffect::HagglingDiscount(d) if (d - 0.05).abs() < 0.001));
        assert_eq!(img, 200); // 300 - 100
        assert_eq!(upgrades.haggling_tier, 1);
    }

    #[test]
    fn buy_upgrade_energy_tank_escalating_costs() {
        let mut img = 1500;
        let mut upgrades = PlayerUpgrades::default();
        // Tier 1: cost 100, delta 50
        let r = buy_upgrade("energy_tank", &mut img, &mut upgrades).unwrap();
        assert!(matches!(r, UpgradeEffect::EnergyTankDelta(d) if (d - 50.0).abs() < 0.01));
        assert_eq!(img, 1400);
        // Tier 2: cost 200, delta 75
        let r = buy_upgrade("energy_tank", &mut img, &mut upgrades).unwrap();
        assert!(matches!(r, UpgradeEffect::EnergyTankDelta(d) if (d - 75.0).abs() < 0.01));
        assert_eq!(img, 1200);
        // Tier 3: cost 400, delta 100
        let r = buy_upgrade("energy_tank", &mut img, &mut upgrades).unwrap();
        assert!(matches!(r, UpgradeEffect::EnergyTankDelta(d) if (d - 100.0).abs() < 0.01));
        assert_eq!(img, 800);
        // Tier 4: cost 800, delta 125
        let r = buy_upgrade("energy_tank", &mut img, &mut upgrades).unwrap();
        assert!(matches!(r, UpgradeEffect::EnergyTankDelta(d) if (d - 125.0).abs() < 0.01));
        assert_eq!(img, 0);
        assert_eq!(upgrades.energy_tank_tier, 4);
    }

    #[test]
    fn buy_upgrade_unknown_id() {
        let mut img = 1000;
        let mut upgrades = PlayerUpgrades::default();
        let result = buy_upgrade("nonexistent", &mut img, &mut upgrades);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown upgrade"));
    }

    #[test]
    fn haggling_discount_by_tier() {
        assert!((haggling_discount(0) - 0.0).abs() < 0.001);
        assert!((haggling_discount(1) - 0.05).abs() < 0.001);
        assert!((haggling_discount(2) - 0.10).abs() < 0.001);
        assert!((haggling_discount(3) - 0.15).abs() < 0.001);
        assert!((haggling_discount(4) - 0.20).abs() < 0.001);
        assert!((haggling_discount(5) - 0.0).abs() < 0.001); // out of range
    }

    #[test]
    fn upgrade_tier_defaults_to_zero() {
        let upgrades = PlayerUpgrades::default();
        assert_eq!(upgrades.energy_tank_tier, 0);
        assert_eq!(upgrades.haggling_tier, 0);
    }

    #[test]
    fn player_upgrades_serde_round_trip() {
        let upgrades = PlayerUpgrades {
            energy_tank_tier: 2,
            haggling_tier: 3,
        };
        let json = serde_json::to_string(&upgrades).unwrap();
        let parsed: PlayerUpgrades = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.energy_tank_tier, 2);
        assert_eq!(parsed.haggling_tier, 3);
    }

    #[test]
    fn player_upgrades_deserialize_missing_fields() {
        let json = "{}";
        let parsed: PlayerUpgrades = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.energy_tank_tier, 0);
        assert_eq!(parsed.haggling_tier, 0);
    }
}
```

- [ ] **Step 2: Register the module**

Add to `src-tauri/src/item/mod.rs`:

```rust
pub mod imagination;
```

(Add after the existing `pub mod energy;` line.)

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd src-tauri && cargo test item::imagination`
Expected: All 14 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/item/imagination.rs src-tauri/src/item/mod.rs
git commit -m "feat(imagination): add iMG types, upgrade definitions, earn/buy logic with tests"
```

---

### Task 2: PickupFeedback Color Extension

**Files:**
- Modify: `src-tauri/src/item/types.rs:289-299`
- Modify: `src/lib/types.ts:239-246`
- Modify: `src/lib/engine/renderer.ts:466-469`

- [ ] **Step 1: Add optional color field to Rust PickupFeedback**

In `src-tauri/src/item/types.rs`, change `PickupFeedback`:

```rust
/// Floating feedback text after pickup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickupFeedback {
    pub id: u64,
    pub text: String,
    pub success: bool,
    pub x: f64,
    pub y: f64,
    pub age_secs: f64,
    /// Optional hex color override (e.g. "#c084fc"). When present,
    /// the renderer uses this instead of the success-based green/red.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}
```

- [ ] **Step 2: Fix all existing PickupFeedback instantiations**

Every existing `PickupFeedback { ... }` construction must add `color: None`. Search the codebase for all sites:

Files that construct `PickupFeedback`:
- `src-tauri/src/item/interaction.rs` — multiple sites in `execute_interaction`
- `src-tauri/src/engine/state.rs` — `craft_recipe`, `tick` (various feedback sites)
- `src-tauri/src/lib.rs` — `vendor_buy`, `vendor_sell`, `eat_item`

Add `color: None,` to every `PickupFeedback { ... }` block in all these files.

- [ ] **Step 3: Add color field to TypeScript PickupFeedback**

In `src/lib/types.ts`, change:

```typescript
export interface PickupFeedback {
  id: number;
  text: string;
  success: boolean;
  x: number;
  y: number;
  ageSecs: number;
  color?: string;
}
```

- [ ] **Step 4: Use color in the PixiJS renderer**

In `src/lib/engine/renderer.ts`, change the feedback text creation (around line 466-468):

```typescript
        if (!existing) {
          const fillColor = fb.color
            ? parseInt(fb.color.replace('#', ''), 16)
            : fb.success ? 0x7ae87a : 0xe87a7a;
          const text = new Text({
            text: fb.text,
            style: { fontSize: 14, fill: fillColor },
          });
```

- [ ] **Step 5: Run tests to verify nothing broke**

Run: `cd src-tauri && cargo test`
Run: `npx vitest run`
Expected: All existing tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/item/types.rs src-tauri/src/item/interaction.rs src-tauri/src/engine/state.rs src-tauri/src/lib.rs src/lib/types.ts src/lib/engine/renderer.ts
git commit -m "feat(feedback): add optional color field to PickupFeedback for custom feedback colors"
```

---

### Task 3: Vendor Haggling Discount

**Files:**
- Modify: `src-tauri/src/item/vendor.rs:24-53`

- [ ] **Step 1: Write the failing tests**

Add these tests at the end of the `mod tests` block in `src-tauri/src/item/vendor.rs`:

```rust
    #[test]
    fn buy_with_haggling_discount() {
        let defs = test_item_defs();
        let store = test_store();
        let mut inv = Inventory::new(10);
        // Cherry base_cost=3, 10% discount → floor(3 * 0.90) = floor(2.7) = 2
        // Buy 5 cherries at 2c each = 10. 50 - 10 = 40
        let result = buy("cherry", 5, 50, &mut inv, &defs, &store, 0.10);
        assert_eq!(result, Ok(40));
        assert_eq!(inv.count_item("cherry"), 5);
    }

    #[test]
    fn buy_with_haggling_minimum_price() {
        // Even with 20% discount, minimum price is 1
        let mut defs = test_item_defs();
        defs.insert(
            "cheap".to_string(),
            ItemDef {
                id: "cheap".to_string(),
                name: "Cheap".to_string(),
                description: "Very cheap.".to_string(),
                category: "misc".to_string(),
                stack_limit: 50,
                icon: "cheap".to_string(),
                base_cost: Some(1),
                energy_value: None,
            },
        );
        let store = StoreDef {
            name: "Store".to_string(),
            buy_multiplier: 0.5,
            inventory: vec!["cheap".to_string()],
        };
        let mut inv = Inventory::new(10);
        // base_cost=1, 20% discount → floor(1 * 0.80) = 0 → clamped to 1
        let result = buy("cheap", 1, 50, &mut inv, &defs, &store, 0.20);
        assert_eq!(result, Ok(49));
    }

    #[test]
    fn buy_with_zero_haggling() {
        let defs = test_item_defs();
        let store = test_store();
        let mut inv = Inventory::new(10);
        // 0% discount = original behavior: cherry at base_cost=3
        let result = buy("cherry", 5, 50, &mut inv, &defs, &store, 0.0);
        assert_eq!(result, Ok(35));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test item::vendor`
Expected: FAIL — `buy` doesn't accept 7th argument yet.

- [ ] **Step 3: Add haggling_discount parameter to buy()**

In `src-tauri/src/item/vendor.rs`, change the `buy` function signature and price calculation:

```rust
pub fn buy(
    item_id: &str,
    count: u32,
    currants: u64,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
    store: &StoreDef,
    haggling_discount: f64,
) -> Result<u64, String> {
    // Item must be in vendor's inventory
    if !store.inventory.iter().any(|id| id == item_id) {
        return Err(format!("Item '{item_id}' is not sold here"));
    }

    // Item must have a base_cost
    let def = item_defs
        .get(item_id)
        .ok_or_else(|| format!("Unknown item '{item_id}'"))?;
    let base_cost = def
        .base_cost
        .ok_or_else(|| format!("Item '{item_id}' has no price"))?;

    // Apply haggling discount, minimum price of 1
    let unit_price = if haggling_discount > 0.0 {
        ((base_cost as f64) * (1.0 - haggling_discount)).floor().max(1.0) as u32
    } else {
        base_cost
    };

    // Total cost must fit in u64 and player must have enough
    let total_cost = (unit_price as u64)
        .checked_mul(count as u64)
        .ok_or_else(|| "Cost overflow".to_string())?;
    if currants < total_cost {
        return Err(format!(
            "Not enough currants (need {total_cost}, have {currants})"
        ));
    }
```

(The rest of the function — inventory add and return — remains unchanged.)

- [ ] **Step 4: Fix all existing buy() call sites to pass 0.0**

Update all existing calls to `buy()` to pass `0.0` as the last argument:

In `src-tauri/src/item/vendor.rs` tests — update `buy_success`, `buy_insufficient_currants`, `buy_inventory_full`, `buy_item_not_in_vendor_inventory` to pass `0.0`:

```rust
    // Example for buy_success — same pattern for all:
    let result = buy("cherry", 5, 50, &mut inv, &defs, &store, 0.0);
```

In `src-tauri/src/lib.rs` `vendor_buy` command — the call to `item::vendor::buy(...)` needs `0.0` temporarily (will be replaced with actual haggling in Task 6):

```rust
    let new_balance = item::vendor::buy(&item_id, count, currants, &mut state.inventory, &item_defs, &store, 0.0)?;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd src-tauri && cargo test item::vendor`
Expected: All tests pass including the 3 new haggling tests.

Run: `cd src-tauri && cargo test`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/item/vendor.rs src-tauri/src/lib.rs
git commit -m "feat(vendor): add haggling discount parameter to buy() with minimum price of 1"
```

---

### Task 4: Imagination in GameState, SaveState, and RenderFrame

**Files:**
- Modify: `src-tauri/src/engine/state.rs`

- [ ] **Step 1: Add imports and default functions**

At the top of `src-tauri/src/engine/state.rs`, add the import:

```rust
use crate::item::imagination::PlayerUpgrades;
```

(Add after the existing `use crate::item::interaction;` line.)

Add default functions near the existing `default_energy()`:

```rust
fn default_imagination() -> u64 {
    0
}

fn default_upgrades() -> PlayerUpgrades {
    PlayerUpgrades::default()
}
```

- [ ] **Step 2: Add fields to SaveState**

In the `SaveState` struct, add after `last_trade_id`:

```rust
    #[serde(default = "default_imagination")]
    pub imagination: u64,
    #[serde(default = "default_upgrades")]
    pub upgrades: PlayerUpgrades,
```

- [ ] **Step 3: Add fields to GameState**

In the `GameState` struct, add after `last_trade_id`:

```rust
    pub imagination: u64,
    pub upgrades: PlayerUpgrades,
```

- [ ] **Step 4: Add fields to RenderFrame**

In the `RenderFrame` struct, add after `max_energy`:

```rust
    pub imagination: u64,
    pub upgrades: PlayerUpgrades,
```

- [ ] **Step 5: Update GameState::new()**

In `GameState::new()`, add after `last_trade_id: None,`:

```rust
            imagination: 0,
            upgrades: PlayerUpgrades::default(),
```

- [ ] **Step 6: Update save_state()**

In `save_state()`, add after `last_trade_id: self.last_trade_id,`:

```rust
            imagination: self.imagination,
            upgrades: self.upgrades.clone(),
```

- [ ] **Step 7: Update restore_save()**

In `restore_save()`, add after `self.last_trade_id = save.last_trade_id;`:

```rust
        self.imagination = save.imagination;
        self.upgrades = save.upgrades.clone();
```

- [ ] **Step 8: Update the RenderFrame construction in tick()**

In `tick()`, where the `RenderFrame` is constructed (the `Some(RenderFrame { ... })` block), add after `max_energy: self.max_energy,`:

```rust
            imagination: self.imagination,
            upgrades: self.upgrades.clone(),
```

- [ ] **Step 9: Write tests**

Add these tests at the end of the `mod tests` block in `state.rs`:

```rust
    #[test]
    fn save_state_imagination_default() {
        let json = r#"{
            "streetId": "demo_meadow",
            "x": 0, "y": 0,
            "facing": "right",
            "inventory": []
        }"#;
        let save: SaveState = serde_json::from_str(json).unwrap();
        assert_eq!(save.imagination, 0);
        assert_eq!(save.upgrades.energy_tank_tier, 0);
        assert_eq!(save.upgrades.haggling_tier, 0);
    }

    #[test]
    fn save_state_imagination_round_trip() {
        let save = SaveState {
            street_id: "demo_meadow".to_string(),
            x: 0.0,
            y: 0.0,
            facing: Direction::Right,
            inventory: vec![],
            avatar: AvatarAppearance::default(),
            currants: 50,
            energy: 600.0,
            max_energy: 600.0,
            last_trade_id: None,
            imagination: 42,
            upgrades: PlayerUpgrades {
                energy_tank_tier: 2,
                haggling_tier: 1,
            },
        };
        let json = serde_json::to_string(&save).unwrap();
        let parsed: SaveState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.imagination, 42);
        assert_eq!(parsed.upgrades.energy_tank_tier, 2);
        assert_eq!(parsed.upgrades.haggling_tier, 1);
    }
```

- [ ] **Step 10: Run tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass.

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/engine/state.rs
git commit -m "feat(imagination): add iMG and upgrades to GameState, SaveState, RenderFrame"
```

---

### Task 5: Earn iMG from Harvesting

**Files:**
- Modify: `src-tauri/src/item/interaction.rs:199-340`
- Modify: `src-tauri/src/engine/state.rs:555-566`

- [ ] **Step 1: Add imagination parameter to execute_interaction**

In `src-tauri/src/item/interaction.rs`, change the `execute_interaction` signature to add `imagination: &mut u64` after `energy: &mut f64`:

```rust
pub fn execute_interaction(
    nearest: &NearestInteractable,
    inventory: &mut Inventory,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
    item_defs: &ItemDefs,
    rng: &mut impl Rng,
    entity_states: &mut HashMap<String, EntityInstanceState>,
    game_time: f64,
    energy: &mut f64,
    imagination: &mut u64,
) -> InteractionResult {
```

- [ ] **Step 2: Add iMG earning and feedback after the yield loop**

In the harvest entity branch, after the yield loop (after the `for yield_entry in &def.yields { ... }` block) and before `result.interaction_type = Some(InteractionType::Entity { ... })`, add:

```rust
            // Earn iMG from all produced items (regardless of inventory space)
            let produced_items: Vec<(&str, u32)> = def
                .yields
                .iter()
                .map(|y| {
                    let count = result
                        .feedback
                        .iter()
                        .filter(|f| f.success)
                        .filter_map(|f| {
                            // Match by item name in feedback text
                            // Simpler: track production count alongside yield loop
                            None::<u32>
                        })
                        .sum::<u32>();
                    (y.item.as_str(), count)
                })
                .collect();
```

Wait — that approach is fragile. Instead, collect the production counts during the yield loop itself. Modify the yield loop to track totals:

Right before the existing yield loop `for yield_entry in &def.yields {`, add:

```rust
            let mut produced_items: Vec<(&str, u32)> = Vec::new();
```

Inside the yield loop, right after `let count = rng.gen_range(yield_entry.min..=yield_entry.max);`, add:

```rust
                produced_items.push((yield_entry.item.as_str(), count));
```

Then after the yield loop closes, before `result.interaction_type = Some(...)`, add:

```rust
            // Earn iMG from production
            let img_earned = crate::item::imagination::earn_from_harvest(&produced_items, item_defs);
            if img_earned > 0 {
                *imagination = imagination.saturating_add(img_earned);
                result.feedback.push(PickupFeedback {
                    id: 0,
                    text: format!("+{img_earned} iMG"),
                    success: true,
                    x: entity.x,
                    y: entity.y,
                    age_secs: 0.0,
                    color: Some("#c084fc".to_string()),
                });
            }
```

- [ ] **Step 3: Update the call site in state.rs**

In `src-tauri/src/engine/state.rs`, update the `execute_interaction` call (around line 555) to pass `&mut self.imagination`:

```rust
                    let result = interaction::execute_interaction(
                        nearest,
                        &mut self.inventory,
                        &self.world_entities,
                        &self.entity_defs,
                        &self.world_items,
                        &self.item_defs,
                        rng,
                        &mut self.entity_states,
                        self.game_time,
                        &mut self.energy,
                        &mut self.imagination,
                    );
```

- [ ] **Step 4: Fix all existing tests in interaction.rs**

Every test calling `execute_interaction` needs the new `&mut imagination` parameter. Add `let mut imagination: u64 = 0;` to each test and pass `&mut imagination` as the last argument.

- [ ] **Step 5: Add the harvest iMG test**

Add this test in `src-tauri/src/item/interaction.rs`:

```rust
    #[test]
    fn harvest_earns_imagination() {
        let (entities, entity_defs, item_defs) = test_data();
        let mut inv = Inventory::new(16);
        let mut entity_states = HashMap::new();
        let mut rng = StdRng::seed_from_u64(42);
        let mut energy = 600.0;
        let mut imagination: u64 = 0;
        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 10.0,
        };
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            1.0,
            &mut energy,
            &mut imagination,
        );
        // Should have earned iMG based on cherry base_cost=3
        assert!(imagination > 0);
        // Should have purple iMG feedback
        let img_feedback = result.feedback.iter().find(|f| f.text.contains("iMG"));
        assert!(img_feedback.is_some());
        assert_eq!(img_feedback.unwrap().color, Some("#c084fc".to_string()));
    }

    #[test]
    fn harvest_earns_img_even_when_inventory_full() {
        let (entities, entity_defs, item_defs) = test_data();
        let mut inv = Inventory::new(1);
        // Fill inventory
        inv.add("cherry", 50, &item_defs);
        let mut entity_states = HashMap::new();
        let mut rng = StdRng::seed_from_u64(42);
        let mut energy = 600.0;
        let mut imagination: u64 = 0;
        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 10.0,
        };
        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            1.0,
            &mut energy,
            &mut imagination,
        );
        // iMG earned even though items overflowed
        assert!(imagination > 0);
    }
```

- [ ] **Step 6: Run tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/item/interaction.rs src-tauri/src/engine/state.rs
git commit -m "feat(imagination): earn iMG from harvesting with purple feedback"
```

---

### Task 6: Earn iMG from Crafting, Wire Haggling in vendor_buy, and buy_upgrade IPC

**Files:**
- Modify: `src-tauri/src/engine/state.rs:255-280` (craft_recipe)
- Modify: `src-tauri/src/lib.rs` (vendor_buy, new buy_upgrade command)

- [ ] **Step 1: Earn iMG from crafting in craft_recipe**

In `src-tauri/src/engine/state.rs`, in the `craft_recipe` method, after the existing feedback loop and before the audio event push, add:

```rust
        // Earn iMG from crafted outputs
        let produced: Vec<(&str, u32)> = result
            .iter()
            .map(|o| (o.item_id.as_str(), o.count))
            .collect();
        let img_earned = crate::item::imagination::earn_from_craft(&produced, &self.item_defs);
        if img_earned > 0 {
            self.imagination = self.imagination.saturating_add(img_earned);
            self.pickup_feedback.push(PickupFeedback {
                id: self.next_feedback_id,
                text: format!("+{img_earned} iMG"),
                success: true,
                x: self.player.x,
                y: self.player.y,
                age_secs: 0.0,
                color: Some("#c084fc".to_string()),
            });
            self.next_feedback_id += 1;
        }
```

- [ ] **Step 2: Wire haggling discount in vendor_buy IPC**

In `src-tauri/src/lib.rs`, in the `vendor_buy` command, replace the temporary `0.0` with the actual haggling discount. Change:

```rust
    let new_balance = item::vendor::buy(&item_id, count, currants, &mut state.inventory, &item_defs, &store, 0.0)?;
```

To:

```rust
    let discount = item::imagination::haggling_discount(state.upgrades.haggling_tier);
    let new_balance = item::vendor::buy(&item_id, count, currants, &mut state.inventory, &item_defs, &store, discount)?;
```

- [ ] **Step 3: Add buy_upgrade IPC command**

In `src-tauri/src/lib.rs`, add the new command (before the trade IPC commands section):

```rust
#[tauri::command]
fn buy_upgrade(upgrade_id: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let result = item::imagination::buy_upgrade(
        &upgrade_id,
        &mut state.imagination,
        &mut state.upgrades,
    )?;

    // Apply upgrade effect
    let px = state.player.x;
    let py = state.player.y;

    match result {
        item::imagination::UpgradeEffect::EnergyTankDelta(delta) => {
            state.max_energy += delta;
            state.energy += delta;
            let fb_id = state.next_feedback_id;
            state.next_feedback_id += 1;
            state.pickup_feedback.push(item::types::PickupFeedback {
                id: fb_id,
                text: format!("+{} max energy!", delta as u32),
                success: true,
                x: px,
                y: py,
                age_secs: 0.0,
                color: Some("#4ade80".to_string()),
            });
        }
        item::imagination::UpgradeEffect::HagglingDiscount(discount) => {
            let fb_id = state.next_feedback_id;
            state.next_feedback_id += 1;
            state.pickup_feedback.push(item::types::PickupFeedback {
                id: fb_id,
                text: format!("Haggling → {}%", (discount * 100.0).round() as u32),
                success: true,
                x: px,
                y: py,
                age_secs: 0.0,
                color: Some("#fbbf24".to_string()),
            });
        }
    }

    Ok(serde_json::json!({
        "imagination": state.imagination,
        "upgrades": state.upgrades,
        "energy": state.energy,
        "maxEnergy": state.max_energy,
    }))
}
```

- [ ] **Step 4: Register buy_upgrade in invoke_handler**

In `src-tauri/src/lib.rs`, add `buy_upgrade,` to the `invoke_handler` list (after `eat_item,`).

- [ ] **Step 5: Add craft iMG test in state.rs**

Add at the end of `mod tests` in `state.rs`:

```rust
    #[test]
    fn craft_earns_imagination() {
        let item_defs =
            crate::item::loader::parse_item_defs(include_str!("../../../assets/items.json"))
                .unwrap();
        let entity_defs =
            crate::item::loader::parse_entity_defs(include_str!("../../../assets/entities.json"))
                .unwrap();
        let recipe_defs =
            crate::item::loader::parse_recipe_defs(include_str!("../../../assets/recipes.json"))
                .unwrap();
        let track_catalog = crate::engine::jukebox::TrackCatalog::default();
        let store_catalog: crate::item::types::StoreCatalog =
            serde_json::from_str(include_str!("../../../assets/stores.json")).unwrap();
        let mut state = GameState::new(800.0, 600.0, item_defs, entity_defs, recipe_defs, track_catalog, store_catalog);

        // Give player cherry_pie ingredients
        state.inventory.add("cherry", 10, &state.item_defs.clone());
        state.inventory.add("grain", 5, &state.item_defs.clone());
        state.inventory.add("pot", 1, &state.item_defs.clone());

        let before = state.imagination;
        let _ = state.craft_recipe("cherry_pie");
        assert!(state.imagination > before);
        // cherry_pie base_cost=20, 2x multiplier = 40
        assert_eq!(state.imagination - before, 40);
    }
```

- [ ] **Step 6: Run tests**

Run: `cd src-tauri && cargo test`
Expected: All tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/engine/state.rs src-tauri/src/lib.rs
git commit -m "feat(imagination): earn iMG from crafting, wire haggling discount, add buy_upgrade IPC"
```

---

### Task 7: Frontend Types and IPC

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`

- [ ] **Step 1: Add types to TypeScript**

In `src/lib/types.ts`, add `PlayerUpgrades` interface (after `EatResult`):

```typescript
export interface PlayerUpgrades {
  energyTankTier: number;
  hagglingTier: number;
}

export interface BuyUpgradeResult {
  imagination: number;
  upgrades: PlayerUpgrades;
  energy: number;
  maxEnergy: number;
}
```

Add fields to `RenderFrame` (after `maxEnergy: number;`):

```typescript
  imagination: number;
  upgrades: PlayerUpgrades;
```

Add fields to `SavedState` (after `maxEnergy?: number;`):

```typescript
  imagination?: number;
  upgrades?: PlayerUpgrades;
```

- [ ] **Step 2: Add buyUpgrade IPC function**

In `src/lib/ipc.ts`, add the import for `BuyUpgradeResult` to the import line:

```typescript
import type { ..., BuyUpgradeResult, ... } from './types';
```

Add the function (after `eatItem`):

```typescript
export async function buyUpgrade(upgradeId: string): Promise<BuyUpgradeResult> {
  return invoke<BuyUpgradeResult>('buy_upgrade', { upgradeId });
}
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/types.ts src/lib/ipc.ts
git commit -m "feat(imagination): add frontend types and buyUpgrade IPC"
```

---

### Task 8: ImaginationHud and UpgradePanel Components

**Files:**
- Create: `src/lib/components/ImaginationHud.svelte`
- Create: `src/lib/components/ImaginationHud.test.ts`
- Create: `src/lib/components/UpgradePanel.svelte`
- Create: `src/lib/components/UpgradePanel.test.ts`

- [ ] **Step 1: Create ImaginationHud component**

Create `src/lib/components/ImaginationHud.svelte`:

```svelte
<script lang="ts">
  let { imagination = 0, onOpen }: { imagination: number; onOpen?: () => void } = $props();
</script>

<button
  type="button"
  class="imagination-hud"
  role="status"
  aria-label="Imagination: {imagination} iMG"
  onclick={onOpen}
>
  <span class="img-icon">✦</span>
  <span class="img-amount">{imagination} iMG</span>
</button>

<style>
  .imagination-hud {
    position: fixed;
    top: 44px;
    right: 12px;
    background: rgba(26, 26, 46, 0.85);
    color: #c084fc;
    padding: 6px 12px;
    border-radius: 16px;
    font-weight: bold;
    font-size: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
    z-index: 50;
    user-select: none;
    border: 1px solid transparent;
    cursor: pointer;
    transition: border-color 0.2s;
    font-family: inherit;
  }
  .imagination-hud:hover {
    border-color: #c084fc;
  }
  .imagination-hud:focus-visible {
    outline: 2px solid #c084fc;
    outline-offset: 2px;
  }
  .img-icon {
    font-size: 10px;
  }
</style>
```

- [ ] **Step 2: Create ImaginationHud tests**

Create `src/lib/components/ImaginationHud.test.ts`:

```typescript
// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import ImaginationHud from './ImaginationHud.svelte';

describe('ImaginationHud', () => {
  it('renders iMG amount', () => {
    render(ImaginationHud, { props: { imagination: 156 } });
    const amount = document.querySelector('.img-amount');
    expect(amount?.textContent).toBe('156 iMG');
  });

  it('has accessible role="status"', () => {
    render(ImaginationHud, { props: { imagination: 42 } });
    const hud = document.querySelector('[role="status"]');
    expect(hud).toBeDefined();
    expect(hud?.getAttribute('aria-label')).toContain('42');
  });

  it('calls onOpen when clicked', async () => {
    const onOpen = vi.fn();
    render(ImaginationHud, { props: { imagination: 100, onOpen } });
    const btn = document.querySelector('.imagination-hud') as HTMLElement;
    await fireEvent.click(btn);
    expect(onOpen).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 3: Create UpgradePanel component**

Create `src/lib/components/UpgradePanel.svelte`:

```svelte
<script lang="ts">
  import type { PlayerUpgrades, BuyUpgradeResult } from '../types';
  import { buyUpgrade } from '../ipc';

  let {
    visible = false,
    imagination = 0,
    upgrades = { energyTankTier: 0, hagglingTier: 0 },
    maxEnergy = 600,
    onClose,
  }: {
    visible?: boolean;
    imagination: number;
    upgrades: PlayerUpgrades;
    maxEnergy: number;
    onClose?: () => void;
  } = $props();

  let isPurchasing = $state(false);
  let purchaseError = $state<string | null>(null);
  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;

  const energyTankTiers = [
    { cost: 100, maxEnergy: 650, delta: 50 },
    { cost: 200, maxEnergy: 725, delta: 75 },
    { cost: 400, maxEnergy: 825, delta: 100 },
    { cost: 800, maxEnergy: 950, delta: 125 },
  ];

  const hagglingTiers = [
    { cost: 100, discount: 5 },
    { cost: 200, discount: 10 },
    { cost: 400, discount: 15 },
    { cost: 800, discount: 20 },
  ];

  let energyTankMaxed = $derived(upgrades.energyTankTier >= 4);
  let hagglingMaxed = $derived(upgrades.hagglingTier >= 4);
  let nextEnergyTier = $derived(energyTankMaxed ? null : energyTankTiers[upgrades.energyTankTier]);
  let nextHagglingTier = $derived(hagglingMaxed ? null : hagglingTiers[upgrades.hagglingTier]);
  let currentDiscount = $derived(
    upgrades.hagglingTier > 0 ? hagglingTiers[upgrades.hagglingTier - 1].discount : 0
  );

  $effect(() => {
    if (visible && dialogEl && !dialogEl.open) {
      previousFocus = document.activeElement as HTMLElement | null;
      dialogEl.showModal();
    } else if (!visible && dialogEl?.open) {
      dialogEl.close();
      previousFocus?.focus();
    }
  });

  function handleClose() {
    purchaseError = null;
    onClose?.();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      handleClose();
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === dialogEl) {
      handleClose();
    }
  }

  async function handleBuy(upgradeId: string) {
    if (isPurchasing) return;
    isPurchasing = true;
    purchaseError = null;
    try {
      await buyUpgrade(upgradeId);
    } catch (e) {
      purchaseError = String(e);
    } finally {
      isPurchasing = false;
    }
  }
</script>

{#if visible}
<dialog
  bind:this={dialogEl}
  class="upgrade-panel"
  aria-label="Imagination Upgrades"
  onkeydown={handleKeydown}
  onclick={handleBackdropClick}
>
  <div class="panel-content" onclick={(e) => e.stopPropagation()}>
    <div class="panel-header">
      <span class="panel-title">✦ Imagination</span>
      <span class="panel-balance">{imagination} iMG</span>
    </div>

    <!-- Energy Tank -->
    <div class="upgrade-card">
      <div class="card-header">
        <span class="card-name energy-color">⚡ Energy Tank</span>
        <span class="card-tier">Tier {upgrades.energyTankTier} / 4</span>
      </div>
      <div class="card-effect">Max energy: {Math.round(maxEnergy)}</div>
      <div class="tier-dots">
        {#each Array(4) as _, i}
          <div class="dot" class:filled={i < upgrades.energyTankTier}></div>
        {/each}
      </div>
      {#if energyTankMaxed}
        <div class="max-badge">MAX</div>
      {:else if nextEnergyTier}
        <div class="card-next">
          <span>Next: max energy → {nextEnergyTier.maxEnergy}</span>
          <button
            type="button"
            class="buy-btn"
            disabled={imagination < nextEnergyTier.cost || isPurchasing}
            onclick={() => handleBuy('energy_tank')}
          >
            {nextEnergyTier.cost} iMG
          </button>
        </div>
      {/if}
    </div>

    <!-- Vendor Haggling -->
    <div class="upgrade-card">
      <div class="card-header">
        <span class="card-name haggling-color">🤝 Vendor Haggling</span>
        <span class="card-tier">Tier {upgrades.hagglingTier} / 4</span>
      </div>
      <div class="card-effect">Vendor discount: {currentDiscount}%</div>
      <div class="tier-dots">
        {#each Array(4) as _, i}
          <div class="dot" class:filled={i < upgrades.hagglingTier}></div>
        {/each}
      </div>
      {#if hagglingMaxed}
        <div class="max-badge">MAX</div>
      {:else if nextHagglingTier}
        <div class="card-next">
          <span>Next: discount → {nextHagglingTier.discount}%</span>
          <button
            type="button"
            class="buy-btn"
            disabled={imagination < nextHagglingTier.cost || isPurchasing}
            onclick={() => handleBuy('haggling')}
          >
            {nextHagglingTier.cost} iMG
          </button>
        </div>
      {/if}
    </div>

    {#if purchaseError}
      <div class="purchase-error" role="alert">{purchaseError}</div>
    {/if}

    <div class="close-hint">Click outside or press Escape to close</div>
  </div>
</dialog>
{/if}

<style>
  dialog.upgrade-panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    left: 0;
    width: 100%;
    height: 100%;
    max-width: 100%;
    max-height: 100%;
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    z-index: 100;
  }
  dialog.upgrade-panel::backdrop {
    background: rgba(0, 0, 0, 0.3);
  }

  .panel-content {
    position: absolute;
    top: 12px;
    right: 12px;
    width: 320px;
    background: rgba(26, 26, 46, 0.95);
    border-radius: 12px;
    padding: 20px;
    border: 1px solid rgba(192, 132, 252, 0.3);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    color: white;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
  }
  .panel-title {
    color: #c084fc;
    font-weight: bold;
    font-size: 16px;
  }
  .panel-balance {
    color: #c084fc;
    font-size: 14px;
    font-weight: bold;
  }

  .upgrade-card {
    background: rgba(255, 255, 255, 0.05);
    border-radius: 8px;
    padding: 14px;
    margin-bottom: 12px;
  }
  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
  }
  .card-name {
    font-weight: bold;
    font-size: 13px;
  }
  .energy-color { color: #4ade80; }
  .haggling-color { color: #fbbf24; }
  .card-tier {
    color: rgba(255, 255, 255, 0.5);
    font-size: 11px;
  }
  .card-effect {
    color: rgba(255, 255, 255, 0.6);
    font-size: 11px;
    margin-bottom: 10px;
  }

  .tier-dots {
    display: flex;
    gap: 4px;
    margin-bottom: 10px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.15);
  }
  .dot.filled {
    background: #c084fc;
  }

  .card-next {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
    color: rgba(255, 255, 255, 0.7);
  }

  .buy-btn {
    background: #7c3aed;
    color: white;
    border: none;
    border-radius: 6px;
    padding: 4px 12px;
    font-size: 11px;
    font-weight: bold;
    cursor: pointer;
    font-family: inherit;
    transition: opacity 0.2s;
  }
  .buy-btn:hover:not(:disabled) {
    opacity: 0.85;
  }
  .buy-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .buy-btn:focus-visible {
    outline: 2px solid #c084fc;
    outline-offset: 2px;
  }

  .max-badge {
    text-align: center;
    color: #c084fc;
    font-weight: bold;
    font-size: 12px;
    padding: 4px;
  }

  .purchase-error {
    color: #f87171;
    font-size: 11px;
    text-align: center;
    margin-top: 8px;
  }

  .close-hint {
    margin-top: 14px;
    text-align: center;
    color: rgba(255, 255, 255, 0.3);
    font-size: 10px;
  }
</style>
```

- [ ] **Step 4: Create UpgradePanel tests**

Create `src/lib/components/UpgradePanel.test.ts`:

```typescript
// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import UpgradePanel from './UpgradePanel.svelte';

vi.mock('../ipc', () => ({
  buyUpgrade: vi.fn().mockResolvedValue({
    imagination: 0,
    upgrades: { energyTankTier: 1, hagglingTier: 0 },
    energy: 650,
    maxEnergy: 650,
  }),
}));

const defaultProps = {
  visible: true,
  imagination: 500,
  upgrades: { energyTankTier: 1, hagglingTier: 0 },
  maxEnergy: 650,
};

describe('UpgradePanel', () => {
  it('shows both upgrade paths', () => {
    render(UpgradePanel, { props: defaultProps });
    const cards = document.querySelectorAll('.upgrade-card');
    expect(cards.length).toBe(2);
  });

  it('shows correct tier for energy tank', () => {
    render(UpgradePanel, { props: defaultProps });
    const tiers = document.querySelectorAll('.card-tier');
    expect(tiers[0]?.textContent).toContain('Tier 1 / 4');
  });

  it('buy button enabled when sufficient iMG', () => {
    render(UpgradePanel, { props: defaultProps });
    const buttons = document.querySelectorAll('.buy-btn') as NodeListOf<HTMLButtonElement>;
    // Energy tank tier 2 costs 200, we have 500
    expect(buttons[0]?.disabled).toBe(false);
  });

  it('buy button disabled when insufficient iMG', () => {
    render(UpgradePanel, { props: { ...defaultProps, imagination: 50 } });
    const buttons = document.querySelectorAll('.buy-btn') as NodeListOf<HTMLButtonElement>;
    // Energy tank tier 2 costs 200, we have 50
    expect(buttons[0]?.disabled).toBe(true);
  });

  it('shows MAX when tier is 4', () => {
    render(UpgradePanel, {
      props: {
        ...defaultProps,
        upgrades: { energyTankTier: 4, hagglingTier: 0 },
        maxEnergy: 950,
      },
    });
    const maxBadge = document.querySelector('.max-badge');
    expect(maxBadge?.textContent).toContain('MAX');
  });

  it('calls buyUpgrade IPC on click', async () => {
    const { buyUpgrade: mockBuy } = await import('../ipc');
    render(UpgradePanel, { props: defaultProps });
    const buttons = document.querySelectorAll('.buy-btn') as NodeListOf<HTMLButtonElement>;
    await fireEvent.click(buttons[0]);
    expect(mockBuy).toHaveBeenCalledWith('energy_tank');
  });

  it('has dialog with aria-label', () => {
    render(UpgradePanel, { props: defaultProps });
    const dialog = document.querySelector('dialog');
    expect(dialog?.getAttribute('aria-label')).toBe('Imagination Upgrades');
  });
});
```

- [ ] **Step 5: Run tests**

Run: `npx vitest run`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/ImaginationHud.svelte src/lib/components/ImaginationHud.test.ts src/lib/components/UpgradePanel.svelte src/lib/components/UpgradePanel.test.ts
git commit -m "feat(imagination): add ImaginationHud and UpgradePanel Svelte components with tests"
```

---

### Task 9: App.svelte Integration

**Files:**
- Modify: `src/App.svelte`

- [ ] **Step 1: Add imports**

In `src/App.svelte`, add the imports alongside existing HUD imports:

```typescript
  import ImaginationHud from './lib/components/ImaginationHud.svelte';
  import UpgradePanel from './lib/components/UpgradePanel.svelte';
```

- [ ] **Step 2: Add upgrade panel state**

Near the existing state declarations (alongside `inventoryOpen`, `jukeboxOpen`, etc.), add:

```typescript
  let upgradePanelOpen = $state(false);
```

- [ ] **Step 3: Add components to the template**

In the template, after the `<EnergyHud>` line, add:

```svelte
    <ImaginationHud
      imagination={latestFrame?.imagination ?? 0}
      onOpen={() => { upgradePanelOpen = true; }}
    />
    <UpgradePanel
      visible={upgradePanelOpen}
      imagination={latestFrame?.imagination ?? 0}
      upgrades={latestFrame?.upgrades ?? { energyTankTier: 0, hagglingTier: 0 }}
      maxEnergy={latestFrame?.maxEnergy ?? 600}
      onClose={() => { upgradePanelOpen = false; }}
    />
```

- [ ] **Step 4: Run full test suite**

Run: `cd src-tauri && cargo test`
Run: `npx vitest run`
Run: `cd src-tauri && cargo clippy`
Expected: All tests pass, no clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add src/App.svelte
git commit -m "feat(imagination): integrate ImaginationHud and UpgradePanel into App.svelte"
```

---

## Self-Review Checklist

**Spec coverage:**
- [x] `imagination: u64` on GameState/SaveState/RenderFrame → Task 4
- [x] `PlayerUpgrades` struct with serde defaults → Task 1
- [x] Upgrade definitions as Rust consts → Task 1
- [x] `earn_imagination` function → Task 1
- [x] Earn from harvest (1x multiplier) → Task 5
- [x] Earn from craft (2x multiplier) → Task 6
- [x] iMG from production count, not inventory receipt → Task 5
- [x] `buy_upgrade` function → Task 1
- [x] Energy tank effect (energy + max_energy delta) → Task 6
- [x] Haggling discount on vendor buy → Task 3, Task 6
- [x] Minimum price of 1 → Task 3
- [x] `buy_upgrade` IPC command → Task 6
- [x] PickupFeedback color extension → Task 2
- [x] Purple iMG feedback → Tasks 5, 6
- [x] ImaginationHud (clickable pill, top-right below currants) → Task 8
- [x] UpgradePanel (dialog, both paths, tier dots, buy button) → Task 8
- [x] Save/restore backward compat → Task 4
- [x] Saturating add for iMG overflow → Task 1
- [x] Frontend types + IPC → Task 7
- [x] App.svelte integration → Task 9
- [x] All test cases from spec → Tasks 1-8

**Placeholder scan:** No TBDs, TODOs, or vague steps found.

**Type consistency:** `PlayerUpgrades`, `UpgradeEffect`, `earn_from_harvest`, `earn_from_craft`, `buy_upgrade`, `haggling_discount` — all names consistent across tasks.
