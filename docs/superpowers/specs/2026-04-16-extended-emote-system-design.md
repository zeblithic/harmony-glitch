# Extended Emote System — Design (ZEB-76)

**Status:** Design approved, ready for implementation plan.
**Linear:** [ZEB-76](https://linear.app/zeblith/issue/ZEB-76/glitch-extended-emote-system-dance-hug-wave-etc).
**Related:** [ZEB-130](https://linear.app/zeblith/issue/ZEB-130/glitch-chat-slash-command-system-parser-handler-registry) (filed during this brainstorm — chat slash-command parser, a future enabler for keyboard-native `/dance`-style triggers).

## Goal

Expand the emote vocabulary beyond Hi with a starter set of five new emotes — **dance, wave, hug, high-five, applaud** — backed by a unified data model, a bottom-palette picker UX, tiered rate-limiting, and per-emote privacy toggles. Hi's existing viral-variant semantics are preserved unchanged.

## Motivation

The social foundation (PRs #50, #51, #53, #54) shipped with a single emote (Hi). Players currently have no vocabulary for celebration, affection, self-expression, or casual greeting. ZEB-76 fills that gap without building a data-driven emote registry (YAGNI until we know which fields matter).

## Design decisions (brainstorm outcomes)

### 1. Scope — five emotes in v1

**Dance, wave, hug, high-five, applaud.** Chosen to cover the main tonal categories:

| Emote | Category | Target model |
|---|---|---|
| Dance | Self-expression (solo) | Broadcast, no target |
| Wave | Casual greeting | Targeted or broadcast |
| Hug | Affection | Must target; must be in range |
| High-five | Celebration (co-op) | Must target; must be in range |
| Applaud | Celebration (you-focused) | Targeted or broadcast |

Rejected alternatives:
- **Bigger vocabulary (8-10 emotes)** — dilutes mood-mechanic signal; more animation work.
- **Data-driven registry** — designing config fields in advance of knowing which emotes actually need them is guessing.

### 2. Targeted-emote consent model — one-way + per-emote privacy toggle

Targeted emotes (hug, high-five) fire one-way without a request/accept flow — matches original Glitch's spontaneous feel and avoids a "waiting for confirmation" UI state. Each player has per-emote privacy toggles (`accept_hug`, `accept_high_five`) defaulting to `true`; when off, incoming messages are silently dropped.

Rationale: Glitch's P2P model has no platform-level moderation to retrofit later. Building a lightweight opt-out now is much cheaper than adding one after abuse patterns emerge. Block-list + rate-limits handle spam; privacy toggles handle "I don't want to be touched" in a low-friction way.

### 3. Trigger UX — bottom palette (`EmotePalette.svelte`)

Toggle key **E** opens/closes a bottom-anchored `<dialog>`-based palette listing all six emotes (Hi + five new). Number keys 1-6 fire when open. Clicking a button fires and auto-closes.

Reuses the established `<dialog>` pattern from `SkillsPanel.svelte` and `DialoguePanel.svelte` — Escape-to-close, focus trap, arrow-key navigation.

Rejected alternatives:
- **Radial wheel** — aesthetically nicer but held-key + aim state-machine is non-trivial for a five-emote vocabulary. Revisit when the vocabulary exceeds ~8 emotes.
- **Direct per-emote hotkeys** — bombs on discoverability and crowds out other planned features (inventory, chat).

### 4. Data model — tagged enum `EmoteKind`

```rust
pub enum EmoteKind {
    Hi(HiVariant),
    Dance,
    Wave,
    Hug,
    HighFive,
    Applaud,
}

pub struct EmoteMessage {
    pub kind: EmoteKind,
    pub target: Option<[u8; 16]>,
}
```

Wire format: `{"kind":{"hi":"bats"},"target":null}` or `{"kind":"hug","target":"ab..."}`.

Rejected alternatives:
- **Flat struct with `variant: Option<HiVariant>`** — permits illegal states like `kind=Wave, variant=Stars`.
- **Separate `NetMessage` arm per emote** — over-normalization; 5+ new wire variants for no type-safety benefit.

The current `EmoteMessage { emote_type: EmoteType::Hi, variant: HiVariant, target }` becomes the above. `EmoteType` and the bare `variant` field are deleted.

### 5. Rate-limiting — tiered cooldowns with palette grey-out

Two layers:

- **Global cooldown**: 2s between any two emote fires (anti-mash).
- **Per-pair-per-emote cooldown** (targeted emotes only):
  - Hug: 60s between two hugs with the same target.
  - High-five: 30s between two high-fives with the same target.
  - Wave/applaud: no per-pair cooldown (too casual to bother).

When a palette button is cooldown-locked, it greys out and shows a countdown (`"42s"` in a tooltip/badge). Same pattern as the "needs-target" dim.

**Critical — receiver-side mirror:** Every cooldown check on the sender also runs on the receiver. A malicious client that strips the sender grey-out still gets its over-limit messages dropped by the recipient. This is a recurring P2P discipline: every client-side validation also exists on the receive path.

### 6. Mood pipeline — per-emote deltas

| Emote | Sender | Target | Witness (≤300px) |
|---|---|---|---|
| Dance | +2 | — | +1 |
| Wave | +1 | +1 (if targeted) | — |
| Hug | +5 | +5 | — |
| High-five | +3 | +3 | — |
| Applaud | +1 | +3 (if targeted) | +3 (if broadcast) |

Reuses the social foundation's existing `apply_mood_delta` pipeline — no new mood plumbing. Witness mood for dance/applaud is the one new delivery path: the receive handler scans nearby remote-player frames and applies the witness delta to self if within radius.

Ratios matter more than absolutes. Hug (+5) is the "meaningful social act" anchor; wave (+1) is the throwaway greeting; others sit between. Tuning happens in playtest, not design.

### 7. Hi backward compatibility

Hi preserves all its current semantics:
- 11 cosmetic variants (`HiVariant`)
- Once-per-day-per-target send limit
- Viral variant catching (adopt sender's variant on receive)
- Mood deltas 0 / 5 / 10 (blocked / non-match / match)

Only change is that Hi is now one `EmoteKind::Hi(HiVariant)` variant among six and fires through the unified `emote()` IPC. `emote_hi` IPC remains as a thin wrapper so the **H** key shortcut keeps working.

### 8. Privacy toggle storage — in-memory for v1

Per-emote privacy flags (`accept_hug`, `accept_high_five`) live on `EmoteState`. No persistence across restarts. Exposed via two new IPC commands:

- `set_emote_privacy(kind: EmoteKind, accept: bool)`
- `get_emote_privacy() -> { hug: bool, high_five: bool }`

Persistence follow-up if playtest shows players routinely re-toggling after restart.

### 9. Animation — emoji, float-above-head, synchronous for targeted pairs

Reuses `EmoteAnimation.svelte` with an extended emoji map:

```
dance=💃   wave=👋   hug=🤗   high-five=🖐️   applaud=👏
```

Targeted emotes play the same emoji above *both* sender and receiver simultaneously — no inter-avatar choreography in v1 (that's art-pipeline work). The existing 2s float-up-and-fade `@keyframes emote-float` animation applies uniformly.

## Architecture

### Modules touched

| File | Role | Type of change |
|---|---|---|
| `src-tauri/src/emote/types.rs` | Wire types + `EmoteState` | Replace `EmoteType` with `EmoteKind`; add cooldown + privacy state |
| `src-tauri/src/emote/mod.rs` | Re-exports | Export new types |
| `src-tauri/src/lib.rs` | IPC + network receive | New unified `emote()` IPC; extend `EmoteReceived` handler with receiver-side cooldown/privacy/witness-radius logic; wire `emote_hi` as a wrapper |
| `src-tauri/src/network/types.rs` | `NetMessage` | No change — `NetMessage::Emote(EmoteMessage)` already in place |
| `src/lib/types.ts` | Mirror wire types | Add `EmoteKind` discriminated union |
| `src/lib/ipc.ts` | IPC bindings | `emote()`, `setEmotePrivacy()`, `getEmotePrivacy()` |
| `src/lib/components/EmotePalette.svelte` | NEW — palette UI | Bottom-anchored `<dialog>`, per-emote state, countdown ticks |
| `src/lib/components/EmoteAnimation.svelte` | Extend emoji map | Add 5 new kinds |
| `src/App.svelte` | Wire E-key | Palette toggle; keep H-key as Hi shortcut |

### Data flow

**Send path:**
```
E-key → EmotePalette opens
→ number key / click → ipc.emote(kind, target?)
  (Rust) emote_cmd:
    → check global cooldown
    → check per-pair cooldown (targeted only)
    → check target-in-range (hug / high-five)
    → apply sender mood delta
    → record cooldowns
    → net_state.publish_emote(EmoteMessage { kind, target })
    → emit `emote_sent` event
→ (frontend) EmoteAnimation floats over sender avatar
```

**Receive path:**
```
NetMessage::Emote received → NetworkAction::EmoteReceived
→ emote_receive handler:
    → drop if sender blocked
    → drop if targeted & not at us
    → drop if receiver cooldown would violate (mirror of sender)
    → drop if privacy toggle off for this kind
    → route by kind:
        Hi → existing handle_incoming_hi
        Dance → witness-radius scan + mood
        Wave → target mood
        Hug/HighFive → target mood
        Applaud → target/witness mood
    → emit `emote_received` event
→ (frontend) EmoteAnimation floats over remote avatar
```

### Error handling

- **Cooldown violated (sender)**: IPC returns a structured `EmoteFireResult::Cooldown { remaining_ms }` — palette uses it to drive the countdown display. Not a thrown error.
- **No target in range**: IPC returns `EmoteFireResult::NoTargetInRange`. Palette leaves the button dim, shows tooltip.
- **Privacy off (recipient side)**: Silent drop on recipient. Sender gets no feedback (intentional — don't leak "I've blocked your hugs" signal).
- **Malformed `EmoteMessage` on wire**: serde failure → dropped at `NetMessage` deserialization layer, before reaching the handler.

## Testing strategy

### Rust unit tests (`src-tauri/src/emote/types.rs`)

- Tagged enum serde round-trip for every `EmoteKind` variant (including Hi-with-variant).
- MTU bound: longest serialized `EmoteMessage` < 500 bytes.
- Cooldown logic: global cooldown blocks second fire within 2s; per-pair cooldown is per-target-per-kind independent.
- Privacy toggle: `accept_hug=false` drops incoming hug, lets high-five through.
- `EmoteKindTag` discriminant collapses all Hi variants to one entry.

### Rust integration tests (`src-tauri/src/network/state.rs`)

Two-peer fixture extending existing patterns:

- A hugs B → both get +5 mood → A's second hug within 60s is cooldown-blocked → after 60s simulated, third hug succeeds.
- A has `accept_hug=false` → B hugs A → A drops the message, A's mood unchanged, B's mood unchanged (no feedback to B).
- A dances at center of street → B within 300px gets +1 witness mood → C at 500px gets nothing.
- Malicious-client scenario: synthetic over-limit `NetMessage::Emote` arrives out-of-cooldown; receiver drops.

### Frontend tests (`src/lib/components/EmotePalette.test.ts`)

- All 6 emote buttons render.
- Number keys 1-6 fire correct IPC calls.
- Disabled-state rendering matches `disabled` prop (dim class + tooltip text).
- Countdown tick via `$effect` + fake timer updates visible text every 1s.

## Non-goals (explicitly out of scope)

- **Persistent privacy settings** — in-memory only for v1. Follow-up issue if playtest shows need.
- **Inter-avatar animation choreography** (leaning toward each other, contact poses) — requires sprite-animation pipeline that doesn't exist yet.
- **Explicit target-picker UI** — nearest-in-range matches existing Hi behavior; explicit picker is a v2 add.
- **Chat slash-command triggers** (`/hug alice`) — tracked as [ZEB-130](https://linear.app/zeblith/issue/ZEB-130/glitch-chat-slash-command-system-parser-handler-registry). Requires a slash-command parser that doesn't exist yet.
- **Animated sprite assets** — emoji for v1.
- **Rate-limit tuning via config** — constants inline for v1; extract to config if playtest demands.

## Risk notes

- **Mood-economy farm vectors — the tightest concern.** Several scenarios produce mood-rate well above Hi's ~50/day baseline:
  1. **Self-dance loop**: dance fires at global-cooldown-cap (one per 2s) yield +2 sender mood each → **+60/min self-pump**.
  2. **Paired-witness loop**: one player dances while another stands within 300px → witness gets +1/2s → **+30/min passive**.
  3. **Hug loop**: two players hug each other at per-pair cap (one per 60s) → +5 each → **+10/min combined**. Least concerning of the three.
  4. **Applaud broadcast**: no per-pair cooldown; can target-applaud same player every 2s for +3 each → **+90/min passive to target**.

  **Architectural mitigation available but not yet spec'd as required:** the per-pair cooldown hashmap (`(sender, EmoteKindTag) → Instant`) can be reused for a *mood-reward cooldown* distinct from the *fire cooldown*. The fire is always permitted (subject to its own cooldown); the mood delta applies only once per N seconds per pair.
  
  Suggested follow-up during implementation: add a `reward_cooldown` parallel to `fire_cooldown`, with longer windows (~5-10 min) for dance/applaud. Alternative is to tune the base mood deltas way down (+0.5 or +0.2) so farm-rate is bounded by mood-cap regen. Which approach to take is a **decision for the plan-writing phase** — noting here so the plan author can choose with eyes open rather than shipping raw numbers.
- **Receiver-side cooldown divergence:** sender and receiver must use the same clock semantics (both use `Instant::now()` at message handling). Clock skew is bounded by message latency, which is seconds — well below the 60s hug cooldown.
- **Palette key collision:** **E** is currently unbound in the global key layer — confirmed by reading `App.svelte`. Flag for implementer: verify no conflict at implementation time, and if found, fall back to **`** (backtick) or **T**.
