use std::collections::HashMap;

use rand::Rng;

use crate::item::inventory::Inventory;
use crate::item::types::{
    EntityDefs, EntityInstanceState, InteractionPrompt, ItemDefs, PickupFeedback, WorldEntity,
    WorldItem,
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
    entity_states: &HashMap<String, EntityInstanceState>,
    game_time: f64,
) -> InteractionPrompt {
    match nearest {
        NearestInteractable::Entity { index, .. } => {
            let entity = &entities[*index];
            let def = entity_defs.get(&entity.entity_type);

            // Jukebox entities use "Listen" prompt, always actionable
            if let Some(d) = def {
                if d.playlist.is_some() {
                    return InteractionPrompt {
                        verb: d.verb.clone(),
                        target_name: d.name.clone(),
                        target_x: entity.x,
                        target_y: entity.y,
                        actionable: true,
                        entity_id: Some(entity.id.clone()),
                    };
                }
            }

            // Vendor entities use their verb ("Shop"), always actionable
            if let Some(d) = def {
                if d.store.is_some() {
                    return InteractionPrompt {
                        verb: d.verb.clone(),
                        target_name: d.name.clone(),
                        target_x: entity.x,
                        target_y: entity.y,
                        actionable: true,
                        entity_id: Some(entity.id.clone()),
                    };
                }
            }

            // Check entity state for cooldown/depletion
            if let Some(state) = entity_states.get(&entity.id) {
                if state.depleted_until > game_time {
                    let remaining = (state.depleted_until - game_time).ceil() as u32;
                    return InteractionPrompt {
                        verb: format!("Regrowing... ({}s)", remaining),
                        target_name: String::new(),
                        target_x: entity.x,
                        target_y: entity.y,
                        actionable: false,
                        entity_id: None,
                    };
                }
                if state.cooldown_until > game_time {
                    let remaining = (state.cooldown_until - game_time).ceil() as u32;
                    return InteractionPrompt {
                        verb: format!("Available in {}s", remaining),
                        target_name: String::new(),
                        target_x: entity.x,
                        target_y: entity.y,
                        actionable: false,
                        entity_id: None,
                    };
                }
            }

            InteractionPrompt {
                verb: def.map(|d| d.verb.clone()).unwrap_or_else(|| "Use".into()),
                target_name: def
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| "Unknown".into()),
                target_x: entity.x,
                target_y: entity.y,
                actionable: true,
                entity_id: None,
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
                entity_id: None,
            }
        }
    }
}

/// Classification of what happened during an interaction (for audio).
#[derive(Debug)]
pub enum InteractionType {
    Entity { entity_type: String },
    GroundItem { item_id: String },
    Rejected,
    Jukebox { entity_id: String },
    Vendor { entity_id: String },
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
    /// Audio classification of the interaction result.
    pub interaction_type: Option<InteractionType>,
}

/// Execute an interaction with the nearest interactable.
#[allow(clippy::too_many_arguments)]
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
) -> InteractionResult {
    let mut result = InteractionResult {
        feedback: vec![],
        spawned_items: vec![],
        remove_ground_item: None,
        update_ground_item: None,
        interaction_type: None,
    };

    match nearest {
        NearestInteractable::Entity { index, .. } => {
            let entity = &entities[*index];
            let Some(def) = entity_defs.get(&entity.entity_type) else {
                return result;
            };

            // Jukebox entities don't harvest — return a Jukebox interaction type
            if def.playlist.is_some() {
                result.interaction_type = Some(InteractionType::Jukebox {
                    entity_id: entity.id.clone(),
                });
                return result;
            }

            // Vendor entities don't harvest — return a Vendor interaction type
            if def.store.is_some() {
                result.interaction_type = Some(InteractionType::Vendor {
                    entity_id: entity.id.clone(),
                });
                return result;
            }

            // Lazy-init entity state on first interaction
            let state = entity_states
                .entry(entity.id.clone())
                .or_insert_with(|| EntityInstanceState::new(def.max_harvests));

            // 1. Depletion check — uses strict `>` so that respawn_secs=0.0
            //    (depleted_until == game_time) passes through immediately.
            if state.depleted_until > game_time {
                let remaining = (state.depleted_until - game_time).ceil() as u32;
                result.feedback.push(PickupFeedback {
                    id: 0,
                    text: format!("Regrowing... ({}s)", remaining),
                    success: false,
                    x: entity.x,
                    y: entity.y,
                    age_secs: 0.0,
                });
                result.interaction_type = Some(InteractionType::Rejected);
                return result;
            }

            // 2. Cooldown check
            if state.cooldown_until > game_time {
                let remaining = (state.cooldown_until - game_time).ceil() as u32;
                result.feedback.push(PickupFeedback {
                    id: 0,
                    text: format!("Available in {}s", remaining),
                    success: false,
                    x: entity.x,
                    y: entity.y,
                    age_secs: 0.0,
                });
                result.interaction_type = Some(InteractionType::Rejected);
                return result;
            }

            // Harvest yields (same as before)
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
                        id: 0,
                        text: format!("+{} x{}", name, added),
                        success: true,
                        x: entity.x,
                        y: entity.y,
                        age_secs: 0.0,
                    });
                }

                if overflow > 0 {
                    result.spawned_items.push((
                        yield_entry.item.clone(),
                        overflow,
                        entity.x,
                        entity.y,
                    ));
                    result.feedback.push(PickupFeedback {
                        id: 0,
                        text: "Inventory full!".into(),
                        success: false,
                        x: entity.x,
                        y: entity.y,
                        age_secs: 0.0,
                    });
                }
            }

            result.interaction_type = Some(InteractionType::Entity {
                entity_type: entity.entity_type.clone(),
            });

            // 3. Post-harvest state update — always runs, even if yield overflowed to ground.
            // Overflow items are recoverable, so the harvest "counts" regardless of inventory space.
            if def.max_harvests > 0 {
                debug_assert!(
                    state.harvests_remaining > 0,
                    "harvests_remaining should never be 0 before decrement"
                );
                state.harvests_remaining -= 1;
                if state.harvests_remaining == 0 {
                    state.depleted_until = game_time + def.respawn_secs;
                    state.cooldown_until = 0.0; // clear stale cooldown — depletion and cooldown are mutually exclusive
                    state.harvests_remaining = def.max_harvests;
                } else {
                    state.cooldown_until = game_time + def.cooldown_secs;
                }
            } else {
                // max_harvests == 0: infinite mode, cooldown only
                state.cooldown_until = game_time + def.cooldown_secs;
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
                    id: 0,
                    text: format!("+{} x{}", name, added),
                    success: true,
                    x: item.x,
                    y: item.y,
                    age_secs: 0.0,
                });
            }

            if overflow == 0 {
                result.remove_ground_item = Some(*index);
                result.interaction_type = Some(InteractionType::GroundItem {
                    item_id: item.item_id.clone(),
                });
            } else if added > 0 {
                result.update_ground_item = Some((*index, overflow));
                result.interaction_type = Some(InteractionType::GroundItem {
                    item_id: item.item_id.clone(),
                });
            } else {
                result.feedback.push(PickupFeedback {
                    id: 0,
                    text: "Inventory full!".into(),
                    success: false,
                    x: item.x,
                    y: item.y,
                    age_secs: 0.0,
                });
                result.interaction_type = Some(InteractionType::Rejected);
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
    use std::collections::HashMap;

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
                base_cost: None,
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
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
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
        assert!(matches!(result, Some(NearestInteractable::Entity { .. })));
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
        let mut entity_states = HashMap::new();

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
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );

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
        let mut entity_states = HashMap::new();

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
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );

        assert!(!result.spawned_items.is_empty());
        assert!(result.feedback.iter().any(|f| !f.success));
    }

    #[test]
    fn execute_ground_item_pickup() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(4);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

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
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &[],
            &entity_defs,
            &items,
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );

        assert_eq!(inv.slots[0].as_ref().unwrap().count, 3);
        assert_eq!(result.remove_ground_item, Some(0));
    }

    #[test]
    fn execute_ground_item_partial_pickup() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(1);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

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
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &[],
            &entity_defs,
            &items,
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );

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
        let entity_states = HashMap::new();
        let prompt = build_prompt(
            &nearest,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &entity_states,
            0.0,
        );
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
        let entity_states = HashMap::new();
        let prompt = build_prompt(
            &nearest,
            &[],
            &entity_defs,
            &items,
            &item_defs,
            &entity_states,
            0.0,
        );
        assert_eq!(prompt.verb, "Pick up");
        assert_eq!(prompt.target_name, "Cherry x3");
    }

    #[test]
    fn harvest_decrements_harvests_remaining() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(16);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

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

        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );

        let state = entity_states.get("t1").unwrap();
        assert_eq!(state.harvests_remaining, 2);
    }

    #[test]
    fn interaction_rejected_during_cooldown() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(16);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

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

        // First harvest at t=0
        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );
        let count_after_first = inv.slots[0].as_ref().unwrap().count;

        // Try again at t=2.0 (within 5s cooldown)
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            2.0,
        );

        assert!(result
            .feedback
            .iter()
            .any(|f| !f.success && f.text.contains("Available")));
        assert_eq!(inv.slots[0].as_ref().unwrap().count, count_after_first);
    }

    #[test]
    fn cooldown_expires_after_cooldown_secs() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(16);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

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

        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );
        let count_after_first = inv.slots[0].as_ref().unwrap().count;

        // Harvest at t=5.0 (cooldown expired)
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            5.0,
        );

        assert!(result.feedback.iter().any(|f| f.success));
        assert!(inv.slots[0].as_ref().unwrap().count > count_after_first);
    }

    #[test]
    fn depletion_triggers_after_last_harvest() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(16);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

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

        // 3 harvests at t=0, 5, 10
        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );
        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            5.0,
        );
        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            10.0,
        );

        let state = entity_states.get("t1").unwrap();
        assert!(state.depleted_until > 10.0);
        assert_eq!(state.harvests_remaining, 3); // pre-set for respawn

        // Try at t=15 (within 30s respawn)
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            15.0,
        );
        assert!(result
            .feedback
            .iter()
            .any(|f| !f.success && f.text.contains("Regrowing")));
    }

    #[test]
    fn full_cycle_harvest_deplete_respawn() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(16);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

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

        // Exhaust all 3 harvests
        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );
        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            5.0,
        );
        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            10.0,
        );

        // Depleted at t=10, respawn at t=40 (10 + 30)
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            40.0,
        );
        assert!(result.feedback.iter().any(|f| f.success));

        let state = entity_states.get("t1").unwrap();
        assert_eq!(state.harvests_remaining, 2); // was 3, decremented to 2
    }

    #[test]
    fn lazy_init_creates_state_on_first_interaction() {
        let item_defs = test_item_defs();
        let entity_defs = test_entity_defs();
        let mut inv = Inventory::new(16);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

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

        assert!(!entity_states.contains_key("t1"));

        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );

        assert!(entity_states.contains_key("t1"));
    }

    #[test]
    fn max_harvests_zero_means_no_depletion() {
        let item_defs = test_item_defs();
        let mut entity_defs = EntityDefs::new();
        entity_defs.insert(
            "infinite".into(),
            EntityDef {
                id: "infinite".into(),
                name: "Infinite".into(),
                verb: "Use".into(),
                yields: vec![YieldEntry {
                    item: "cherry".into(),
                    min: 1,
                    max: 1,
                }],
                cooldown_secs: 0.0,
                max_harvests: 0,
                respawn_secs: 0.0,
                sprite_class: "test".into(),
                interact_radius: 80.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
            },
        );
        let mut inv = Inventory::new(16);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

        let entities = vec![WorldEntity {
            id: "inf1".into(),
            entity_type: "infinite".into(),
            x: 0.0,
            y: 0.0,
        }];
        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 0.0,
        };

        for i in 0..10 {
            let result = execute_interaction(
                &nearest,
                &mut inv,
                &entities,
                &entity_defs,
                &[],
                &item_defs,
                &mut rng,
                &mut entity_states,
                i as f64,
            );
            assert!(
                result.feedback.iter().any(|f| f.success),
                "Harvest {} should succeed",
                i
            );
        }

        let state = entity_states.get("inf1").unwrap();
        assert_eq!(state.depleted_until, 0.0);
    }

    #[test]
    fn build_prompt_shows_cooldown_status() {
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

        let mut entity_states = HashMap::new();
        entity_states.insert(
            "t1".into(),
            EntityInstanceState {
                harvests_remaining: 2,
                cooldown_until: 5.0,
                depleted_until: 0.0,
                ..EntityInstanceState::new(0)
            },
        );

        let prompt = build_prompt(
            &nearest,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &entity_states,
            2.0,
        );
        assert!(!prompt.actionable);
        assert!(prompt.verb.contains("Available"));
        assert!(prompt.verb.contains("3")); // ceil(5.0 - 2.0) = 3
        assert!(prompt.target_name.is_empty());
    }

    #[test]
    fn build_prompt_shows_depleted_status() {
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

        let mut entity_states = HashMap::new();
        entity_states.insert(
            "t1".into(),
            EntityInstanceState {
                harvests_remaining: 3,
                cooldown_until: 0.0,
                depleted_until: 40.0,
                ..EntityInstanceState::new(0)
            },
        );

        let prompt = build_prompt(
            &nearest,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &entity_states,
            12.0,
        );
        assert!(!prompt.actionable);
        assert!(prompt.verb.contains("Regrowing"));
        assert!(prompt.verb.contains("28")); // ceil(40.0 - 12.0) = 28
    }

    #[test]
    fn build_prompt_ready_entity_is_actionable() {
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
        let entity_states = HashMap::new();

        let prompt = build_prompt(
            &nearest,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &entity_states,
            0.0,
        );
        assert!(prompt.actionable);
        assert_eq!(prompt.verb, "Harvest");
        assert_eq!(prompt.target_name, "Fruit Tree");
    }

    #[test]
    fn build_prompt_ground_item_always_actionable() {
        let entity_defs = test_entity_defs();
        let item_defs = test_item_defs();
        let items = vec![WorldItem {
            id: "i1".into(),
            item_id: "cherry".into(),
            count: 1,
            x: 50.0,
            y: 0.0,
        }];
        let nearest = NearestInteractable::GroundItem {
            index: 0,
            distance: 5.0,
        };
        let entity_states = HashMap::new();

        let prompt = build_prompt(
            &nearest,
            &[],
            &entity_defs,
            &items,
            &item_defs,
            &entity_states,
            0.0,
        );
        assert!(prompt.actionable);
        assert_eq!(prompt.verb, "Pick up");
    }

    #[test]
    fn instant_respawn_when_respawn_secs_zero() {
        let item_defs = test_item_defs();
        let mut entity_defs = EntityDefs::new();
        entity_defs.insert(
            "fast".into(),
            EntityDef {
                id: "fast".into(),
                name: "Fast".into(),
                verb: "Use".into(),
                yields: vec![YieldEntry {
                    item: "cherry".into(),
                    min: 1,
                    max: 1,
                }],
                cooldown_secs: 0.0,
                max_harvests: 1,
                respawn_secs: 0.0,
                sprite_class: "test".into(),
                interact_radius: 80.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: None,
                audio_radius: None,
                store: None,
            },
        );
        let mut inv = Inventory::new(16);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

        let entities = vec![WorldEntity {
            id: "f1".into(),
            entity_type: "fast".into(),
            x: 0.0,
            y: 0.0,
        }];
        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 0.0,
        };

        // First harvest depletes (max_harvests=1)
        execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );

        // Immediately available (respawn_secs=0, so depleted_until = 0.0)
        let result = execute_interaction(
            &nearest,
            &mut inv,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &mut rng,
            &mut entity_states,
            0.0,
        );
        assert!(result.feedback.iter().any(|f| f.success));
    }

    #[test]
    fn jukebox_entity_returns_jukebox_interaction_type() {
        let item_defs = test_item_defs();
        let mut entity_defs = EntityDefs::new();
        entity_defs.insert(
            "jukebox".into(),
            EntityDef {
                id: "jukebox".into(),
                name: "Tavern Jukebox".into(),
                verb: "Listen".into(),
                yields: vec![],
                cooldown_secs: 0.0,
                max_harvests: 0,
                respawn_secs: 0.0,
                sprite_class: "jukebox".into(),
                interact_radius: 100.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: Some(vec!["track-a".into(), "track-b".into()]),
                audio_radius: Some(400.0),
                store: None,
            },
        );
        let mut inv = Inventory::new(4);
        let mut rng = StdRng::seed_from_u64(42);
        let mut entity_states = HashMap::new();

        let entities = vec![WorldEntity {
            id: "jb1".into(),
            entity_type: "jukebox".into(),
            x: 0.0,
            y: 0.0,
        }];
        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 0.0,
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
            0.0,
        );

        assert!(result.feedback.is_empty());
        assert!(result.spawned_items.is_empty());
        match result.interaction_type {
            Some(InteractionType::Jukebox { entity_id }) => {
                assert_eq!(entity_id, "jb1");
            }
            other => panic!("Expected Jukebox interaction type, got {:?}", other),
        }
    }

    #[test]
    fn jukebox_build_prompt_always_actionable() {
        let item_defs = test_item_defs();
        let mut entity_defs = EntityDefs::new();
        entity_defs.insert(
            "jukebox".into(),
            EntityDef {
                id: "jukebox".into(),
                name: "Tavern Jukebox".into(),
                verb: "Listen".into(),
                yields: vec![],
                cooldown_secs: 0.0,
                max_harvests: 0,
                respawn_secs: 0.0,
                sprite_class: "jukebox".into(),
                interact_radius: 100.0,
                walk_speed: None,
                wander_radius: None,
                bob_amplitude: None,
                bob_frequency: None,
                playlist: Some(vec!["track-a".into(), "track-b".into()]),
                audio_radius: Some(400.0),
                store: None,
            },
        );

        let entities = vec![WorldEntity {
            id: "jb1".into(),
            entity_type: "jukebox".into(),
            x: 50.0,
            y: -10.0,
        }];
        let nearest = NearestInteractable::Entity {
            index: 0,
            distance: 5.0,
        };
        let entity_states = HashMap::new();

        let prompt = build_prompt(
            &nearest,
            &entities,
            &entity_defs,
            &[],
            &item_defs,
            &entity_states,
            0.0,
        );

        assert_eq!(prompt.verb, "Listen");
        assert_eq!(prompt.target_name, "Tavern Jukebox");
        assert!(prompt.actionable);
        assert_eq!(prompt.entity_id, Some("jb1".into()));
    }
}
