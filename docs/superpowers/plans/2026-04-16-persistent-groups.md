# Persistent Groups/Clubs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a decentralized persistent group membership system as a shared `harmony-groups` crate, integrated into harmony-glitch with IPC, networking, persistence, and frontend UI.

**Architecture:** Membership state is an Authenticated Causal DAG of ML-DSA signed operations. A deterministic resolver (topological sort + authorization replay + Strong Removal) materializes the current membership list. The core logic is a sans-I/O shared crate (`harmony-groups`); harmony-glitch wraps it with Tauri IPC, Zenoh networking, JSON persistence, and Svelte 5 frontend components.

**Tech Stack:** Rust (no_std-first crate with `std` feature), BLAKE3 (content addressing), postcard + serde (serialization), Zenoh pub/sub (networking), Tauri v2 (IPC), Svelte 5 runes (frontend)

**Repos:**
- Tasks 1–7: `~/work/zeblithic/harmony` (branch: `feat/harmony-groups-crate`)
- Tasks 8–10: `~/work/zeblithic/harmony-glitch` (branch: `feat/zeb-75-persistent-groups`)

**Spec:** `docs/superpowers/specs/2026-04-16-persistent-groups-design.md`

---

## File Structure

### harmony-groups crate (`~/work/zeblithic/harmony/crates/harmony-groups/`)

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Crate manifest with workspace deps |
| `src/lib.rs` | Module exports, no_std guard |
| `src/types.rs` | All type definitions: GroupId, Role, GroupAction, GroupOp, GroupState, MemberEntry |
| `src/op.rs` | Op creation, canonical serialization, BLAKE3 content-addressing |
| `src/dag.rs` | DAG construction, validation, ancestor queries |
| `src/resolver.rs` | Topological sort + authorization replay + Strong Removal |
| `src/error.rs` | ResolveError, ValidationError enums |
| `src/sync.rs` | find_missing_ops and tip-based sync helpers |

### harmony-glitch integration (`~/work/zeblithic/harmony-glitch/`)

| File | Responsibility |
|------|---------------|
| `src-tauri/Cargo.toml` | Add harmony-groups dependency |
| `src-tauri/src/social/groups.rs` | GroupManager: in-memory DAG store, persistence, Zenoh dispatch |
| `src-tauri/src/social/mod.rs` | Wire GroupManager into SocialState |
| `src-tauri/src/network/types.rs` | Add GroupOp variant to NetMessage |
| `src-tauri/src/lib.rs` | IPC commands, message dispatch, event emission |
| `src/lib/ipc.ts` | Frontend group commands and event types |
| `src/lib/components/GroupListPanel.svelte` | Group list in social sidebar |
| `src/lib/components/GroupDetailPanel.svelte` | Group member list with actions |
| `src/lib/components/GroupCreateDialog.svelte` | Group creation form |
| `src/lib/components/GroupInvitePrompt.svelte` | Incoming invite notification |

---

### Task 1: Crate Scaffold + Core Types

**Files:**
- Create: `crates/harmony-groups/Cargo.toml`
- Create: `crates/harmony-groups/src/lib.rs`
- Create: `crates/harmony-groups/src/types.rs`
- Create: `crates/harmony-groups/src/error.rs`
- Modify: `Cargo.toml` (workspace root)

**Working directory:** `~/work/zeblithic/harmony`

- [ ] **Step 1: Create feature branch**

```bash
cd ~/work/zeblithic/harmony
git fetch origin
git checkout -b feat/harmony-groups-crate origin/main
```

- [ ] **Step 2: Create Cargo.toml**

Create `crates/harmony-groups/Cargo.toml`:

```toml
[package]
name = "harmony-groups"
description = "Decentralized persistent group membership for the Harmony network"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
blake3 = { workspace = true }
hashbrown = { workspace = true, features = ["serde"] }
postcard = { workspace = true }
serde = { workspace = true, default-features = false, features = ["derive", "alloc"] }

[features]
default = ["std"]
std = [
    "blake3/std",
    "postcard/use-std",
    "serde/std",
]

[dev-dependencies]
serde_json = { workspace = true }
rand = { workspace = true }
```

- [ ] **Step 3: Register in workspace**

Add to the `members` array in `Cargo.toml` (workspace root), after `"crates/harmony-discovery"`:

```toml
    "crates/harmony-groups",
```

Add to `[workspace.dependencies]` section, after the `harmony-engram` line:

```toml
harmony-groups = { path = "crates/harmony-groups", default-features = false }
```

- [ ] **Step 4: Create error.rs**

Create `crates/harmony-groups/src/error.rs`:

```rust
use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    EmptyDag,
    NoGenesis,
    MultipleGenesis,
    MissingParent { op: [u8; 32], parent: [u8; 32] },
    CycleDetected,
    InvalidGenesis,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDag => write!(f, "empty DAG"),
            Self::NoGenesis => write!(f, "no genesis (Create) op found"),
            Self::MultipleGenesis => write!(f, "multiple genesis ops"),
            Self::MissingParent { op, parent } => {
                write!(f, "op {:02x}{:02x}… references missing parent {:02x}{:02x}…",
                    op[0], op[1], parent[0], parent[1])
            }
            Self::CycleDetected => write!(f, "cycle detected in DAG"),
            Self::InvalidGenesis => write!(f, "genesis op must be a Create action"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ResolveError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    Unauthorized(&'static str),
    InvalidAction(&'static str),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized(msg) => write!(f, "unauthorized: {msg}"),
            Self::InvalidAction(msg) => write!(f, "invalid action: {msg}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ValidationError {}
```

- [ ] **Step 5: Create types.rs**

Create `crates/harmony-groups/src/types.rs`:

```rust
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

pub type GroupId = [u8; 16];
pub type MemberAddr = [u8; 16];
pub type OpId = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum Role {
    Founder = 0,
    Officer = 1,
    Member = 2,
}

impl Role {
    pub fn power_level(self) -> u8 {
        self as u8
    }

    pub fn outranks(self, other: Role) -> bool {
        (self as u8) < (other as u8)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupMode {
    InviteOnly,
    Open,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupAction {
    Create { name: String, mode: GroupMode },
    Invite { target: MemberAddr },
    Join,
    Accept { invite_op: OpId },
    Leave,
    Kick { target: MemberAddr },
    Promote { target: MemberAddr, new_role: Role },
    Demote { target: MemberAddr, new_role: Role },
    Dissolve,
    UpdateInfo { name: Option<String>, mode: Option<GroupMode> },
}

/// Canonical payload for content addressing — excludes `id` and `signature`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupOpPayload {
    pub parents: Vec<OpId>,
    pub author: MemberAddr,
    pub timestamp: u64,
    pub action: GroupAction,
}

/// A single node in the membership DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupOp {
    pub id: OpId,
    pub parents: Vec<OpId>,
    pub author: MemberAddr,
    pub signature: Vec<u8>,
    pub timestamp: u64,
    pub action: GroupAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberEntry {
    pub role: Role,
    pub joined_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupState {
    pub group_id: GroupId,
    pub name: String,
    pub mode: GroupMode,
    pub founder: MemberAddr,
    pub members: BTreeMap<MemberAddr, MemberEntry>,
    pub dissolved: bool,
    pub head_ops: Vec<OpId>,
}

impl Default for GroupState {
    fn default() -> Self {
        Self {
            group_id: [0; 16],
            name: String::new(),
            mode: GroupMode::InviteOnly,
            founder: [0; 16],
            members: BTreeMap::new(),
            dissolved: false,
            head_ops: Vec::new(),
        }
    }
}

impl GroupState {
    pub fn role_of(&self, addr: &MemberAddr) -> Option<Role> {
        self.members.get(addr).map(|e| e.role)
    }

    pub fn is_member(&self, addr: &MemberAddr) -> bool {
        self.members.contains_key(addr)
    }
}
```

- [ ] **Step 6: Create lib.rs**

Create `crates/harmony-groups/src/lib.rs`:

```rust
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod dag;
pub mod error;
pub mod op;
pub mod resolver;
pub mod sync;
pub mod types;

pub use error::{ResolveError, ValidationError};
pub use resolver::resolve;
pub use sync::ops_to_send;
pub use types::{
    GroupAction, GroupId, GroupMode, GroupOp, GroupOpPayload, GroupState, MemberAddr, MemberEntry,
    OpId, Role,
};
```

Note: `dag`, `op`, `resolver`, and `sync` modules will be created in subsequent tasks. Create them as empty files for now so the crate compiles:

```bash
mkdir -p crates/harmony-groups/src
touch crates/harmony-groups/src/op.rs
touch crates/harmony-groups/src/dag.rs
touch crates/harmony-groups/src/resolver.rs
touch crates/harmony-groups/src/sync.rs
```

- [ ] **Step 7: Write tests for core types**

Add to the bottom of `crates/harmony-groups/src/types.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_power_levels() {
        assert_eq!(Role::Founder.power_level(), 0);
        assert_eq!(Role::Officer.power_level(), 1);
        assert_eq!(Role::Member.power_level(), 2);
    }

    #[test]
    fn role_outranks() {
        assert!(Role::Founder.outranks(Role::Officer));
        assert!(Role::Founder.outranks(Role::Member));
        assert!(Role::Officer.outranks(Role::Member));
        assert!(!Role::Member.outranks(Role::Officer));
        assert!(!Role::Officer.outranks(Role::Founder));
        assert!(!Role::Founder.outranks(Role::Founder));
    }

    #[test]
    fn role_ordering() {
        assert!(Role::Founder < Role::Officer);
        assert!(Role::Officer < Role::Member);
    }

    #[test]
    fn group_state_role_of() {
        let mut state = GroupState::default();
        let addr = [0xAA; 16];
        state.members.insert(addr, MemberEntry { role: Role::Officer, joined_at: 100 });
        assert_eq!(state.role_of(&addr), Some(Role::Officer));
        assert_eq!(state.role_of(&[0xBB; 16]), None);
    }

    #[test]
    fn group_action_serde_round_trip() {
        let action = GroupAction::Create {
            name: String::from("Test Club"),
            mode: GroupMode::InviteOnly,
        };
        let bytes = postcard::to_allocvec(&action).unwrap();
        let decoded: GroupAction = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(action, decoded);
    }

    #[test]
    fn group_op_serde_round_trip() {
        let op = GroupOp {
            id: [0x01; 32],
            parents: vec![],
            author: [0xAA; 16],
            signature: vec![0xDE, 0xAD],
            timestamp: 1713300000,
            action: GroupAction::Create {
                name: String::from("My Club"),
                mode: GroupMode::Open,
            },
        };
        let bytes = postcard::to_allocvec(&op).unwrap();
        let decoded: GroupOp = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(op, decoded);
    }
}
```

- [ ] **Step 8: Verify tests pass**

Run: `cd ~/work/zeblithic/harmony && cargo test -p harmony-groups`
Expected: all tests pass

- [ ] **Step 9: Commit**

```bash
git add crates/harmony-groups/ Cargo.toml
git commit -m "feat(harmony-groups): scaffold crate with core types and errors"
```

---

### Task 2: Op Creation + BLAKE3 Content Addressing

**Files:**
- Create: `crates/harmony-groups/src/op.rs`

**Working directory:** `~/work/zeblithic/harmony`

- [ ] **Step 1: Write tests for op creation**

Replace the contents of `crates/harmony-groups/src/op.rs` with:

```rust
use crate::types::*;
use alloc::vec::Vec;

impl GroupOp {
    /// Compute the content-addressed OpId from a canonical payload.
    pub fn compute_id(payload: &GroupOpPayload) -> OpId {
        let bytes = postcard::to_allocvec(payload).expect("valid payload serializes");
        let hash = blake3::hash(&bytes);
        let mut id = [0u8; 32];
        id.copy_from_slice(hash.as_bytes());
        id
    }

    /// Create a new unsigned op. Returns the op (with empty signature) and
    /// the canonical bytes the caller should sign with ML-DSA.
    pub fn new_unsigned(
        parents: Vec<OpId>,
        author: MemberAddr,
        timestamp: u64,
        action: GroupAction,
    ) -> (Self, Vec<u8>) {
        let payload = GroupOpPayload {
            parents: parents.clone(),
            author,
            timestamp,
            action: action.clone(),
        };
        let canonical = postcard::to_allocvec(&payload).expect("valid payload serializes");
        let id = Self::compute_id(&payload);
        let op = Self {
            id,
            parents,
            author,
            signature: Vec::new(),
            timestamp,
            action,
        };
        (op, canonical)
    }

    /// Verify that the op's ID matches its content.
    pub fn verify_id(&self) -> bool {
        let payload = GroupOpPayload {
            parents: self.parents.clone(),
            author: self.author,
            timestamp: self.timestamp,
            action: self.action.clone(),
        };
        Self::compute_id(&payload) == self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn compute_id_deterministic() {
        let payload = GroupOpPayload {
            parents: vec![],
            author: [0xAA; 16],
            timestamp: 1000,
            action: GroupAction::Create {
                name: String::from("Club"),
                mode: GroupMode::InviteOnly,
            },
        };
        let id1 = GroupOp::compute_id(&payload);
        let id2 = GroupOp::compute_id(&payload);
        assert_eq!(id1, id2);
        assert_ne!(id1, [0; 32]);
    }

    #[test]
    fn different_payloads_produce_different_ids() {
        let p1 = GroupOpPayload {
            parents: vec![],
            author: [0xAA; 16],
            timestamp: 1000,
            action: GroupAction::Create {
                name: String::from("Club A"),
                mode: GroupMode::InviteOnly,
            },
        };
        let p2 = GroupOpPayload {
            parents: vec![],
            author: [0xAA; 16],
            timestamp: 1000,
            action: GroupAction::Create {
                name: String::from("Club B"),
                mode: GroupMode::InviteOnly,
            },
        };
        assert_ne!(GroupOp::compute_id(&p1), GroupOp::compute_id(&p2));
    }

    #[test]
    fn new_unsigned_returns_consistent_id() {
        let (op, _canonical) = GroupOp::new_unsigned(
            vec![],
            [0xBB; 16],
            2000,
            GroupAction::Create {
                name: String::from("Test"),
                mode: GroupMode::Open,
            },
        );
        assert!(op.verify_id());
        assert!(op.signature.is_empty());
    }

    #[test]
    fn verify_id_detects_tamper() {
        let (mut op, _) = GroupOp::new_unsigned(
            vec![],
            [0xCC; 16],
            3000,
            GroupAction::Join,
        );
        op.timestamp = 9999;
        assert!(!op.verify_id());
    }

    #[test]
    fn canonical_bytes_are_stable() {
        let (_, canonical1) = GroupOp::new_unsigned(
            vec![[0x01; 32]],
            [0xDD; 16],
            4000,
            GroupAction::Leave,
        );
        let (_, canonical2) = GroupOp::new_unsigned(
            vec![[0x01; 32]],
            [0xDD; 16],
            4000,
            GroupAction::Leave,
        );
        assert_eq!(canonical1, canonical2);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd ~/work/zeblithic/harmony && cargo test -p harmony-groups`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/harmony-groups/src/op.rs
git commit -m "feat(harmony-groups): op creation with BLAKE3 content addressing"
```

---

### Task 3: DAG Building + Validation

**Files:**
- Create: `crates/harmony-groups/src/dag.rs`

**Working directory:** `~/work/zeblithic/harmony`

- [ ] **Step 1: Implement DAG builder**

Replace the contents of `crates/harmony-groups/src/dag.rs`:

```rust
use crate::error::ResolveError;
use crate::types::*;
use alloc::vec::Vec;
use hashbrown::{HashMap, HashSet};

pub struct Dag {
    pub ops: HashMap<OpId, GroupOp>,
    pub children: HashMap<OpId, Vec<OpId>>,
    pub genesis: OpId,
}

impl Dag {
    pub fn build(ops: &[GroupOp]) -> Result<Self, ResolveError> {
        if ops.is_empty() {
            return Err(ResolveError::EmptyDag);
        }

        let mut map: HashMap<OpId, GroupOp> = HashMap::new();
        let mut children: HashMap<OpId, Vec<OpId>> = HashMap::new();
        let mut genesis = None;

        for op in ops {
            if map.contains_key(&op.id) {
                continue;
            }
            map.insert(op.id, op.clone());

            if op.parents.is_empty() {
                match &op.action {
                    GroupAction::Create { .. } => {}
                    _ => return Err(ResolveError::InvalidGenesis),
                }
                if genesis.is_some() {
                    return Err(ResolveError::MultipleGenesis);
                }
                genesis = Some(op.id);
            }

            for parent in &op.parents {
                children.entry(*parent).or_default().push(op.id);
            }
        }

        let genesis = genesis.ok_or(ResolveError::NoGenesis)?;

        for op in map.values() {
            for parent in &op.parents {
                if !map.contains_key(parent) {
                    return Err(ResolveError::MissingParent {
                        op: op.id,
                        parent: *parent,
                    });
                }
            }
        }

        Ok(Self {
            ops: map,
            children,
            genesis,
        })
    }

    /// Returns the set of all ancestor OpIds for a given op (transitive parents).
    pub fn ancestors(&self, op_id: &OpId) -> HashSet<OpId> {
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        if let Some(op) = self.ops.get(op_id) {
            stack.extend_from_slice(&op.parents);
        }
        while let Some(id) = stack.pop() {
            if visited.insert(id) {
                if let Some(op) = self.ops.get(&id) {
                    stack.extend_from_slice(&op.parents);
                }
            }
        }
        visited
    }

    /// Returns OpIds that have no children (current DAG tips).
    pub fn head_ops(&self) -> Vec<OpId> {
        let all_children: HashSet<OpId> =
            self.children.values().flatten().copied().collect();
        let mut heads: Vec<OpId> = self
            .ops
            .keys()
            .filter(|id| !all_children.contains(id))
            .copied()
            .collect();
        heads.sort();
        heads
    }

    /// Compute in-degree (number of parents within the DAG) for each op.
    pub fn in_degrees(&self) -> HashMap<OpId, usize> {
        self.ops
            .values()
            .map(|op| (op.id, op.parents.len()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::*;
    use alloc::string::String;

    fn make_create(author: MemberAddr) -> GroupOp {
        let (op, _) = GroupOp::new_unsigned(
            vec![],
            author,
            100,
            GroupAction::Create {
                name: String::from("Test"),
                mode: GroupMode::InviteOnly,
            },
        );
        op
    }

    fn make_op(parents: Vec<OpId>, author: MemberAddr, ts: u64, action: GroupAction) -> GroupOp {
        let (op, _) = GroupOp::new_unsigned(parents, author, ts, action);
        op
    }

    #[test]
    fn build_single_genesis() {
        let genesis = make_create([0xAA; 16]);
        let dag = Dag::build(&[genesis.clone()]).unwrap();
        assert_eq!(dag.genesis, genesis.id);
        assert_eq!(dag.ops.len(), 1);
    }

    #[test]
    fn build_empty_dag() {
        assert_eq!(Dag::build(&[]).unwrap_err(), ResolveError::EmptyDag);
    }

    #[test]
    fn build_no_genesis() {
        let op = make_op(vec![[0xFF; 32]], [0xAA; 16], 100, GroupAction::Join);
        let result = Dag::build(&[op]);
        assert!(matches!(result, Err(ResolveError::MissingParent { .. }) | Err(ResolveError::NoGenesis)));
    }

    #[test]
    fn build_multiple_genesis() {
        let g1 = make_create([0xAA; 16]);
        let mut g2 = make_create([0xBB; 16]);
        g2.timestamp = 200;
        let (g2, _) = GroupOp::new_unsigned(vec![], [0xBB; 16], 200, GroupAction::Create {
            name: String::from("Other"),
            mode: GroupMode::Open,
        });
        assert_eq!(Dag::build(&[g1, g2]).unwrap_err(), ResolveError::MultipleGenesis);
    }

    #[test]
    fn build_missing_parent() {
        let genesis = make_create([0xAA; 16]);
        let orphan = make_op(vec![[0xFF; 32]], [0xBB; 16], 200, GroupAction::Join);
        let result = Dag::build(&[genesis, orphan]);
        assert!(matches!(result, Err(ResolveError::MissingParent { .. })));
    }

    #[test]
    fn build_linear_chain() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let invite = make_op(
            vec![genesis.id],
            founder,
            200,
            GroupAction::Invite { target: [0xBB; 16] },
        );
        let accept = make_op(
            vec![invite.id],
            [0xBB; 16],
            300,
            GroupAction::Accept { invite_op: invite.id },
        );
        let dag = Dag::build(&[genesis, invite, accept]).unwrap();
        assert_eq!(dag.ops.len(), 3);
    }

    #[test]
    fn head_ops_returns_tips() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let invite = make_op(
            vec![genesis.id],
            founder,
            200,
            GroupAction::Invite { target: [0xBB; 16] },
        );
        let dag = Dag::build(&[genesis, invite.clone()]).unwrap();
        assert_eq!(dag.head_ops(), vec![invite.id]);
    }

    #[test]
    fn ancestors_transitive() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let op1 = make_op(vec![genesis.id], founder, 200, GroupAction::Leave);
        let op2 = make_op(vec![op1.id], [0xBB; 16], 300, GroupAction::Join);
        let dag = Dag::build(&[genesis.clone(), op1.clone(), op2.clone()]).unwrap();
        let anc = dag.ancestors(&op2.id);
        assert!(anc.contains(&genesis.id));
        assert!(anc.contains(&op1.id));
        assert!(!anc.contains(&op2.id));
    }

    #[test]
    fn deduplicates_ops() {
        let genesis = make_create([0xAA; 16]);
        let dag = Dag::build(&[genesis.clone(), genesis.clone()]).unwrap();
        assert_eq!(dag.ops.len(), 1);
    }

    #[test]
    fn invalid_genesis_action() {
        let (op, _) = GroupOp::new_unsigned(vec![], [0xAA; 16], 100, GroupAction::Join);
        assert_eq!(Dag::build(&[op]).unwrap_err(), ResolveError::InvalidGenesis);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd ~/work/zeblithic/harmony && cargo test -p harmony-groups`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/harmony-groups/src/dag.rs
git commit -m "feat(harmony-groups): DAG building and validation"
```

---

### Task 4: Resolver — Topological Sort + Authorization Replay

**Files:**
- Create: `crates/harmony-groups/src/resolver.rs`

**Working directory:** `~/work/zeblithic/harmony`

This is the core algorithm. It combines topological sort with authorization replay in a single pass. Concurrent ops at the same authority level are validated as a batch against the pre-batch state (enabling mutual kicks between equal-rank members).

- [ ] **Step 1: Implement the resolver**

Replace the contents of `crates/harmony-groups/src/resolver.rs`:

```rust
use crate::dag::Dag;
use crate::error::ResolveError;
use crate::types::*;
use alloc::vec::Vec;
use hashbrown::HashMap;

/// Materialize a GroupState from a set of ops.
///
/// The resolver is a pure function: ops in → state out. No I/O, no crypto
/// verification (callers pre-verify signatures before inserting into the DAG).
///
/// Algorithm:
/// 1. Build DAG, compute in-degrees
/// 2. Process ops via Kahn's topological sort
/// 3. At each step, group ready (in-degree 0) ops by author authority level
/// 4. Process highest-authority batch first: validate ALL against current state,
///    then apply ALL valid ones (enables mutual kicks at equal rank)
/// 5. Within a batch, apply in lexicographic OpId order for determinism
pub fn resolve(group_id: GroupId, ops: &[GroupOp]) -> Result<GroupState, ResolveError> {
    let dag = Dag::build(ops)?;
    let mut state = GroupState::default();
    state.group_id = group_id;

    let mut in_degree = dag.in_degrees();
    let mut ready: Vec<OpId> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut processed = 0usize;

    while !ready.is_empty() {
        sort_ready(&mut ready, &dag, &state, processed == 0);

        let batch = extract_batch(&ready, &dag, &state, processed == 0);
        let batch_ids: Vec<OpId> = ready.drain(..batch).collect();

        let valid_ids: Vec<OpId> = batch_ids
            .iter()
            .filter(|id| is_authorized(&dag.ops[id], &state, &dag))
            .copied()
            .collect();

        for id in &valid_ids {
            apply_op(&dag.ops[id], &mut state);
        }

        processed += batch_ids.len();

        for op_id in &batch_ids {
            if let Some(children) = dag.children.get(op_id) {
                for child in children {
                    if let Some(deg) = in_degree.get_mut(child) {
                        *deg -= 1;
                        if *deg == 0 {
                            ready.push(*child);
                        }
                    }
                }
            }
        }
    }

    if processed != dag.ops.len() {
        return Err(ResolveError::CycleDetected);
    }

    state.head_ops = dag.head_ops();
    Ok(state)
}

fn sort_ready(ready: &mut Vec<OpId>, dag: &Dag, state: &GroupState, is_genesis: bool) {
    ready.sort_by(|a, b| {
        if is_genesis {
            return a.cmp(b);
        }
        let op_a = &dag.ops[a];
        let op_b = &dag.ops[b];
        let auth_a = state.role_of(&op_a.author).map(|r| r.power_level()).unwrap_or(255);
        let auth_b = state.role_of(&op_b.author).map(|r| r.power_level()).unwrap_or(255);
        auth_a.cmp(&auth_b).then_with(|| a.cmp(b))
    });
}

fn extract_batch(ready: &[OpId], dag: &Dag, state: &GroupState, is_genesis: bool) -> usize {
    if is_genesis || ready.len() <= 1 {
        return 1;
    }
    let first_op = &dag.ops[&ready[0]];
    let first_auth = state
        .role_of(&first_op.author)
        .map(|r| r.power_level())
        .unwrap_or(255);
    ready
        .iter()
        .position(|id| {
            let op = &dag.ops[id];
            let auth = state.role_of(&op.author).map(|r| r.power_level()).unwrap_or(255);
            auth != first_auth
        })
        .unwrap_or(ready.len())
}

fn is_authorized(op: &GroupOp, state: &GroupState, dag: &Dag) -> bool {
    if state.dissolved {
        return false;
    }
    match &op.action {
        GroupAction::Create { .. } => state.members.is_empty(),
        GroupAction::Invite { target } => {
            matches!(state.role_of(&op.author), Some(Role::Founder | Role::Officer))
                && !state.is_member(target)
        }
        GroupAction::Join => state.mode == GroupMode::Open && !state.is_member(&op.author),
        GroupAction::Accept { invite_op } => {
            if state.is_member(&op.author) {
                return false;
            }
            // Verify the referenced invite exists and targets us
            dag.ops.get(invite_op).is_some_and(|inv| {
                matches!(&inv.action, GroupAction::Invite { target } if *target == op.author)
            })
        }
        GroupAction::Leave => state.is_member(&op.author),
        GroupAction::Kick { target } => {
            let author_role = state.role_of(&op.author);
            let target_role = state.role_of(target);
            match (author_role, target_role) {
                (Some(Role::Founder), Some(_)) => op.author != *target,
                (Some(Role::Officer), Some(Role::Member)) => true,
                _ => false,
            }
        }
        GroupAction::Promote { target, new_role } => {
            matches!(state.role_of(&op.author), Some(Role::Founder))
                && state.is_member(target)
                && !matches!(new_role, Role::Founder)
        }
        GroupAction::Demote { target, new_role } => {
            matches!(state.role_of(&op.author), Some(Role::Founder))
                && state.is_member(target)
                && op.author != *target
                && target_role_valid_for_demote(state, target, new_role)
        }
        GroupAction::Dissolve => matches!(state.role_of(&op.author), Some(Role::Founder)),
        GroupAction::UpdateInfo { .. } => {
            matches!(state.role_of(&op.author), Some(Role::Founder))
        }
    }
}

fn target_role_valid_for_demote(state: &GroupState, target: &MemberAddr, new_role: &Role) -> bool {
    state
        .role_of(target)
        .is_some_and(|current| new_role.power_level() > current.power_level())
}

fn apply_op(op: &GroupOp, state: &mut GroupState) {
    match &op.action {
        GroupAction::Create { name, mode } => {
            state.name = name.clone();
            state.mode = *mode;
            state.founder = op.author;
            state.members.insert(
                op.author,
                MemberEntry {
                    role: Role::Founder,
                    joined_at: op.timestamp,
                },
            );
        }
        GroupAction::Invite { .. } => {}
        GroupAction::Join | GroupAction::Accept { .. } => {
            state.members.insert(
                op.author,
                MemberEntry {
                    role: Role::Member,
                    joined_at: op.timestamp,
                },
            );
        }
        GroupAction::Leave => {
            state.members.remove(&op.author);
            if op.author == state.founder {
                promote_new_founder(state);
            }
            if state.members.is_empty() {
                state.dissolved = true;
            }
        }
        GroupAction::Kick { target } => {
            let was_founder = *target == state.founder;
            state.members.remove(target);
            if was_founder {
                promote_new_founder(state);
            }
            if state.members.is_empty() {
                state.dissolved = true;
            }
        }
        GroupAction::Promote { target, new_role } | GroupAction::Demote { target, new_role } => {
            if let Some(entry) = state.members.get_mut(target) {
                entry.role = *new_role;
            }
        }
        GroupAction::Dissolve => {
            state.dissolved = true;
            state.members.clear();
        }
        GroupAction::UpdateInfo { name, mode } => {
            if let Some(n) = name {
                state.name = n.clone();
            }
            if let Some(m) = mode {
                state.mode = *m;
            }
        }
    }
}

fn promote_new_founder(state: &mut GroupState) {
    let new_founder = state
        .members
        .iter()
        .filter(|(_, e)| e.role == Role::Officer)
        .min_by_key(|(addr, e)| (e.joined_at, **addr))
        .or_else(|| {
            state
                .members
                .iter()
                .min_by_key(|(addr, e)| (e.joined_at, **addr))
        })
        .map(|(addr, _)| *addr);

    if let Some(addr) = new_founder {
        if let Some(entry) = state.members.get_mut(&addr) {
            entry.role = Role::Founder;
        }
        state.founder = addr;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::*;
    use alloc::string::String;

    fn gid() -> GroupId {
        [0x42; 16]
    }

    fn make_create(author: MemberAddr) -> GroupOp {
        let (op, _) = GroupOp::new_unsigned(
            vec![],
            author,
            100,
            GroupAction::Create {
                name: String::from("Test Club"),
                mode: GroupMode::InviteOnly,
            },
        );
        op
    }

    fn make_op(parents: Vec<OpId>, author: MemberAddr, ts: u64, action: GroupAction) -> GroupOp {
        let (op, _) = GroupOp::new_unsigned(parents, author, ts, action);
        op
    }

    #[test]
    fn resolve_genesis_only() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let state = resolve(gid(), &[genesis]).unwrap();
        assert_eq!(state.name, "Test Club");
        assert_eq!(state.mode, GroupMode::InviteOnly);
        assert_eq!(state.founder, founder);
        assert_eq!(state.members.len(), 1);
        assert_eq!(state.role_of(&founder), Some(Role::Founder));
        assert!(!state.dissolved);
    }

    #[test]
    fn resolve_invite_accept_flow() {
        let founder = [0xAA; 16];
        let joiner = [0xBB; 16];
        let genesis = make_create(founder);
        let invite = make_op(
            vec![genesis.id],
            founder,
            200,
            GroupAction::Invite { target: joiner },
        );
        let accept = make_op(
            vec![invite.id],
            joiner,
            300,
            GroupAction::Accept { invite_op: invite.id },
        );
        let state = resolve(gid(), &[genesis, invite, accept]).unwrap();
        assert_eq!(state.members.len(), 2);
        assert!(state.is_member(&joiner));
        assert_eq!(state.role_of(&joiner), Some(Role::Member));
    }

    #[test]
    fn resolve_open_group_join() {
        let founder = [0xAA; 16];
        let (genesis, _) = GroupOp::new_unsigned(
            vec![],
            founder,
            100,
            GroupAction::Create {
                name: String::from("Open Club"),
                mode: GroupMode::Open,
            },
        );
        let joiner = [0xCC; 16];
        let join = make_op(vec![genesis.id], joiner, 200, GroupAction::Join);
        let state = resolve(gid(), &[genesis, join]).unwrap();
        assert_eq!(state.members.len(), 2);
        assert!(state.is_member(&joiner));
    }

    #[test]
    fn resolve_join_rejected_on_invite_only() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let joiner = [0xCC; 16];
        let join = make_op(vec![genesis.id], joiner, 200, GroupAction::Join);
        let state = resolve(gid(), &[genesis, join]).unwrap();
        assert_eq!(state.members.len(), 1);
        assert!(!state.is_member(&joiner));
    }

    #[test]
    fn resolve_kick() {
        let founder = [0xAA; 16];
        let member = [0xBB; 16];
        let genesis = make_create(founder);
        let invite = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: member });
        let accept = make_op(vec![invite.id], member, 300, GroupAction::Accept { invite_op: invite.id });
        let kick = make_op(vec![accept.id], founder, 400, GroupAction::Kick { target: member });
        let state = resolve(gid(), &[genesis, invite, accept, kick]).unwrap();
        assert_eq!(state.members.len(), 1);
        assert!(!state.is_member(&member));
    }

    #[test]
    fn resolve_member_cannot_kick() {
        let founder = [0xAA; 16];
        let m1 = [0xBB; 16];
        let m2 = [0xCC; 16];
        let genesis = make_create(founder);
        let inv1 = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: m1 });
        let inv2 = make_op(vec![genesis.id], founder, 201, GroupAction::Invite { target: m2 });
        let acc1 = make_op(vec![inv1.id], m1, 300, GroupAction::Accept { invite_op: inv1.id });
        let acc2 = make_op(vec![inv2.id], m2, 301, GroupAction::Accept { invite_op: inv2.id });
        let bad_kick = make_op(vec![acc1.id, acc2.id], m1, 400, GroupAction::Kick { target: m2 });
        let state = resolve(gid(), &[genesis, inv1, inv2, acc1, acc2, bad_kick]).unwrap();
        assert_eq!(state.members.len(), 3);
        assert!(state.is_member(&m2));
    }

    #[test]
    fn resolve_promote_demote() {
        let founder = [0xAA; 16];
        let member = [0xBB; 16];
        let genesis = make_create(founder);
        let invite = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: member });
        let accept = make_op(vec![invite.id], member, 300, GroupAction::Accept { invite_op: invite.id });
        let promote = make_op(vec![accept.id], founder, 400, GroupAction::Promote { target: member, new_role: Role::Officer });
        let state = resolve(gid(), &[genesis, invite, accept, promote]).unwrap();
        assert_eq!(state.role_of(&member), Some(Role::Officer));

        let demote = make_op(vec![promote.id], founder, 500, GroupAction::Demote { target: member, new_role: Role::Member });
        let state2 = resolve(gid(), &[genesis, invite, accept, promote, demote]).unwrap();
        assert_eq!(state2.role_of(&member), Some(Role::Member));
    }

    #[test]
    fn resolve_officer_can_invite_and_kick_member() {
        let founder = [0xAA; 16];
        let officer = [0xBB; 16];
        let target = [0xCC; 16];
        let genesis = make_create(founder);
        let inv_off = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: officer });
        let acc_off = make_op(vec![inv_off.id], officer, 300, GroupAction::Accept { invite_op: inv_off.id });
        let promote = make_op(vec![acc_off.id], founder, 400, GroupAction::Promote { target: officer, new_role: Role::Officer });
        let inv_target = make_op(vec![promote.id], officer, 500, GroupAction::Invite { target });
        let acc_target = make_op(vec![inv_target.id], target, 600, GroupAction::Accept { invite_op: inv_target.id });
        let kick = make_op(vec![acc_target.id], officer, 700, GroupAction::Kick { target });
        let state = resolve(gid(), &[genesis, inv_off, acc_off, promote, inv_target, acc_target, kick]).unwrap();
        assert!(!state.is_member(&target));
        assert!(state.is_member(&officer));
    }

    #[test]
    fn resolve_officer_cannot_kick_officer() {
        let founder = [0xAA; 16];
        let off1 = [0xBB; 16];
        let off2 = [0xCC; 16];
        let genesis = make_create(founder);
        let inv1 = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: off1 });
        let inv2 = make_op(vec![genesis.id], founder, 201, GroupAction::Invite { target: off2 });
        let acc1 = make_op(vec![inv1.id], off1, 300, GroupAction::Accept { invite_op: inv1.id });
        let acc2 = make_op(vec![inv2.id], off2, 301, GroupAction::Accept { invite_op: inv2.id });
        let p1 = make_op(vec![acc1.id], founder, 400, GroupAction::Promote { target: off1, new_role: Role::Officer });
        let p2 = make_op(vec![acc2.id], founder, 401, GroupAction::Promote { target: off2, new_role: Role::Officer });
        let bad_kick = make_op(vec![p1.id, p2.id], off1, 500, GroupAction::Kick { target: off2 });
        let state = resolve(gid(), &[genesis, inv1, inv2, acc1, acc2, p1, p2, bad_kick]).unwrap();
        assert!(state.is_member(&off2));
    }

    #[test]
    fn resolve_dissolve() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let dissolve = make_op(vec![genesis.id], founder, 200, GroupAction::Dissolve);
        let state = resolve(gid(), &[genesis, dissolve]).unwrap();
        assert!(state.dissolved);
        assert!(state.members.is_empty());
    }

    #[test]
    fn resolve_leave_auto_promotes_officer() {
        let founder = [0xAA; 16];
        let officer = [0xBB; 16];
        let genesis = make_create(founder);
        let invite = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: officer });
        let accept = make_op(vec![invite.id], officer, 300, GroupAction::Accept { invite_op: invite.id });
        let promote = make_op(vec![accept.id], founder, 400, GroupAction::Promote { target: officer, new_role: Role::Officer });
        let leave = make_op(vec![promote.id], founder, 500, GroupAction::Leave);
        let state = resolve(gid(), &[genesis, invite, accept, promote, leave]).unwrap();
        assert_eq!(state.founder, officer);
        assert_eq!(state.role_of(&officer), Some(Role::Founder));
        assert!(!state.is_member(&founder));
    }

    #[test]
    fn resolve_leave_auto_promotes_member_if_no_officers() {
        let founder = [0xAA; 16];
        let member = [0xBB; 16];
        let genesis = make_create(founder);
        let invite = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: member });
        let accept = make_op(vec![invite.id], member, 300, GroupAction::Accept { invite_op: invite.id });
        let leave = make_op(vec![accept.id], founder, 400, GroupAction::Leave);
        let state = resolve(gid(), &[genesis, invite, accept, leave]).unwrap();
        assert_eq!(state.founder, member);
        assert_eq!(state.role_of(&member), Some(Role::Founder));
    }

    #[test]
    fn resolve_last_member_leaves_dissolves() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let leave = make_op(vec![genesis.id], founder, 200, GroupAction::Leave);
        let state = resolve(gid(), &[genesis, leave]).unwrap();
        assert!(state.dissolved);
        assert!(state.members.is_empty());
    }

    #[test]
    fn resolve_update_info() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let update = make_op(
            vec![genesis.id],
            founder,
            200,
            GroupAction::UpdateInfo {
                name: Some(String::from("New Name")),
                mode: Some(GroupMode::Open),
            },
        );
        let state = resolve(gid(), &[genesis, update]).unwrap();
        assert_eq!(state.name, "New Name");
        assert_eq!(state.mode, GroupMode::Open);
    }

    #[test]
    fn resolve_ops_after_dissolve_rejected() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let dissolve = make_op(vec![genesis.id], founder, 200, GroupAction::Dissolve);
        let late_join = make_op(vec![dissolve.id], [0xBB; 16], 300, GroupAction::Join);
        let state = resolve(gid(), &[genesis, dissolve, late_join]).unwrap();
        assert!(state.dissolved);
        assert!(state.members.is_empty());
    }

    #[test]
    fn resolve_non_member_promote_rejected() {
        let founder = [0xAA; 16];
        let stranger = [0xFF; 16];
        let genesis = make_create(founder);
        let bad_promote = make_op(
            vec![genesis.id],
            stranger,
            200,
            GroupAction::Promote { target: founder, new_role: Role::Officer },
        );
        let state = resolve(gid(), &[genesis, bad_promote]).unwrap();
        assert_eq!(state.role_of(&founder), Some(Role::Founder));
    }

    #[test]
    fn resolve_duplicate_accept_is_noop() {
        let founder = [0xAA; 16];
        let joiner = [0xBB; 16];
        let genesis = make_create(founder);
        let invite = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: joiner });
        let accept1 = make_op(vec![invite.id], joiner, 300, GroupAction::Accept { invite_op: invite.id });
        let accept2 = make_op(vec![accept1.id], joiner, 400, GroupAction::Accept { invite_op: invite.id });
        let state = resolve(gid(), &[genesis, invite, accept1, accept2]).unwrap();
        assert_eq!(state.members.len(), 2);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd ~/work/zeblithic/harmony && cargo test -p harmony-groups`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/harmony-groups/src/resolver.rs
git commit -m "feat(harmony-groups): resolver with topological sort and authorization replay"
```

---

### Task 5: Strong Removal — Concurrent Conflict Resolution

**Files:**
- Modify: `crates/harmony-groups/src/resolver.rs` (add conflict resolution tests)

**Working directory:** `~/work/zeblithic/harmony`

The batch-processing design in Task 4 already handles Strong Removal: concurrent ops at the same authority level are validated against the pre-batch state, and higher-authority ops sort first. This task adds comprehensive tests proving it works for all spec scenarios.

- [ ] **Step 1: Add Strong Removal tests**

Append these tests inside the existing `mod tests` in `crates/harmony-groups/src/resolver.rs`:

```rust
    // ── Strong Removal / Conflict Resolution ──────────────────────────

    #[test]
    fn concurrent_founder_demotes_officer_voids_officer_kick() {
        // Founder demotes Officer A; Officer A concurrently kicks Member B.
        // Officer A's kick should be voided.
        let founder = [0x01; 16];
        let officer = [0x02; 16];
        let member = [0x03; 16];

        let genesis = make_create(founder);
        let inv_off = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: officer });
        let inv_mem = make_op(vec![genesis.id], founder, 201, GroupAction::Invite { target: member });
        let acc_off = make_op(vec![inv_off.id], officer, 300, GroupAction::Accept { invite_op: inv_off.id });
        let acc_mem = make_op(vec![inv_mem.id], member, 301, GroupAction::Accept { invite_op: inv_mem.id });
        let promote = make_op(vec![acc_off.id, acc_mem.id], founder, 400, GroupAction::Promote { target: officer, new_role: Role::Officer });

        // Branch point: both ops have `promote` as parent (concurrent)
        let demote = make_op(vec![promote.id], founder, 500, GroupAction::Demote { target: officer, new_role: Role::Member });
        let kick = make_op(vec![promote.id], officer, 500, GroupAction::Kick { target: member });

        let state = resolve(gid(), &[genesis, inv_off, inv_mem, acc_off, acc_mem, promote, demote, kick]).unwrap();

        // Founder's demote wins (power 0 > power 1), officer's kick voided
        assert_eq!(state.role_of(&officer), Some(Role::Member));
        assert!(state.is_member(&member), "member should survive — officer's kick was voided");
    }

    #[test]
    fn concurrent_mutual_officer_kicks() {
        // Two Officers concurrently kick each other → both should be removed.
        let founder = [0x01; 16];
        let off_a = [0x02; 16];
        let off_b = [0x03; 16];

        let genesis = make_create(founder);
        let inv_a = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: off_a });
        let inv_b = make_op(vec![genesis.id], founder, 201, GroupAction::Invite { target: off_b });
        let acc_a = make_op(vec![inv_a.id], off_a, 300, GroupAction::Accept { invite_op: inv_a.id });
        let acc_b = make_op(vec![inv_b.id], off_b, 301, GroupAction::Accept { invite_op: inv_b.id });
        let prom_a = make_op(vec![acc_a.id], founder, 400, GroupAction::Promote { target: off_a, new_role: Role::Officer });
        let prom_b = make_op(vec![acc_b.id], founder, 401, GroupAction::Promote { target: off_b, new_role: Role::Officer });

        // Branch point: both kicks have the same parents (concurrent, equal authority)
        // Officers can't kick officers, so both should fail authorization
        let kick_a = make_op(vec![prom_a.id, prom_b.id], off_a, 500, GroupAction::Kick { target: off_b });
        let kick_b = make_op(vec![prom_a.id, prom_b.id], off_b, 500, GroupAction::Kick { target: off_a });

        let state = resolve(gid(), &[genesis, inv_a, inv_b, acc_a, acc_b, prom_a, prom_b, kick_a, kick_b]).unwrap();

        // Officers can't kick other officers — both kicks fail authorization
        assert!(state.is_member(&off_a));
        assert!(state.is_member(&off_b));
    }

    #[test]
    fn concurrent_founder_kick_voids_officer_promote() {
        // Officer promotes Member while Founder concurrently kicks that Officer.
        // Officer's promote should be voided.
        let founder = [0x01; 16];
        let officer = [0x02; 16];
        let member = [0x03; 16];

        let genesis = make_create(founder);
        let inv_off = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: officer });
        let inv_mem = make_op(vec![genesis.id], founder, 201, GroupAction::Invite { target: member });
        let acc_off = make_op(vec![inv_off.id], officer, 300, GroupAction::Accept { invite_op: inv_off.id });
        let acc_mem = make_op(vec![inv_mem.id], member, 301, GroupAction::Accept { invite_op: inv_mem.id });
        let promote = make_op(vec![acc_off.id, acc_mem.id], founder, 400, GroupAction::Promote { target: officer, new_role: Role::Officer });

        // Concurrent: founder kicks officer, officer invites a new person
        let kick = make_op(vec![promote.id], founder, 500, GroupAction::Kick { target: officer });
        let new_invite = make_op(vec![promote.id], officer, 500, GroupAction::Invite { target: [0x04; 16] });

        let state = resolve(gid(), &[genesis, inv_off, inv_mem, acc_off, acc_mem, promote, kick, new_invite]).unwrap();

        assert!(!state.is_member(&officer), "officer should be kicked");
        // Officer's invite is voided because officer was kicked by higher authority
    }

    #[test]
    fn concurrent_ops_deterministic_regardless_of_input_order() {
        // Same ops in different insertion order must produce identical state.
        let founder = [0x01; 16];
        let member = [0xBB; 16];
        let genesis = make_create(founder);
        let invite = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: member });
        let accept = make_op(vec![invite.id], member, 300, GroupAction::Accept { invite_op: invite.id });
        let kick = make_op(vec![accept.id], founder, 400, GroupAction::Kick { target: member });

        let ops_forward = vec![genesis.clone(), invite.clone(), accept.clone(), kick.clone()];
        let ops_reverse = vec![kick.clone(), accept.clone(), invite.clone(), genesis.clone()];
        let ops_shuffled = vec![accept.clone(), genesis.clone(), kick.clone(), invite.clone()];

        let s1 = resolve(gid(), &ops_forward).unwrap();
        let s2 = resolve(gid(), &ops_reverse).unwrap();
        let s3 = resolve(gid(), &ops_shuffled).unwrap();

        assert_eq!(s1.members, s2.members);
        assert_eq!(s2.members, s3.members);
        assert_eq!(s1.dissolved, s2.dissolved);
        assert_eq!(s2.dissolved, s3.dissolved);
    }

    #[test]
    fn kicked_member_subsequent_ops_rejected() {
        let founder = [0x01; 16];
        let member = [0xBB; 16];
        let genesis = make_create(founder);
        let invite = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: member });
        let accept = make_op(vec![invite.id], member, 300, GroupAction::Accept { invite_op: invite.id });
        let kick = make_op(vec![accept.id], founder, 400, GroupAction::Kick { target: member });
        // Kicked member tries to leave (should be rejected — not a member anymore)
        let late_leave = make_op(vec![kick.id], member, 500, GroupAction::Leave);
        let state = resolve(gid(), &[genesis, invite, accept, kick, late_leave]).unwrap();
        assert!(!state.is_member(&member));
        assert_eq!(state.members.len(), 1);
    }
```

- [ ] **Step 2: Run tests**

Run: `cd ~/work/zeblithic/harmony && cargo test -p harmony-groups`
Expected: all tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/harmony-groups/src/resolver.rs
git commit -m "test(harmony-groups): Strong Removal and conflict resolution scenarios"
```

---

### Task 6: Sync Helpers + Property-Based Shuffle Test

**Files:**
- Create: `crates/harmony-groups/src/sync.rs`
- Modify: `crates/harmony-groups/src/resolver.rs` (add shuffle invariant test)

**Working directory:** `~/work/zeblithic/harmony`

- [ ] **Step 1: Implement sync helpers**

Replace the contents of `crates/harmony-groups/src/sync.rs`:

```rust
use crate::types::*;
use alloc::vec::Vec;
use hashbrown::HashSet;

/// Given our local DAG tips and a remote peer's tips, determine which of our
/// ops the remote peer is likely missing. Returns OpIds the remote doesn't have.
///
/// This is a best-effort heuristic: if we have ops that are descendants of the
/// remote's tips, the remote is behind. We send those ops plus any ops not
/// reachable from the remote's tips.
pub fn ops_to_send(local_ops: &[GroupOp], remote_tips: &[OpId]) -> Vec<OpId> {
    let remote_tip_set: HashSet<OpId> = remote_tips.iter().copied().collect();
    let local_ids: HashSet<OpId> = local_ops.iter().map(|op| op.id).collect();

    if remote_tips.is_empty() {
        return local_ops.iter().map(|op| op.id).collect();
    }

    // Walk backwards from each remote tip to find all ancestors the remote has
    let mut remote_has: HashSet<OpId> = HashSet::new();
    let parent_map: hashbrown::HashMap<OpId, &[OpId]> = local_ops
        .iter()
        .map(|op| (op.id, op.parents.as_slice()))
        .collect();

    let mut stack: Vec<OpId> = remote_tips
        .iter()
        .filter(|id| local_ids.contains(id))
        .copied()
        .collect();
    while let Some(id) = stack.pop() {
        if remote_has.insert(id) {
            if let Some(parents) = parent_map.get(&id) {
                stack.extend_from_slice(parents);
            }
        }
    }

    local_ops
        .iter()
        .filter(|op| !remote_has.contains(&op.id))
        .map(|op| op.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::*;
    use alloc::string::String;

    fn make_create(author: MemberAddr) -> GroupOp {
        let (op, _) = GroupOp::new_unsigned(
            vec![],
            author,
            100,
            GroupAction::Create { name: String::from("Test"), mode: GroupMode::InviteOnly },
        );
        op
    }

    fn make_op(parents: Vec<OpId>, author: MemberAddr, ts: u64, action: GroupAction) -> GroupOp {
        let (op, _) = GroupOp::new_unsigned(parents, author, ts, action);
        op
    }

    #[test]
    fn empty_remote_tips_sends_all() {
        let genesis = make_create([0xAA; 16]);
        let result = ops_to_send(&[genesis.clone()], &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], genesis.id);
    }

    #[test]
    fn remote_at_same_tip_sends_nothing() {
        let genesis = make_create([0xAA; 16]);
        let result = ops_to_send(&[genesis.clone()], &[genesis.id]);
        assert!(result.is_empty());
    }

    #[test]
    fn remote_behind_gets_missing_ops() {
        let founder = [0xAA; 16];
        let genesis = make_create(founder);
        let op1 = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: [0xBB; 16] });
        let op2 = make_op(vec![op1.id], [0xBB; 16], 300, GroupAction::Accept { invite_op: op1.id });

        let result = ops_to_send(&[genesis.clone(), op1.clone(), op2.clone()], &[genesis.id]);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&op1.id));
        assert!(result.contains(&op2.id));
    }

    #[test]
    fn unknown_remote_tip_sends_all() {
        let genesis = make_create([0xAA; 16]);
        let result = ops_to_send(&[genesis.clone()], &[[0xFF; 32]]);
        assert_eq!(result.len(), 1);
    }
}
```

- [ ] **Step 2: Add shuffle invariant property test**

Append to the test module in `crates/harmony-groups/src/resolver.rs`:

```rust
    #[test]
    fn shuffle_invariant_randomized() {
        // The single most important invariant: shuffling op input order
        // must always produce the identical materialized state.
        use rand::seq::SliceRandom;
        use rand::SeedableRng;

        let founder = [0x01; 16];
        let m1 = [0x02; 16];
        let m2 = [0x03; 16];
        let m3 = [0x04; 16];

        let genesis = make_create(founder);
        let inv1 = make_op(vec![genesis.id], founder, 200, GroupAction::Invite { target: m1 });
        let inv2 = make_op(vec![genesis.id], founder, 201, GroupAction::Invite { target: m2 });
        let inv3 = make_op(vec![genesis.id], founder, 202, GroupAction::Invite { target: m3 });
        let acc1 = make_op(vec![inv1.id], m1, 300, GroupAction::Accept { invite_op: inv1.id });
        let acc2 = make_op(vec![inv2.id], m2, 301, GroupAction::Accept { invite_op: inv2.id });
        let acc3 = make_op(vec![inv3.id], m3, 302, GroupAction::Accept { invite_op: inv3.id });
        let promote = make_op(vec![acc1.id], founder, 400, GroupAction::Promote { target: m1, new_role: Role::Officer });
        let kick = make_op(vec![acc2.id, promote.id], m1, 500, GroupAction::Kick { target: m2 });

        let all_ops = vec![genesis, inv1, inv2, inv3, acc1, acc2, acc3, promote, kick];

        let reference = resolve(gid(), &all_ops).unwrap();

        let mut rng = rand::rngs::StdRng::seed_from_u64(12345);
        for _ in 0..50 {
            let mut shuffled = all_ops.clone();
            shuffled.shuffle(&mut rng);
            let result = resolve(gid(), &shuffled).unwrap();
            assert_eq!(result.members, reference.members, "shuffle produced different members");
            assert_eq!(result.founder, reference.founder, "shuffle produced different founder");
            assert_eq!(result.dissolved, reference.dissolved, "shuffle produced different dissolved");
            assert_eq!(result.name, reference.name, "shuffle produced different name");
        }
    }
```

- [ ] **Step 3: Run tests**

Run: `cd ~/work/zeblithic/harmony && cargo test -p harmony-groups`
Expected: all tests pass (including the 50-iteration shuffle test)

- [ ] **Step 4: Commit**

```bash
git add crates/harmony-groups/src/sync.rs crates/harmony-groups/src/resolver.rs
git commit -m "feat(harmony-groups): sync helpers and shuffle-invariant property test"
```

---

### Task 7: GroupManager + Persistence (harmony-glitch)

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/social/groups.rs`
- Modify: `src-tauri/src/social/mod.rs`
- Modify: `src-tauri/src/network/types.rs`

**Working directory:** `~/work/zeblithic/harmony-glitch`

- [ ] **Step 1: Create feature branch**

```bash
cd ~/work/zeblithic/harmony-glitch
git fetch origin
git checkout -b feat/zeb-75-persistent-groups origin/main
```

- [ ] **Step 2: Add harmony-groups dependency**

Add to `src-tauri/Cargo.toml` after the `harmony-zenoh` line:

```toml
harmony-groups = { path = "../../harmony/crates/harmony-groups" }
```

- [ ] **Step 3: Add GroupOp to NetMessage**

In `src-tauri/src/network/types.rs`, add a new variant to the `NetMessage` enum after the `Social` variant:

```rust
    GroupOp(harmony_groups::GroupOp),
```

Add a round-trip test for the new variant in the existing test module in the same file.

- [ ] **Step 4: Create groups.rs**

Create `src-tauri/src/social/groups.rs`:

```rust
use harmony_groups::{
    resolve, ops_to_send, GroupAction, GroupId, GroupMode, GroupOp, GroupOpPayload, GroupState,
    MemberAddr, MemberEntry, OpId, Role,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Manages all groups the local player belongs to or has been invited to.
pub struct GroupManager {
    /// GroupId → list of ops in the DAG
    groups: BTreeMap<GroupId, Vec<GroupOp>>,
    /// GroupId → cached materialized state (re-resolved on mutation)
    states: BTreeMap<GroupId, GroupState>,
    /// Pending incoming invites: GroupId → (inviter, invite op, group name)
    pub pending_invites: BTreeMap<GroupId, PendingGroupInvite>,
    /// Data directory for persistence
    data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingGroupInvite {
    pub group_id: GroupId,
    pub inviter: MemberAddr,
    pub inviter_name: String,
    pub group_name: String,
    pub invite_op: GroupOp,
    pub received_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupIndexEntry {
    pub name: String,
    pub our_role: String,
    pub member_count: usize,
    pub op_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GroupIndex {
    groups: BTreeMap<String, GroupIndexEntry>,
}

impl GroupManager {
    pub fn new(data_dir: PathBuf) -> Self {
        let mut mgr = Self {
            groups: BTreeMap::new(),
            states: BTreeMap::new(),
            pending_invites: BTreeMap::new(),
            data_dir,
        };
        mgr.load_all();
        mgr
    }

    fn groups_dir(&self) -> PathBuf {
        self.data_dir.join("groups")
    }

    fn group_path(&self, group_id: &GroupId) -> PathBuf {
        self.groups_dir().join(format!("{}.json", hex::encode(group_id)))
    }

    fn index_path(&self) -> PathBuf {
        self.groups_dir().join("index.json")
    }

    fn load_all(&mut self) {
        let dir = self.groups_dir();
        if !dir.exists() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if stem == "index" {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(ops): Result<Vec<GroupOp>, _> = serde_json::from_slice(&bytes) else {
                continue;
            };
            let Ok(group_id_bytes) = hex::decode(stem) else {
                continue;
            };
            if group_id_bytes.len() != 16 {
                continue;
            }
            let mut group_id = [0u8; 16];
            group_id.copy_from_slice(&group_id_bytes);
            if let Ok(state) = resolve(group_id, &ops) {
                self.states.insert(group_id, state);
                self.groups.insert(group_id, ops);
            }
        }
    }

    fn persist_group(&self, group_id: &GroupId) {
        let dir = self.groups_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Some(ops) = self.groups.get(group_id) {
            if let Ok(json) = serde_json::to_string_pretty(ops) {
                let _ = crate::persistence::atomic_write(
                    &self.group_path(group_id),
                    json.as_bytes(),
                    None,
                );
            }
        }
        self.persist_index();
    }

    fn persist_index(&self) {
        let mut index = GroupIndex {
            groups: BTreeMap::new(),
        };
        for (gid, state) in &self.states {
            index.groups.insert(
                hex::encode(gid),
                GroupIndexEntry {
                    name: state.name.clone(),
                    our_role: format!("{:?}", state.role_of(&[0; 16]).unwrap_or(Role::Member)),
                    member_count: state.members.len(),
                    op_count: self.groups.get(gid).map(|o| o.len()).unwrap_or(0),
                },
            );
        }
        if let Ok(json) = serde_json::to_string_pretty(&index) {
            let _ = crate::persistence::atomic_write(
                &self.index_path(),
                json.as_bytes(),
                None,
            );
        }
    }

    /// Add an op to a group's DAG, re-resolve, and persist.
    pub fn merge_op(&mut self, group_id: GroupId, op: GroupOp) -> Result<&GroupState, String> {
        let ops = self.groups.entry(group_id).or_default();
        if ops.iter().any(|o| o.id == op.id) {
            return self.states.get(&group_id).ok_or("group not found".into());
        }
        ops.push(op);
        let state = resolve(group_id, ops).map_err(|e| format!("{e}"))?;
        self.states.insert(group_id, state);
        self.persist_group(&group_id);
        self.states.get(&group_id).ok_or("resolve succeeded but state missing".into())
    }

    /// Merge multiple ops (e.g., from a sync response).
    pub fn merge_ops(&mut self, group_id: GroupId, new_ops: Vec<GroupOp>) -> Result<&GroupState, String> {
        let ops = self.groups.entry(group_id).or_default();
        let existing: std::collections::HashSet<OpId> = ops.iter().map(|o| o.id).collect();
        for op in new_ops {
            if !existing.contains(&op.id) {
                ops.push(op);
            }
        }
        let state = resolve(group_id, ops).map_err(|e| format!("{e}"))?;
        self.states.insert(group_id, state);
        self.persist_group(&group_id);
        self.states.get(&group_id).ok_or("resolve succeeded but state missing".into())
    }

    pub fn get_state(&self, group_id: &GroupId) -> Option<&GroupState> {
        self.states.get(group_id)
    }

    pub fn get_ops(&self, group_id: &GroupId) -> Option<&[GroupOp]> {
        self.groups.get(group_id).map(|v| v.as_slice())
    }

    pub fn my_groups(&self, our_addr: &MemberAddr) -> Vec<&GroupState> {
        self.states
            .values()
            .filter(|s| s.is_member(our_addr) && !s.dissolved)
            .collect()
    }

    pub fn known_group_ids(&self) -> Vec<GroupId> {
        self.groups.keys().copied().collect()
    }

    pub fn head_ops(&self, group_id: &GroupId) -> Vec<OpId> {
        self.states
            .get(group_id)
            .map(|s| s.head_ops.clone())
            .unwrap_or_default()
    }

    pub fn ops_to_send(&self, group_id: &GroupId, remote_tips: &[OpId]) -> Vec<GroupOp> {
        let Some(ops) = self.groups.get(group_id) else {
            return vec![];
        };
        let ids_to_send = ops_to_send(ops, remote_tips);
        let id_set: std::collections::HashSet<OpId> = ids_to_send.into_iter().collect();
        ops.iter().filter(|op| id_set.contains(&op.id)).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_create(author: MemberAddr) -> (GroupOp, Vec<u8>) {
        GroupOp::new_unsigned(
            vec![],
            author,
            100,
            GroupAction::Create {
                name: "Test Club".into(),
                mode: GroupMode::InviteOnly,
            },
        )
    }

    #[test]
    fn create_and_persist_group() {
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());
        let founder = [0xAA; 16];
        let group_id: GroupId = [0x42; 16];
        let (genesis, _) = make_create(founder);
        mgr.merge_op(group_id, genesis).unwrap();

        assert!(mgr.get_state(&group_id).is_some());
        assert_eq!(mgr.get_state(&group_id).unwrap().name, "Test Club");

        // Verify file exists
        let path = dir.path().join("groups").join(format!("{}.json", hex::encode(group_id)));
        assert!(path.exists());
    }

    #[test]
    fn reload_from_disk() {
        let dir = TempDir::new().unwrap();
        let founder = [0xAA; 16];
        let group_id: GroupId = [0x42; 16];
        let (genesis, _) = make_create(founder);

        {
            let mut mgr = GroupManager::new(dir.path().to_path_buf());
            mgr.merge_op(group_id, genesis).unwrap();
        }

        let mgr2 = GroupManager::new(dir.path().to_path_buf());
        assert!(mgr2.get_state(&group_id).is_some());
        assert_eq!(mgr2.get_state(&group_id).unwrap().name, "Test Club");
    }

    #[test]
    fn my_groups_filters_by_membership() {
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());
        let founder = [0xAA; 16];
        let other = [0xBB; 16];
        let g1: GroupId = [0x01; 16];
        let g2: GroupId = [0x02; 16];

        let (gen1, _) = GroupOp::new_unsigned(vec![], founder, 100, GroupAction::Create { name: "Club A".into(), mode: GroupMode::InviteOnly });
        let (gen2, _) = GroupOp::new_unsigned(vec![], other, 100, GroupAction::Create { name: "Club B".into(), mode: GroupMode::InviteOnly });

        mgr.merge_op(g1, gen1).unwrap();
        mgr.merge_op(g2, gen2).unwrap();

        let my = mgr.my_groups(&founder);
        assert_eq!(my.len(), 1);
        assert_eq!(my[0].name, "Club A");
    }

    #[test]
    fn dedup_merge() {
        let dir = TempDir::new().unwrap();
        let mut mgr = GroupManager::new(dir.path().to_path_buf());
        let founder = [0xAA; 16];
        let group_id: GroupId = [0x42; 16];
        let (genesis, _) = make_create(founder);
        mgr.merge_op(group_id, genesis.clone()).unwrap();
        mgr.merge_op(group_id, genesis).unwrap(); // duplicate
        assert_eq!(mgr.get_ops(&group_id).unwrap().len(), 1);
    }
}
```

- [ ] **Step 5: Wire GroupManager into SocialState**

Add to `src-tauri/src/social/mod.rs`:

Add the module declaration at the top:
```rust
pub mod groups;
```

Note: GroupManager is NOT part of SocialState — it's managed separately as a Tauri managed state because it needs its own persistence lifecycle and doesn't participate in the game tick loop. It will be registered as `app.manage(GroupManagerWrapper(...))` in lib.rs (Task 9).

- [ ] **Step 6: Add dev-dependency for tempfile**

Add to `src-tauri/Cargo.toml` under `[dev-dependencies]`:

```toml
tempfile = "3"
```

- [ ] **Step 7: Run tests**

Run: `cd ~/work/zeblithic/harmony-glitch/src-tauri && cargo test -- social::groups`
Expected: all tests pass

- [ ] **Step 8: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch
git add src-tauri/Cargo.toml src-tauri/src/social/groups.rs src-tauri/src/social/mod.rs src-tauri/src/network/types.rs
git commit -m "feat: GroupManager with persistence and DAG management (ZEB-75)"
```

---

### Task 8: IPC Commands + Network Integration

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Working directory:** `~/work/zeblithic/harmony-glitch`

This task adds IPC command handlers for all group operations and wires up network dispatch for incoming group ops. Follow the existing buddy/party IPC patterns exactly.

- [ ] **Step 1: Add GroupManagerWrapper managed state**

In `src-tauri/src/lib.rs`, add near the other wrapper types (around line 30-50):

```rust
pub struct GroupManagerWrapper(pub std::sync::Mutex<crate::social::groups::GroupManager>);
```

In the `.setup(|app| { ... })` block, after the existing managed state registrations, add:

```rust
    let groups_dir = data_dir.join("groups");
    let _ = std::fs::create_dir_all(&groups_dir);
    app.manage(GroupManagerWrapper(std::sync::Mutex::new(
        crate::social::groups::GroupManager::new(data_dir.clone()),
    )));
```

- [ ] **Step 2: Add group IPC commands**

Add the following IPC command handlers. Place them after the existing party commands (around line 1135). Each follows the established pattern: validate input, lock state, create op, merge, publish to network, emit event.

```rust
#[tauri::command]
fn group_create(name: String, mode: String, app: AppHandle) -> Result<String, String> {
    let group_mode = match mode.as_str() {
        "open" => harmony_groups::GroupMode::Open,
        _ => harmony_groups::GroupMode::InviteOnly,
    };
    let mut group_id = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut group_id);

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let (op, _canonical) = harmony_groups::GroupOp::new_unsigned(
        vec![],
        our_address,
        now_secs(&app) as u64,
        harmony_groups::GroupAction::Create { name, mode: group_mode },
    );
    // In production, sign canonical bytes with ML-DSA here

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(&app, op);
    let group_hex = hex::encode(group_id);
    let _ = app.emit("group_created", serde_json::json!({ "groupId": group_hex }));
    Ok(group_hex)
}

#[tauri::command]
fn group_invite(group_id_hex: String, peer_hash: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let target = parse_addr(&peer_hash)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

    let state = groups.get_state(&group_id).ok_or("Group not found")?;
    let role = state.role_of(&our_address).ok_or("Not a member of this group")?;
    if !matches!(role, harmony_groups::Role::Founder | harmony_groups::Role::Officer) {
        return Err("Only Founder/Officer can invite".into());
    }
    if state.is_member(&target) {
        return Err("Already a member".into());
    }
    let parents = state.head_ops.clone();
    drop(groups);

    let (op, _) = harmony_groups::GroupOp::new_unsigned(
        parents,
        our_address,
        now_secs(&app) as u64,
        harmony_groups::GroupAction::Invite { target },
    );

    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(&app, op);
    Ok(())
}

#[tauri::command]
fn group_accept(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;

    let invite = groups.pending_invites.remove(&group_id)
        .ok_or("No pending invite for this group")?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    // First merge the invite op we received
    groups.merge_op(group_id, invite.invite_op.clone())?;
    let parents = groups.head_ops(&group_id);

    let (op, _) = harmony_groups::GroupOp::new_unsigned(
        parents,
        our_address,
        now_secs(&app) as u64,
        harmony_groups::GroupAction::Accept { invite_op: invite.invite_op.id },
    );

    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(&app, op);
    let _ = app.emit("group_joined", serde_json::json!({ "groupId": group_id_hex }));
    Ok(())
}

#[tauri::command]
fn group_decline(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.pending_invites.remove(&group_id);
    Ok(())
}

#[tauri::command]
fn group_join(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    let state = groups.get_state(&group_id).ok_or("Group not found")?;
    if state.mode != harmony_groups::GroupMode::Open {
        return Err("Group is invite-only".into());
    }
    let parents = state.head_ops.clone();
    drop(groups);

    let (op, _) = harmony_groups::GroupOp::new_unsigned(
        parents,
        our_address,
        now_secs(&app) as u64,
        harmony_groups::GroupAction::Join,
    );

    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(&app, op);
    let _ = app.emit("group_joined", serde_json::json!({ "groupId": group_id_hex }));
    Ok(())
}

#[tauri::command]
fn group_leave(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    let state = groups.get_state(&group_id).ok_or("Group not found")?;
    if !state.is_member(&our_address) {
        return Err("Not a member".into());
    }
    let parents = state.head_ops.clone();
    drop(groups);

    let (op, _) = harmony_groups::GroupOp::new_unsigned(
        parents,
        our_address,
        now_secs(&app) as u64,
        harmony_groups::GroupAction::Leave,
    );

    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(&app, op);
    let _ = app.emit("group_left", serde_json::json!({ "groupId": group_id_hex }));
    Ok(())
}

#[tauri::command]
fn group_kick(group_id_hex: String, peer_hash: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let target = parse_addr(&peer_hash)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    let state = groups.get_state(&group_id).ok_or("Group not found")?;
    let our_role = state.role_of(&our_address).ok_or("Not a member")?;
    let target_role = state.role_of(&target).ok_or("Target not a member")?;
    if !our_role.outranks(target_role) && our_role != harmony_groups::Role::Founder {
        return Err("Insufficient permissions to kick".into());
    }
    let parents = state.head_ops.clone();
    drop(groups);

    let (op, _) = harmony_groups::GroupOp::new_unsigned(
        parents,
        our_address,
        now_secs(&app) as u64,
        harmony_groups::GroupAction::Kick { target },
    );

    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(&app, op);
    let _ = app.emit("group_member_kicked", serde_json::json!({ "groupId": group_id_hex, "targetHash": peer_hash }));
    Ok(())
}

#[tauri::command]
fn group_promote(group_id_hex: String, peer_hash: String, app: AppHandle) -> Result<(), String> {
    group_role_change(group_id_hex, peer_hash, harmony_groups::Role::Officer, true, &app)
}

#[tauri::command]
fn group_demote(group_id_hex: String, peer_hash: String, app: AppHandle) -> Result<(), String> {
    group_role_change(group_id_hex, peer_hash, harmony_groups::Role::Member, false, &app)
}

fn group_role_change(group_id_hex: String, peer_hash: String, new_role: harmony_groups::Role, is_promote: bool, app: &AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let target = parse_addr(&peer_hash)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    let state = groups.get_state(&group_id).ok_or("Group not found")?;
    if !matches!(state.role_of(&our_address), Some(harmony_groups::Role::Founder)) {
        return Err("Only Founder can change roles".into());
    }
    let parents = state.head_ops.clone();
    drop(groups);

    let action = if is_promote {
        harmony_groups::GroupAction::Promote { target, new_role }
    } else {
        harmony_groups::GroupAction::Demote { target, new_role }
    };

    let (op, _) = harmony_groups::GroupOp::new_unsigned(
        parents,
        our_address,
        now_secs(app) as u64,
        action,
    );

    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(app, op);
    let event = if is_promote { "group_member_promoted" } else { "group_member_demoted" };
    let _ = app.emit(event, serde_json::json!({ "groupId": group_id_hex, "targetHash": peer_hash }));
    Ok(())
}

#[tauri::command]
fn group_dissolve(group_id_hex: String, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    let state = groups.get_state(&group_id).ok_or("Group not found")?;
    if !matches!(state.role_of(&our_address), Some(harmony_groups::Role::Founder)) {
        return Err("Only Founder can dissolve".into());
    }
    let parents = state.head_ops.clone();
    drop(groups);

    let (op, _) = harmony_groups::GroupOp::new_unsigned(
        parents,
        our_address,
        now_secs(&app) as u64,
        harmony_groups::GroupAction::Dissolve,
    );

    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(&app, op);
    let _ = app.emit("group_dissolved", serde_json::json!({ "groupId": group_id_hex }));
    Ok(())
}

#[tauri::command]
fn group_update_info(group_id_hex: String, name: Option<String>, mode: Option<String>, app: AppHandle) -> Result<(), String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let mode_enum = mode.map(|m| match m.as_str() {
        "open" => harmony_groups::GroupMode::Open,
        _ => harmony_groups::GroupMode::InviteOnly,
    });

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    let state = groups.get_state(&group_id).ok_or("Group not found")?;
    if !matches!(state.role_of(&our_address), Some(harmony_groups::Role::Founder)) {
        return Err("Only Founder can update info".into());
    }
    let parents = state.head_ops.clone();
    drop(groups);

    let (op, _) = harmony_groups::GroupOp::new_unsigned(
        parents,
        our_address,
        now_secs(&app) as u64,
        harmony_groups::GroupAction::UpdateInfo { name, mode: mode_enum },
    );

    let mut groups = gm.0.lock().map_err(|e| e.to_string())?;
    groups.merge_op(group_id, op.clone())?;
    drop(groups);

    publish_group_op(&app, op);
    let _ = app.emit("group_info_updated", serde_json::json!({ "groupId": group_id_hex }));
    Ok(())
}

#[tauri::command]
fn get_group_state(group_id_hex: String, app: AppHandle) -> Result<serde_json::Value, String> {
    let group_id = parse_group_id(&group_id_hex)?;
    let gm = app.state::<GroupManagerWrapper>();
    let groups = gm.0.lock().map_err(|e| e.to_string())?;
    let state = groups.get_state(&group_id).ok_or("Group not found")?;
    Ok(serialize_group_state(state))
}

#[tauri::command]
fn get_my_groups(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let gm = app.state::<GroupManagerWrapper>();
    let groups = gm.0.lock().map_err(|e| e.to_string())?;
    let my = groups.my_groups(&our_address);
    Ok(my.iter().map(|s| serialize_group_state(s)).collect())
}

// ── Helpers ───────────────────────────────────────────────────────────

fn parse_group_id(hex_str: &str) -> Result<[u8; 16], String> {
    let bytes = hex::decode(hex_str).map_err(|_| "Invalid group ID hex")?;
    bytes.try_into().map_err(|_| "Group ID must be 16 bytes".into())
}

fn parse_addr(hex_str: &str) -> Result<[u8; 16], String> {
    let bytes = hex::decode(hex_str).map_err(|_| "Invalid address hex")?;
    bytes.try_into().map_err(|_| "Address must be 16 bytes".into())
}

fn publish_group_op(app: &AppHandle, op: harmony_groups::GroupOp) {
    let net = app.state::<NetworkWrapper>();
    let mut net_state = net.0.lock().unwrap();
    let net_msg = crate::network::types::NetMessage::GroupOp(op);
    if let Ok(payload) = serde_json::to_vec(&net_msg) {
        let actions = net_state.publish_to_all_peers(
            &payload,
            crate::network::types::PubTopic::Event,
            &mut rand::rngs::OsRng,
        );
        drop(net_state);
        execute_network_actions(app, actions);
    }
}

fn serialize_group_state(state: &harmony_groups::GroupState) -> serde_json::Value {
    let members: Vec<serde_json::Value> = state.members.iter().map(|(addr, entry)| {
        serde_json::json!({
            "addressHash": hex::encode(addr),
            "role": format!("{:?}", entry.role),
            "joinedAt": entry.joined_at,
            "isFounder": *addr == state.founder,
        })
    }).collect();
    serde_json::json!({
        "groupId": hex::encode(state.group_id),
        "name": state.name,
        "mode": if state.mode == harmony_groups::GroupMode::Open { "open" } else { "invite_only" },
        "founderHash": hex::encode(state.founder),
        "members": members,
        "memberCount": state.members.len(),
        "dissolved": state.dissolved,
    })
}
```

- [ ] **Step 3: Register IPC commands**

Add to the `generate_handler![]` macro in `lib.rs`, after the party commands:

```rust
    group_create,
    group_invite,
    group_accept,
    group_decline,
    group_join,
    group_leave,
    group_kick,
    group_promote,
    group_demote,
    group_dissolve,
    group_update_info,
    get_group_state,
    get_my_groups,
```

- [ ] **Step 4: Handle incoming GroupOp in network dispatch**

In the `handle_network_actions` function (or wherever `NetworkAction` is matched), add handling for `NetMessage::GroupOp`:

```rust
NetMessage::GroupOp(group_op) => {
    // Determine which group this op belongs to by checking our GroupManager
    let gm = app.state::<GroupManagerWrapper>();
    let mut groups = gm.0.lock().unwrap();

    // Try to merge into all groups we know about — the resolver will
    // reject ops that don't belong (missing parents, unauthorized)
    for group_id in groups.known_group_ids() {
        let _ = groups.merge_op(group_id, group_op.clone());
    }
    // Also handle as a potential invite if it's an Invite targeting us
    if let harmony_groups::GroupAction::Invite { target } = &group_op.action {
        let net = app.state::<NetworkWrapper>();
        let our_addr = net.0.lock().unwrap().our_address_hash();
        if *target == our_addr {
            // Store as pending invite — we'll need the group_id from context
            let _ = app.emit("group_invite_received", serde_json::json!({
                "inviterHash": hex::encode(group_op.author),
                "opId": hex::encode(group_op.id),
            }));
        }
    }
}
```

Note: Add a `known_group_ids()` method to `GroupManager` that returns all tracked group IDs.

- [ ] **Step 5: Verify compilation**

Run: `cd ~/work/zeblithic/harmony-glitch/src-tauri && cargo check`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch
git add src-tauri/src/lib.rs
git commit -m "feat: group IPC commands and network dispatch (ZEB-75)"
```

---

### Task 9: Frontend IPC + Types

**Files:**
- Modify: `src/lib/ipc.ts`

**Working directory:** `~/work/zeblithic/harmony-glitch`

- [ ] **Step 1: Add group types and IPC functions**

Append to `src/lib/ipc.ts`, after the emote section:

```typescript
// ── Groups ──────────────────────────────────────────────────────────

export interface GroupMemberInfo {
  addressHash: string;
  role: string;
  joinedAt: number;
  isFounder: boolean;
}

export interface GroupStateResult {
  groupId: string;
  name: string;
  mode: string;
  founderHash: string;
  members: GroupMemberInfo[];
  memberCount: number;
  dissolved: boolean;
}

export async function groupCreate(name: string, mode: string): Promise<string> {
  return invoke<string>('group_create', { name, mode });
}
export async function groupInvite(groupIdHex: string, peerHash: string): Promise<void> {
  return invoke<void>('group_invite', { groupIdHex, peerHash });
}
export async function groupAccept(groupIdHex: string): Promise<void> {
  return invoke<void>('group_accept', { groupIdHex });
}
export async function groupDecline(groupIdHex: string): Promise<void> {
  return invoke<void>('group_decline', { groupIdHex });
}
export async function groupJoin(groupIdHex: string): Promise<void> {
  return invoke<void>('group_join', { groupIdHex });
}
export async function groupLeave(groupIdHex: string): Promise<void> {
  return invoke<void>('group_leave', { groupIdHex });
}
export async function groupKick(groupIdHex: string, peerHash: string): Promise<void> {
  return invoke<void>('group_kick', { groupIdHex, peerHash });
}
export async function groupPromote(groupIdHex: string, peerHash: string): Promise<void> {
  return invoke<void>('group_promote', { groupIdHex, peerHash });
}
export async function groupDemote(groupIdHex: string, peerHash: string): Promise<void> {
  return invoke<void>('group_demote', { groupIdHex, peerHash });
}
export async function groupDissolve(groupIdHex: string): Promise<void> {
  return invoke<void>('group_dissolve', { groupIdHex });
}
export async function groupUpdateInfo(groupIdHex: string, name?: string, mode?: string): Promise<void> {
  return invoke<void>('group_update_info', { groupIdHex, name, mode });
}
export async function getGroupState(groupIdHex: string): Promise<GroupStateResult> {
  return invoke<GroupStateResult>('get_group_state', { groupIdHex });
}
export async function getMyGroups(): Promise<GroupStateResult[]> {
  return invoke<GroupStateResult[]>('get_my_groups');
}

// ── Group event listeners ───────────────────────────────────────────

export type GroupEvent =
  | { type: 'created'; groupId: string }
  | { type: 'joined'; groupId: string }
  | { type: 'left'; groupId: string }
  | { type: 'dissolved'; groupId: string }
  | { type: 'invite_received'; inviterHash: string; opId: string }
  | { type: 'member_kicked'; groupId: string; targetHash: string }
  | { type: 'member_promoted'; groupId: string; targetHash: string }
  | { type: 'member_demoted'; groupId: string; targetHash: string }
  | { type: 'info_updated'; groupId: string };

export async function onGroupEvent(callback: (event: GroupEvent) => void): Promise<UnlistenFn> {
  const unlistens = await Promise.all([
    listen<{ groupId: string }>('group_created', (e) =>
      callback({ type: 'created', groupId: e.payload.groupId })),
    listen<{ groupId: string }>('group_joined', (e) =>
      callback({ type: 'joined', groupId: e.payload.groupId })),
    listen<{ groupId: string }>('group_left', (e) =>
      callback({ type: 'left', groupId: e.payload.groupId })),
    listen<{ groupId: string }>('group_dissolved', (e) =>
      callback({ type: 'dissolved', groupId: e.payload.groupId })),
    listen<{ inviterHash: string; opId: string }>('group_invite_received', (e) =>
      callback({ type: 'invite_received', ...e.payload })),
    listen<{ groupId: string; targetHash: string }>('group_member_kicked', (e) =>
      callback({ type: 'member_kicked', ...e.payload })),
    listen<{ groupId: string; targetHash: string }>('group_member_promoted', (e) =>
      callback({ type: 'member_promoted', ...e.payload })),
    listen<{ groupId: string; targetHash: string }>('group_member_demoted', (e) =>
      callback({ type: 'member_demoted', ...e.payload })),
    listen<{ groupId: string }>('group_info_updated', (e) =>
      callback({ type: 'info_updated', groupId: e.payload.groupId })),
  ]);
  return () => unlistens.forEach(u => u());
}
```

- [ ] **Step 2: Run frontend type check**

Run: `cd ~/work/zeblithic/harmony-glitch && npx svelte-check`
Expected: no type errors

- [ ] **Step 3: Commit**

```bash
git add src/lib/ipc.ts
git commit -m "feat: frontend group IPC types and event listeners (ZEB-75)"
```

---

### Task 10: Frontend Components

**Files:**
- Create: `src/lib/components/GroupListPanel.svelte`
- Create: `src/lib/components/GroupDetailPanel.svelte`
- Create: `src/lib/components/GroupCreateDialog.svelte`
- Create: `src/lib/components/GroupInvitePrompt.svelte`

**Working directory:** `~/work/zeblithic/harmony-glitch`

- [ ] **Step 1: Create GroupListPanel.svelte**

Create `src/lib/components/GroupListPanel.svelte`:

```svelte
<script lang="ts">
  import type { GroupStateResult } from '$lib/ipc';

  let { groups, visible, onSelect, onCreate }: {
    groups: GroupStateResult[];
    visible: boolean;
    onSelect: (groupId: string) => void;
    onCreate: () => void;
  } = $props();

  function roleBadge(members: GroupStateResult['members'], founderHash: string): string {
    return '';
  }
</script>

{#if visible}
  <div class="group-list-panel">
    <div class="group-list-header">
      <span class="group-list-title">Groups</span>
      <button class="group-create-btn" onclick={onCreate} aria-label="Create new group">+</button>
    </div>
    {#if groups.length === 0}
      <div class="group-empty">No groups yet</div>
    {:else}
      <ul class="group-list">
        {#each groups as group (group.groupId)}
          <li class="group-entry">
            <button class="group-entry-btn" onclick={() => onSelect(group.groupId)}>
              <span class="group-name">{group.name}</span>
              <span class="group-meta">{group.memberCount} {group.memberCount === 1 ? 'member' : 'members'}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .group-list-panel { padding: 8px; }
  .group-list-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
  .group-list-title { color: #e0e0e0; font-size: 13px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; }
  .group-create-btn { background: rgba(88, 101, 242, 0.3); color: #5865f2; border: none; border-radius: 4px; width: 24px; height: 24px; cursor: pointer; font-size: 16px; line-height: 1; }
  .group-create-btn:hover { background: rgba(88, 101, 242, 0.5); }
  .group-empty { color: #888; font-size: 13px; padding: 8px 0; }
  .group-list { list-style: none; margin: 0; padding: 0; }
  .group-entry { margin-bottom: 2px; }
  .group-entry-btn { display: flex; justify-content: space-between; align-items: center; width: 100%; background: rgba(255, 255, 255, 0.05); border: none; border-radius: 4px; padding: 6px 10px; cursor: pointer; color: #e0e0e0; font-size: 13px; }
  .group-entry-btn:hover { background: rgba(255, 255, 255, 0.1); }
  .group-meta { color: #888; font-size: 11px; }
</style>
```

- [ ] **Step 2: Create GroupDetailPanel.svelte**

Create `src/lib/components/GroupDetailPanel.svelte`:

```svelte
<script lang="ts">
  import type { GroupStateResult } from '$lib/ipc';

  let { group, ourHash, onLeave, onKick, onPromote, onDemote, onDissolve, onBack }: {
    group: GroupStateResult;
    ourHash: string;
    onLeave: () => void;
    onKick: (hash: string) => void;
    onPromote: (hash: string) => void;
    onDemote: (hash: string) => void;
    onDissolve: () => void;
    onBack: () => void;
  } = $props();

  const ourRole = $derived(group.members.find(m => m.addressHash === ourHash)?.role ?? 'Member');
  const isFounder = $derived(ourRole === 'Founder');
  const isOfficer = $derived(ourRole === 'Officer' || isFounder);
</script>

<div class="group-detail">
  <div class="group-detail-header">
    <button class="back-btn" onclick={onBack} aria-label="Back to group list">←</button>
    <span class="group-detail-name">{group.name}</span>
    <span class="group-detail-mode">{group.mode === 'open' ? 'Open' : 'Invite Only'}</span>
  </div>

  <ul class="group-member-list">
    {#each group.members as member (member.addressHash)}
      <li class="group-member" class:founder={member.isFounder}>
        <span class="member-name">{member.addressHash.slice(0, 8)}…</span>
        <span class="role-badge">{member.role}</span>
        {#if isFounder && member.addressHash !== ourHash}
          <div class="member-actions">
            {#if member.role === 'Member'}
              <button onclick={() => onPromote(member.addressHash)} title="Promote to Officer">↑</button>
            {/if}
            {#if member.role === 'Officer'}
              <button onclick={() => onDemote(member.addressHash)} title="Demote to Member">↓</button>
            {/if}
            <button onclick={() => onKick(member.addressHash)} title="Kick">✕</button>
          </div>
        {:else if isOfficer && member.role === 'Member' && member.addressHash !== ourHash}
          <button onclick={() => onKick(member.addressHash)} title="Kick">✕</button>
        {/if}
      </li>
    {/each}
  </ul>

  <div class="group-detail-actions">
    <button class="leave-btn" onclick={onLeave}>Leave Group</button>
    {#if isFounder}
      <button class="dissolve-btn" onclick={onDissolve}>Dissolve Group</button>
    {/if}
  </div>
</div>

<style>
  .group-detail { padding: 8px; }
  .group-detail-header { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
  .back-btn { background: none; border: none; color: #888; cursor: pointer; font-size: 16px; padding: 2px 6px; }
  .back-btn:hover { color: #e0e0e0; }
  .group-detail-name { color: #e0e0e0; font-size: 14px; font-weight: 600; flex: 1; }
  .group-detail-mode { color: #888; font-size: 11px; }
  .group-member-list { list-style: none; margin: 0; padding: 0; }
  .group-member { display: flex; align-items: center; gap: 8px; padding: 4px 8px; border-radius: 4px; }
  .group-member:hover { background: rgba(255, 255, 255, 0.05); }
  .group-member.founder { }
  .member-name { color: #e0e0e0; font-size: 13px; flex: 1; }
  .role-badge { color: #888; font-size: 11px; padding: 1px 6px; background: rgba(255, 255, 255, 0.08); border-radius: 3px; }
  .member-actions { display: flex; gap: 4px; }
  .member-actions button { background: rgba(255, 255, 255, 0.08); border: none; border-radius: 3px; color: #ccc; cursor: pointer; padding: 2px 6px; font-size: 12px; }
  .member-actions button:hover { background: rgba(255, 255, 255, 0.15); }
  .group-detail-actions { margin-top: 12px; display: flex; gap: 8px; }
  .leave-btn { padding: 6px 14px; background: rgba(255, 255, 255, 0.1); border: none; border-radius: 4px; color: #ccc; cursor: pointer; font-size: 13px; }
  .leave-btn:hover { background: rgba(255, 255, 255, 0.2); }
  .dissolve-btn { padding: 6px 14px; background: rgba(237, 66, 69, 0.2); border: none; border-radius: 4px; color: #ed4245; cursor: pointer; font-size: 13px; }
  .dissolve-btn:hover { background: rgba(237, 66, 69, 0.3); }
</style>
```

- [ ] **Step 3: Create GroupCreateDialog.svelte**

Create `src/lib/components/GroupCreateDialog.svelte`:

```svelte
<script lang="ts">
  let { visible, onCreate, onCancel }: {
    visible: boolean;
    onCreate: (name: string, mode: string) => void;
    onCancel: () => void;
  } = $props();

  let name = $state('');
  let mode = $state('invite_only');
  let nameInput: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (visible) {
      name = '';
      mode = 'invite_only';
      nameInput?.focus();
    }
  });

  function submit() {
    const trimmed = name.trim();
    if (trimmed.length === 0) return;
    onCreate(trimmed, mode);
  }
</script>

{#if visible}
  <div class="group-create-dialog" role="dialog" aria-modal="true" aria-label="Create group">
    <p class="dialog-title">Create Group</p>
    <input
      bind:this={nameInput}
      bind:value={name}
      class="name-input"
      placeholder="Group name"
      maxlength="50"
      onkeydown={(e) => { if (e.key === 'Enter') submit(); }}
    />
    <div class="mode-toggle">
      <label>
        <input type="radio" bind:group={mode} value="invite_only" /> Invite Only
      </label>
      <label>
        <input type="radio" bind:group={mode} value="open" /> Open
      </label>
    </div>
    <div class="dialog-actions">
      <button class="create-btn" onclick={submit} disabled={name.trim().length === 0}>Create</button>
      <button class="cancel-btn" onclick={onCancel}>Cancel</button>
    </div>
  </div>
{/if}

<style>
  .group-create-dialog { position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); background: rgba(30, 30, 46, 0.98); border: 1px solid rgba(255, 255, 255, 0.15); border-radius: 8px; padding: 20px; z-index: 300; min-width: 280px; box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6); }
  .dialog-title { margin: 0 0 12px; color: #e0e0e0; font-size: 16px; font-weight: 600; }
  .name-input { width: 100%; padding: 8px 10px; background: rgba(0, 0, 0, 0.3); border: 1px solid rgba(255, 255, 255, 0.15); border-radius: 4px; color: #e0e0e0; font-size: 14px; box-sizing: border-box; }
  .name-input:focus { outline: none; border-color: #5865f2; }
  .mode-toggle { margin: 12px 0; display: flex; gap: 16px; }
  .mode-toggle label { color: #ccc; font-size: 13px; cursor: pointer; display: flex; align-items: center; gap: 4px; }
  .dialog-actions { display: flex; gap: 8px; justify-content: flex-end; }
  .create-btn { padding: 6px 16px; background: #5865f2; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 13px; }
  .create-btn:hover { background: #4752c4; }
  .create-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .cancel-btn { padding: 6px 16px; background: rgba(255, 255, 255, 0.1); color: #ccc; border: none; border-radius: 4px; cursor: pointer; font-size: 13px; }
  .cancel-btn:hover { background: rgba(255, 255, 255, 0.2); }
</style>
```

- [ ] **Step 4: Create GroupInvitePrompt.svelte**

Create `src/lib/components/GroupInvitePrompt.svelte`:

```svelte
<script lang="ts">
  let joinBtn: HTMLButtonElement | undefined = $state();

  let {
    inviterName = '',
    groupName = '',
    visible = false,
    onAccept = undefined,
    onDecline = undefined,
  }: {
    inviterName: string;
    groupName: string;
    visible: boolean;
    onAccept?: () => void;
    onDecline?: () => void;
  } = $props();

  $effect(() => {
    if (visible) joinBtn?.focus();
  });
</script>

{#if visible}
  <div class="group-invite-prompt" role="alertdialog" aria-modal="true" aria-label="Group invite from {inviterName}">
    <p class="prompt-text"><strong>{inviterName}</strong> invited you to <strong>{groupName}</strong></p>
    <div class="prompt-actions">
      <button bind:this={joinBtn} class="prompt-btn accept" onclick={() => onAccept?.()} aria-label="Join {groupName}">Join</button>
      <button class="prompt-btn decline" onclick={() => onDecline?.()} aria-label="Decline invite to {groupName}">Decline</button>
    </div>
  </div>
{/if}

<style>
  .group-invite-prompt { position: fixed; top: 120px; left: 50%; transform: translateX(-50%); background: rgba(30, 30, 46, 0.95); border: 1px solid rgba(255, 255, 255, 0.15); border-radius: 8px; padding: 12px 20px; z-index: 200; display: flex; align-items: center; gap: 16px; box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5); }
  .prompt-text { margin: 0; color: #e0e0e0; font-size: 14px; }
  .prompt-actions { display: flex; gap: 8px; }
  .prompt-btn { padding: 6px 14px; border: none; border-radius: 4px; font-size: 13px; cursor: pointer; }
  .prompt-btn.accept { background: #5865f2; color: white; }
  .prompt-btn.accept:hover { background: #4752c4; }
  .prompt-btn.decline { background: rgba(255, 255, 255, 0.1); color: #ccc; }
  .prompt-btn.decline:hover { background: rgba(255, 255, 255, 0.2); }
  .prompt-btn:focus-visible { outline: 2px solid #fbbf24; outline-offset: 2px; }
</style>
```

- [ ] **Step 5: Run frontend type check**

Run: `cd ~/work/zeblithic/harmony-glitch && npx svelte-check`
Expected: no type errors

- [ ] **Step 6: Run all Rust tests**

Run: `cd ~/work/zeblithic/harmony-glitch/src-tauri && cargo test`
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch
git add src/lib/components/GroupListPanel.svelte src/lib/components/GroupDetailPanel.svelte src/lib/components/GroupCreateDialog.svelte src/lib/components/GroupInvitePrompt.svelte
git commit -m "feat: group frontend components — list, detail, create, invite prompt (ZEB-75)"
```
