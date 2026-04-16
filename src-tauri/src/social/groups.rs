use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use harmony_groups::{GroupId, GroupOp, GroupState, MemberAddr, OpId};

/// A pending invite that has been received but not yet accepted or declined.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingGroupInvite {
    pub group_id: [u8; 16],
    pub inviter: [u8; 16],
    pub inviter_name: String,
    pub group_name: String,
    pub invite_op: harmony_groups::GroupOp,
    pub received_at: f64,
}

/// Manages all known groups: op logs, resolved states, and pending invites.
///
/// Persistence is per-group: each group's ops are stored as
/// `{data_dir}/groups/{hex_group_id}.json` (a JSON array of `GroupOp`).
/// An `index.json` in the same directory tracks which group IDs are known.
pub struct GroupManager {
    data_dir: PathBuf,
    /// Raw op logs, keyed by group ID.
    op_logs: BTreeMap<GroupId, Vec<GroupOp>>,
    /// Cached resolved state for each group.
    states: BTreeMap<GroupId, GroupState>,
    /// Pending invites we have received but not yet acted on.
    pub pending_invites: BTreeMap<GroupId, PendingGroupInvite>,
}

impl GroupManager {
    /// Create a new `GroupManager`, restoring persisted groups from `data_dir`.
    pub fn new(data_dir: PathBuf) -> Self {
        let mut mgr = Self {
            data_dir,
            op_logs: BTreeMap::new(),
            states: BTreeMap::new(),
            pending_invites: BTreeMap::new(),
        };
        mgr.load_all();
        mgr
    }

    /// Load all persisted groups from `{data_dir}/groups/*.json`.
    ///
    /// Each file is a JSON array of `GroupOp`. Invalid or unreadable files are
    /// silently skipped.
    fn load_all(&mut self) {
        let groups_dir = self.data_dir.join("groups");
        let read_dir = match std::fs::read_dir(&groups_dir) {
            Ok(rd) => rd,
            Err(_) => return, // directory doesn't exist yet — nothing to load
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            // Only process *.json files, skip index.json and *.tmp files.
            match path.extension().and_then(|e| e.to_str()) {
                Some("json") => {}
                _ => continue,
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_owned(),
                None => continue,
            };
            if stem == "index" {
                continue;
            }

            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let ops: Vec<GroupOp> = match serde_json::from_slice(&data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if ops.is_empty() {
                continue;
            }

            // Resolve the ops into a GroupState.
            let state = match harmony_groups::resolve(&ops) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let group_id = state.group_id;
            self.op_logs.insert(group_id, ops);
            self.states.insert(group_id, state);
        }
    }

    /// Persist a group's op log to `{data_dir}/groups/{hex_group_id}.json`
    /// and update `index.json`.
    fn persist_group(&self, group_id: GroupId) -> Result<(), String> {
        let groups_dir = self.data_dir.join("groups");
        std::fs::create_dir_all(&groups_dir)
            .map_err(|e| format!("Failed to create groups dir: {e}"))?;

        let hex_id = hex::encode(group_id);
        let ops_path = groups_dir.join(format!("{hex_id}.json"));

        let ops = self.op_logs.get(&group_id).map(Vec::as_slice).unwrap_or(&[]);
        let data =
            serde_json::to_vec(ops).map_err(|e| format!("Failed to serialize ops: {e}"))?;
        crate::persistence::atomic_write(&ops_path, &data, None)?;

        // Update index.json with the list of all known group IDs.
        let index_path = groups_dir.join("index.json");
        let all_ids: Vec<String> = self.op_logs.keys().map(hex::encode).collect();
        let index_data = serde_json::to_vec(&all_ids)
            .map_err(|e| format!("Failed to serialize index: {e}"))?;
        crate::persistence::atomic_write(&index_path, &index_data, None)?;

        Ok(())
    }

    /// Merge a single op into a group's DAG.
    ///
    /// Deduplicates by `op.id` before appending. Re-resolves the full DAG and
    /// caches the new state. Persists to disk.
    pub fn merge_op(
        &mut self,
        group_id: GroupId,
        op: GroupOp,
    ) -> Result<&GroupState, String> {
        let ops = self.op_logs.entry(group_id).or_default();
        // Dedup check.
        if ops.iter().any(|o| o.id == op.id) {
            // Already present — return the cached state (or re-resolve if missing).
            return self
                .states
                .get(&group_id)
                .ok_or_else(|| "state cache miss after dedup".to_string());
        }
        ops.push(op);

        let state = match harmony_groups::resolve(self.op_logs[&group_id].as_slice()) {
            Ok(s) => s,
            Err(e) => {
                self.op_logs.get_mut(&group_id).and_then(|v| v.pop());
                return Err(format!("resolve failed: {e:?}"));
            }
        };
        self.states.insert(group_id, state);

        self.persist_group(group_id)?;

        Ok(self.states.get(&group_id).unwrap())
    }

    /// Merge multiple ops at once (e.g. received from a sync peer).
    ///
    /// All ops are deduped before appending, then the DAG is re-resolved once.
    pub fn merge_ops(
        &mut self,
        group_id: GroupId,
        ops: Vec<GroupOp>,
    ) -> Result<&GroupState, String> {
        let log = self.op_logs.entry(group_id).or_default();
        let mut seen: std::collections::HashSet<OpId> =
            log.iter().map(|o| o.id).collect();
        let mut added = false;
        for op in ops {
            if seen.insert(op.id) {
                log.push(op);
                added = true;
            }
        }

        if !added {
            return self
                .states
                .get(&group_id)
                .ok_or_else(|| "no new ops and no cached state".to_string());
        }

        let state = harmony_groups::resolve(self.op_logs[&group_id].as_slice())
            .map_err(|e| format!("resolve failed: {e:?}"))?;
        self.states.insert(group_id, state);

        self.persist_group(group_id)?;

        Ok(self.states.get(&group_id).unwrap())
    }

    /// Return the cached resolved state for `group_id`, if known.
    pub fn get_state(&self, group_id: GroupId) -> Option<&GroupState> {
        self.states.get(&group_id)
    }

    /// Return the raw op log for `group_id`, if known.
    pub fn get_ops(&self, group_id: GroupId) -> Option<&[GroupOp]> {
        self.op_logs.get(&group_id).map(Vec::as_slice)
    }

    /// Return all groups where `our_addr` is a current (non-dissolved) member.
    pub fn my_groups(&self, our_addr: MemberAddr) -> Vec<&GroupState> {
        self.states
            .values()
            .filter(|s| !s.dissolved && s.is_member(&our_addr))
            .collect()
    }

    /// Return all known group IDs.
    pub fn known_group_ids(&self) -> Vec<GroupId> {
        self.op_logs.keys().copied().collect()
    }

    /// Return the current DAG head op IDs (ops that are not a parent of any other op).
    pub fn head_ops(&self, group_id: GroupId) -> Vec<OpId> {
        let ops = match self.op_logs.get(&group_id) {
            Some(v) => v,
            None => return vec![],
        };
        // Collect all op IDs that appear as parents.
        let mut referenced: std::collections::HashSet<OpId> = std::collections::HashSet::new();
        for op in ops {
            for &parent in &op.parents {
                referenced.insert(parent);
            }
        }
        // Heads are ops whose ID is not referenced as a parent.
        ops.iter()
            .filter(|o| !referenced.contains(&o.id))
            .map(|o| o.id)
            .collect()
    }

    /// Return op IDs that the remote peer is missing, based on their reported tips.
    pub fn ops_to_send(&self, group_id: GroupId, remote_tips: &[OpId]) -> Vec<GroupOp> {
        let ops = match self.op_logs.get(&group_id) {
            Some(v) => v,
            None => return vec![],
        };
        let ids_to_send = harmony_groups::ops_to_send(ops, remote_tips);
        let id_set: std::collections::HashSet<OpId> = ids_to_send.into_iter().collect();
        ops.iter().filter(|o| id_set.contains(&o.id)).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harmony_groups::{GroupAction, GroupMode, GroupOp};
    use tempfile::TempDir;

    const FOUNDER: MemberAddr = [0x01; 16];
    const ALICE: MemberAddr = [0x02; 16];
    const GROUP_ID_A: GroupId = [0xAA; 16];
    const GROUP_ID_B: GroupId = [0xBB; 16];

    fn genesis(author: MemberAddr, group_id: GroupId, name: &str) -> GroupOp {
        let (op, _) = GroupOp::new_unsigned(
            vec![],
            author,
            1_700_000_000,
            GroupAction::Create {
                group_id,
                name: name.to_string(),
                mode: GroupMode::InviteOnly,
            },
        );
        op
    }

    #[test]
    fn create_and_persist_group() {
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        let op = genesis(FOUNDER, GROUP_ID_A, "Alpha");
        mgr.merge_op(GROUP_ID_A, op).unwrap();

        // State should be resolved.
        let state = mgr.get_state(GROUP_ID_A).unwrap();
        assert_eq!(state.group_id, GROUP_ID_A);
        assert_eq!(state.name, "Alpha");
        assert!(state.is_member(&FOUNDER));

        // File should exist on disk.
        let hex_id = hex::encode(GROUP_ID_A);
        let file_path = dir.path().join("groups").join(format!("{hex_id}.json"));
        assert!(file_path.exists(), "persisted file should exist");
    }

    #[test]
    fn reload_from_disk() {
        let dir = TempDir::new().unwrap();

        // Create manager, persist a group.
        {
            let mut mgr = GroupManager::new(dir.path().to_path_buf());
            let op = genesis(FOUNDER, GROUP_ID_A, "Beta");
            mgr.merge_op(GROUP_ID_A, op).unwrap();
        }

        // Create a new manager from the same dir — it should reload.
        let mgr2 = GroupManager::new(dir.path().to_path_buf());
        let state = mgr2.get_state(GROUP_ID_A).expect("state should survive reload");
        assert_eq!(state.name, "Beta");
        assert!(state.is_member(&FOUNDER));
    }

    #[test]
    fn my_groups_filters_by_membership() {
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        // Group A: founded by FOUNDER.
        let op_a = genesis(FOUNDER, GROUP_ID_A, "GroupA");
        mgr.merge_op(GROUP_ID_A, op_a).unwrap();

        // Group B: founded by ALICE.
        let op_b = genesis(ALICE, GROUP_ID_B, "GroupB");
        mgr.merge_op(GROUP_ID_B, op_b).unwrap();

        // FOUNDER is a member of GroupA but not GroupB.
        let founder_groups = mgr.my_groups(FOUNDER);
        assert_eq!(founder_groups.len(), 1);
        assert_eq!(founder_groups[0].group_id, GROUP_ID_A);

        // ALICE is a member of GroupB but not GroupA.
        let alice_groups = mgr.my_groups(ALICE);
        assert_eq!(alice_groups.len(), 1);
        assert_eq!(alice_groups[0].group_id, GROUP_ID_B);
    }

    #[test]
    fn dedup_merge() {
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        let op = genesis(FOUNDER, GROUP_ID_A, "Gamma");
        // Merge the same op twice.
        mgr.merge_op(GROUP_ID_A, op.clone()).unwrap();
        mgr.merge_op(GROUP_ID_A, op).unwrap();

        // Op log should only contain the op once.
        let ops = mgr.get_ops(GROUP_ID_A).unwrap();
        assert_eq!(ops.len(), 1);

        // State should still be valid.
        let state = mgr.get_state(GROUP_ID_A).unwrap();
        assert_eq!(state.members.len(), 1);
    }
}
