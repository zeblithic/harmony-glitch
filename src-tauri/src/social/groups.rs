use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use harmony_groups::{GroupId, GroupOp, GroupState, MemberAddr, OpId, ResolveError};

/// Classifies the outcome of a `try_merge` so callers can distinguish
/// "genuinely new op" from "we already have it" and from "couldn't resolve
/// yet (missing ancestors)" and from "hard error that should not be retried".
///
/// `TransientFailure` covers local I/O errors (persist failures, disk full,
/// permission denied) that may succeed on a later retry. These ops are
/// buffered in the orphan pool so they aren't silently lost.
#[derive(Debug)]
enum MergeOutcome {
    Applied,
    Duplicate,
    MissingAncestor,
    TransientFailure(String),
    Rejected(String),
}

/// Typed error for the internal resolve+persist path. Lets the orphan
/// classifier pattern-match on concrete `ResolveError` variants rather than
/// substring-matching a Debug-formatted string — if the upstream enum is
/// renamed, extended, or its Debug output changes, this becomes a compile
/// error instead of a silent mis-classification.
#[derive(Debug)]
enum MergeError {
    Resolve(ResolveError),
    Persist(String),
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Preserve the legacy "resolve failed: {Debug}" surface so
            // existing IPC error messages don't change shape.
            Self::Resolve(e) => write!(f, "resolve failed: {e:?}"),
            Self::Persist(msg) => f.write_str(msg),
        }
    }
}

/// Map a typed `ResolveError` into the orphan-machinery outcome. Exhaustive
/// so new variants force an explicit classification decision at compile time.
fn classify_resolve_error(err: ResolveError) -> MergeOutcome {
    match err {
        // Structurally incomplete DAG — a later op may supply the missing
        // ancestor, so buffer in the orphan pool.
        ResolveError::NoGenesis
        | ResolveError::MissingParent { .. }
        | ResolveError::EmptyDag => MergeOutcome::MissingAncestor,
        // Structural defects of the ops we already hold. No amount of later
        // ops repairs them, so drop rather than buffer.
        ResolveError::MultipleGenesis
        | ResolveError::CycleDetected
        | ResolveError::InvalidGenesis
        | ResolveError::InvalidOpId { .. } => {
            MergeOutcome::Rejected(format!("resolve failed: {err:?}"))
        }
    }
}

/// Find the OpId of the most recent `Invite` op in `ops` that targets
/// `our_addr` and has not been referenced by an `Accept` op authored by us.
/// Does not filter the declined-invites set — that filtering is applied by
/// the caller (`GroupManager::find_outstanding_invite`).
fn outstanding_invite_op(ops: &[GroupOp], our_addr: MemberAddr) -> Option<OpId> {
    let accepted: std::collections::HashSet<OpId> = ops
        .iter()
        .filter_map(|o| match &o.action {
            harmony_groups::GroupAction::Accept { invite_op } if o.author == our_addr => {
                Some(*invite_op)
            }
            _ => None,
        })
        .collect();
    ops.iter()
        .filter(|o| {
            matches!(
                &o.action,
                harmony_groups::GroupAction::Invite { invitee } if *invitee == our_addr
            )
        })
        .filter(|o| !accepted.contains(&o.id))
        .max_by_key(|o| o.timestamp)
        .map(|o| o.id)
}

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
/// `load_all` discovers groups by scanning the directory.
pub struct GroupManager {
    data_dir: PathBuf,
    /// Raw op logs, keyed by group ID.
    op_logs: BTreeMap<GroupId, Vec<GroupOp>>,
    /// Cached resolved state for each group.
    states: BTreeMap<GroupId, GroupState>,
    /// Pending invites we have received but not yet acted on.
    pub pending_invites: BTreeMap<GroupId, PendingGroupInvite>,
    /// OpIds of invite ops the user explicitly declined. Persisted so that
    /// `rebuild_pending_invites` skips them on restart — otherwise the
    /// declined prompt would resurface every session. Purely local state;
    /// peers don't need to know the user declined (no Decline action in
    /// the group protocol).
    declined_invite_ops: std::collections::BTreeSet<OpId>,
    /// Ops that failed to merge (missing ancestors) — retried on each
    /// successful merge. In-memory only; lost on restart, which is fine
    /// since the sender will eventually re-broadcast or sync will catch up.
    /// Capped at `ORPHAN_POOL_CAP` per group; oldest entries are evicted
    /// first to bound memory use under a hostile gossip flood.
    orphan_ops: BTreeMap<GroupId, Vec<GroupOp>>,
}

/// Per-group orphan pool cap. Protects against memory exhaustion from a
/// peer gossiping large numbers of ops that reference ancestors we will
/// never receive (e.g. wrong group, spoofed ancestors, or a malicious flood).
const ORPHAN_POOL_CAP: usize = 64;

impl GroupManager {
    /// Create a new `GroupManager`, restoring persisted groups from `data_dir`.
    pub fn new(data_dir: PathBuf) -> Self {
        let mut mgr = Self {
            data_dir,
            op_logs: BTreeMap::new(),
            states: BTreeMap::new(),
            pending_invites: BTreeMap::new(),
            declined_invite_ops: std::collections::BTreeSet::new(),
            orphan_ops: BTreeMap::new(),
        };
        mgr.load_all();
        mgr.load_declined_invites();
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
            // Only process *.json files. Skip *.tmp files, and tolerate a
            // legacy index.json left over from an earlier implementation.
            match path.extension().and_then(|e| e.to_str()) {
                Some("json") => {}
                _ => continue,
            }
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_owned(),
                None => continue,
            };
            // A group's file stem is the hex encoding of its 16-byte ID.
            // Non-matching files (index.json, declined_invites.json, etc.)
            // are metadata and skipped here.
            if stem.len() != 32 || !stem.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }

            let data = match std::fs::read(&path) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("[groups] failed to read {}: {e}", path.display());
                    continue;
                }
            };
            let ops: Vec<GroupOp> = match serde_json::from_slice(&data) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("[groups] failed to parse {}: {e}", path.display());
                    continue;
                }
            };
            if ops.is_empty() {
                continue;
            }

            let state = match harmony_groups::resolve(&ops) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[groups] failed to resolve {}: {e}", path.display());
                    continue;
                }
            };
            let group_id = state.group_id;
            self.op_logs.insert(group_id, ops);
            self.states.insert(group_id, state);
        }
    }

    /// Load the set of declined invite OpIds from
    /// `{data_dir}/groups/declined_invites.json`. Missing or malformed files
    /// leave the set empty.
    fn load_declined_invites(&mut self) {
        let path = self.data_dir.join("groups").join("declined_invites.json");
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => return,
        };
        let hexes: Vec<String> = match serde_json::from_slice(&data) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[groups] failed to parse declined_invites.json: {e}");
                return;
            }
        };
        for h in hexes {
            if let Ok(bytes) = hex::decode(&h) {
                if let Ok(op_id) = <[u8; 32]>::try_from(bytes.as_slice()) {
                    self.declined_invite_ops.insert(op_id);
                }
            }
        }
    }

    /// Persist the declined invite set to
    /// `{data_dir}/groups/declined_invites.json` atomically.
    fn persist_declined_invites(&self) -> Result<(), String> {
        let groups_dir = self.data_dir.join("groups");
        std::fs::create_dir_all(&groups_dir)
            .map_err(|e| format!("Failed to create groups dir: {e}"))?;
        let path = groups_dir.join("declined_invites.json");
        let hexes: Vec<String> = self.declined_invite_ops.iter().map(hex::encode).collect();
        let data = serde_json::to_vec(&hexes)
            .map_err(|e| format!("Failed to serialize declined invites: {e}"))?;
        #[cfg(unix)]
        let mode = Some(0o600);
        #[cfg(not(unix))]
        let mode = None;
        crate::persistence::atomic_write(&path, &data, mode)
    }

    /// Mark the current outstanding invite targeting `our_addr` in `group_id`
    /// as declined, persist that decision, and remove any in-memory pending
    /// entry.
    ///
    /// Returns `true` if there was either a pending invite or an unaccepted
    /// invite op in the persisted log to decline. Returns `false` if neither
    /// exists (caller may want to surface that as a user-visible error).
    pub fn decline_invite(
        &mut self,
        group_id: GroupId,
        our_addr: MemberAddr,
    ) -> Result<bool, String> {
        let had_pending = self.pending_invites.remove(&group_id).is_some();
        // `find_outstanding_invite` filters by declined_invite_ops, so
        // re-declining a previously-declined invite finds nothing. For the
        // decline path we need the raw lookup.
        let outstanding = self
            .op_logs
            .get(&group_id)
            .and_then(|ops| outstanding_invite_op(ops, our_addr));
        let declined_any = if let Some(op_id) = outstanding {
            self.declined_invite_ops.insert(op_id);
            self.persist_declined_invites()?;
            true
        } else {
            false
        };
        Ok(had_pending || declined_any)
    }

    /// Persist a group's op log to `{data_dir}/groups/{hex_group_id}.json`.
    ///
    /// Each file is self-contained: `load_all` discovers groups by scanning
    /// the directory, so there's no separate index to keep in sync (which
    /// previously created a two-phase-commit hazard).
    fn persist_group(&self, group_id: GroupId) -> Result<(), String> {
        let groups_dir = self.data_dir.join("groups");
        std::fs::create_dir_all(&groups_dir)
            .map_err(|e| format!("Failed to create groups dir: {e}"))?;

        let hex_id = hex::encode(group_id);
        let ops_path = groups_dir.join(format!("{hex_id}.json"));

        let ops = self.op_logs.get(&group_id).map(Vec::as_slice).unwrap_or(&[]);
        let data =
            serde_json::to_vec(ops).map_err(|e| format!("Failed to serialize ops: {e}"))?;
        #[cfg(unix)]
        let mode = Some(0o600);
        #[cfg(not(unix))]
        let mode = None;

        crate::persistence::atomic_write(&ops_path, &data, mode)
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
        // Dedup check — duplicates are Ok, not an error.
        if let Some(ops) = self.op_logs.get(&group_id) {
            if ops.iter().any(|o| o.id == op.id) {
                return self
                    .states
                    .get(&group_id)
                    .ok_or_else(|| "state cache miss after dedup".to_string());
            }
        }

        self.try_resolve_and_persist(group_id, op)
            .map_err(|e| e.to_string())?;
        // `try_resolve_and_persist` populates `states` on success.
        self.states
            .get(&group_id)
            .ok_or_else(|| "state cache miss after merge".to_string())
    }

    /// Append `op` to the group's log and commit if both the DAG resolve and
    /// the on-disk persist succeed. On either failure, the push is rolled
    /// back so the log matches its pre-call state.
    fn try_resolve_and_persist(
        &mut self,
        group_id: GroupId,
        op: GroupOp,
    ) -> Result<(), MergeError> {
        let op_id = op.id;
        self.op_logs.entry(group_id).or_default().push(op);

        let state = match harmony_groups::resolve(self.op_logs[&group_id].as_slice()) {
            Ok(s) => s,
            Err(e) => {
                self.rollback_push(group_id, op_id);
                return Err(MergeError::Resolve(e));
            }
        };

        if let Err(e) = self.persist_group(group_id) {
            self.rollback_push(group_id, op_id);
            return Err(MergeError::Persist(e));
        }
        self.states.insert(group_id, state);
        Ok(())
    }

    /// Reverse the most recent push into `op_logs[group_id]`, asserting the
    /// popped op matches the one we pushed. Prevents the "future refactor
    /// appends between push and rollback" foot-gun: silently popping the
    /// wrong op would corrupt the log. A mismatch here is a programmer
    /// error, so we panic loudly rather than drift.
    fn rollback_push(&mut self, group_id: GroupId, expected_id: OpId) {
        let log = self
            .op_logs
            .get_mut(&group_id)
            .expect("rollback: op log must exist — we just pushed to it");
        match log.pop() {
            Some(popped) if popped.id == expected_id => {}
            Some(other) => panic!(
                "merge_op rollback popped wrong op: expected {:02x?}, got {:02x?}",
                &expected_id[..4],
                &other.id[..4]
            ),
            None => panic!("merge_op rollback: log was empty when rolling back push"),
        }
    }

    /// Merge a single op, buffering in the orphan pool on failure and retrying
    /// the pool on every successful merge.
    ///
    /// Returns a tuple `(merged, just_applied_ids)`:
    /// - `merged` is `true` if `op` (or a previously-orphaned op) became part
    ///   of the resolved DAG during this call.
    /// - `just_applied_ids` lists the `OpId`s that transitioned from orphan
    ///   to applied during this call. Duplicates and hard failures are NOT
    ///   included, so callers can safely emit events only for genuinely new
    ///   ops.
    pub fn merge_op_with_orphans(
        &mut self,
        group_id: GroupId,
        op: GroupOp,
    ) -> (bool, Vec<OpId>) {
        let op_id = op.id;
        let mut applied: Vec<OpId> = Vec::new();

        match self.try_merge(group_id, op.clone()) {
            MergeOutcome::Applied => applied.push(op_id),
            MergeOutcome::Duplicate => {
                // Already have it — no state change, no event.
            }
            MergeOutcome::MissingAncestor => {
                self.push_orphan(group_id, op);
            }
            MergeOutcome::TransientFailure(e) => {
                // Local I/O error (e.g. disk full). Buffer for retry so
                // the op is not lost when conditions improve.
                eprintln!("[groups] merge_op transient failure (will retry): {e}");
                self.push_orphan(group_id, op);
            }
            MergeOutcome::Rejected(e) => {
                // Structural rejection — will fail the same way on retry.
                eprintln!("[groups] merge_op rejected: {e}");
            }
        }

        // Always retry the orphan pool — a prior orphan may now resolve thanks
        // to ops that arrived in between, or thanks to the op we just merged.
        let newly_applied = self.retry_orphans(group_id);
        applied.extend(newly_applied);

        (!applied.is_empty(), applied)
    }

    /// Buffer `op` in the orphan pool for `group_id`, deduping by id and
    /// enforcing `ORPHAN_POOL_CAP`. When full, the oldest op is evicted —
    /// newer ops are more likely to reference recently-seen ancestors and
    /// therefore more likely to eventually resolve.
    fn push_orphan(&mut self, group_id: GroupId, op: GroupOp) {
        let pool = self.orphan_ops.entry(group_id).or_default();
        if pool.iter().any(|o| o.id == op.id) {
            return;
        }
        if pool.len() >= ORPHAN_POOL_CAP {
            pool.remove(0);
        }
        pool.push(op);
    }

    /// Attempt to merge `op` and classify the outcome. The classification
    /// depends on the pre-merge state and the error (if any): whether the op
    /// is already present (Duplicate), whether resolve failed because an
    /// ancestor is missing (MissingAncestor), whether a local I/O error
    /// occurred (TransientFailure — retryable), or whether the resolver
    /// rejected the op structurally (Rejected — not retryable).
    fn try_merge(&mut self, group_id: GroupId, op: GroupOp) -> MergeOutcome {
        if let Some(ops) = self.op_logs.get(&group_id) {
            if ops.iter().any(|o| o.id == op.id) {
                return MergeOutcome::Duplicate;
            }
        }
        match self.try_resolve_and_persist(group_id, op) {
            Ok(()) => MergeOutcome::Applied,
            Err(MergeError::Resolve(err)) => classify_resolve_error(err),
            // Persist/serialize/I/O errors — may succeed on a later retry.
            Err(MergeError::Persist(msg)) => MergeOutcome::TransientFailure(msg),
        }
    }

    /// Retry the orphan pool for `group_id`, applying every op that now resolves.
    /// Returns the `OpId`s that moved from orphan → applied.
    fn retry_orphans(&mut self, group_id: GroupId) -> Vec<OpId> {
        let mut applied: Vec<OpId> = Vec::new();
        // Loop until a full pass makes no progress — a single orphan may
        // unblock another.
        loop {
            let pool = match self.orphan_ops.get_mut(&group_id) {
                Some(p) if !p.is_empty() => std::mem::take(p),
                _ => break,
            };
            let mut still_orphaned: Vec<GroupOp> = Vec::new();
            let mut progress = false;
            for op in pool {
                let oid = op.id;
                match self.try_merge(group_id, op.clone()) {
                    MergeOutcome::Applied => {
                        applied.push(oid);
                        progress = true;
                    }
                    MergeOutcome::Duplicate => {
                        // Shouldn't happen (we just took from the orphan
                        // pool), but if it does, drop silently.
                        progress = true;
                    }
                    MergeOutcome::MissingAncestor => {
                        still_orphaned.push(op);
                    }
                    MergeOutcome::TransientFailure(e) => {
                        eprintln!("[groups] orphan retry transient failure: {e}");
                        // Keep in the pool for future retries.
                        still_orphaned.push(op);
                    }
                    MergeOutcome::Rejected(e) => {
                        eprintln!("[groups] orphan retry rejected: {e}");
                        progress = true;
                    }
                }
            }
            // `std::mem::take` above replaced the pool with an empty Vec.
            // Either refill it with the ops that couldn't resolve this pass,
            // or drop the key entirely so we don't accumulate empty entries
            // for every group that has ever had an orphan.
            if still_orphaned.is_empty() {
                self.orphan_ops.remove(&group_id);
            } else {
                self.orphan_ops.insert(group_id, still_orphaned);
            }
            if !progress {
                break;
            }
        }
        applied
    }

    /// Find any outstanding invite op targeting `our_addr` in the op log of
    /// `group_id`, excluding any invite the user previously declined. An
    /// invite is outstanding if it exists in the log but `our_addr` has not
    /// yet authored an `Accept` referencing it and is not currently a member.
    ///
    /// Used by `group_accept` to recover after restart, when the ephemeral
    /// `pending_invites` map is empty but the persisted op log still carries
    /// the invite.
    pub fn find_outstanding_invite(
        &self,
        group_id: GroupId,
        our_addr: MemberAddr,
    ) -> Option<GroupOp> {
        // Already a member — nothing outstanding.
        if let Some(state) = self.states.get(&group_id) {
            if state.is_member(&our_addr) {
                return None;
            }
        }
        let ops = self.op_logs.get(&group_id)?;
        let op_id = outstanding_invite_op(ops, our_addr)?;
        if self.declined_invite_ops.contains(&op_id) {
            return None;
        }
        ops.iter().find(|o| o.id == op_id).cloned()
    }

    /// Rebuild `pending_invites` from persisted op logs after restart.
    /// Called once `our_addr` is known (during Tauri setup).
    ///
    /// Returns the list of `GroupId`s that got a newly-populated pending
    /// invite, so the caller can emit `group_invite_received` events.
    pub fn rebuild_pending_invites(
        &mut self,
        our_addr: MemberAddr,
        now_secs: f64,
    ) -> Vec<GroupId> {
        let mut rebuilt: Vec<GroupId> = Vec::new();
        let group_ids: Vec<GroupId> = self.op_logs.keys().copied().collect();
        for gid in group_ids {
            if self.pending_invites.contains_key(&gid) {
                continue;
            }
            if let Some(invite_op) = self.find_outstanding_invite(gid, our_addr) {
                let group_name = self
                    .states
                    .get(&gid)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                self.pending_invites.insert(
                    gid,
                    PendingGroupInvite {
                        group_id: gid,
                        inviter: invite_op.author,
                        inviter_name: String::new(),
                        group_name,
                        invite_op,
                        received_at: now_secs,
                    },
                );
                rebuilt.push(gid);
            }
        }
        rebuilt
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

    /// Invariant enforcer: `pending_invites` should never hold an entry for
    /// a group the local user is already a member of. Callers should invoke
    /// this after any merge that can change membership (accepts, orphan
    /// resolves). Returns `true` if we are a current member.
    pub fn prune_pending_invite_if_member(
        &mut self,
        group_id: GroupId,
        our_addr: MemberAddr,
    ) -> bool {
        let is_member = self
            .states
            .get(&group_id)
            .map(|s| s.is_member(&our_addr))
            .unwrap_or(false);
        if is_member {
            self.pending_invites.remove(&group_id);
        }
        is_member
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

    #[test]
    fn orphan_invite_resolves_when_create_arrives() {
        // Simulates a late joiner: the Invite op arrives before the Create.
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        let create = genesis(FOUNDER, GROUP_ID_A, "LateJoin");
        let (invite, _) = GroupOp::new_unsigned(
            vec![create.id],
            FOUNDER,
            1_700_000_001,
            GroupAction::Invite { invitee: ALICE },
        );

        // Invite arrives first — can't resolve without its parent.
        let (progressed, applied) = mgr.merge_op_with_orphans(GROUP_ID_A, invite.clone());
        assert!(!progressed, "invite alone should not progress");
        assert!(applied.is_empty());
        assert!(mgr.get_state(GROUP_ID_A).is_none());

        // Create arrives — both should now apply (create directly, invite via retry).
        let (progressed, applied) = mgr.merge_op_with_orphans(GROUP_ID_A, create.clone());
        assert!(progressed);
        assert!(applied.contains(&create.id));
        assert!(applied.contains(&invite.id), "invite orphan should have been retried");

        let state = mgr.get_state(GROUP_ID_A).unwrap();
        assert!(state.is_member(&FOUNDER));
        // Alice is invited but hasn't accepted — still not a member.
        assert!(!state.is_member(&ALICE));
    }

    #[test]
    fn orphan_pool_entry_removed_when_drained() {
        // retry_orphans uses std::mem::take to drain the pool into a local
        // Vec. If every orphan resolves (or gets rejected), the map should
        // not retain an empty Vec for the group — otherwise every group
        // that ever had an orphan leaks a tiny entry forever.
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        let create = genesis(FOUNDER, GROUP_ID_A, "Drain");
        let (invite, _) = GroupOp::new_unsigned(
            vec![create.id],
            FOUNDER,
            1_700_000_001,
            GroupAction::Invite { invitee: ALICE },
        );

        // Invite orphans because its parent (create) hasn't arrived.
        let (_p, _a) = mgr.merge_op_with_orphans(GROUP_ID_A, invite.clone());
        assert_eq!(
            mgr.orphan_ops.get(&GROUP_ID_A).map(|v| v.len()),
            Some(1),
            "invite should be pooled"
        );

        // Create arrives — invite is drained from the orphan pool.
        let (_p, _a) = mgr.merge_op_with_orphans(GROUP_ID_A, create);
        assert!(
            !mgr.orphan_ops.contains_key(&GROUP_ID_A),
            "pool key must be removed once drained, not left as empty Vec"
        );
    }

    #[test]
    fn orphan_pool_dedups_repeated_ops() {
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        let (orphan, _) = GroupOp::new_unsigned(
            vec![[0xFF; 32]], // parent not in the DAG
            FOUNDER,
            1_700_000_001,
            GroupAction::Leave,
        );

        // Add the same orphan twice — pool should not grow.
        let (p1, a1) = mgr.merge_op_with_orphans(GROUP_ID_A, orphan.clone());
        let (p2, a2) = mgr.merge_op_with_orphans(GROUP_ID_A, orphan.clone());
        assert!(!p1 && !p2);
        assert!(a1.is_empty() && a2.is_empty());
        assert_eq!(mgr.orphan_ops.get(&GROUP_ID_A).map(|v| v.len()), Some(1));
    }

    #[test]
    fn orphan_pool_caps_at_limit_and_evicts_oldest() {
        // Hostile-flood safety: a peer that gossips many ops referencing a
        // never-seen ancestor cannot grow our memory without bound. When the
        // pool is full, the oldest op is evicted (newer ops are more likely
        // to resolve once ancestors arrive).
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        // Push CAP+3 distinct orphans all referencing a parent not in the DAG.
        let mut first_ids: Vec<OpId> = Vec::new();
        for i in 0..(ORPHAN_POOL_CAP + 3) {
            let (orphan, _) = GroupOp::new_unsigned(
                vec![[0xFF; 32]],
                FOUNDER,
                1_700_000_000 + i as u64,
                GroupAction::Leave,
            );
            if i < 3 {
                first_ids.push(orphan.id);
            }
            let (_p, _a) = mgr.merge_op_with_orphans(GROUP_ID_A, orphan);
        }

        let pool = mgr.orphan_ops.get(&GROUP_ID_A).unwrap();
        assert_eq!(
            pool.len(),
            ORPHAN_POOL_CAP,
            "pool must be capped at ORPHAN_POOL_CAP"
        );
        for id in first_ids {
            assert!(
                !pool.iter().any(|o| o.id == id),
                "oldest entries must be evicted first"
            );
        }
    }

    #[test]
    fn prune_pending_invite_when_membership_acquired() {
        // When an Accept op resolves (e.g. an orphaned one unblocking after
        // its ancestors arrive), a previously-stored pending_invites entry
        // for that group must be cleared — otherwise the frontend shows a
        // stale invite prompt for a group we already belong to.
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        let create = genesis(FOUNDER, GROUP_ID_A, "Prune");
        let (invite, _) = GroupOp::new_unsigned(
            vec![create.id],
            FOUNDER,
            1_700_000_001,
            GroupAction::Invite { invitee: ALICE },
        );
        mgr.merge_op(GROUP_ID_A, create.clone()).unwrap();
        mgr.merge_op(GROUP_ID_A, invite.clone()).unwrap();

        // Simulate the prompt-stored pending entry that the handler would
        // have created for Alice.
        mgr.pending_invites.insert(
            GROUP_ID_A,
            PendingGroupInvite {
                group_id: GROUP_ID_A,
                inviter: FOUNDER,
                inviter_name: String::new(),
                group_name: "Prune".into(),
                invite_op: invite.clone(),
                received_at: 0.0,
            },
        );

        // Before Alice accepts, she's not a member → no pruning.
        assert!(!mgr.prune_pending_invite_if_member(GROUP_ID_A, ALICE));
        assert!(mgr.pending_invites.contains_key(&GROUP_ID_A));

        // Alice accepts → she becomes a member → pruning drops the entry.
        let (accept, _) = GroupOp::new_unsigned(
            vec![invite.id],
            ALICE,
            1_700_000_002,
            GroupAction::Accept { invite_op: invite.id },
        );
        mgr.merge_op(GROUP_ID_A, accept).unwrap();
        assert!(mgr.prune_pending_invite_if_member(GROUP_ID_A, ALICE));
        assert!(!mgr.pending_invites.contains_key(&GROUP_ID_A));
    }

    #[test]
    fn merge_op_with_orphans_does_not_reapply_duplicates() {
        // A duplicate must not appear in `applied`, so the caller doesn't
        // re-emit group_state_changed / group_invite_received events.
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        let create = genesis(FOUNDER, GROUP_ID_A, "Dup");
        let (_progressed1, applied1) = mgr.merge_op_with_orphans(GROUP_ID_A, create.clone());
        assert_eq!(applied1, vec![create.id]);

        let (progressed2, applied2) = mgr.merge_op_with_orphans(GROUP_ID_A, create.clone());
        assert!(!progressed2, "duplicate must not be reported as progress");
        assert!(applied2.is_empty(), "duplicate must not appear in applied");
    }

    #[test]
    fn find_outstanding_invite_returns_unaccepted_invite() {
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());

        let create = genesis(FOUNDER, GROUP_ID_A, "Restart");
        let (invite, _) = GroupOp::new_unsigned(
            vec![create.id],
            FOUNDER,
            1_700_000_001,
            GroupAction::Invite { invitee: ALICE },
        );
        mgr.merge_op(GROUP_ID_A, create).unwrap();
        mgr.merge_op(GROUP_ID_A, invite.clone()).unwrap();

        // Alice hasn't accepted yet — invite is outstanding.
        assert_eq!(
            mgr.find_outstanding_invite(GROUP_ID_A, ALICE).map(|o| o.id),
            Some(invite.id)
        );

        // Once Alice accepts, the invite is no longer outstanding.
        let (accept, _) = GroupOp::new_unsigned(
            vec![invite.id],
            ALICE,
            1_700_000_002,
            GroupAction::Accept { invite_op: invite.id },
        );
        mgr.merge_op(GROUP_ID_A, accept).unwrap();
        assert!(mgr.find_outstanding_invite(GROUP_ID_A, ALICE).is_none());
    }

    #[test]
    fn rebuild_pending_invites_after_restart() {
        let dir = TempDir::new().unwrap();

        // Session 1: FOUNDER invites ALICE, then the app restarts before
        // ALICE accepts.
        let invite_id = {
            let mut mgr = GroupManager::new(dir.path().to_path_buf());
            let create = genesis(FOUNDER, GROUP_ID_A, "Persists");
            let (invite, _) = GroupOp::new_unsigned(
                vec![create.id],
                FOUNDER,
                1_700_000_001,
                GroupAction::Invite { invitee: ALICE },
            );
            mgr.merge_op(GROUP_ID_A, create).unwrap();
            mgr.merge_op(GROUP_ID_A, invite.clone()).unwrap();
            invite.id
        };

        // Session 2: fresh manager loads persisted ops. pending_invites
        // starts empty; rebuild populates it.
        let mut mgr2 = GroupManager::new(dir.path().to_path_buf());
        assert!(mgr2.pending_invites.is_empty());
        let rebuilt = mgr2.rebuild_pending_invites(ALICE, 42.0);
        assert_eq!(rebuilt, vec![GROUP_ID_A]);
        let pending = mgr2.pending_invites.get(&GROUP_ID_A).unwrap();
        assert_eq!(pending.invite_op.id, invite_id);
        assert_eq!(pending.inviter, FOUNDER);
        assert_eq!(pending.group_name, "Persists");
        assert_eq!(pending.received_at, 42.0);
    }

    #[test]
    fn declined_invite_does_not_resurface_after_restart() {
        let dir = TempDir::new().unwrap();

        // Session 1: create group, FOUNDER invites ALICE, ALICE declines.
        let invite_id = {
            let mut mgr = GroupManager::new(dir.path().to_path_buf());
            let create = genesis(FOUNDER, GROUP_ID_A, "Decline");
            let (invite, _) = GroupOp::new_unsigned(
                vec![create.id],
                FOUNDER,
                1_700_000_001,
                GroupAction::Invite { invitee: ALICE },
            );
            mgr.merge_op(GROUP_ID_A, create).unwrap();
            mgr.merge_op(GROUP_ID_A, invite.clone()).unwrap();
            // Rebuild populates pending_invites, then decline clears & persists.
            mgr.rebuild_pending_invites(ALICE, 0.0);
            assert!(mgr.decline_invite(GROUP_ID_A, ALICE).unwrap());
            invite.id
        };

        // Session 2: rebuild_pending_invites should skip the declined invite.
        let mut mgr2 = GroupManager::new(dir.path().to_path_buf());
        assert!(mgr2.declined_invite_ops.contains(&invite_id));
        let rebuilt = mgr2.rebuild_pending_invites(ALICE, 1.0);
        assert!(rebuilt.is_empty(), "declined invite must not resurface");
        assert!(mgr2.pending_invites.is_empty());
        assert!(mgr2.find_outstanding_invite(GROUP_ID_A, ALICE).is_none());
    }

    #[test]
    fn classify_resolve_error_covers_every_variant() {
        // Missing-ancestor variants → buffer in orphan pool for later retry.
        assert!(matches!(
            classify_resolve_error(ResolveError::NoGenesis),
            MergeOutcome::MissingAncestor
        ));
        assert!(matches!(
            classify_resolve_error(ResolveError::EmptyDag),
            MergeOutcome::MissingAncestor
        ));
        assert!(matches!(
            classify_resolve_error(ResolveError::MissingParent {
                op: [0xAA; 32],
                parent: [0xBB; 32],
            }),
            MergeOutcome::MissingAncestor
        ));

        // Structurally broken DAGs → drop, never resolve even with more ops.
        assert!(matches!(
            classify_resolve_error(ResolveError::MultipleGenesis),
            MergeOutcome::Rejected(_)
        ));
        assert!(matches!(
            classify_resolve_error(ResolveError::CycleDetected),
            MergeOutcome::Rejected(_)
        ));
        assert!(matches!(
            classify_resolve_error(ResolveError::InvalidGenesis),
            MergeOutcome::Rejected(_)
        ));
        assert!(matches!(
            classify_resolve_error(ResolveError::InvalidOpId { op: [0xCC; 32] }),
            MergeOutcome::Rejected(_)
        ));
    }

    #[test]
    fn merge_error_display_preserves_ipc_surface() {
        // IPC error messages are plain strings — make sure our typed error
        // still stringifies with the historical "resolve failed: …" prefix
        // and that persist errors pass through verbatim.
        let resolve = MergeError::Resolve(ResolveError::NoGenesis);
        assert!(resolve.to_string().starts_with("resolve failed:"));

        let persist = MergeError::Persist("disk full".into());
        assert_eq!(persist.to_string(), "disk full");
    }
}
