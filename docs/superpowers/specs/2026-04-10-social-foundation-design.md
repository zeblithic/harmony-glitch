# Social Foundation

**Date:** 2026-04-10
**Status:** Approved

## Overview

Social foundation for Harmony Glitch: mood metabolics, hi emotes with daily viral variants, a trust-integrated buddy system, and ephemeral parties. These four subsystems create the core social loop — mood creates the need for social interaction, emotes and parties are the response, buddies are the persistent relationship layer.

This design is faithful to the original Glitch social mechanics (sourced from `tinyspeck/glitch-GameServerJS`) while embracing Harmony's decentralized P2P architecture. There is no central server — social state is local-first with mutual-witness for bilateral relationships and leader-authority for parties.

### Scope

**In scope (this spec):**
- Mood stat with three-tier decay curve (original Glitch faithful)
- Hi emote system with daily BLAKE3-seeded viral variants
- Buddy system with mutual-witness add and trust integration
- Party system with ephemeral leader-authority model
- Integration into existing proximity, trust, networking, and dialogue systems

**Out of scope (tracked as future Linear issues):**
- Persistent groups/clubs
- Extended emotes beyond hi (dance, hug, wave, etc.)
- Mood affecting skill learning speed / crafting quality / trust thresholds
- Buddy presence broadcasts across streets
- Street Spirit / gregariousness events
- Buff/debuff system
- Party spaces
- Social achievements

## Architecture

Follows the existing Rust-owns-logic / PixiJS-renders / Svelte-does-UI split. Three new Rust modules under a `SocialState` aggregator on `GameState`, mirroring the `NetworkState` pattern:

- `src-tauri/src/mood/` — stat tracking, three-tier decay, mood sources/effects
- `src-tauri/src/emote/` — hi emote system, viral variants, cooldowns, proximity targeting
- `src-tauri/src/social/` — buddy ledger, party sessions, `SocialState` aggregator, block list

### State Authority Model

- **Mood, energy:** Local-only, self-attested. Peers cannot verify your mood. Same as original Glitch where the server trusted client-reported mood.
- **Buddies, blocks:** Mutual-witness. Adding a buddy requires both parties to agree. Removing/blocking is unilateral.
- **Parties:** Leader-authority. The leader's node is authoritative for party membership. Works because parties are same-street only (leader is always reachable).
- **Emotes:** Fire-and-forget broadcast. Emote messages are sent to all peers on the street. The trust/epoch system prevents spam from fresh identities.

---

## Mood Metabolics

### Data Model

`mood: f64` and `max_mood: f64` on `GameState`. New players start at 100.0/100.0 (matching original Glitch's base max at level 1).

Exposed in `RenderFrame` as `mood: f64` and `max_mood: f64` for the HUD.

### Three-Tier Decay

Faithful to original Glitch's asymmetric curve, adapted to per-tick execution (original ran every 60s, ours runs at 60Hz with `dt`-based accumulation):

| Mood % | Decay Rate | Feel |
|--------|-----------|------|
| >80% | 1.5% of max_mood per minute | Fast drain — social pressure zone |
| 50-80% | 0.5% of max_mood per minute | Moderate — you notice but don't panic |
| <50% | 0.25% of max_mood per minute | Gentle — the floor is soft |

Implementation: `mood_decay(mood, max_mood, dt) -> f64` pure function in `mood/decay.rs`. Rate constants as module-level consts, converted to per-second values for `dt`-based tick.

```
DECAY_HIGH: 0.015 / 60.0    // 1.5% of max per minute, as per-second rate
DECAY_MID:  0.005 / 60.0    // 0.5% of max per minute
DECAY_LOW:  0.0025 / 60.0   // 0.25% of max per minute

THRESHOLD_HIGH: 0.80
THRESHOLD_MID:  0.50
```

Mood is clamped to `[0.0, max_mood]`. Never goes negative. No death mechanic from zero mood — zero mood means reduced progression efficiency.

### Mood Sources

| Source | Delta | Caller |
|--------|-------|--------|
| Hi emote (no match) | +5 | `EmoteState::handle_incoming_emote()` |
| Hi emote (variant match) | +10 | `EmoteState::handle_incoming_emote()` |
| Eating food | +item.mood_value | `eat()` in `item/energy.rs` |
| Quest completion | +reward.mood | `QuestTracker::complete_quest()` |
| Passive decay | -(curve rate * dt) | `MoodState::tick()` |

All sources call `MoodState::apply_mood_change(delta: f64)` — mood doesn't know about emotes; emotes don't know about mood internals. One-way dependency via a simple method.

### `mood_value` on Items

`mood_value: Option<u32>` added to `ItemDef` in `items.json`, alongside existing `energy_value`. Items can have both (food restores energy AND mood), either, or neither.

| Item | Energy Value | Mood Value |
|------|-------------|------------|
| cherry | 12 | 3 |
| grain | 10 | 2 |
| meat | 20 | 5 |
| milk | 15 | 4 |
| bread | 80 | 20 |
| cherry_pie | 100 | 30 |
| steak | 90 | 25 |
| butter | 60 | 15 |

Crafted foods give disproportionately more mood than raw foods (same pattern as energy), reinforcing the crafting loop.

### Economic Effect

Below 50% mood, imagination earnings are reduced:

```rust
pub fn mood_multiplier(mood: f64, max_mood: f64) -> f64 {
    let pct = mood / max_mood;
    if pct >= 0.5 {
        1.0
    } else {
        0.5 + pct  // scales from 0.5 at 0% to 1.0 at 50%
    }
}
```

This hooks into existing imagination grant points — quest rewards and crafting completions multiply by `mood_multiplier()`.

### Mood Protection

Mood decay is suppressed:
- During active dialogue (`GameState.active_dialogue.is_some()`)
- During the first 5 minutes after loading a save (grace period, tracked as `mood_grace_until: f64` on `MoodState`)

### Mood HUD

`MoodHud.svelte` — positioned top-left, stacked below `EnergyHud`.

- Purple/pink fill bar representing `mood / max_mood`
- Numeric value displayed (e.g., "72")
- Same semi-transparent dark pill styling as CurrantHud and EnergyHud (`rgba(26,26,46,0.85)`)
- `role="status"` live region for accessibility
- Color shifts toward grey when mood below 50% as visual warning
- `pointer-events: none` — doesn't intercept game clicks
- Updates reactively from `RenderFrame.mood` and `RenderFrame.max_mood`

### Save State

`mood: f64` and `max_mood: f64` added to `SaveState` with `#[serde(default = "default_mood")]` returning 100.0. Backward-compatible with existing saves.

### Testing

- Mood decay at each tier: >80%, 50-80%, <50% — correct rate applied
- Mood decay clamped at 0.0, never negative
- `apply_mood_change` capped at `max_mood`
- `mood_multiplier` returns 1.0 at 50%+, scales linearly below
- Mood decay suppressed during active dialogue
- Mood decay suppressed during grace period
- `mood_value` on ItemDef parsing (Some and None)
- Eating food with `mood_value` restores mood
- SaveState round-trip with mood
- SaveState missing mood defaults to 100.0

---

## Hi Emote System

### Daily Variant Assignment

11 variants (faithful to original Glitch): `Bats, Birds, Butterflies, Cubes, Flowers, Hands, Hearts, Hi, Pigs, Rocketships, Stars`

Each player's daily variant is deterministic:

```
variant_index = BLAKE3(identity_bytes || "hi-variant" || "2026-04-10") mod 11
```

No server needed. Any peer can independently compute any other peer's expected daily variant by knowing their `address_hash` and the current date.

### Viral Spreading

When player A hi's player B:
1. Player B "catches" player A's active variant
2. Stored as `caught_variant: Option<HiVariant>` in `EmoteState`
3. A player's *active* variant is their caught variant if set, otherwise their daily seed variant
4. If both players share the same active variant at the moment of hi → match bonus

Flow example:
1. You start the day with seeded variant `hearts`
2. Someone hi's you with `butterflies` → you now show `butterflies`
3. You hi someone who also has `butterflies` → match! +10 mood to both

### Interaction Mechanics

**Targeting:** `proximity_scan` extended to include remote players. Nearest player within 400px is the target (matching original Glitch hi radius). If no player nearby, the hi plays as an untargeted emote (visual only, no mood reward).

**Cooldown:** One hi per unique player per day. Tracked as `hi_today: HashSet<[u8; 16]>` in `EmoteState`, cleared on date change. No limit on how many different players you can hi.

**Epoch gate:** Sending hi requires `PeerEpoch::Initiate` or above (300s co-presence). Sandbox players can receive hi's but cannot send them. Prevents spam from fresh identities.

### Network Message

New `NetMessage::Emote(EmoteMessage)` variant:

```rust
pub struct EmoteMessage {
    pub emote_type: EmoteType,     // Hi (expandable later)
    pub variant: HiVariant,
    pub target: Option<[u8; 16]>,  // targeted peer, if any
}
```

Sent to all peers on the street (not just the target) so everyone sees the emote animation.

### Mood Effects

| Scenario | Mood Gain | Who Receives |
|----------|-----------|-------------|
| Hi a player (no variant match) | +5 | Target only |
| Hi a player (variant match) | +10 | Both players |
| Receive a hi (no variant match) | +5 | You (the target) |
| Receive a hi (variant match) | +10 | You (the target) |
| Untargeted hi (no one nearby) | 0 | Nobody |

The initiator gets mood only on a match — incentivizes seeking out players with your variant rather than spamming hi at everyone.

### Receiving a Hi

When `EmoteMessage` arrives with `target == Some(our_address_hash)`:
1. Check sender is not blocked
2. Check we haven't already been hi'd by this sender today
3. Catch sender's variant (`caught_variant = Some(sender_variant)`)
4. Compute match: does sender's variant == our active variant at time of receipt?
5. Apply mood: +5 base, or +10 if match
6. Record sender in `hi_received_today: HashSet<[u8; 16]>` to prevent repeat mood gains

### Frontend

- Emote hotkey (`H` key) triggers hi toward nearest player via `emote_hi()` IPC
- Floating emote animation: variant sprite (hearts, butterflies, etc.) rises from initiator toward target
- Feedback text: "+5 mood" or "+10 mood! Matching butterflies!" (green, reusing `PickupFeedback`)
- Current variant indicator: small icon near mood bar showing your active variant
- Interaction prompt when near a remote player includes "Hi" as an action option

### Ephemeral State

`hi_today`, `hi_received_today`, and `caught_variant` are not persisted in `SaveState`. They reset on date change or app restart. `last_hi_date: Option<String>` in `SaveState` tracks when to clear daily state (if the date has changed since last save).

### Testing

- BLAKE3 variant seeding is deterministic for same identity+date
- BLAKE3 variant seeding differs across dates
- BLAKE3 variant seeding differs across identities
- Variant catching: receiving a hi updates caught_variant
- Active variant: caught_variant overrides daily seed
- Match detection: same variant = match, different = no match
- Cooldown: second hi to same player in same day rejected
- Cooldown: hi to different player accepted
- Epoch gate: Sandbox player cannot send hi
- Epoch gate: Sandbox player can receive hi
- Blocked sender: hi ignored, no mood change
- Mood applied correctly: +5 no match, +10 match
- Daily state cleared on date change

---

## Buddy System

### Data Model

```rust
pub struct BuddyEntry {
    pub address_hash: [u8; 16],
    pub display_name: String,          // cached, updated on contact
    pub added_date: String,            // ISO date
    pub co_presence_total: f64,        // lifetime seconds spent on same street
    pub last_seen_date: Option<String>,
}
```

Buddy list stored as `buddies: Vec<BuddyEntry>` in `SaveState`. The `display_name` is a cache updated whenever the buddy is encountered on a street (names are mutable, `address_hash` is the stable key).

### Adding a Buddy (Mutual-Witness)

1. Player A targets Player B via proximity (within 400px, same interaction pipeline as emotes)
2. Player A sends `NetMessage::Social(SocialMessage::BuddyRequest { from: address_hash })`
3. Player B sees a prompt: "PlayerA wants to be buddies! Accept / Decline"
4. Player B accepts → sends `SocialMessage::BuddyAccept { from: address_hash }`
5. Both sides create a `BuddyEntry` and persist to `SaveState`

Guards:
- Cannot send request if target is already a buddy
- Cannot send request if target has blocked you (the message is silently dropped on their end)
- Cannot send request if you have blocked the target
- Pending requests expire after 90 seconds

### Removing a Buddy

Unilateral — you can remove anyone without their consent. Sends `SocialMessage::BuddyRemove { from: address_hash }` so the other side can clean up, but this is advisory (they might be offline). On next contact, if one side has the buddy entry and the other doesn't, the entry-holder gets a silent removal.

### Trust Integration

Being buddies provides concrete trust benefits in the existing `TrustStore`:

| Benefit | Mechanism |
|---------|-----------|
| Initial trust boost | On buddy add: `PeerTrust.opinion.update_positive(0.2)` |
| Co-presence accrual bonus | Between buddies: co-presence accumulates 50% faster toward epoch advancement |
| Vouch shortcut | Buddies can vouch for each other without full Citizen requirement; buddy vouch carries 75% of normal vouch weight |
| Trust decay resistance | Passive trust decay (`0.0001 * dt`) halved for buddies |

The initial trust boost is meaningful but not dominant — a buddy who cheats will still lose trust through the normal violation/gossip path.

### Blocking

`blocked: Vec<[u8; 16]>` in `SaveState`. Blocked players:
- Cannot send buddy requests, hi emotes, trade requests, or party invites to you
- Their chat messages are hidden client-side (filtered in frontend)
- They are still rendered on screen (no invisibility — that would be gameplay-exploitable)
- Blocking is unilateral, no notification sent to the blocked player
- Blocking a current buddy removes the buddy entry first

### Network Messages

```rust
pub enum SocialMessage {
    BuddyRequest { from: [u8; 16] },
    BuddyAccept { from: [u8; 16] },
    BuddyDecline { from: [u8; 16] },
    BuddyRemove { from: [u8; 16] },
    // Party messages defined in Party section
}
```

Added as `NetMessage::Social(SocialMessage)` on the existing event PubSub topic.

### Visual Indicators

When a buddy is on the same street:
- Name rendered in gold/amber (instead of white) above their avatar
- Small star icon next to their name
- `RemotePlayerFrame` gains `is_buddy: bool` so the frontend can make these decisions

### IPC Commands

| Command | Purpose |
|---------|---------|
| `buddy_request(peer_hash)` | Send buddy request to nearby player |
| `buddy_accept(peer_hash)` | Accept pending request |
| `buddy_decline(peer_hash)` | Decline pending request |
| `buddy_remove(peer_hash)` | Remove buddy |
| `block_player(peer_hash)` | Add to blocklist |
| `unblock_player(peer_hash)` | Remove from blocklist |
| `get_buddy_list()` | Returns buddy entries for UI |
| `get_blocked_list()` | Returns block list for UI |

### Testing

- Add buddy: mutual-witness flow produces entries on both sides
- Add buddy: request to existing buddy suppressed
- Add buddy: request to blocked player silently dropped
- Remove buddy: unilateral, local entry deleted
- Remove buddy: advisory message sent to peer
- Block player: added to blocked list, buddy entry removed if present
- Block player: hi/trade/buddy/party messages from blocked player ignored
- Unblock player: removed from blocked list
- Trust boost on buddy add: opinion shifts positive by 0.2
- Trust decay resistance: buddy decay rate halved
- Co-presence bonus: accumulates 50% faster for buddies
- Display name cache: updated when buddy encountered on street
- SaveState round-trip with buddies and blocked lists

---

## Party System

### Data Model

```rust
pub struct PartyState {
    pub party: Option<ActiveParty>,
    pub pending_invite: Option<PendingPartyInvite>,
}

pub struct ActiveParty {
    pub leader: [u8; 16],
    pub members: Vec<PartyMember>,  // includes leader
    pub created_at: f64,
}

pub struct PartyMember {
    pub address_hash: [u8; 16],
    pub display_name: String,
    pub joined_at: f64,
}

pub struct PendingPartyInvite {
    pub leader: [u8; 16],
    pub leader_name: String,
    pub members: Vec<[u8; 16]>,
    pub received_at: f64,           // for 90s timeout
}
```

Parties are **ephemeral** — not persisted in `SaveState`. They exist only while players are connected on the same street. Max party size: 5.

### Lifecycle

**Creation:** Implicit — inviting someone creates a party with you as leader if you're not already in one.

**Inviting:**
1. Leader targets a nearby player (400px proximity)
2. Sends `SocialMessage::PartyInvite { leader, members }` (members list so invitee knows who's in the party)
3. Target sees prompt: "PlayerA invites you to a party with [member list]. Join / Decline"
4. 90-second timeout (matching original Glitch)
5. On accept: target sends `PartyAccept`, leader broadcasts `PartyMemberJoined` to all members
6. Guards: cannot invite blocked players, players already in a party, or players at Sandbox epoch

**Leaving:**
- Any member can leave → sends `PartyLeave`
- If the leader leaves, leadership transfers to the longest-tenured member, `PartyLeaderChanged` broadcast
- If only one member remains, party dissolves → `PartyDissolved` broadcast

**Disbanding:**
- Party dissolves when all members leave
- 30-second grace period for street transitions — if all remaining members leave the street, party dissolves after 30s unless someone returns

### Leader Authority

The leader's node is authoritative for party state. This works because:
- Parties are same-street only (leader is always reachable via existing PubSub)
- Leader relays membership changes to all members via broadcast
- If the leader disconnects (stale timeout), leadership transfers to longest-tenured remaining member
- Non-leaders can: leave, send party chat. Only the leader can: invite, kick.

### Party Chat

New chat channel scoped to party members. `ChatMessage` gains a `channel` field:

```rust
pub enum ChatChannel {
    Street,   // existing behavior (all peers on street)
    Party,    // party members only
}
```

`ChatChannel` defaults to `Street` via `#[serde(default)]` for backward compatibility with existing `ChatMessage` format.

Party messages are filtered on send — only delivered to current `ActiveParty.members` address hashes. Frontend shows a channel toggle or `/party` prefix.

### Mood Bonus

While in a party with 2+ members on the same street, mood decay rate is reduced by 25%. Applied as a multiplier in `MoodState::tick()`:

```
effective_decay = base_decay * if in_party { 0.75 } else { 1.0 }
```

Incentivizes grouping up without punishing solo play.

### Epoch Gate

Creating/joining parties requires `PeerEpoch::Initiate` (300s co-presence). Same gate as trading and chat.

### Network Messages

```rust
// Added to SocialMessage enum
PartyInvite { leader: [u8; 16], members: Vec<[u8; 16]> },
PartyAccept { from: [u8; 16] },
PartyDecline { from: [u8; 16] },
PartyLeave { from: [u8; 16] },
PartyKick { target: [u8; 16] },
PartyMemberJoined { member: [u8; 16], display_name: String },
PartyMemberLeft { member: [u8; 16] },
PartyDissolved,
PartyLeaderChanged { new_leader: [u8; 16] },
```

### Visual Indicators

- Party members get a colored border/icon next to their name (distinct from buddy star)
- `RemotePlayerFrame` gains `party_role: Option<PartyRole>` where `PartyRole` is `Leader` or `Member`
- Party member list in a small collapsible panel

### Frontend

- `PartyPanel.svelte` — member list, leave button, kick button (leader only)
- Party chat integrated into `ChatInput.svelte` with channel selector
- Invite via interaction prompt when near a player

### IPC Commands

| Command | Purpose |
|---------|---------|
| `party_invite(peer_hash)` | Invite nearby player (creates party if needed) |
| `party_accept()` | Accept pending invite |
| `party_decline()` | Decline pending invite |
| `party_leave()` | Leave current party |
| `party_kick(peer_hash)` | Kick member (leader only) |
| `get_party_state()` | Returns party info for UI |

### Testing

- Create party: inviting a player when not in a party creates one with you as leader
- Invite: requires proximity (400px)
- Invite: requires Initiate epoch
- Invite: cannot invite blocked or already-in-party players
- Invite: max 5 members enforced
- Invite timeout: 90 seconds
- Accept: both sides update party state
- Leave: member removed, others notified
- Leader leave: leadership transfers to longest-tenured
- Last member leave: party dissolves
- Kick: leader-only, target removed and notified
- Grace period: party survives 30s when members transition streets
- Party chat: messages reach party members only
- Party chat: channel defaults to Street for backward compat
- Mood bonus: decay reduced 25% while in party with 2+ members
- Mood bonus: solo party (others left) no bonus

---

## Integration Layer

### SocialState Aggregator

```rust
// social/mod.rs
pub struct SocialState {
    pub mood: MoodState,
    pub emotes: EmoteState,
    pub buddies: BuddyState,
    pub party: PartyState,
    pub blocked: Vec<[u8; 16]>,
}
```

Lives on `GameState` as `pub social: SocialState`. Single tick entry point:

```rust
impl SocialState {
    pub fn tick(&mut self, dt: f64, context: &SocialTickContext) { ... }
}

pub struct SocialTickContext<'a> {
    pub remote_players: &'a [RemotePlayerFrame],
    pub trust_store: &'a TrustStore,
    pub current_date: &'a str,
    pub in_dialogue: bool,
    pub game_time: f64,
}
```

`SocialTickContext` carries read-only references the social systems need from other parts of game state. This avoids circular dependencies.

### RemotePlayerFrame Extensions

```rust
pub struct RemotePlayerFrame {
    // existing fields
    pub address_hash: String,
    pub display_name: String,
    pub x: f64, pub y: f64,
    pub facing: String,
    pub on_ground: bool,
    pub animation: AnimationState,
    pub avatar: Option<AvatarAppearance>,
    // new social fields
    pub epoch: String,                          // "Sandbox", "Initiate", "Citizen"
    pub is_buddy: bool,
    pub party_role: Option<String>,             // "Leader", "Member", or absent
    pub emote_animation: Option<EmoteAnimationFrame>,
}

pub struct EmoteAnimationFrame {
    pub variant: String,         // "hearts", "butterflies", etc.
    pub target_hash: Option<String>,
    pub started_at: f64,
}
```

Populated during step-7 augmentation in `lib.rs::game_loop`, where `NetworkState` remote frames are merged into `RenderFrame`. Cross-references buddy list, party state, and epoch data.

Mood is **not** broadcast to peers. Your mood bar is visible only to you.

### Proximity Extension

`proximity_scan` in `interaction.rs` gains a new candidate type:

```rust
pub enum NearestInteractable {
    Entity { index: usize, distance: f64 },
    GroundItem { index: usize, distance: f64 },
    RemotePlayer { address_hash: [u8; 16], distance: f64 },
}
```

Remote players are candidates at 400px range (social interaction radius). When nearest interactable is a remote player, the interaction prompt shows contextual verbs based on available actions: "Hi", "Trade", "Invite to party", "Add buddy" — filtered by epoch, existing relationships, and party state.

### NetMessage Extension

```rust
pub enum NetMessage {
    PlayerState(PlayerNetState),
    Chat(ChatMessage),           // gains ChatChannel field
    Presence(PresenceEvent),
    AvatarUpdate(Box<AvatarAppearance>),
    Trade(TradeMessage),
    Gossip(GossipEnvelope),
    Vouch(VouchMessage),
    Emote(EmoteMessage),         // new
    Social(SocialMessage),       // new
}
```

### SaveState Extensions

```rust
pub struct SaveState {
    // ... existing fields ...

    #[serde(default = "default_mood")]
    pub mood: f64,
    #[serde(default = "default_mood")]
    pub max_mood: f64,
    #[serde(default)]
    pub buddies: Vec<BuddySaveEntry>,
    #[serde(default)]
    pub blocked: Vec<String>,        // address_hash hex strings
    #[serde(default)]
    pub last_hi_date: Option<String>,
}
```

All new fields use `#[serde(default)]` for backward compatibility. Parties and emote daily state are intentionally excluded — they are ephemeral.

### IPC Commands Summary

| Command | Module | Purpose |
|---------|--------|---------|
| `get_mood()` | mood | Returns `{ mood, maxMood }` |
| `emote_hi()` | emote | Trigger hi toward nearest player |
| `buddy_request(peer_hash)` | social | Send buddy request |
| `buddy_accept(peer_hash)` | social | Accept pending request |
| `buddy_decline(peer_hash)` | social | Decline pending request |
| `buddy_remove(peer_hash)` | social | Remove buddy |
| `block_player(peer_hash)` | social | Add to blocklist |
| `unblock_player(peer_hash)` | social | Remove from blocklist |
| `get_buddy_list()` | social | Returns buddy list for UI |
| `get_blocked_list()` | social | Returns block list for UI |
| `party_invite(peer_hash)` | social | Invite player to party |
| `party_accept()` | social | Accept party invite |
| `party_decline()` | social | Decline party invite |
| `party_leave()` | social | Leave current party |
| `party_kick(peer_hash)` | social | Kick member (leader only) |
| `get_party_state()` | social | Returns party info for UI |

### Frontend Components

| Component | Purpose |
|-----------|---------|
| `MoodHud.svelte` | Mood bar, top-left below EnergyHud |
| `PartyPanel.svelte` | Party member list, leave/kick actions |
| `BuddyListPanel.svelte` | Buddy list with co-presence and last seen |
| `SocialPrompt.svelte` | Contextual interaction menu for remote players |
| `EmoteAnimation.svelte` | Floating variant sprite animation |

### TypeScript Types

```typescript
// additions to types.ts
interface RenderFrame {
    // ... existing fields ...
    mood: number;
    maxMood: number;
}

interface RemotePlayerFrame {
    // ... existing fields ...
    epoch: string;
    isBuddy: boolean;
    partyRole: string | null;
    emoteAnimation: EmoteAnimationFrame | null;
}

interface EmoteAnimationFrame {
    variant: string;
    targetHash: string | null;
    startedAt: number;
}
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Hi at Sandbox epoch | IPC returns error, "Need more time in world" |
| Hi same player twice in one day | IPC returns error, "Already greeted today" |
| Hi with no player nearby | Untargeted emote plays, no mood, no error |
| Buddy request to existing buddy | IPC returns error, suppressed |
| Buddy request to blocked player | IPC returns error |
| Buddy request while target offline/gone | Message dropped, no crash |
| Party invite at max capacity | IPC returns error, "Party is full" |
| Party invite to Sandbox player | IPC returns error |
| Party kick by non-leader | IPC returns error, "Only the leader can kick" |
| Party action with no active party | IPC returns error |
| Eat food with mood_value at full mood | Mood capped at max, food still consumed (energy may still be needed) |
| Save file missing mood/buddy fields | Defaults applied (100.0 mood, empty lists) |
| Blocked player sends any social message | Silently dropped, no error to sender |

---

## Out of Scope

- Mood affecting skill learning speed, crafting quality, or trust thresholds (future enhancement)
- Extended emote types beyond hi (dance, hug, wave)
- Buddy presence broadcasts across streets (cross-street online status)
- Persistent groups/clubs
- Street Spirit / gregariousness loneliness events
- Buff/debuff system
- Party spaces (temporary locations)
- Social achievements / hi streaks
- Mood upgrades via imagination (decay rate reduction upgrades)
- NPC social interactions (NPC-as-buddy, NPC mood reactions)
- Trust persistence across sessions (currently cleared on street change)
- Death / Hell system interaction with mood
