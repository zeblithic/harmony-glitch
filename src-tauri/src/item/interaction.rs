use rand::Rng;

use crate::item::inventory::Inventory;
use crate::item::types::{
    EntityDefs, InteractionPrompt, ItemDefs, PickupFeedback, WorldEntity, WorldItem,
};

/// Fixed pickup radius for ground items (pixels).
const GROUND_ITEM_PICKUP_RADIUS: f64 = 60.0;

/// What kind of interactable is nearest.
#[derive(Debug)]
pub enum NearestInteractable {
    Entity { index: usize, distance: f64 },
    GroundItem { index: usize, distance: f64 },
}

/// Find the nearest interactable within range of the player.
/// Returns None if nothing is in range.
/// Entities take priority over ground items at equal distance.
pub fn proximity_scan(
    player_x: f64,
    player_y: f64,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
) -> Option<NearestInteractable> {
    let mut best: Option<NearestInteractable> = None;
    let mut best_dist = f64::MAX;

    for (i, entity) in entities.iter().enumerate() {
        let dx = player_x - entity.x;
        let dy = player_y - entity.y;
        let dist = (dx * dx + dy * dy).sqrt();
        let radius = entity_defs
            .get(&entity.entity_type)
            .map(|d| d.interact_radius)
            .unwrap_or(60.0);
        if dist <= radius && dist < best_dist {
            best_dist = dist;
            best = Some(NearestInteractable::Entity {
                index: i,
                distance: dist,
            });
        }
    }

    for (i, item) in world_items.iter().enumerate() {
        let dx = player_x - item.x;
        let dy = player_y - item.y;
        let dist = (dx * dx + dy * dy).sqrt();
        // Ground items only win if strictly closer (entities take priority at equal distance)
        if dist <= GROUND_ITEM_PICKUP_RADIUS && dist < best_dist {
            best_dist = dist;
            best = Some(NearestInteractable::GroundItem {
                index: i,
                distance: dist,
            });
        }
    }

    best
}

/// Build an interaction prompt for the nearest interactable.
pub fn build_prompt(
    nearest: &NearestInteractable,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
    item_defs: &ItemDefs,
) -> InteractionPrompt {
    match nearest {
        NearestInteractable::Entity { index, .. } => {
            let entity = &entities[*index];
            let def = entity_defs.get(&entity.entity_type);
            InteractionPrompt {
                verb: def.map(|d| d.verb.clone()).unwrap_or_else(|| "Use".into()),
                target_name: def
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| "Unknown".into()),
                target_x: entity.x,
                target_y: entity.y,
                actionable: true,
            }
        }
        NearestInteractable::GroundItem { index, .. } => {
            let item = &world_items[*index];
            let name = item_defs
                .get(&item.item_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "Item".into());
            let target_name = if item.count > 1 {
                format!("{} x{}", name, item.count)
            } else {
                name
            };
            InteractionPrompt {
                verb: "Pick up".into(),
                target_name,
                target_x: item.x,
                target_y: item.y,
                actionable: true,
            }
        }
    }
}

/// Result of executing an interaction.
pub struct InteractionResult {
    pub feedback: Vec<PickupFeedback>,
    /// Ground items to spawn (overflow when inventory full).
    pub spawned_items: Vec<(String, u32, f64, f64)>, // (item_id, count, x, y)
    /// Index of ground item to remove (if fully picked up).
    pub remove_ground_item: Option<usize>,
    /// Updated count for ground item (if partially picked up).
    pub update_ground_item: Option<(usize, u32)>,
}

/// Execute an interaction with the nearest interactable.
/// Note: `cooldownSecs` on EntityDef is parsed but not enforced yet — cooldowns
/// and entity state/depletion are deferred to a future phase.
pub fn execute_interaction(
    nearest: &NearestInteractable,
    inventory: &mut Inventory,
    entities: &[WorldEntity],
    entity_defs: &EntityDefs,
    world_items: &[WorldItem],
    item_defs: &ItemDefs,
    rng: &mut impl Rng,
) -> InteractionResult {
    let mut result = InteractionResult {
        feedback: vec![],
        spawned_items: vec![],
        remove_ground_item: None,
        update_ground_item: None,
    };

    match nearest {
        NearestInteractable::Entity { index, .. } => {
            let entity = &entities[*index];
            let Some(def) = entity_defs.get(&entity.entity_type) else {
                return result;
            };

            for yield_entry in &def.yields {
                let count = rng.gen_range(yield_entry.min..=yield_entry.max);
                let overflow = inventory.add(&yield_entry.item, count, item_defs);
                let added = count - overflow;

                if added > 0 {
                    let name = item_defs
                        .get(&yield_entry.item)
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| yield_entry.item.clone());
                    result.feedback.push(PickupFeedback {
                        id: 0, // assigned by GameState
                        text: format!("+{} x{}", name, added),
                        success: true,
                        x: entity.x,
                        y: entity.y,
                        age_secs: 0.0,
                    });
                }

                if overflow > 0 {
                    result
                        .spawned_items
                        .push((yield_entry.item.clone(), overflow, entity.x, entity.y));
                    result.feedback.push(PickupFeedback {
                        id: 0, // assigned by GameState
                        text: "Inventory full!".into(),
                        success: false,
                        x: entity.x,
                        y: entity.y,
                        age_secs: 0.0,
                    });
                }
            }
        }
        NearestInteractable::GroundItem { index, .. } => {
            let item = &world_items[*index];
            let overflow = inventory.add(&item.item_id, item.count, item_defs);
            let added = item.count - overflow;

            if added > 0 {
                let name = item_defs
                    .get(&item.item_id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| item.item_id.clone());
                result.feedback.push(PickupFeedback {
                    id: 0, // assigned by GameState
                    text: format!("+{} x{}", name, added),
                    success: true,
                    x: item.x,
                    y: item.y,
                    age_secs: 0.0,
                });
            }

            if overflow == 0 {
                result.remove_ground_item = Some(*index);
            } else if added > 0 {
                result.update_ground_item = Some((*index, overflow));
            } else {
                result.feedback.push(PickupFeedback {
                    id: 0, // assigned by GameState
                    text: "Inventory full!".into(),
                    success: false,
                    x: item.x,
                    y: item.y,
                    age_secs: 0.0,
                });
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::{EntityDef, ItemDef, YieldEntry};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn test_item_defs() -> ItemDefs {
        let mut defs = ItemDefs::new();
        defs.insert(
            "cherry".into(),
            ItemDef {
                id: "cherry".into(),
                name: "Cherry".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 5,
                icon: "cherry".into(),
            },
        );
        defs
    }

    fn test_entity_defs() -> EntityDefs {
        let mut defs = EntityDefs::new();
        defs.insert(
            "fruit_tree".into(),
            EntityDef {
                id: "fruit_tree".into(),
                name: "Fruit Tree".into(),
                verb: "Harvest".into(),
                yields: vec![YieldEntry {
                    item: "cherry".into(),
                    min: 2,
                    max: 2, // Fixed for deterministic tests
                }],
                cooldown_secs: 5.0,
                max_harvests: 3,
                respawn_secs: 30.0,
                sprite_class: "tree_fruit".into(),
                interact_radius: 80.0,
            },
        );
        defs
    }

    #[test]
    fn proximity_scan_finds_entity_in_range() {
        let entity_defs = test_entity_defs();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 50.0,
            y: 0.0,
        }];
        let result = proximity_scan(40.0, 0.0, &entities, &entity_defs, &[]);
        assert!(matches!(
            result,
            Some(NearestInteractable::Entity { index: 0, .. })
        ));
    }

    #[test]
    fn proximity_scan_ignores_out_of_range() {
        let entity_defs = test_entity_defs();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 500.0,
            y: 0.0,
        }];
        let result = proximity_scan(0.0, 0.0, &entities, &entity_defs, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn proximity_scan_prefers_entity_over_ground_item_at_equal_distance() {
        let entity_defs = test_entity_defs();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 10.0,
            y: 0.0,
        }];
        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 1,
            x: 10.0,
            y: 0.0,
        }];
        let result = proximity_scan(10.0, 0.0, &entities, &entity_defs, &items);
        assert!(matches!(
            result,
            Some(NearestInteractable::Entity { .. })
        ));
    }

    #[test]
    fn proximity_scan_finds_ground_item() {
        let entity_defs = test_entity_defs();
        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 3,
            x: 30.0,
            y: 0.0,
        }];
        let result = proximity_scan(20.0, 0.0, &[], &entity_defs, &items);
        assert!(matches!(
            result,
            Some(NearestInteractable::GroundItem { index: 0, .. })
        ));
    }

    #[test]
    fn execute_entity_interaction_adds_to_inventory() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(4);
        let mut rng = StdRng::seed_from_u64(42);

        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 0.0,
            y: 0.0,
        }];

        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 0.0,
        };
        let result =
            execute_interaction(&nearest, &mut inv, &entities, &entity_defs, &[], &item_defs, &mut rng);

        assert_eq!(inv.slots[0].as_ref().unwrap().item_id, "cherry");
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 2);
        assert!(result.feedback.iter().any(|f| f.success));
    }

    #[test]
    fn execute_entity_interaction_overflows_to_ground() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(1);
        let mut rng = StdRng::seed_from_u64(42);

        // Fill inventory
        inv.add("cherry", 5, &item_defs);

        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 0.0,
            y: 0.0,
        }];

        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 0.0,
        };
        let result =
            execute_interaction(&nearest, &mut inv, &entities, &entity_defs, &[], &item_defs, &mut rng);

        assert!(!result.spawned_items.is_empty());
        assert!(result.feedback.iter().any(|f| !f.success));
    }

    #[test]
    fn execute_ground_item_pickup() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(4);
        let mut rng = StdRng::seed_from_u64(42);

        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 3,
            x: 0.0,
            y: 0.0,
        }];

        let nearest = NearestInteractable::GroundItem {
            index: 0,
            distance: 0.0,
        };
        let result =
            execute_interaction(&nearest, &mut inv, &[], &entity_defs, &items, &item_defs, &mut rng);

        assert_eq!(inv.slots[0].as_ref().unwrap().count, 3);
        assert_eq!(result.remove_ground_item, Some(0));
    }

    #[test]
    fn execute_ground_item_partial_pickup() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(1);
        let mut rng = StdRng::seed_from_u64(42);

        // Partially fill so only 2 more fit (limit 5, have 3)
        inv.add("cherry", 3, &item_defs);

        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 5,
            x: 0.0,
            y: 0.0,
        }];

        let nearest = NearestInteractable::GroundItem {
            index: 0,
            distance: 0.0,
        };
        let result =
            execute_interaction(&nearest, &mut inv, &[], &entity_defs, &items, &item_defs, &mut rng);

        assert_eq!(inv.slots[0].as_ref().unwrap().count, 5);
        assert_eq!(result.update_ground_item, Some((0, 3))); // 3 left on ground
    }

    #[test]
    fn build_prompt_for_entity() {
        let entity_defs = test_entity_defs();
        let item_defs = test_item_defs();
        let entities = vec![WorldEntity {
            id: "t1".into(),
            entity_type: "fruit_tree".into(),
            x: 100.0,
            y: -2.0,
        }];
        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 10.0,
        };
        let prompt = build_prompt(&nearest, &entities, &entity_defs, &[], &item_defs);
        assert_eq!(prompt.verb, "Harvest");
        assert_eq!(prompt.target_name, "Fruit Tree");
    }

    #[test]
    fn build_prompt_for_ground_item() {
        let entity_defs = test_entity_defs();
        let item_defs = test_item_defs();
        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 3,
            x: 50.0,
            y: 0.0,
        }];
        let nearest = NearestInteractable::GroundItem {
            index: 0,
            distance: 5.0,
        };
        let prompt = build_prompt(&nearest, &[], &entity_defs, &items, &item_defs);
        assert_eq!(prompt.verb, "Pick up");
        assert_eq!(prompt.target_name, "Cherry x3");
    }
}
