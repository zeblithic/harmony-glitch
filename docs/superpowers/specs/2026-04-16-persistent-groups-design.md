# Persistent Groups/Clubs Design (ZEB-75)

## Goal

A decentralized, persistent group membership system for the Harmony P2P ecosystem. Groups survive sessions, support hierarchical authority (Founder → Officer → Member), and converge to consistent state across offline/online peers without central servers or quorums.

## Architecture

Membership state is modeled as an **Authenticated Causal DAG** of cryptographically signed operations. Each operation (join, kick, promote, etc.) is a node in a directed acyclic graph with explicit causal parent references. A deterministic resolver traverses the DAG via topological sort and applies authorization checks + Strong Removal conflict resolution to produce a materialized membership list. This approach is informed by Matrix State Resolution v2 and the p2panda Strong Removal model.

The core logic lives in a new **shared `harmony-groups` crate** — sans-I/O, pure data structures and resolver. Both harmony-glitch (MMO) and harmony-client (chat) consume it with app-specific wrappers for persistence, networking, and UI.

## Tech Stack

- **Shared crate:** `harmony-groups` in `harmony/crates/harmony-groups/` (Rust, no-std compatible where possible)
- **Transport:** Zenoh pub/sub for real-time op propagation, Zenoh queryable for offline catch-up
- **Crypto:** ML-DSA signatures on all ops, verified via app-supplied callback (crate is crypto-agnostic)
- **Identity:** 16-byte address hashes (existing `MemberAddr` / `[u8; 16]` format)
- **Persistence:** Local JSON files per group, atomic writes via temp + rename
- **Frontend:** Svelte 5 components in harmony-glitch, reactive via Tauri events

---

## Core Data Model

### Identifiers

- `GroupId` — `[u8; 16]`, random, generated at creation time
- `MemberAddr` — `[u8; 16]`, existing address hash format
- `OpId` — `[u8; 32]`, BLAKE3 hash of the canonical serialized op payload (content-addressed)

### Role Hierarchy

Three fixed tiers with numeric power levels for deterministic tie-breaking:

| Role | Power Level | Can Invite | Can Kick | Can Promote/Demote | Can Dissolve | Can Update Info |
|------|-------------|------------|----------|--------------------|--------------|-----------------|
| Founder | 0 | Yes | Anyone | Yes | Yes | Yes |
| Officer | 1 | Yes | Members only | No | No | No |
| Member | 2 | No | No | No | No | No |

Lower power level = higher authority. Founder is always power 0.

### GroupOp (DAG Node)

Each operation in the DAG:

```
GroupOp:
  id: OpId              — BLAKE3(canonical_payload), content-addressed
  parents: Vec<OpId>    — causal predecessors (empty only for genesis Create op)
  author: MemberAddr    — op creator's address hash
  signature: Vec<u8>    — ML-DSA signature over canonical payload
  timestamp: f64        — author's local clock (advisory, used for display, not authoritative for ordering)
  action: GroupAction
```

### GroupAction (Operation Types)

```
GroupAction enum:
  Create { name: String, mode: GroupMode }         — genesis op, author becomes Founder
  Invite { target: MemberAddr }                    — Officer+ invites someone
  Join { }                                         — self-join (valid only for Open groups)
  Accept { invite_op: OpId }                       — accepts a specific Invite op
  Leave { }                                        — voluntary departure
  Kick { target: MemberAddr }                      — Officer+ removes Member; Founder removes anyone
  Promote { target: MemberAddr, new_role: Role }   — Founder-only
  Demote { target: MemberAddr, new_role: Role }    — Founder-only
  Dissolve { }                                     — Founder-only, terminates the group
  UpdateInfo { name: Option<String>, mode: Option<GroupMode> } — Founder-only metadata changes
```

### GroupMode

```
GroupMode enum:
  InviteOnly  — only Officer+ can invite, others join via Accept
  Open        — anyone can publish a Join op
```

### Materialized GroupState (Resolver Output)

```
GroupState:
  group_id: GroupId
  name: String
  mode: GroupMode
  founder: MemberAddr
  members: BTreeMap<MemberAddr, MemberEntry>
  dissolved: bool
  head_ops: Vec<OpId>       — current DAG tips (for constructing new ops' parent lists)

MemberEntry:
  role: Role
  joined_at: f64            — timestamp from the Accept/Join op
```

---

## DAG Resolver

The resolver is a pure function: `resolve(ops: &[GroupOp], verify: &VerifyFn) -> Result<GroupState, ResolveError>`.

### Step 1 — Build the Graph

Index all ops by ID. Validate that every parent reference points to an existing op. Identify the genesis op — the single `Create` action with empty parents. Reject DAGs with zero or multiple genesis ops.

### Step 2 — Topological Sort

Using Kahn's algorithm, produce a total ordering respecting causal dependencies (parents before children). For concurrent ops (no causal relationship), break ties deterministically:

1. **Higher authority first** — based on the author's role at their causal position in the running state. Lower power level number wins (Founder=0 > Officer=1 > Member=2).
2. **Lexicographic op ID** — if equal authority, compare `OpId` bytes lexicographically.

This guarantees every node produces the identical ordering for the same set of ops.

### Step 3 — Replay with Authorization

Walk the sorted ops, maintaining a running `GroupState`. For each op:

1. Verify the ML-DSA signature via the app-supplied `VerifyFn` callback
2. Check the author's current role in the running state authorizes the action:
   - `Create`: must be genesis, no prior state
   - `Invite`: author must be Officer or Founder, target must not already be a member
   - `Join`: group mode must be Open, author must not already be a member
   - `Accept`: a valid `Invite` op targeting the author must exist, and the author must not already be a member (duplicate accepts are no-ops)
   - `Leave`: author must be a current member. If the Founder leaves: promote the longest-tenured Officer to Founder (by earliest `joined_at`; tie-break by lexicographic `MemberAddr`). If no Officers exist, promote the longest-tenured Member. If no other members exist, the group is dissolved.
   - `Kick`: author must outrank target (Founder kicks anyone, Officer kicks Members only)
   - `Promote`/`Demote`: author must be Founder
   - `Dissolve`: author must be Founder
   - `UpdateInfo`: author must be Founder
3. If authorized: apply the mutation
4. If unauthorized: skip silently (op stays in DAG for replication, does not affect materialized state)

### Step 4 — Strong Removal (Transitive Invalidation)

When a `Kick` or `Demote` op is applied against a member, any ops authored by that member that are **concurrent** with the Kick/Demote are retroactively invalidated. "Concurrent" means the op does not have the Kick/Demote in its ancestor chain (it was created without knowledge of the demotion).

During replay, before applying each op, check: was the author kicked or demoted by a concurrent op whose author has higher authority? If yes, skip the op.

**Conflict resolution examples:**

| Scenario | Resolution |
|----------|------------|
| Founder demotes Officer A; Officer A concurrently kicks Member B | Officer A's kick is voided (Founder outranks) |
| Two Officers concurrently kick each other | Both kicks applied (equal authority, mutual removal) |
| Officer promotes Member while Founder concurrently kicks that Officer | Promotion voided (Founder outranks) |
| Founder A and demoted-Officer B both invite same target concurrently | Only Founder's invite survives; Officer's is voided |

---

## Storage

### Local Persistence

Each group's op log is stored at `{data_dir}/groups/{group_id_hex}.json` — a JSON array of serialized `GroupOp` structs. Writes use the atomic temp-file + rename pattern (consistent with `follows.json` in harmony-client and `savegame.json` in harmony-glitch).

A group index file at `{data_dir}/groups/index.json` maps group IDs to lightweight metadata for quick listing:

```json
{
  "groups": {
    "a1b2c3...": { "name": "Cool Club", "our_role": "officer", "member_count": 12, "last_op_count": 47 }
  }
}
```

### DAG Compaction

No compaction in the MVP. For groups of 2-100 members, the DAG grows slowly — membership ops are infrequent compared to chat messages. A very active 100-member club generating 10 membership ops per day for a year produces ~3,650 ops × ~2.6KB ≈ 9.5MB. Compaction via epoch snapshots is a future optimization.

---

## Networking & Sync

### Real-Time Propagation

When a local user creates a new op:

1. Create the `GroupOp`, sign with ML-DSA, add to local DAG
2. Re-resolve to update materialized state
3. Persist updated op log to disk
4. Publish the serialized op to Zenoh: `harmony/groups/{group_id}/ops`
5. Online group members receive via subscriber, validate, merge, re-resolve

### Offline Catch-Up

When a peer comes online or encounters new group members:

1. Peer publishes a **sync request** to `harmony/groups/{group_id}/sync` containing their current DAG tip hashes (`head_ops`)
2. Online members compare the requester's tips against their own DAG
3. Responders send back any ops the requester is missing
4. Requester merges received ops, re-resolves, persists

### Sync Triggers

- App startup (for all groups the user belongs to)
- Street transition (new peers become reachable)
- Receiving a group op with unknown parent hashes (indicates we're behind)

### Future: Zenoh Storage Migration

The op format and Zenoh key expressions are designed for future migration to Zenoh's native replicated storage with built-in anti-entropy (temporal slicing + hierarchical hashing). The local JSON persistence would become a cache/fallback.

---

## Integration Layer

### harmony-groups Crate API (Sans-I/O)

The crate exposes:

- `GroupOp::new(action, author, parents, timestamp) -> GroupOp` — creates an unsigned op (app signs it)
- `resolve(ops, verify_fn) -> Result<GroupState, ResolveError>` — materializes state from ops
- `serialize_ops(ops) -> Vec<u8>` / `deserialize_ops(bytes) -> Vec<GroupOp>` — for persistence and network
- `find_missing_ops(local_tips, remote_tips, local_ops) -> Vec<OpId>` — sync helper
- `validate_op(op, current_state) -> Result<(), ValidationError>` — pre-flight check before signing

The crate does NOT handle: file I/O, Zenoh pub/sub, cryptographic signing/verification (takes a callback), display name resolution, or UI events.

### harmony-glitch Integration (`src-tauri/src/social/groups.rs`)

- `GroupManager` struct: in-memory op logs (`BTreeMap<GroupId, Vec<GroupOp>>`), data directory path, Zenoh session handle
- IPC commands: `group_create`, `group_invite`, `group_accept`, `group_join`, `group_leave`, `group_kick`, `group_promote`, `group_demote`, `group_dissolve`, `group_update_info`, `get_group_state`, `get_my_groups`
- Zenoh subscriber on `harmony/groups/*/ops` for real-time ops
- Zenoh queryable on `harmony/groups/*/sync` for catch-up requests
- Tauri events to frontend: `group_invite_received`, `group_member_joined`, `group_member_left`, `group_member_promoted`, `group_member_demoted`, `group_member_kicked`, `group_dissolved`, `group_info_updated`
- Persists to `{data_dir}/groups/` on every mutation

### SocialMessage Independence

Group ops travel over Zenoh directly on `harmony/groups/` key expressions — they are NOT wrapped in the existing `SocialMessage` enum. Groups are a platform-level primitive independent of the glitch-specific social layer. Parties, buddies, and groups are separate systems sharing the same identity layer.

### Signature Verification

Apps provide a callback: `Fn(author: MemberAddr, payload: &[u8], signature: &[u8]) -> bool`. In harmony-glitch, this looks up the author's ML-DSA public key from the identity/peer cache. The crate never touches crypto directly.

### harmony-client Integration (Future)

Same crate, different wrapper. The `NavNode` type `'group-chat'` would be backed by a real `harmony-groups` DAG. Channel permissions derive from materialized `GroupState` roles. Same Zenoh key expressions and op format — groups created in either app are visible in both.

---

## Frontend (harmony-glitch)

### New Components

- **`GroupCreateDialog.svelte`** — Name input, mode toggle (invite-only/open), create button. Launched from the social panel.
- **`GroupListPanel.svelte`** — Lists the player's groups with name, member count, role badge. Clicking opens detail. Lives in social sidebar alongside buddy list and party panel.
- **`GroupDetailPanel.svelte`** — Group members with roles, join date. Founder/Officer see kick, promote/demote buttons. Founder sees dissolve and settings. Members see leave. Similar layout to `PartyPanel.svelte`.
- **`GroupInvitePrompt.svelte`** — Incoming invite notification. Shows group name, inviter name, member count. Accept/Decline. Follows `PartyInvitePrompt.svelte` and `BuddyRequestPrompt.svelte` patterns.
- **`SocialPrompt.svelte` extension** — Existing right-click player menu gains "Invite to Group" with a sub-menu of groups where the user has invite permission.

### State Flow

Frontend calls IPC → Rust creates signed `GroupOp` → publishes to Zenoh + persists → re-resolves DAG → emits Tauri event → frontend updates via Svelte 5 `$state` reactivity.

The frontend never sees the DAG — it receives materialized `GroupState` snapshots via `get_group_state` / `get_my_groups` and incremental Tauri events.

### Not In MVP

- Group chat (future feature building on group membership)
- Group properties/halls/balances (original Glitch feature, future work)
- Group discovery/search (join via invite or encountering members on a street)
- Application join mode (future third `GroupMode` variant)

---

## Testing Strategy

### Unit Tests (harmony-groups crate)

- **Happy path:** Create → invite → accept → verify membership. Open group join. Leave. Dissolve.
- **Authorization enforcement:** Member tries to kick (rejected). Officer tries to promote (rejected). Non-member invites (rejected). Kicked member's subsequent ops ignored.
- **Strong Removal / conflict resolution:** Founder demotes Officer while Officer concurrently kicks Member → kick invalidated. Mutual officer kicks → both applied. Officer promotes while being concurrently kicked by Founder → promotion voided.
- **Deterministic ordering:** Construct concurrent ops at same authority level, verify identical materialized state regardless of op insertion order.
- **DAG integrity:** Missing parent references. Duplicate op IDs. Genesis op validation (exactly one Create, no parents).
- **Edge cases:** Dissolve while invites pending. Self-kick. Promote to current role. Join on invite-only group. Accept for nonexistent invite.

### Property-Based / Fuzz Tests

Generate random sequences of valid `GroupOp`s, shuffle input order, resolve — materialized state must be identical across all orderings. This is the single most important invariant.

### Integration Tests (harmony-glitch)

- IPC round-trip: frontend command → Rust handler → op created → event emitted → `get_group_state` correct
- Persistence: create group, restart app, verify group loads from disk
- Zenoh pub/sub: two test peers, one creates an op, other receives and merges

---

## Decisions & Rationale

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Shared vs. app-specific | Shared `harmony-groups` crate | Both apps need group membership; platform-level primitive |
| Authority model | 3 fixed tiers (Founder/Officer/Member) | Minimum viable that demonstrates DAG architecture; extensible to custom roles later |
| Join modes | Invite-only + Open | Covers private clubs and public hangouts; application mode deferred |
| Party relationship | Completely separate | Party system just hardened (ZEB-84); avoid regression; party→club upgrade is future UX convenience |
| Conflict resolution | Authenticated Causal DAG + Strong Removal | Only approach that handles authority + offline convergence correctly |
| DAG compaction | None in MVP | ~9.5MB worst case for very active group over a year; YAGNI |
| Persistence | Local JSON files | Follows existing patterns; path to Zenoh storage migration |
| Group ops transport | Direct Zenoh, not SocialMessage | Platform-level primitive, not glitch-specific social feature |
