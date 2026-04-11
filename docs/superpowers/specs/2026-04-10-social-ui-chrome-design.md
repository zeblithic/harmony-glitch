# Social UI Chrome — Design Spec

**Goal:** Make the social systems (buddies, parties, emotes) visible and interactive in the frontend so two players can actually use them in a multiplayer session.

**Architecture:** Wire backend social events to frontend listeners, add dedicated prompt components for incoming requests/invites, connect the existing SocialPrompt stubs, and populate the existing BuddyListPanel/PartyPanel with real data via event-driven refresh.

**Tech Stack:** Svelte 5 (runes), TypeScript, Tauri IPC events

---

## Context

PR #50 (ZEB-83) wired all social state machines to the P2P network. The backend emits events (`buddy_request_received`, `party_invite_received`, `emote_received`, etc.) but the frontend has no listeners. Existing components (`BuddyListPanel`, `PartyPanel`, `SocialPrompt`, `MoodHud`) are rendered but receive empty/default data because nothing calls `getBuddyList()` or `getPartyState()`.

The gap is event plumbing and two missing prompt components, not a ground-up build.

## What Already Exists

| Component | Status |
|-----------|--------|
| `BuddyListPanel.svelte` | Rendered, but `buddies` array never populated |
| `PartyPanel.svelte` | Rendered, but `partyInParty` always false |
| `SocialPrompt.svelte` | Rendered, but `canInvite=false`, `canBuddy=false`, callbacks are `() => {}` |
| `MoodHud.svelte` | Fully implemented and wired |
| `EmoteAnimation.svelte` | Fully implemented and wired |
| `TradePrompt.svelte` | Fully implemented — serves as pattern for new prompts |
| `ipc.ts` social functions | All IPC commands defined (`buddyRequest`, `partyInvite`, etc.) |
| `ipc.ts` social listeners | None — `onTradeEvent` exists as the pattern to follow |

## What We're Building

### 1. Event Listeners in `ipc.ts`

Add event listener functions following the `onTradeEvent` pattern:

```typescript
// Buddy events
export interface BuddyEvent {
  type: 'request_received' | 'accepted' | 'declined' | 'removed';
  fromHash: string;
  fromName?: string;
}

export function onBuddyEvent(callback: (event: BuddyEvent) => void): Promise<UnlistenFn> {
  // Listen to buddy_request_received, buddy_accepted, buddy_declined, buddy_removed
  // Normalize into unified BuddyEvent shape
}

// Party events
export interface PartyEvent {
  type: 'invite_received' | 'member_joined' | 'invite_declined'
      | 'leader_changed' | 'member_left' | 'kick' | 'dissolved';
  // Fields vary by type — see backend payloads
}

export function onPartyEvent(callback: (event: PartyEvent) => void): Promise<UnlistenFn> {
  // Listen to all party_* events, normalize into PartyEvent
}

// Emote events
export interface EmoteEvent {
  senderHash: string;
  senderName: string;
  variant: string;
  moodDelta: number;
}

export function onEmoteReceived(callback: (event: EmoteEvent) => void): Promise<UnlistenFn>
```

**Implementation note:** Each `on*` function registers multiple Tauri `listen()` calls internally and returns a combined unlisten function that tears down all of them.

### 2. BuddyRequestPrompt.svelte (new component)

Follows `TradePrompt.svelte` pattern exactly:

- **Position:** Fixed, top-center (`top: 80px; left: 50%; transform: translateX(-50%)`)
- **Role:** `role="alertdialog"` with `aria-label="Buddy request from {senderName}"`
- **Content:** "{senderName} wants to be buddies" with Accept / Decline buttons
- **Props:** `{ visible: boolean; senderName: string; senderHash: string; onAccept: () => void; onDecline: () => void }`
- **Styling:** Same dark translucent background as TradePrompt

### 3. PartyInvitePrompt.svelte (new component)

Same pattern:

- **Position:** Fixed, top-center (same as BuddyRequestPrompt — only one shows at a time)
- **Role:** `role="alertdialog"` with `aria-label="Party invite from {leaderName}"`
- **Content:** "{leaderName} invited you to a party ({memberCount} members)" with Join / Decline buttons
- **Props:** `{ visible: boolean; leaderName: string; memberCount: number; onAccept: () => void; onDecline: () => void }`

### 4. Wire SocialPrompt Stubs

In `App.svelte`, change:

```svelte
canInvite={false}
canBuddy={false}
onInvite={() => {}}
onBuddy={() => {}}
```

To:

```svelte
canInvite={!target.inParty && (partyIsLeader || !partyInParty)}
canBuddy={!target.isBuddy}
onInvite={() => partyInvite(target.addressHash).catch(console.error)}
onBuddy={() => buddyRequest(target.addressHash).catch(console.error)}
```

**Invite logic:** Show "Invite" when the target isn't already in our party AND either we're the leader (can invite to existing party) or we're not in a party (auto-creates one). Hide if we're a non-leader member (can't invite).

**Buddy logic:** Show "Buddy" when the target isn't already our buddy. The `isBuddy` field comes from `NearestSocialTarget` in `RenderFrame`.

### 5. Event-Driven State Refresh in App.svelte

**Pull-on-push pattern:** When a social event arrives, re-fetch the full state via IPC rather than trying to apply incremental updates. This avoids state duplication between frontend and backend.

```
buddy event → getBuddyList() → update buddies state
party event → getPartyState() → update partyInParty/partyMembers/partyIsLeader state
```

**Prompt visibility state:** Add `$state` variables for the two new prompts:

```typescript
let buddyRequestVisible = $state(false);
let buddyRequestName = $state('');
let buddyRequestHash = $state('');

let partyInviteVisible = $state(false);
let partyInviteName = $state('');
let partyInviteCount = $state(0);
```

**Event handler wiring (in the game startup block alongside existing `onTradeEvent`):**

- `onBuddyEvent`: On `request_received`, set prompt visible. On `accepted`/`declined`/`removed`, refresh buddy list.
- `onPartyEvent`: On `invite_received`, set prompt visible. On all other events, refresh party state. On `kick`/`dissolved`, also clear party state.
- `onEmoteReceived`: No UI action needed for functional MVP (emote animation already works via RenderFrame).

**Initial data fetch:** On game start (alongside existing setup), call `getBuddyList()` and `getPartyState()` once to populate initial state.

## Accessibility

All new components follow existing patterns:

- `role="alertdialog"` on prompts (same as TradePrompt)
- `aria-label` on all buttons with context (e.g., "Accept buddy request from Alice")
- `:focus-visible` states with 2px gold outlines
- Prompts use `aria-live="assertive"` wrapper for screen reader announcement

## Files Changed

| File | Change |
|------|--------|
| `src/lib/ipc.ts` | Add `BuddyEvent`, `PartyEvent`, `EmoteEvent` types; add `onBuddyEvent`, `onPartyEvent`, `onEmoteReceived` listener functions |
| `src/lib/components/BuddyRequestPrompt.svelte` | New — incoming buddy request accept/decline prompt |
| `src/lib/components/PartyInvitePrompt.svelte` | New — incoming party invite accept/decline prompt |
| `src/App.svelte` | Wire event listeners, add prompt state, wire SocialPrompt callbacks, add initial data fetch |

## Out of Scope

- Emote text feedback / match bonus display
- Notification toasts for buddy accepted/declined
- Keyboard shortcuts for social actions
- Chat UI improvements
- Unified notification queue system
