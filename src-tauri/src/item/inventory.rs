use crate::item::types::{ItemDefs, ItemStack};

/// Player inventory — fixed-size array of optional item stacks.
#[derive(Debug, Clone)]
pub struct Inventory {
    pub slots: Vec<Option<ItemStack>>,
    pub capacity: usize,
}

impl Inventory {
    pub fn new(capacity: usize) -> Self {
        Self {
            slots: vec![None; capacity],
            capacity,
        }
    }

    /// Try to add items. Returns the count that couldn't fit.
    /// First stacks onto existing matching slots, then fills empty slots.
    pub fn add(&mut self, item_id: &str, mut count: u32, defs: &ItemDefs) -> u32 {
        let stack_limit = defs.get(item_id).map(|d| d.stack_limit).unwrap_or(1);

        // Phase 1: stack onto existing slots with the same item
        for slot in self.slots.iter_mut() {
            if count == 0 {
                break;
            }
            if let Some(stack) = slot {
                if stack.item_id == item_id && stack.count < stack_limit {
                    let room = stack_limit - stack.count;
                    let added = count.min(room);
                    stack.count += added;
                    count -= added;
                }
            }
        }

        // Phase 2: fill empty slots
        for slot in self.slots.iter_mut() {
            if count == 0 {
                break;
            }
            if slot.is_none() {
                let added = count.min(stack_limit);
                *slot = Some(ItemStack {
                    item_id: item_id.to_string(),
                    count: added,
                });
                count -= added;
            }
        }

        count // overflow
    }

    /// Remove items from a specific slot. Returns actual count removed.
    pub fn remove(&mut self, slot: usize, count: u32) -> u32 {
        if slot >= self.capacity {
            return 0;
        }
        if let Some(stack) = &mut self.slots[slot] {
            let removed = count.min(stack.count);
            stack.count -= removed;
            if stack.count == 0 {
                self.slots[slot] = None;
            }
            removed
        } else {
            0
        }
    }

    /// Drop entire stack from slot — returns what was there.
    pub fn drop_item(&mut self, slot: usize) -> Option<ItemStack> {
        if slot >= self.capacity {
            return None;
        }
        self.slots[slot].take()
    }

    /// Check if any room exists for this item type.
    pub fn has_room_for(&self, item_id: &str, defs: &ItemDefs) -> bool {
        let stack_limit = defs.get(item_id).map(|d| d.stack_limit).unwrap_or(1);

        // Any empty slot?
        if self.slots.iter().any(|s| s.is_none()) {
            return true;
        }

        // Any existing stack with room?
        self.slots.iter().any(|s| {
            s.as_ref()
                .is_some_and(|stack| stack.item_id == item_id && stack.count < stack_limit)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;

    fn test_defs() -> ItemDefs {
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
        defs.insert(
            "grain".into(),
            ItemDef {
                id: "grain".into(),
                name: "Grain".into(),
                description: "".into(),
                category: "food".into(),
                stack_limit: 10,
                icon: "grain".into(),
            },
        );
        defs
    }

    #[test]
    fn add_to_empty_inventory() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        let overflow = inv.add("cherry", 3, &defs);
        assert_eq!(overflow, 0);
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 3);
    }

    #[test]
    fn add_stacks_onto_existing() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        let overflow = inv.add("cherry", 2, &defs);
        assert_eq!(overflow, 0);
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 5);
        assert!(inv.slots[1].is_none());
    }

    #[test]
    fn add_overflows_to_new_slot() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 4, &defs);
        let overflow = inv.add("cherry", 3, &defs);
        assert_eq!(overflow, 0);
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 5);
        assert_eq!(inv.slots[1].as_ref().unwrap().count, 2);
    }

    #[test]
    fn add_returns_overflow_when_full() {
        let defs = test_defs();
        let mut inv = Inventory::new(2);
        inv.add("cherry", 5, &defs);
        inv.add("cherry", 5, &defs);
        let overflow = inv.add("cherry", 3, &defs);
        assert_eq!(overflow, 3);
    }

    #[test]
    fn add_different_items_use_separate_slots() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 2, &defs);
        inv.add("grain", 3, &defs);
        assert_eq!(inv.slots[0].as_ref().unwrap().item_id, "cherry");
        assert_eq!(inv.slots[1].as_ref().unwrap().item_id, "grain");
    }

    #[test]
    fn remove_from_slot() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 5, &defs);
        let removed = inv.remove(0, 3);
        assert_eq!(removed, 3);
        assert_eq!(inv.slots[0].as_ref().unwrap().count, 2);
    }

    #[test]
    fn remove_all_clears_slot() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        let removed = inv.remove(0, 3);
        assert_eq!(removed, 3);
        assert!(inv.slots[0].is_none());
    }

    #[test]
    fn remove_from_empty_slot() {
        let mut inv = Inventory::new(4);
        let removed = inv.remove(0, 5);
        assert_eq!(removed, 0);
    }

    #[test]
    fn remove_capped_at_available() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        let removed = inv.remove(0, 10);
        assert_eq!(removed, 3);
        assert!(inv.slots[0].is_none());
    }

    #[test]
    fn drop_item_returns_stack() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        let dropped = inv.drop_item(0).unwrap();
        assert_eq!(dropped.item_id, "cherry");
        assert_eq!(dropped.count, 3);
        assert!(inv.slots[0].is_none());
    }

    #[test]
    fn drop_item_from_empty_slot() {
        let mut inv = Inventory::new(4);
        assert!(inv.drop_item(0).is_none());
    }

    #[test]
    fn drop_item_out_of_bounds() {
        let mut inv = Inventory::new(4);
        assert!(inv.drop_item(99).is_none());
    }

    #[test]
    fn has_room_for_empty_inventory() {
        let defs = test_defs();
        let inv = Inventory::new(4);
        assert!(inv.has_room_for("cherry", &defs));
    }

    #[test]
    fn has_room_for_existing_stack_with_space() {
        let defs = test_defs();
        let mut inv = Inventory::new(1);
        inv.add("cherry", 3, &defs);
        assert!(inv.has_room_for("cherry", &defs));
    }

    #[test]
    fn no_room_when_full() {
        let defs = test_defs();
        let mut inv = Inventory::new(1);
        inv.add("cherry", 5, &defs);
        assert!(!inv.has_room_for("cherry", &defs));
        assert!(!inv.has_room_for("grain", &defs));
    }
}
