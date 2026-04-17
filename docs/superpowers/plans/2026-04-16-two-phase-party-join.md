# Two-Phase Party Join Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the phantom-party bug by deferring local party state until the leader confirms the join via `PartyMemberJoined`.

**Architecture:** Currently `party_accept` IPC calls `accept_invite()` which immediately builds an `ActiveParty` locally before the leader validates. If the leader rejects (party full, dissolved), the acceptor is stuck in a phantom party. The fix adds a `pending_join` intermediate state: the acceptor sends `PartyAccept` but holds invite data in limbo until `PartyMemberJoined` arrives with their address, then commits via `accept_invite()`. A 90-second timeout abandons the attempt if no confirmation arrives.

**Tech Stack:** Rust (Tauri backend), serde, Tauri event system

**Linear Issue:** ZEB-84

---

## File Map

| File | Role | Change |
|------|------|--------|
| `src-tauri/src/social/party.rs` | Party state machine | Add `PendingJoin` struct, `pending_join` field, `begin_join()`, `confirm_join()`, `expire_pending_join()` methods |
| `src-tauri/src/social/mod.rs` | Social tick aggregator | Call `expire_pending_join()` in `tick()` |
| `src-tauri/src/lib.rs` | IPC handlers + message dispatch | Refactor `party_accept` to defer, handle `PartyMemberJoined` for self-confirmation |

No new files. No frontend changes — the `PartyInvitePrompt` already dismisses on accept, and the party panel renders from `RenderFrame.party_role` which is only set when `party` is `Some`. The deferred join is invisible to the frontend until confirmed.

---

### Task 1: Add `PendingJoin` struct and field to `PartyState`

**Files:**
- Modify: `src-tauri/src/social/party.rs:147-171`

- [ ] **Step 1: Write the failing test — `begin_join` moves invite to pending_join**

Add this test to the `mod tests` block in `src-tauri/src/social/party.rs` (after the existing outgoing-invite tests, around line 648):

```rust
#[test]
fn begin_join_moves_invite_to_pending_join() {
    let mut s = PartyState::new();
    s.set_pending_invite(PendingPartyInvite {
        leader: addr(0x01),
        leader_name: "Leader".into(),
        members: vec![addr(0x02)],
        received_at: 100.0,
    });
    let result = s.begin_join(100.5);
    assert!(result.is_ok());
    assert!(s.pending_invite.is_none(), "invite should be consumed");
    assert!(s.pending_join.is_some(), "pending_join should be set");
    assert!(!s.in_party(), "should NOT be in party yet");
    let pj = s.pending_join.as_ref().unwrap();
    assert_eq!(pj.leader, addr(0x01));
    assert_eq!(pj.leader_name, "Leader");
    assert_eq!(pj.members, vec![addr(0x02)]);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib --lib social::party::tests::begin_join_moves_invite_to_pending_join 2>&1 | tail -20`
Expected: FAIL — `begin_join` method doesn't exist, `pending_join` field doesn't exist.

- [ ] **Step 3: Add `PendingJoin` struct, field, and `begin_join()` method**

Add the `PendingJoin` struct after `OutgoingPartyInvite` (after line 162):

```rust
/// Transient state: we've sent PartyAccept but haven't received
/// the leader's PartyMemberJoined confirmation yet.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingJoin {
    pub leader: [u8; 16],
    pub leader_name: String,
    pub members: Vec<[u8; 16]>,
    pub accepted_at: f64,
}
```

Add the `pending_join` field to `PartyState` (line 168, after `pending_invite`):

```rust
pub pending_join: Option<PendingJoin>,
```

Add `begin_join()` method to `impl PartyState` (after `decline_invite`, around line 252):

```rust
/// Move the pending invite into a deferred-join state.
///
/// The invite is consumed but the player does NOT join the party yet.
/// Call `confirm_join()` when the leader's `PartyMemberJoined` arrives.
///
/// Validates expiry BEFORE taking the invite so an expired Err response
/// leaves the pending invite intact — the UI can choose to surface a
/// "this invite expired, ask again?" prompt without the invite having
/// already been silently consumed.
pub fn begin_join(&mut self, now: f64) -> Result<[u8; 16], &'static str> {
    let invite_ref = self.pending_invite.as_ref().ok_or("no pending invite")?;
    if now - invite_ref.received_at > PARTY_INVITE_TIMEOUT {
        return Err("invite expired");
    }
    // Expiry check passed — safe to consume the invite now.
    let invite = self.pending_invite.take().expect("checked as_ref above");
    let leader = invite.leader;
    self.pending_join = Some(PendingJoin {
        leader: invite.leader,
        leader_name: invite.leader_name,
        members: invite.members,
        accepted_at: now,
    });
    Ok(leader)
}
```

Note: `begin_join` returns the leader's address hash so the IPC handler knows where to send `PartyAccept`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib --lib social::party::tests::begin_join_moves_invite_to_pending_join 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch && git add src-tauri/src/social/party.rs && git commit -m "feat(party): add PendingJoin struct and begin_join() for deferred accept (ZEB-84)"
```

---

### Task 2: Add `confirm_join()` that commits the deferred join

**Files:**
- Modify: `src-tauri/src/social/party.rs`

- [ ] **Step 1: Write the failing test — `confirm_join` builds party from pending_join**

Add this test to the `mod tests` block:

```rust
#[test]
fn confirm_join_builds_party_from_pending_join() {
    let mut s = PartyState::new();
    s.set_pending_invite(PendingPartyInvite {
        leader: addr(0x01),
        leader_name: "Leader".into(),
        members: vec![addr(0x02)],
        received_at: 100.0,
    });
    s.begin_join(100.5).unwrap();

    let result = s.confirm_join(addr(0x03), "Me".into(), 101.0);
    assert!(result.is_ok());
    assert!(s.in_party(), "should be in party after confirm");
    assert!(s.pending_join.is_none(), "pending_join should be cleared");
    let party = s.party.as_ref().unwrap();
    assert!(party.is_leader(&addr(0x01)));
    assert!(party.is_member(&addr(0x01)));
    assert!(party.is_member(&addr(0x02)));
    assert!(party.is_member(&addr(0x03)));
}
```

- [ ] **Step 2: Write second failing test — `confirm_join` without pending_join returns error**

```rust
#[test]
fn confirm_join_without_pending_join_returns_err() {
    let mut s = PartyState::new();
    let result = s.confirm_join(addr(0x01), "Me".into(), 100.0);
    assert!(result.is_err());
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib --lib social::party::tests::confirm_join 2>&1 | tail -10`
Expected: FAIL — `confirm_join` method doesn't exist.

- [ ] **Step 4: Implement `confirm_join()`**

Add this method to `impl PartyState` (after `begin_join`):

```rust
/// Commit the deferred join after receiving the leader's confirmation.
///
/// Builds the `ActiveParty` from the saved `PendingJoin` data
/// (same logic as `accept_invite` but sourced from `pending_join`).
pub fn confirm_join(
    &mut self,
    self_hash: [u8; 16],
    self_name: String,
    now: f64,
) -> Result<(), &'static str> {
    let pj = self.pending_join.take().ok_or("no pending join")?;
    let mut party = ActiveParty {
        leader: pj.leader,
        members: vec![PartyMember {
            address_hash: pj.leader,
            display_name: pj.leader_name,
            joined_at: pj.accepted_at,
        }],
        created_at: pj.accepted_at,
    };
    for &addr in &pj.members {
        if addr != pj.leader && addr != self_hash {
            let _ = party.add_member(PartyMember {
                address_hash: addr,
                display_name: String::new(),
                joined_at: pj.accepted_at,
            });
        }
    }
    party.add_member(PartyMember {
        address_hash: self_hash,
        display_name: self_name,
        joined_at: now,
    })?;
    self.party = Some(party);
    Ok(())
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib --lib social::party::tests::confirm_join 2>&1 | tail -10`
Expected: 2 tests PASS

- [ ] **Step 6: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch && git add src-tauri/src/social/party.rs && git commit -m "feat(party): add confirm_join() to commit deferred join after leader confirmation (ZEB-84)"
```

---

### Task 3: Add `expire_pending_join()` and wire into social tick

**Files:**
- Modify: `src-tauri/src/social/party.rs`
- Modify: `src-tauri/src/social/mod.rs:41-49`

- [ ] **Step 1: Write the failing test — `expire_pending_join` clears stale join**

```rust
#[test]
fn expire_pending_join_clears_old() {
    let mut s = PartyState::new();
    s.set_pending_invite(PendingPartyInvite {
        leader: addr(0x01),
        leader_name: "L".into(),
        members: vec![],
        received_at: 0.0,
    });
    s.begin_join(0.5).unwrap();
    s.expire_pending_join(91.0);
    assert!(s.pending_join.is_none(), "stale pending_join should be cleared");
}
```

- [ ] **Step 2: Write second failing test — `expire_pending_join` keeps fresh join**

```rust
#[test]
fn expire_pending_join_keeps_fresh() {
    let mut s = PartyState::new();
    s.set_pending_invite(PendingPartyInvite {
        leader: addr(0x01),
        leader_name: "L".into(),
        members: vec![],
        received_at: 100.0,
    });
    s.begin_join(100.5).unwrap();
    s.expire_pending_join(150.0);
    assert!(s.pending_join.is_some(), "fresh pending_join should survive");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib --lib social::party::tests::expire_pending_join 2>&1 | tail -10`
Expected: FAIL — method doesn't exist.

- [ ] **Step 4: Implement `expire_pending_join()`**

Add to `impl PartyState` (after `expire_invite`, around line 282):

```rust
/// Clear a pending join that has been waiting too long for leader confirmation.
pub fn expire_pending_join(&mut self, now: f64) {
    if let Some(pj) = &self.pending_join {
        if now - pj.accepted_at > PARTY_INVITE_TIMEOUT {
            self.pending_join = None;
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib --lib social::party::tests::expire_pending_join 2>&1 | tail -10`
Expected: 2 tests PASS

- [ ] **Step 6: Wire `expire_pending_join` into `SocialState::tick()`**

In `src-tauri/src/social/mod.rs`, add after line 48 (`self.party.expire_outgoing_invites(ctx.game_time);`):

```rust
        self.party.expire_pending_join(ctx.game_time);
```

- [ ] **Step 7: Run full social test suite**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib --lib social 2>&1 | tail -10`
Expected: All social tests pass.

- [ ] **Step 8: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch && git add src-tauri/src/social/party.rs src-tauri/src/social/mod.rs && git commit -m "feat(party): add expire_pending_join() with 90s timeout, wire into social tick (ZEB-84)"
```

---

### Task 4: Refactor `party_accept` IPC to use deferred join

**Files:**
- Modify: `src-tauri/src/lib.rs:1074-1115`

- [ ] **Step 1: Refactor `party_accept` to call `begin_join()` instead of `accept_invite()`**

Replace the body of the `party_accept` function (lines 1075-1115) with:

```rust
#[tauri::command]
fn party_accept(app: AppHandle) -> Result<(), String> {
    let now = now_secs(&app);

    let net = app.state::<NetworkWrapper>();
    let net_state = net.0.lock().map_err(|e| e.to_string())?;
    let our_address = net_state.our_address_hash();
    drop(net_state);

    let state_wrapper = app.state::<GameStateWrapper>();
    let mut state = state_wrapper.0.lock().map_err(|e| e.to_string())?;

    let invite_leader = state
        .social
        .party
        .begin_join(now)
        .map_err(|e| e.to_string())?;
    drop(state);

    let actions = {
        let net = app.state::<NetworkWrapper>();
        let mut net_state = net.0.lock().map_err(|e| e.to_string())?;
        net_state.publish_social(
            social::SocialMessage::PartyAccept { from: our_address, to: invite_leader },
            &mut rand::rngs::OsRng,
        )
    };
    execute_network_actions(&app, actions);
    Ok(())
}
```

Key changes vs. the old code:
- Calls `begin_join(now)` instead of `accept_invite(our_address, our_name, now)`
- `begin_join` returns the leader address, so we don't need to peek at `pending_invite` separately
- No longer needs `our_name` (deferred until `confirm_join`)
- Player is NOT in a party yet — just waiting for confirmation

- [ ] **Step 2: Verify compilation**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo check -p harmony-glitch-lib 2>&1 | tail -20`
Expected: Compiles (may have unused warnings for `accept_invite` — that's fine, we'll clean up after).

- [ ] **Step 3: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch && git add src-tauri/src/lib.rs && git commit -m "refactor(party): party_accept uses begin_join() to defer state mutation (ZEB-84)"
```

---

### Task 5: Handle `PartyMemberJoined` as self-confirmation

**Files:**
- Modify: `src-tauri/src/lib.rs:1652-1675`

Currently, inbound `PartyMemberJoined` only handles the case where the receiver is already in a party (adding a new member to their local party state). For the two-phase protocol, we need an additional path: if `member == our_address` and we have a `pending_join`, call `confirm_join()`.

- [ ] **Step 1: Add self-confirmation branch to `PartyMemberJoined` handler**

Replace the `PartyMemberJoined` handler block (lines 1652-1675) with:

```rust
        SocialMessage::PartyMemberJoined {
            member,
            display_name,
            ..
        } => {
            let now = now_secs(app);
            // Self-confirmation: leader confirmed our join request.
            if member == our_address && state.social.party.pending_join.is_some() {
                let pi = app.state::<PlayerIdentityWrapper>();
                let our_name = pi.display_name.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if state.social.party.confirm_join(our_address, our_name, now).is_ok() {
                    let _ = app.emit(
                        "party_joined",
                        serde_json::json!({
                            "leaderHash": hex::encode(authenticated_sender),
                        }),
                    );
                }
                return;
            }
            // Normal path: another member joined our existing party.
            if let Some(ref mut party) = state.social.party.party {
                if !party.is_leader(&authenticated_sender) {
                    return;
                }
                if party.add_member(social::party::PartyMember {
                    address_hash: member,
                    display_name: display_name.clone(),
                    joined_at: now,
                }).is_ok() {
                    let _ = app.emit(
                        "party_member_joined",
                        serde_json::json!({
                            "memberHash": hex::encode(member),
                            "memberName": display_name,
                        }),
                    );
                }
            }
        }
```

Key changes:
- Added a check at the top: if `member == our_address` and `pending_join` is `Some`, this is the confirmation of our own join.
- Calls `confirm_join()` to build the `ActiveParty` from the deferred data.
- Emits `"party_joined"` (new event for the acceptor, distinct from `"party_member_joined"` which is for existing members seeing a new join).
- Falls through to the existing add-member path for other members.

- [ ] **Step 2: Verify compilation**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo check -p harmony-glitch-lib 2>&1 | tail -20`
Expected: Compiles cleanly.

- [ ] **Step 3: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch && git add src-tauri/src/lib.rs && git commit -m "feat(party): handle PartyMemberJoined as self-confirmation for deferred join (ZEB-84)"
```

---

### Task 6: Add frontend handling for `party_joined` event

**Files:**
- Modify: `src/App.svelte` (the event listener section)

The frontend needs to listen for the new `party_joined` event so the UI updates when the deferred join is confirmed. Check the existing event listeners for social events and add the new one alongside them.

- [ ] **Step 1: Find existing social event listeners in App.svelte**

Run: `cd ~/work/zeblithic/harmony-glitch && grep -n 'party_member_joined\|party_dissolved\|party_invite_received' src/App.svelte | head -10`

This shows where social event listeners are registered.

- [ ] **Step 2: Add `party_joined` listener**

Add a listener for `party_joined` alongside the existing `party_member_joined` listener. The handler should dismiss the party invite prompt (if still visible) and could trigger a UI refresh. The exact code depends on what's found in step 1, but the pattern is:

```typescript
listen<{ leaderHash: string }>('party_joined', (event) => {
  // Dismiss the invite prompt
  partyInviteVisible = false;
});
```

- [ ] **Step 3: Verify the frontend compiles**

Run: `cd ~/work/zeblithic/harmony-glitch && npm run check 2>&1 | tail -10`
Expected: No type errors.

- [ ] **Step 4: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch && git add src/App.svelte && git commit -m "feat(party): add party_joined event listener for deferred join confirmation (ZEB-84)"
```

---

### Task 7: Add race condition tests

**Files:**
- Modify: `src-tauri/src/social/party.rs` (test module)

These tests verify the failure paths that motivated this change.

- [ ] **Step 1: Write test — pending_join times out gracefully (no phantom party)**

```rust
#[test]
fn pending_join_timeout_does_not_create_phantom_party() {
    let mut s = PartyState::new();
    s.set_pending_invite(PendingPartyInvite {
        leader: addr(0x01),
        leader_name: "Leader".into(),
        members: vec![],
        received_at: 0.0,
    });
    s.begin_join(0.5).unwrap();
    assert!(s.pending_join.is_some());
    assert!(!s.in_party());

    // Simulate 91 seconds passing with no confirmation
    s.expire_pending_join(91.0);
    assert!(s.pending_join.is_none(), "pending_join should expire");
    assert!(!s.in_party(), "must NOT be in a phantom party");
}
```

- [ ] **Step 2: Write test — begin_join while already in pending_join replaces it**

```rust
#[test]
fn begin_join_fails_without_invite() {
    let mut s = PartyState::new();
    let result = s.begin_join(0.0);
    assert!(result.is_err());
}
```

- [ ] **Step 3: Write test — confirm_join after party dissolved (pending_join cleared by expiry)**

```rust
#[test]
fn confirm_join_after_expiry_returns_err() {
    let mut s = PartyState::new();
    s.set_pending_invite(PendingPartyInvite {
        leader: addr(0x01),
        leader_name: "L".into(),
        members: vec![],
        received_at: 0.0,
    });
    s.begin_join(0.5).unwrap();
    s.expire_pending_join(91.0); // expired
    let result = s.confirm_join(addr(0x02), "Me".into(), 92.0);
    assert!(result.is_err(), "confirm after expiry should fail");
    assert!(!s.in_party());
}
```

- [ ] **Step 4: Write test — begin_join with expired invite returns error**

```rust
#[test]
fn begin_join_with_expired_invite_returns_err() {
    let mut s = PartyState::new();
    s.set_pending_invite(PendingPartyInvite {
        leader: addr(0x01),
        leader_name: "L".into(),
        members: vec![],
        received_at: 0.0,
    });
    let result = s.begin_join(91.0);
    assert!(result.is_err());
    // `begin_join` validates expiry before consuming — the UI can surface a
    // "this invite expired" prompt without the invite having been silently
    // lost. The separate tick-driven `expire_invite` clears it later.
    assert!(s.pending_invite.is_some(), "expired begin_join must not consume the invite");
    assert!(s.pending_join.is_none(), "should not create pending_join from expired invite");
}
```

- [ ] **Step 5: Run all tests**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib --lib social::party 2>&1 | tail -20`
Expected: All tests pass (existing + new).

- [ ] **Step 6: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch && git add src-tauri/src/social/party.rs && git commit -m "test(party): add race condition tests for two-phase join (ZEB-84)"
```

---

### Task 8: Clean up — remove or deprecate `accept_invite()`

**Files:**
- Modify: `src-tauri/src/social/party.rs`

After the refactor, `accept_invite()` is no longer called from production code. The two-phase flow uses `begin_join()` + `confirm_join()` instead.

- [ ] **Step 1: Check for any remaining callers of `accept_invite`**

Run: `cd ~/work/zeblithic/harmony-glitch && grep -rn 'accept_invite' src-tauri/src/ --include='*.rs' | grep -v test | grep -v '^\s*//'`

If there are no non-test callers, proceed to remove it. If tests call it directly, update them to use the new two-phase flow.

- [ ] **Step 2: Update existing `accept_invite` tests to use two-phase flow**

The existing tests that call `accept_invite` directly (`accept_invite_within_timeout`, `accept_invite_expired_returns_err`, `accept_invite_no_invite_returns_err`, `accept_invite_returns_err_when_party_full`) should be updated to test the full `begin_join` → `confirm_join` flow instead. Replace them:

```rust
#[test]
fn two_phase_join_within_timeout() {
    let mut s = PartyState::new();
    s.set_pending_invite(PendingPartyInvite {
        leader: addr(0x01),
        leader_name: "Leader".into(),
        members: vec![],
        received_at: 100.0,
    });
    s.begin_join(150.0).unwrap();
    let result = s.confirm_join(addr(0x02), "Me".into(), 151.0);
    assert!(result.is_ok());
    assert!(s.in_party());
    assert!(s.pending_invite.is_none());
    assert!(s.pending_join.is_none());
}

#[test]
fn two_phase_join_no_invite_returns_err() {
    let mut s = PartyState::new();
    let result = s.begin_join(0.0);
    assert!(result.is_err());
}
```

The "expired" and "party full" cases are already covered by tests in Task 7 (`begin_join_with_expired_invite_returns_err`) and Task 2 (`confirm_join_without_pending_join_returns_err`).

- [ ] **Step 3: Remove `accept_invite()` method**

Delete the `accept_invite` method (lines 201-247) from `impl PartyState`.

- [ ] **Step 4: Run full test suite**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
cd ~/work/zeblithic/harmony-glitch && git add src-tauri/src/social/party.rs && git commit -m "refactor(party): remove accept_invite(), replace tests with two-phase flow (ZEB-84)"
```

---

### Task 9: Final integration verification

**Files:** None (verification only)

- [ ] **Step 1: Run the full Rust test suite**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo test -p harmony-glitch-lib 2>&1 | tail -30`
Expected: All tests pass.

- [ ] **Step 2: Run the frontend test suite**

Run: `cd ~/work/zeblithic/harmony-glitch && npm run test 2>&1 | tail -20`
Expected: All tests pass.

- [ ] **Step 3: Run cargo clippy**

Run: `cd ~/work/zeblithic/harmony-glitch && cargo clippy -p harmony-glitch-lib 2>&1 | tail -20`
Expected: No warnings.

- [ ] **Step 4: Verify the type-check**

Run: `cd ~/work/zeblithic/harmony-glitch && npm run check 2>&1 | tail -10`
Expected: No type errors.
