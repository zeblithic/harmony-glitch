use crate::item::inventory::Inventory;
use crate::item::types::{ItemDefs, ItemStack};
use crate::trade::types::TradeId;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Records trade intent so that a crash between execute_trade() and
/// write_save_state() can be recovered on the next startup.
///
/// Written atomically before execution; deleted after save succeeds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeJournal {
    pub trade_id: TradeId,
    /// Items we offered (to be removed from our inventory).
    pub removed_items: Vec<ItemStack>,
    /// Currants we offered.
    pub removed_currants: u64,
    /// Items we receive from the peer.
    pub received_items: Vec<ItemStack>,
    /// Currants we receive from the peer.
    pub received_currants: u64,
}

/// Atomically write a trade journal to disk.
pub fn write_journal(path: &Path, journal: &TradeJournal) -> Result<(), String> {
    let json = serde_json::to_string(journal).map_err(|e| e.to_string())?;
    crate::persistence::atomic_write(path, json.as_bytes(), None)
}

/// Read a trade journal from disk. Returns None if missing or corrupted.
pub fn read_journal(path: &Path) -> Option<TradeJournal> {
    let json = match std::fs::read_to_string(path) {
        Ok(j) => j,
        Err(_) => return None,
    };
    match serde_json::from_str(&json) {
        Ok(j) => Some(j),
        Err(e) => {
            eprintln!("[journal] Corrupted trade journal: {e}");
            None
        }
    }
}

/// Delete the journal file. Best-effort — logs on failure.
pub fn clear_journal(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("[journal] Failed to clear trade journal: {e}");
        }
    }
}

/// Replay a journaled trade: remove offered items/currants, add received.
///
/// Called during startup recovery when the save state doesn't reflect a
/// completed trade that was journaled. Idempotent when replayed from
/// the same save state (since the save is reloaded from disk each startup).
pub fn recover(
    journal: &TradeJournal,
    inventory: &mut Inventory,
    currants: &mut u64,
    item_defs: &ItemDefs,
) {
    // Remove what we offered.
    for item in &journal.removed_items {
        inventory.remove_item(&item.item_id, item.count);
    }
    *currants = currants.saturating_sub(journal.removed_currants);

    // Add what we received.
    for item in &journal.received_items {
        let overflow = inventory.add(&item.item_id, item.count, item_defs);
        if overflow > 0 {
            eprintln!(
                "[journal] overflow of {} adding {} during recovery",
                overflow, item.item_id
            );
        }
    }
    *currants += journal.received_currants;
    eprintln!("[journal] recovered trade {}", journal.trade_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn test_item_defs() -> ItemDefs {
        let mut defs = HashMap::new();
        defs.insert(
            "cherry".into(),
            ItemDef {
                id: "cherry".into(),
                name: "Cherry".into(),
                description: "A cherry".into(),
                category: "food".into(),
                stack_limit: 50,
                icon: "cherry".into(),
                base_cost: Some(2),
                energy_value: None,
                mood_value: None,
            },
        );
        defs.insert(
            "grain".into(),
            ItemDef {
                id: "grain".into(),
                name: "Grain".into(),
                description: "A grain".into(),
                category: "food".into(),
                stack_limit: 99,
                icon: "grain".into(),
                base_cost: Some(1),
                energy_value: None,
                mood_value: None,
            },
        );
        defs
    }

    fn make_journal() -> TradeJournal {
        TradeJournal {
            trade_id: 42,
            removed_items: vec![ItemStack {
                item_id: "cherry".into(),
                count: 5,
            }],
            removed_currants: 100,
            received_items: vec![ItemStack {
                item_id: "grain".into(),
                count: 10,
            }],
            received_currants: 50,
        }
    }

    #[test]
    fn journal_round_trip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("trade_journal.json");
        let journal = make_journal();
        write_journal(&path, &journal).unwrap();
        let loaded = read_journal(&path).unwrap();
        assert_eq!(loaded.trade_id, 42);
        assert_eq!(loaded.removed_items.len(), 1);
        assert_eq!(loaded.removed_items[0].item_id, "cherry");
        assert_eq!(loaded.removed_items[0].count, 5);
        assert_eq!(loaded.removed_currants, 100);
        assert_eq!(loaded.received_items[0].item_id, "grain");
        assert_eq!(loaded.received_items[0].count, 10);
        assert_eq!(loaded.received_currants, 50);
    }

    #[test]
    fn missing_journal_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("trade_journal.json");
        assert!(read_journal(&path).is_none());
    }

    #[test]
    fn corrupted_journal_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("trade_journal.json");
        std::fs::write(&path, "not valid json!!!").unwrap();
        assert!(read_journal(&path).is_none());
    }

    #[test]
    fn clear_journal_removes_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("trade_journal.json");
        write_journal(&path, &make_journal()).unwrap();
        assert!(path.exists());
        clear_journal(&path);
        assert!(!path.exists());
    }

    #[test]
    fn clear_journal_noop_if_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("trade_journal.json");
        clear_journal(&path); // should not panic
    }

    #[test]
    fn recovery_applies_mutations() {
        let item_defs = test_item_defs();
        let mut inventory = Inventory::new(16);
        // Start with 10 cherries.
        inventory.add("cherry", 10, &item_defs);
        let mut currants = 200u64;

        let journal = make_journal();
        recover(&journal, &mut inventory, &mut currants, &item_defs);

        // 10 - 5 = 5 cherries remaining.
        assert_eq!(inventory.count_item("cherry"), 5);
        // 0 + 10 = 10 grain received.
        assert_eq!(inventory.count_item("grain"), 10);
        // 200 - 100 + 50 = 150 currants.
        assert_eq!(currants, 150);
    }

    #[test]
    fn recovery_saturates_currants() {
        let item_defs = test_item_defs();
        let mut inventory = Inventory::new(16);
        let mut currants = 50u64; // Less than the 100 offered

        let journal = make_journal();
        recover(&journal, &mut inventory, &mut currants, &item_defs);

        // Saturating sub: 50 - 100 = 0, then + 50 = 50.
        assert_eq!(currants, 50);
    }

    #[test]
    fn recovery_is_idempotent_from_same_save() {
        let item_defs = test_item_defs();
        let journal = make_journal();

        // Simulate two recoveries from the same save state.
        let run_recovery = || {
            let mut inventory = Inventory::new(16);
            inventory.add("cherry", 10, &item_defs);
            let mut currants = 200u64;
            recover(&journal, &mut inventory, &mut currants, &item_defs);
            (
                inventory.count_item("cherry"),
                inventory.count_item("grain"),
                currants,
            )
        };

        let (c1, g1, cur1) = run_recovery();
        let (c2, g2, cur2) = run_recovery();
        assert_eq!((c1, g1, cur1), (c2, g2, cur2));
    }
}
