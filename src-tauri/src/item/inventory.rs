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

    /// Count total quantity of an item across all inventory slots.
    pub fn count_item(&self, item_id: &str) -> u32 {
        self.slots
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|stack| stack.item_id == item_id)
            .map(|stack| stack.count)
            .sum()
    }

    /// Check if `count` items of this type can fit in the inventory.
    /// More precise than `has_room_for` (which only checks for 1 item).
    /// Counts available room across existing stacks + empty slots.
    pub fn has_room_for_count(&self, item_id: &str, count: u32, defs: &ItemDefs) -> bool {
        let stack_limit = defs.get(item_id).map(|d| d.stack_limit).unwrap_or(1);
        let mut room: u32 = 0;
        for slot in &self.slots {
            match slot {
                Some(stack) if stack.item_id == item_id => {
                    room += stack_limit - stack.count;
                }
                None => {
                    room += stack_limit;
                }
                _ => {}
            }
            if room >= count {
                return true;
            }
        }
        room >= count
    }

    /// Remove `count` items of the given item_id across inventory slots.
    /// Caller must verify sufficient quantity exists first via `count_item`.
    pub fn remove_item(&mut self, item_id: &str, mut count: u32) {
        for slot in 0..self.capacity {
            if count == 0 {
                break;
            }
            let matches = self.slots[slot]
                .as_ref()
                .is_some_and(|s| s.item_id == item_id);
            if matches {
                let available = self.slots[slot].as_ref().unwrap().count;
                let to_remove = count.min(available);
                self.remove(slot, to_remove);
                count -= to_remove;
            }
        }
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
                base_cost: None,
                energy_value: None,
                mood_value: None,
                buff_effect: None,
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
                base_cost: None,
                energy_value: None,
                mood_value: None,
                buff_effect: None,
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

    #[test]
    fn count_item_empty_inventory() {
        let inv = Inventory::new(4);
        assert_eq!(inv.count_item("cherry"), 0);
    }

    #[test]
    fn count_item_single_slot() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        assert_eq!(inv.count_item("cherry"), 3);
        assert_eq!(inv.count_item("grain"), 0);
    }

    #[test]
    fn count_item_across_multiple_slots() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 5, &defs); // fills slot 0 (stack_limit=5)
        inv.add("cherry", 3, &defs); // goes to slot 1
        assert_eq!(inv.count_item("cherry"), 8);
    }

    #[test]
    fn has_room_for_count_empty_inventory() {
        let defs = test_defs();
        let inv = Inventory::new(4);
        assert!(inv.has_room_for_count("cherry", 10, &defs)); // 4 slots * 5 limit = 20 room
        assert!(!inv.has_room_for_count("cherry", 21, &defs));
    }

    #[test]
    fn has_room_for_count_partial_stack() {
        let defs = test_defs();
        let mut inv = Inventory::new(2);
        inv.add("cherry", 4, &defs); // slot 0: 4/5
                                     // Room: 1 in slot 0 + 5 in slot 1 = 6
        assert!(inv.has_room_for_count("cherry", 6, &defs));
        assert!(!inv.has_room_for_count("cherry", 7, &defs));
    }

    #[test]
    fn has_room_for_count_full_inventory() {
        let defs = test_defs();
        let mut inv = Inventory::new(1);
        inv.add("cherry", 5, &defs); // slot 0: full
        assert!(!inv.has_room_for_count("cherry", 1, &defs));
    }

    #[test]
    fn remove_item_by_id_single_slot() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 5, &defs);
        inv.remove_item("cherry", 3);
        assert_eq!(inv.count_item("cherry"), 2);
    }

    #[test]
    fn remove_item_by_id_across_slots() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 5, &defs); // slot 0: 5
        inv.add("cherry", 3, &defs); // slot 1: 3
        inv.remove_item("cherry", 7); // removes 5 from slot 0, 2 from slot 1
        assert_eq!(inv.count_item("cherry"), 1);
        assert!(inv.slots[0].is_none()); // slot 0 fully consumed
        assert_eq!(inv.slots[1].as_ref().unwrap().count, 1);
    }

    #[test]
    fn remove_item_clears_empty_slots() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        inv.remove_item("cherry", 3);
        assert!(inv.slots[0].is_none());
        assert_eq!(inv.count_item("cherry"), 0);
    }

    #[test]
    fn remove_item_does_not_touch_other_items() {
        let defs = test_defs();
        let mut inv = Inventory::new(4);
        inv.add("cherry", 3, &defs);
        inv.add("grain", 5, &defs);
        inv.remove_item("cherry", 3);
        assert_eq!(inv.count_item("grain"), 5);
    }
}
