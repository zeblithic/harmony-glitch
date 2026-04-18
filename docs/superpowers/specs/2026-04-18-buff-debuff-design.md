# Buff/Debuff System Design (ZEB-80) — v1

**Linear:** [ZEB-80](https://linear.app/zeblith/issue/ZEB-80/glitch-buffdebuff-system)
**Status:** Design approved, ready for implementation plan
**Scope:** v1 — engine + rookswort as first real item

## Goal

Add a data-driven buff/debuff engine that temporarily modifies mood decay rate, scoped to ship rookswort end-to-end. Generalizes the existing hardcoded `party_bonus: bool` parameter in `MoodState::tick` into a composable `decay_modifier: f64` sourced from all active buffs plus party state. Establishes the abstraction shape for future effect types (energy, social, environmental) without implementing them.

## Non-Goals (v1)

- Additional buff effect variants beyond `MoodDecayMultiplier` (energy buffs, periodic mood-tick, stat multipliers)
- Transferable social buffs (`feeling_called_love`) — requires P2P wiring, belongs in a later issue
- Tier-upgrade / substitute-buff machinery (simulates "stacking") — the engine will *support* this pattern via `on_expire` and stable `kind` keys, but no tiered rookswort variants ship in v1
- Buffifying `party_bonus` itself — stays as a boolean-derived factor in composition; a later pass can convert it when other social buffs land alongside
- Tooltips, animations, or rich HUD treatment for buff icons
- Cross-street buff synchronization or multiplayer buff propagation

## Architectural Decisions

Resolved during brainstorming — captured here so implementation stays on-track.

| Decision | Choice | Rationale |
|---|---|---|
| Scope | Engine + one real item (rookswort) | Validates whole pipeline end-to-end; avoids speculative abstraction |
| Stacking for same-kind buffs | Refresh semantics (overwrite in place) | Simplest data model; original-Glitch precedent; future "stacking" simulated via tier-upgrade pattern in content layer |
| `on_expire: Option<Box<BuffSpec>>` | Include in v1 | Tiny engine cost (~10 lines); enables graceful ramp-down chains and content-driven state machines without further engine changes |
| Buff definition location | Item catalog JSON (`ItemDef.buff_effect`) | Fits existing item-catalog pattern; new stimulants cost zero code changes |
| Effect types for v1 | Only `MoodDecayMultiplier(f64)` | Rookswort needs only this; new variants are pure enum additions |
| `party_bonus` handling | Converted to a factor in composition, not buffified | Minimum diff; party code untouched; buffification can land later alongside transferable social buffs |
| Composition rule | Multiplicative | Bounded (can't go negative); matches Glitch precedent; each buff reads as "multiply decay rate by this" |
| Composition site | `SocialState::tick` | Party state and buff state both live there; `mood.tick` becomes agnostic |

## File Structure

```text
src-tauri/src/buff/          [new module]
├── mod.rs                   BuffState + apply/tick/mood_decay_multiplier
└── types.rs                 ActiveBuff, BuffSpec, BuffEffect enum

src-tauri/src/item/types.rs  [extend]  +buff_effect: Option<BuffSpec> on ItemDef
src-tauri/src/item/interaction.rs [extend]  apply buff in use_item when buff_effect present
src-tauri/src/mood/mod.rs    [modify]  tick signature: party_bonus: bool → decay_modifier: f64
src-tauri/src/social/mod.rs  [modify]  add buffs: BuffState; compose decay_modifier in tick
src-tauri/src/lib.rs         [modify]  expose active_buffs on game-state frame IPC

assets/items.json            [modify]  add buffEffect field to the rookswort entry

src/lib/components/BuffHud.svelte [new]  render active-buffs row in HUD
src/lib/types.ts             [modify]  add BuffFrame + activeBuffs: BuffFrame[] on RenderFrame
```

## Data Model

### `BuffEffect` — tagged enum for effect kinds

```rust
// src-tauri/src/buff/types.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BuffEffect {
    /// Multiplies mood decay rate. 0.5 = half-rate. 0.0 = halts decay.
    /// Values > 1.0 accelerate decay (debuff). Negative values are clamped
    /// to 0.0 by the mood tick to prevent effective mood *gain*.
    MoodDecayMultiplier { value: f64 },
}
```

**JSON shape:** `{ "type": "moodDecayMultiplier", "value": 0.5 }`.

Future variants extend this enum. Unrelated variants are simply ignored by `mood_decay_multiplier()`'s filter_map — no refactor needed when adding e.g. `EnergyDecayMultiplier`.

### `BuffSpec` — static template from JSON

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BuffSpec {
    pub kind: String,                     // stable family key, e.g. "rookswort"
    pub effect: BuffEffect,
    pub duration_secs: f64,
    #[serde(default)]
    pub on_expire: Option<Box<BuffSpec>>, // optional successor on expiry
}
```

The `kind` field is the stable family key used for refresh semantics. All rookswort tiers (if added later) share `kind: "rookswort"` so a new application overwrites the current one in place. Tier-specific differences live in `effect` and `duration_secs`.

`on_expire` supports arbitrary-depth chains via `Box<BuffSpec>`. v1 ships no chained buffs, but the type signature permits content authors to define them in JSON whenever needed.

### `ActiveBuff` — resolved runtime instance

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveBuff {
    pub kind: String,
    pub effect: BuffEffect,
    pub expires_at: f64,                  // absolute game_time
    pub source: String,                   // item id or system identifier, for debugging
    pub on_expire: Option<Box<BuffSpec>>,
}
```

Built from a `BuffSpec` at application time. `expires_at = game_time + spec.duration_secs`.

### `BuffState` — the per-player buff container

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuffState {
    /// Keyed by kind. Refresh semantics: same-kind apply overwrites.
    pub active: HashMap<String, ActiveBuff>,
}
```

Lives on `SocialState` alongside `mood`, `emotes`, `buddies`, `party`.

### `ItemDef` extension

```rust
// src-tauri/src/item/types.rs — add one field
pub struct ItemDef {
    // ... existing fields unchanged ...
    #[serde(default)]
    pub buff_effect: Option<BuffSpec>,
}
```

### Rookswort JSON

```json
{
  "name": "Rookswort",
  "description": "Slows mood loss for 10 minutes.",
  "category": "consumable",
  "stackLimit": 10,
  "icon": "rookswort",
  "buffEffect": {
    "kind": "rookswort",
    "effect": { "type": "moodDecayMultiplier", "value": 0.5 },
    "durationSecs": 600
  }
}
```

The implementation plan verifies whether rookswort already has a catalog entry and creates one if missing. The shape above is authoritative regardless.

## Runtime Flow

### Apply (on `use_item`)

```rust
// In the use_item handler, after item-exists / stack-available validation
if let Some(spec) = &item_def.buff_effect {
    self.social.buffs.apply(spec, game_time, item_id.to_string());
    // then consume the item stack as usual
}
```

`BuffState::apply`:

```rust
pub fn apply(&mut self, spec: &BuffSpec, game_time: f64, source: String) {
    let active = ActiveBuff {
        kind: spec.kind.clone(),
        effect: spec.effect.clone(),
        expires_at: game_time + spec.duration_secs,
        source,
        on_expire: spec.on_expire.clone(),
    };
    self.active.insert(spec.kind.clone(), active);
}
```

Same-kind insert overwrites the existing entry — this is the refresh semantics guarantee.

### Tick

`SocialState::tick` is amended to:

```rust
pub fn tick(&mut self, dt: f64, ctx: &SocialTickContext) {
    self.emotes.check_date_change(ctx.current_date);

    // Expire buffs and fire on_expire chains before mood tick,
    // so the modifier reflects the current frame's active set.
    self.buffs.tick(ctx.game_time);

    // Compose all decay-rate sources multiplicatively.
    // Identity is 1.0 (full normal decay); each factor is a fraction <= 1.0.
    let party_factor = if self.party.has_party_bonus() { 0.75 } else { 1.0 };
    let buff_factor = self.buffs.mood_decay_multiplier();
    let decay_modifier = party_factor * buff_factor;

    self.mood.tick(dt, ctx.game_time, ctx.in_dialogue, decay_modifier);

    // Remaining expirations unchanged
    self.buddies.expire_requests(ctx.game_time);
    self.buddies.expire_outgoing_requests(ctx.game_time);
    self.party.expire_invite(ctx.game_time);
    self.party.expire_outgoing_invites(ctx.game_time);
    self.party.expire_pending_join(ctx.game_time);
}
```

`BuffState::tick(game_time: f64)`:

1. Collect all kinds whose `expires_at <= game_time` into a `Vec<String>` sorted by kind name (for deterministic test behavior).
2. For each expired kind: `remove` from the map, then if its `on_expire` was `Some(spec)`, immediately call `apply(&spec, game_time, source = "on_expire")`.
3. Bound the expansion pass count (e.g., 8 iterations) in case a degenerate `on_expire` chain produces already-expired successors. This is a defensive guard for bad content data, not expected normal behavior.

`BuffState::mood_decay_multiplier()`:

```rust
pub fn mood_decay_multiplier(&self) -> f64 {
    self.active
        .values()
        .filter_map(|b| match b.effect {
            BuffEffect::MoodDecayMultiplier { value } => Some(value),
        })
        .fold(1.0, |acc, v| acc * v)
}
```

Returns `1.0` when no relevant buffs are active (neutral). Future `BuffEffect` variants are invisible to this function — they neither contribute nor disrupt.

### Mood signature change

```rust
// Before
pub fn tick(&mut self, dt: f64, game_time: f64, in_dialogue: bool, party_bonus: bool) {
    // ... guards ...
    let effective_dt = if party_bonus { dt * 0.75 } else { dt };
    // ...
}

// After
pub fn tick(&mut self, dt: f64, game_time: f64, in_dialogue: bool, decay_modifier: f64) {
    // ... existing guards unchanged ...
    let safe_modifier = decay_modifier.max(0.0);
    let effective_dt = dt * safe_modifier;
    self.mood = decay::mood_decay(self.mood, self.max_mood, effective_dt);
}
```

The lower clamp prevents negative effective_dt (which would *increase* mood, nonsense). Values > 1.0 are permitted — this is intentional so future debuff content (e.g., a cursed item that accelerates mood decay) works without re-opening the signature. v1 ships only reduction buffs, so values will always be ≤ 1.0 in practice.

## Persistence

`BuffState` derives `Serialize`/`Deserialize` and is included in the game save struct. In-memory, each `ActiveBuff.expires_at` is absolute against `GameState.game_time`. However, `game_time` itself is **not** persisted — a fresh `GameState` starts at `game_time = 0.0` every session — so storing absolute `expires_at` values verbatim would make a restored buff live for its full original wall-clock lifetime from `t = 0` after app restart.

**Save boundary transform.** `BuffState` provides two helpers that shift `expires_at` across the save/load boundary:

- `to_save_form(current_time)`: returns a clone with `expires_at` rebased to **remaining seconds** (`expires_at - current_time`). Called by `GameState::save_state` with `self.game_time` so the on-disk shape represents durations, not clock values.
- `from_save_form(save, current_time)`: inverse — adds `current_time` to each `expires_at`, converting remaining-seconds back to absolute against the live session's clock. Called by `GameState::restore_save` with the restored session's `self.game_time`.

**On load:** After `from_save_form`, each buff's `expires_at` represents "the session clock value at which this buff expires." Subsequent `tick()` calls expire buffs whose `expires_at <= game_time` exactly as they would have during the original session, and `on_expire` chains fire normally when a buff crosses the expiry boundary. The transform is size-preserving and deterministic, so save/reload during an active buff preserves its remaining duration to the nanosecond.

**Edge case — legacy saves:** If a save file predates the buff system (no `buffs` field), `#[serde(default)]` on `SaveState.buffs` gives an empty `BuffState`. No migration needed.

**Edge case — session time jumps.** `game_time` accumulates during play; wall-clock time between sessions does not. A player who saves mid-buff and resumes two weeks later will find their buff still has the same remaining duration — matching the "game time, not wall time" contract used elsewhere in the engine.

## HUD

### IPC extension

The game-state frame sent to the frontend gains:

```typescript
interface GameStateFrame {
  // ... existing fields ...
  activeBuffs: BuffFrame[];
}

interface BuffFrame {
  kind: string;              // stable id for React keys
  icon: string;              // sprite id — v1 reuses the source item's icon
  label: string;             // display name for accessibility; "Rookswort"
  remainingSecs: number;     // clamped to >= 0
}
```

### Rust side

A small helper in `lib.rs` (or wherever the frame is assembled) walks `social.buffs.active` and builds `BuffFrame`s. The `label` and `icon` come from looking up the buff's `source` (item id) in the item catalog when the source is an item. For system-sourced buffs (none in v1), a fallback display name can be used.

### Frontend component

`src/lib/components/BuffHud.svelte`:

- Horizontal row of icons positioned near the existing mood bar in the top-left HUD cluster (exact placement follows current HUD layout conventions — no new design system needed)
- Each icon: item sprite from the ZEB-131 atlas at a reasonable size (32px target; match existing HUD icons)
- Small text label below each icon: `mm:ss` when remaining > 60s, `Ns` otherwise
- No animations, no tooltips, no expire-flash in v1

Visuals are intentionally minimal. Polish belongs in a follow-up.

## Testing Strategy

### `src-tauri/src/buff/types.rs`

- `buff_spec_roundtrips_json`
- `buff_spec_with_on_expire_chain_roundtrips_json`
- `buff_effect_tagged_serialization_shape` — locks the JSON shape `{ "type": "moodDecayMultiplier", "value": 0.5 }` so future renames don't silently break item catalog files

### `src-tauri/src/buff/mod.rs`

- `apply_inserts_new_buff`
- `apply_same_kind_refreshes_expires_at` — core refresh-semantics test
- `apply_same_kind_replaces_effect_magnitude` — validates the tier-upgrade content pattern
- `tick_removes_expired_buff_without_on_expire`
- `tick_expired_buff_with_on_expire_applies_successor`
- `tick_chain_on_expire_same_kind_replaces_in_place` — successor uses same `kind` as parent
- `tick_chain_terminates_after_bounded_passes` — malicious circular chain guard
- `mood_decay_multiplier_returns_one_when_empty`
- `mood_decay_multiplier_composes_multiplicatively` — two buffs with 0.5 and 0.75 → 0.375
- `deterministic_expiration_order_when_multiple_expire_same_tick` — asserts chain apply order is stable

### `src-tauri/src/mood/mod.rs` — existing tests migrate

- `tick_with_decay_modifier_of_one_matches_unmodified_baseline` — replaces the "no party_bonus" default
- `tick_with_decay_modifier_of_zero_halts_decay`
- `tick_with_decay_modifier_of_0_75_reduces_by_25_percent` — renames old `tick_with_party_bonus_reduces_decay_by_25_percent`
- `tick_with_decay_modifier_above_one_accelerates_decay` — debuff forward-compat
- `tick_clamps_negative_decay_modifier_to_zero` — prevents mood-gain from bad content
- Existing NaN / invalid-input guards unchanged

### `src-tauri/src/social/mod.rs`

- `tick_composes_party_and_buff_multiplicatively` — join party + apply rookswort → mood decays at `dt × 0.75 × 0.5` rate
- `tick_with_no_buffs_and_no_party_preserves_baseline`

### Integration — state.rs tick loop or IPC boundary

- `use_rookswort_applies_buff_and_slows_decay_for_ten_minutes`
- `rookswort_buff_expires_and_decay_returns_to_baseline` — tick past `expires_at`, assert mood_decay_multiplier is 1.0 again
- `rookswort_persists_across_save_and_load` — save with buff active, reload, assert same `expires_at` and still in active map

## Risks and Open Questions

- **Rookswort catalog entry presence.** The plan must verify whether rookswort is already in the item catalog and, if so, whether its current metadata is compatible with adding `buffEffect`. If missing, the implementation creates it with the JSON above.
- **HUD layout conflict.** The existing mood/energy HUD placement is not re-examined in this design. If adding the buff row creates visual crowding, resolution is an implementation-time adjustment.
- **Save format compatibility.** Adding `buffs: BuffState` to the save struct must use `#[serde(default)]` to preserve compatibility with existing saves that lack the field. Plan should add an explicit test that loads a pre-buff save without error.
- **`on_expire` source attribution.** When an `on_expire` buff is applied, its `source` is set to `"on_expire"` in the flow above. This is fine for debugging but produces generic HUD labels for successor buffs. If any v1 chain ever surfaces in the HUD (none planned), this may need revisiting.

## Glossary

- **Buff** — a temporary modifier on a player stat. The term includes debuffs (negative modifiers) in this design; the data model is neutral.
- **Kind** — stable family key that groups all tiers/variants of a buff for refresh-semantics purposes (e.g., `"rookswort"`).
- **Effect** — the specific mechanical operation a buff performs on a stat (e.g., `MoodDecayMultiplier(0.5)`).
- **Refresh semantics** — applying a buff with an already-active same-kind entry overwrites it in place; timer resets to full, magnitude updates to the new effect.
- **Composition** — folding multiple active decay-modifier sources into a single effective `decay_modifier` value. Multiplicative in v1.
