# Social UI Chrome Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire frontend social event listeners, add buddy request and party invite prompts, connect SocialPrompt stubs, and populate BuddyListPanel/PartyPanel with real data.

**Architecture:** Add event listener functions to `ipc.ts`, create two new prompt components following the existing `TradePrompt` pattern, and wire everything together in `App.svelte` using pull-on-push state refresh (event arrives → re-fetch full state via IPC).

**Tech Stack:** Svelte 5 (runes), TypeScript, Tauri IPC events (`@tauri-apps/api/event`)

---

### Task 1: Add social event listeners to ipc.ts

**Files:**
- Modify: `src/lib/ipc.ts:206-233`

This task adds `BuddyEvent`, `PartyEvent`, and `EmoteEvent` types plus three listener functions that each register multiple Tauri `listen()` calls and return a combined unlisten function.

- [ ] **Step 1: Add the BuddyEvent type and onBuddyEvent listener**

Append after the existing social section (after line 233 of `src/lib/ipc.ts`):

```typescript
// ── Social event listeners ───────────────────────────────────────────

export interface BuddyEvent {
  type: 'request_received' | 'accepted' | 'declined' | 'removed';
  fromHash: string;
  fromName?: string;
}

export async function onBuddyEvent(callback: (event: BuddyEvent) => void): Promise<UnlistenFn> {
  const unlistens = await Promise.all([
    listen<{ fromHash: string; fromName: string }>('buddy_request_received', (e) =>
      callback({ type: 'request_received', fromHash: e.payload.fromHash, fromName: e.payload.fromName })),
    listen<{ fromHash: string; fromName: string }>('buddy_accepted', (e) =>
      callback({ type: 'accepted', fromHash: e.payload.fromHash, fromName: e.payload.fromName })),
    listen<{ fromHash: string }>('buddy_declined', (e) =>
      callback({ type: 'declined', fromHash: e.payload.fromHash })),
    listen<{ fromHash: string }>('buddy_removed', (e) =>
      callback({ type: 'removed', fromHash: e.payload.fromHash })),
  ]);
  return () => unlistens.forEach(u => u());
}
```

- [ ] **Step 2: Add the PartyEvent type and onPartyEvent listener**

Append directly after the `onBuddyEvent` function:

```typescript
export type PartyEvent =
  | { type: 'invite_received'; leaderHash: string; leaderName: string; memberCount: number }
  | { type: 'member_joined'; memberHash: string; memberName: string }
  | { type: 'invite_declined'; fromHash: string }
  | { type: 'leader_changed'; newLeaderHash: string }
  | { type: 'member_left'; memberHash: string }
  | { type: 'kick'; targetHash: string }
  | { type: 'dissolved' };

export async function onPartyEvent(callback: (event: PartyEvent) => void): Promise<UnlistenFn> {
  const unlistens = await Promise.all([
    listen<{ leaderHash: string; leaderName: string; memberCount: number }>('party_invite_received', (e) =>
      callback({ type: 'invite_received', ...e.payload })),
    listen<{ memberHash: string; memberName: string }>('party_member_joined', (e) =>
      callback({ type: 'member_joined', ...e.payload })),
    listen<{ fromHash: string }>('party_invite_declined', (e) =>
      callback({ type: 'invite_declined', fromHash: e.payload.fromHash })),
    listen<{ newLeaderHash: string }>('party_leader_changed', (e) =>
      callback({ type: 'leader_changed', newLeaderHash: e.payload.newLeaderHash })),
    listen<{ memberHash: string }>('party_member_left', (e) =>
      callback({ type: 'member_left', memberHash: e.payload.memberHash })),
    listen<{ targetHash: string }>('party_kick', (e) =>
      callback({ type: 'kick', targetHash: e.payload.targetHash })),
    listen<Record<string, never>>('party_dissolved', () =>
      callback({ type: 'dissolved' })),
  ]);
  return () => unlistens.forEach(u => u());
}
```

- [ ] **Step 3: Add the EmoteEvent type and onEmoteReceived listener**

Append directly after `onPartyEvent`:

```typescript
export interface EmoteEvent {
  senderHash: string;
  senderName: string;
  variant: string;
  moodDelta: number;
}

export async function onEmoteReceived(callback: (event: EmoteEvent) => void): Promise<UnlistenFn> {
  return listen<EmoteEvent>('emote_received', (e) => callback(e.payload));
}
```

- [ ] **Step 4: Verify the frontend builds**

Run: `cd /home/zeblith/work/zeblithic/harmony-glitch/.claude/worktrees/social-networking-wire && npm run build`
Expected: Build succeeds with no type errors.

- [ ] **Step 5: Commit**

```bash
git add src/lib/ipc.ts
git commit -m "feat(social-ui): add buddy, party, and emote event listeners to ipc.ts"
```

---

### Task 2: Create BuddyRequestPrompt.svelte

**Files:**
- Create: `src/lib/components/BuddyRequestPrompt.svelte`
- Reference: `src/lib/components/TradePrompt.svelte` (pattern to follow)

- [ ] **Step 1: Create the component**

Create `src/lib/components/BuddyRequestPrompt.svelte`:

```svelte
<script lang="ts">
  let {
    senderName = '',
    visible = false,
    onAccept = undefined,
    onDecline = undefined,
  }: {
    senderName: string;
    visible: boolean;
    onAccept?: () => void;
    onDecline?: () => void;
  } = $props();
</script>

{#if visible}
  <div class="buddy-prompt" role="alertdialog" aria-label="Buddy request from {senderName}">
    <p class="buddy-prompt-text"><strong>{senderName}</strong> wants to be buddies</p>
    <div class="buddy-prompt-actions">
      <button class="buddy-prompt-btn accept" onclick={() => onAccept?.()} aria-label="Accept buddy request from {senderName}">Accept</button>
      <button class="buddy-prompt-btn decline" onclick={() => onDecline?.()} aria-label="Decline buddy request from {senderName}">Decline</button>
    </div>
  </div>
{/if}

<style>
  .buddy-prompt {
    position: fixed;
    top: 80px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(30, 30, 46, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    padding: 12px 20px;
    z-index: 200;
    display: flex;
    align-items: center;
    gap: 16px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }

  .buddy-prompt-text {
    margin: 0;
    color: #e0e0e0;
    font-size: 14px;
  }

  .buddy-prompt-actions {
    display: flex;
    gap: 8px;
  }

  .buddy-prompt-btn {
    padding: 6px 14px;
    border: none;
    border-radius: 4px;
    font-size: 13px;
    cursor: pointer;
  }

  .buddy-prompt-btn.accept {
    background: #5865f2;
    color: white;
  }

  .buddy-prompt-btn.accept:hover {
    background: #4752c4;
  }

  .buddy-prompt-btn.decline {
    background: rgba(255, 255, 255, 0.1);
    color: #ccc;
  }

  .buddy-prompt-btn.decline:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .buddy-prompt-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }
</style>
```

- [ ] **Step 2: Verify the frontend builds**

Run: `npm run build`
Expected: Build succeeds (component isn't used yet, but must compile).

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/BuddyRequestPrompt.svelte
git commit -m "feat(social-ui): add BuddyRequestPrompt component"
```

---

### Task 3: Create PartyInvitePrompt.svelte

**Files:**
- Create: `src/lib/components/PartyInvitePrompt.svelte`
- Reference: `src/lib/components/TradePrompt.svelte` (pattern to follow)

- [ ] **Step 1: Create the component**

Create `src/lib/components/PartyInvitePrompt.svelte`:

```svelte
<script lang="ts">
  let {
    leaderName = '',
    memberCount = 0,
    visible = false,
    onAccept = undefined,
    onDecline = undefined,
  }: {
    leaderName: string;
    memberCount: number;
    visible: boolean;
    onAccept?: () => void;
    onDecline?: () => void;
  } = $props();
</script>

{#if visible}
  <div class="party-prompt" role="alertdialog" aria-label="Party invite from {leaderName}">
    <p class="party-prompt-text"><strong>{leaderName}</strong> invited you to a party ({memberCount} {memberCount === 1 ? 'member' : 'members'})</p>
    <div class="party-prompt-actions">
      <button class="party-prompt-btn accept" onclick={() => onAccept?.()} aria-label="Join {leaderName}'s party">Join</button>
      <button class="party-prompt-btn decline" onclick={() => onDecline?.()} aria-label="Decline party invite from {leaderName}">Decline</button>
    </div>
  </div>
{/if}

<style>
  .party-prompt {
    position: fixed;
    top: 80px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(30, 30, 46, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    padding: 12px 20px;
    z-index: 200;
    display: flex;
    align-items: center;
    gap: 16px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }

  .party-prompt-text {
    margin: 0;
    color: #e0e0e0;
    font-size: 14px;
  }

  .party-prompt-actions {
    display: flex;
    gap: 8px;
  }

  .party-prompt-btn {
    padding: 6px 14px;
    border: none;
    border-radius: 4px;
    font-size: 13px;
    cursor: pointer;
  }

  .party-prompt-btn.accept {
    background: #5865f2;
    color: white;
  }

  .party-prompt-btn.accept:hover {
    background: #4752c4;
  }

  .party-prompt-btn.decline {
    background: rgba(255, 255, 255, 0.1);
    color: #ccc;
  }

  .party-prompt-btn.decline:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .party-prompt-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }
</style>
```

- [ ] **Step 2: Verify the frontend builds**

Run: `npm run build`
Expected: Build succeeds.

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/PartyInvitePrompt.svelte
git commit -m "feat(social-ui): add PartyInvitePrompt component"
```

---

### Task 4: Wire everything together in App.svelte

**Files:**
- Modify: `src/App.svelte`

This task wires the event listeners, adds prompt state, populates buddy/party data, connects the SocialPrompt stubs, and renders the new prompt components.

- [ ] **Step 1: Add imports**

In `src/App.svelte`, update the existing import from `'./lib/ipc'` (line 30) to add the new functions:

Add to the import list: `onBuddyEvent, onPartyEvent, getBuddyList, getPartyState, buddyRequest, buddyAccept, buddyDecline, partyInvite, partyAccept, partyDecline`

Add the new component imports after line 29:

```typescript
import BuddyRequestPrompt from './lib/components/BuddyRequestPrompt.svelte';
import PartyInvitePrompt from './lib/components/PartyInvitePrompt.svelte';
```

- [ ] **Step 2: Add prompt state variables**

After the existing social state block (after line 88), add:

```typescript
let ourAddressHash = '';
let buddyRequestVisible = $state(false);
let buddyRequestName = $state('');
let buddyRequestHash = $state('');
let partyInviteVisible = $state(false);
let partyInviteName = $state('');
let partyInviteCount = $state(0);
```

Also, in the existing `onMount` block where `getIdentity()` is called (around line 92), store the address hash:

```typescript
const identity = await getIdentity();
identityReady = identity.setupComplete;
ourAddressHash = identity.addressHash;
```

- [ ] **Step 3: Add helper functions for refreshing social state**

Add these helper functions inside the `<script>` block (after the new state variables):

```typescript
async function refreshBuddyList() {
  try {
    const result = await getBuddyList();
    buddies = result.buddies;
  } catch (e) {
    console.error('Failed to refresh buddy list:', e);
  }
}

async function refreshPartyState() {
  try {
    const result = await getPartyState();
    partyInParty = result.inParty;
    partyMembers = result.members;
    partyIsLeader = result.leader === ourAddressHash;
  } catch (e) {
    console.error('Failed to refresh party state:', e);
  }
}
```

- [ ] **Step 4: Wire event listeners in onMount**

In the `onMount` block, after the existing `onTradeEvent(...)` call (after line 183), add:

```typescript
    // Listen for buddy events
    onBuddyEvent((event) => {
      switch (event.type) {
        case 'request_received':
          buddyRequestName = event.fromName ?? 'Unknown';
          buddyRequestHash = event.fromHash;
          buddyRequestVisible = true;
          break;
        case 'accepted':
        case 'declined':
        case 'removed':
          refreshBuddyList();
          break;
      }
    });

    // Listen for party events
    onPartyEvent((event) => {
      switch (event.type) {
        case 'invite_received':
          partyInviteName = event.leaderName;
          partyInviteCount = event.memberCount;
          partyInviteVisible = true;
          break;
        case 'kick':
        case 'dissolved':
          partyInParty = false;
          partyMembers = [];
          partyIsLeader = false;
          break;
        default:
          refreshPartyState();
          break;
      }
    });

    // Initial social state fetch
    refreshBuddyList();
    refreshPartyState();
```

- [ ] **Step 5: Wire the SocialPrompt stubs**

Find the `<SocialPrompt>` usage (around line 762). Change:

```svelte
        canInvite={false}
        canBuddy={false}
```

To:

```svelte
        canInvite={!target.inParty && (partyIsLeader || !partyInParty)}
        canBuddy={!target.isBuddy}
```

And change:

```svelte
        onInvite={() => {}}
        onBuddy={() => {}}
```

To:

```svelte
        onInvite={() => partyInvite(target.addressHash).catch(console.error)}
        onBuddy={() => buddyRequest(target.addressHash).catch(console.error)}
```

- [ ] **Step 6: Render the new prompt components**

After the existing `<TradePrompt>` usage (around line 640), add:

```svelte
    <BuddyRequestPrompt
      senderName={buddyRequestName}
      visible={buddyRequestVisible}
      onAccept={async () => {
        try {
          await buddyAccept(buddyRequestHash);
          await refreshBuddyList();
        } catch (e) {
          console.error('Buddy accept failed:', e);
        }
        buddyRequestVisible = false;
      }}
      onDecline={async () => {
        try {
          await buddyDecline(buddyRequestHash);
        } catch (e) {
          console.error('Buddy decline failed:', e);
        }
        buddyRequestVisible = false;
      }}
    />
    <PartyInvitePrompt
      leaderName={partyInviteName}
      memberCount={partyInviteCount}
      visible={partyInviteVisible}
      onAccept={async () => {
        try {
          await partyAccept();
          await refreshPartyState();
        } catch (e) {
          console.error('Party accept failed:', e);
        }
        partyInviteVisible = false;
      }}
      onDecline={async () => {
        try {
          await partyDecline();
        } catch (e) {
          console.error('Party decline failed:', e);
        }
        partyInviteVisible = false;
      }}
    />
```

- [ ] **Step 7: Verify the frontend builds**

Run: `npm run build`
Expected: Build succeeds with no type errors.

- [ ] **Step 8: Commit**

```bash
git add src/App.svelte
git commit -m "feat(social-ui): wire social events, prompts, and SocialPrompt stubs in App.svelte"
```
