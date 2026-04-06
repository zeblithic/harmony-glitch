use crate::item::types::ItemDefs;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Multipliers
// ---------------------------------------------------------------------------

pub const HARVEST_IMG_MULTIPLIER: u64 = 1;
pub const CRAFT_IMG_MULTIPLIER: u64 = 2;

// ---------------------------------------------------------------------------
// Player upgrade state (persisted)
// ---------------------------------------------------------------------------

/// Tracks which upgrade tiers the player has purchased.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayerUpgrades {
    pub energy_tank_tier: u8,
    pub haggling_tier: u8,
}

// ---------------------------------------------------------------------------
// Upgrade definitions
// ---------------------------------------------------------------------------

/// A single tier within an upgrade path.
#[derive(Debug, Clone, Copy)]
pub struct UpgradeTier {
    pub cost: u64,
    pub effect_value: f64,
}

/// A complete upgrade path with 4 tiers.
#[derive(Debug)]
pub struct UpgradePath {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub tiers: [UpgradeTier; 4],
}

/// The effect granted when an upgrade tier is purchased.
#[derive(Debug, Clone, PartialEq)]
pub enum UpgradeEffect {
    /// Increase the player's maximum energy tank by this amount.
    EnergyTankDelta(f64),
    /// Apply a fractional discount to vendor purchases (0.05 = 5% off).
    HagglingDiscount(f64),
}

// ---------------------------------------------------------------------------
// Const upgrade paths
// ---------------------------------------------------------------------------

pub const ENERGY_TANK: UpgradePath = UpgradePath {
    id: "energy_tank",
    name: "Energy Tank",
    description: "Increase your maximum energy capacity.",
    tiers: [
        UpgradeTier { cost: 100, effect_value: 50.0 },
        UpgradeTier { cost: 200, effect_value: 75.0 },
        UpgradeTier { cost: 400, effect_value: 100.0 },
        UpgradeTier { cost: 800, effect_value: 125.0 },
    ],
};

pub const HAGGLING: UpgradePath = UpgradePath {
    id: "haggling",
    name: "Vendor Haggling",
    description: "Negotiate better prices at vendor shops.",
    tiers: [
        UpgradeTier { cost: 100, effect_value: 0.05 },
        UpgradeTier { cost: 200, effect_value: 0.10 },
        UpgradeTier { cost: 400, effect_value: 0.15 },
        UpgradeTier { cost: 800, effect_value: 0.20 },
    ],
};

// ---------------------------------------------------------------------------
// Earning iMG
// ---------------------------------------------------------------------------

/// Compute the total iMG earned for a list of `(item_id, count)` tuples given a multiplier.
///
/// Items without a `base_cost` contribute 0 iMG.
/// Uses saturating arithmetic throughout.
pub fn earn_imagination(items: &[(&str, u32)], item_defs: &ItemDefs, multiplier: u64) -> u64 {
    let mut total: u64 = 0;
    for (item_id, count) in items {
        let Some(def) = item_defs.get(*item_id) else {
            continue;
        };
        let Some(base_cost) = def.base_cost else {
            continue;
        };
        // base_cost (u32) * count (u32) * multiplier (u64), all saturating
        let item_img = (base_cost as u64)
            .saturating_mul(*count as u64)
            .saturating_mul(multiplier);
        total = total.saturating_add(item_img);
    }
    total
}

/// iMG earned when harvesting items (1× multiplier).
pub fn earn_from_harvest(items: &[(&str, u32)], item_defs: &ItemDefs) -> u64 {
    earn_imagination(items, item_defs, HARVEST_IMG_MULTIPLIER)
}

/// iMG earned when crafting items (2× multiplier).
pub fn earn_from_craft(items: &[(&str, u32)], item_defs: &ItemDefs) -> u64 {
    earn_imagination(items, item_defs, CRAFT_IMG_MULTIPLIER)
}

// ---------------------------------------------------------------------------
// Haggling helper
// ---------------------------------------------------------------------------

/// Return the vendor discount fraction for the given haggling tier (0 = none).
/// Returns `0.0` for tier 0 or any tier above 4.
pub fn haggling_discount(tier: u8) -> f64 {
    match tier {
        0 => 0.0,
        1 => HAGGLING.tiers[0].effect_value,
        2 => HAGGLING.tiers[1].effect_value,
        3 => HAGGLING.tiers[2].effect_value,
        4 => HAGGLING.tiers[3].effect_value,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Buying upgrades
// ---------------------------------------------------------------------------

/// Attempt to purchase the next tier of the given upgrade.
///
/// On success, deducts the cost from `imagination` and returns `Ok(UpgradeEffect)`
/// with the newly-gained effect. On failure, returns an error string and leaves
/// both `imagination` and `upgrades` unchanged.
pub fn buy_upgrade(
    upgrade_id: &str,
    imagination: &mut u64,
    upgrades: &mut PlayerUpgrades,
) -> Result<UpgradeEffect, String> {
    match upgrade_id {
        "energy_tank" => {
            let current_tier = upgrades.energy_tank_tier;
            if current_tier >= 4 {
                return Err("Energy Tank is already at max tier".to_string());
            }
            let tier_def = &ENERGY_TANK.tiers[current_tier as usize];
            if *imagination < tier_def.cost {
                return Err(format!(
                    "Need {} iMG but only have {}",
                    tier_def.cost, imagination
                ));
            }
            *imagination -= tier_def.cost;
            upgrades.energy_tank_tier = current_tier + 1;
            Ok(UpgradeEffect::EnergyTankDelta(tier_def.effect_value))
        }
        "haggling" => {
            let current_tier = upgrades.haggling_tier;
            if current_tier >= 4 {
                return Err("Vendor Haggling is already at max tier".to_string());
            }
            let tier_def = &HAGGLING.tiers[current_tier as usize];
            if *imagination < tier_def.cost {
                return Err(format!(
                    "Need {} iMG but only have {}",
                    tier_def.cost, imagination
                ));
            }
            *imagination -= tier_def.cost;
            upgrades.haggling_tier = current_tier + 1;
            Ok(UpgradeEffect::HagglingDiscount(tier_def.effect_value))
        }
        other => Err(format!("Unknown upgrade id '{other}'")),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;
    use std::collections::HashMap;

    // -- helpers -------------------------------------------------------------

    fn test_item_defs() -> ItemDefs {
        let mut defs = HashMap::new();
        // cherry: base_cost = 3
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
        // bread: base_cost = 16
        defs.insert(
            "bread".to_string(),
            ItemDef {
                id: "bread".to_string(),
                name: "Bread".to_string(),
                description: "A loaf of bread.".to_string(),
                category: "food".to_string(),
                stack_limit: 20,
                icon: "bread".to_string(),
                base_cost: Some(16),
                energy_value: Some(30),
            },
        );
        // pot: no base_cost
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

    // -- earn tests ----------------------------------------------------------

    #[test]
    fn earn_from_harvest_uses_base_cost() {
        let defs = test_item_defs();
        // cherry x3, base_cost=3 → 3*3*1 = 9
        assert_eq!(earn_from_harvest(&[("cherry", 3)], &defs), 9);
    }

    #[test]
    fn earn_from_harvest_no_base_cost_earns_zero() {
        let defs = test_item_defs();
        // pot has no base_cost → 0
        assert_eq!(earn_from_harvest(&[("pot", 1)], &defs), 0);
    }

    #[test]
    fn earn_from_craft_uses_double_multiplier() {
        let defs = test_item_defs();
        // bread x1, base_cost=16 → 16*1*2 = 32
        assert_eq!(earn_from_craft(&[("bread", 1)], &defs), 32);
    }

    #[test]
    fn earn_from_craft_multiple_outputs() {
        let defs = test_item_defs();
        // cherry x2 = 3*2*2 = 12, bread x1 = 16*1*2 = 32 → total 44
        assert_eq!(earn_from_craft(&[("cherry", 2), ("bread", 1)], &defs), 44);
    }

    #[test]
    fn earn_ignores_unknown_items() {
        let defs = test_item_defs();
        // "unicorn_horn" is not in defs → contributes 0
        // cherry x1 * 1 = 3
        assert_eq!(earn_from_harvest(&[("unicorn_horn", 10), ("cherry", 1)], &defs), 3);
    }

    // -- buy_upgrade tests ---------------------------------------------------

    #[test]
    fn buy_upgrade_energy_tank_success() {
        let mut upgrades = PlayerUpgrades::default();
        let mut img: u64 = 500;
        // tier 1 costs 100
        let result = buy_upgrade("energy_tank", &mut img, &mut upgrades);
        assert_eq!(result, Ok(UpgradeEffect::EnergyTankDelta(50.0)));
        assert_eq!(upgrades.energy_tank_tier, 1);
        assert_eq!(img, 400);
    }

    #[test]
    fn buy_upgrade_insufficient_img() {
        let mut upgrades = PlayerUpgrades::default();
        let mut img: u64 = 50;
        // tier 1 costs 100
        let result = buy_upgrade("energy_tank", &mut img, &mut upgrades);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("100"), "Expected cost in error: {err}");
        assert!(err.contains("50"), "Expected balance in error: {err}");
        // Tier should not advance
        assert_eq!(upgrades.energy_tank_tier, 0);
        // Balance should not change
        assert_eq!(img, 50);
    }

    #[test]
    fn buy_upgrade_already_maxed() {
        let mut upgrades = PlayerUpgrades {
            energy_tank_tier: 4,
            haggling_tier: 0,
        };
        let mut img: u64 = 9999;
        let result = buy_upgrade("energy_tank", &mut img, &mut upgrades);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("max tier"));
    }

    #[test]
    fn buy_upgrade_haggling_success() {
        let mut upgrades = PlayerUpgrades::default();
        let mut img: u64 = 300;
        // tier 1 costs 100
        let result = buy_upgrade("haggling", &mut img, &mut upgrades);
        assert_eq!(result, Ok(UpgradeEffect::HagglingDiscount(0.05)));
        assert_eq!(upgrades.haggling_tier, 1);
        assert_eq!(img, 200);
    }

    #[test]
    fn buy_upgrade_energy_tank_escalating_costs() {
        let mut upgrades = PlayerUpgrades::default();
        let costs = [100u64, 200, 400, 800];
        let deltas = [50.0f64, 75.0, 100.0, 125.0];
        let mut img: u64 = 10_000;

        for i in 0..4 {
            let result = buy_upgrade("energy_tank", &mut img, &mut upgrades);
            let effect = result.expect("should succeed");
            assert_eq!(effect, UpgradeEffect::EnergyTankDelta(deltas[i]));
            // Cost already deducted inside buy_upgrade
            let expected_remaining = 10_000u64 - costs[..=i].iter().sum::<u64>();
            assert_eq!(img, expected_remaining);
        }
        assert_eq!(upgrades.energy_tank_tier, 4);
        // Next buy should fail
        let result = buy_upgrade("energy_tank", &mut img, &mut upgrades);
        assert!(result.is_err());
    }

    #[test]
    fn buy_upgrade_unknown_id() {
        let mut upgrades = PlayerUpgrades::default();
        let mut img: u64 = 9999;
        let result = buy_upgrade("rocket_boots", &mut img, &mut upgrades);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("rocket_boots"));
    }

    // -- haggling_discount tests --------------------------------------------

    #[test]
    fn haggling_discount_by_tier() {
        assert!((haggling_discount(0) - 0.0).abs() < 1e-9);
        assert!((haggling_discount(1) - 0.05).abs() < 1e-9);
        assert!((haggling_discount(2) - 0.10).abs() < 1e-9);
        assert!((haggling_discount(3) - 0.15).abs() < 1e-9);
        assert!((haggling_discount(4) - 0.20).abs() < 1e-9);
        // Above max returns 0.0 (invalid tier)
        assert!((haggling_discount(5) - 0.0).abs() < 1e-9);
    }

    // -- PlayerUpgrades serde tests -----------------------------------------

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
        assert!(json.contains("energyTankTier"), "expected camelCase: {json}");
        assert!(json.contains("hagglingTier"), "expected camelCase: {json}");

        let decoded: PlayerUpgrades = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, upgrades);
    }

    #[test]
    fn player_upgrades_deserialize_missing_fields() {
        // All fields should fall back to their defaults when absent
        let decoded: PlayerUpgrades = serde_json::from_str("{}").unwrap();
        assert_eq!(decoded.energy_tank_tier, 0);
        assert_eq!(decoded.haggling_tier, 0);
    }
}
