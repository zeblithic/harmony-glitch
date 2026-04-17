# Chat Slash-Command System — Design Spec

**Linear:** [ZEB-130](https://linear.app/zeblith/issue/ZEB-130)
**Date:** 2026-04-17
**Status:** Approved, ready for implementation plan

## Goal

Intercept chat input that starts with `/` and route it to local handlers instead of broadcasting as plain text. Ships ten commands in v1 that cover emote triggers, lightweight moderation, and action-style chat:

```
/hi  /dance  /wave [name]  /hug [name]  /high5 [name]  /applaud
/block <name>  /unblock <name>
/me <action>
/help
```

The current behavior — typing `/dance` broadcasts the literal string `"/dance"` to peers — is a dead end. This spec replaces it with a frontend-only parser + handler registry that reuses existing IPC (`emote`, `emoteHi`, `blockPlayer`, `unblockPlayer`, `sendChat`) and existing render infrastructure (`renderer.addChatBubble`).

## Architecture

**Frontend-only.** Zero Rust or network-schema changes. The slash-command layer is a sender-side affordance on top of the already-shipped chat and emote IPCs.

**Three new TypeScript files:**

```
src/lib/chat/commands.ts       — parser, executor, types (no handlers, no IPC)
src/lib/chat/handlers.ts       — default command handlers + registerDefaultCommands()
src/lib/chat/commands.test.ts  — parser, executor, handler unit tests
```

**Touched:**

- `src/lib/components/ChatInput.svelte` — gains an `onCommand` prop and routes slash input through it before falling through to `sendChat`.
- `src/App.svelte` — builds a fresh `CommandContext` at dispatch time, wires the `pushLocalBubble` adapter to the existing renderer, extracts a shared `fireEmoteWithFeedback(kind, targetHash, ctx)` helper used by palette, hotkeys, and command handlers.

**No existing IPC signatures change.** `sendChat`, `emote`, `emoteHi`, `blockPlayer`, `unblockPlayer`, `getBlockedList`, `getBuddyList` are reused unchanged.

### Dispatch flow

```
ChatInput.handleSubmit(text)
  │
  ├─ parseCommand(text) → null           → sendChat(text)                    (plain chat, unchanged)
  ├─ parseCommand(text) → { literal }    → sendChat(literal.text)            (//foo → "/foo")
  └─ parseCommand(text) → { command }    → onCommand(parsed)
                                              │
                                              └─ App.svelte: executeCommand(parsed, buildContext())
                                                    │
                                                    ├─ registry lookup (case-insensitive)
                                                    ├─ hit  → handler(args, ctx)
                                                    └─ miss → ctx.pushLocalBubble("Unknown command: /{cmd}. Type /help for the list.")
```

## Parser

```ts
type ParsedCommand =
  | { kind: 'command'; cmd: string; args: string; raw: string }
  | { kind: 'literal'; text: string };

function parseCommand(input: string): ParsedCommand | null;
```

**Grammar:**

- Trim input once. Empty → `null`.
- Doesn't start with `/` → `null` (plain chat).
- Starts with `//` → `{ kind: 'literal', text: input.slice(1) }` (IRC-convention escape for a message that really starts with a slash).
- Otherwise: split on the first run of whitespace.
  - `cmd` = chars after `/` up to first whitespace, `.toLowerCase()`.
  - `args` = rest of the line with leading whitespace trimmed; intra-arg whitespace preserved (so `/me waves  hello` keeps the double space).
  - Bare `/` → `{ kind: 'command', cmd: '', args: '', raw: '/' }` — executor treats this as unknown-command.

**Out of scope for the parser:** quoted arguments, flags, fuzzy matching. Handlers that need multi-word args receive the whole rest of the line and split it themselves.

## Handler contract

```ts
type CommandContext = {
  // Read-only state snapshots, built fresh at dispatch time
  remotePlayers: RemotePlayerFrame[];
  nearestSocialTarget: NearestSocialTarget | null;
  buddies: BuddyEntry[];
  localIdentity: PlayerIdentity;

  // Side-effect adapters
  pushLocalBubble: (text: string) => void;
  fireEmote: (kind: EmoteKind, targetHash: string | null) => Promise<void>;
  fireEmoteHi: () => Promise<void>;
  sendChat: (text: string) => Promise<void>;
  blockPlayer: (peerHash: string) => Promise<void>;
  unblockPlayer: (peerHash: string) => Promise<void>;
  getBlockedList: () => Promise<BlockedEntry[]>;  // lazy — /unblock only
};

type CommandHandler = (args: string, ctx: CommandContext) => Promise<void>;
```

**Discipline:**

- Handlers own their full lifecycle: arg validation, name resolution, feedback on error, IPC call.
- Handlers never throw for user-input errors. Instead: `ctx.pushLocalBubble('...')` and return.
- Unexpected errors (IPC rejection, etc.) are allowed to bubble up to the executor, which catches and surfaces `"Command failed: {message}"`.

**`pushLocalBubble` implementation.** App.svelte wires this to the existing renderer call `renderer.addChatBubble(localIdentity.addressHash, text)`. Bubble floats above the local player's avatar, decays on the existing 8-second schedule, zero new renderer code.

**Why dependencies are injected at dispatch time, not registration time.** Handlers stay pure: they receive everything they need by argument and return no side-effects of their own. Tests construct a mock `CommandContext` with stub adapters — no Tauri, no renderer, no mounted Svelte tree required.

### Name resolution helper

Lives in `handlers.ts`, reused by every target-taking handler:

```ts
function resolvePlayerName(
  name: string,
  sources: {
    remotePlayers?: RemotePlayerFrame[];
    buddies?: BuddyEntry[];
    blocked?: BlockedEntry[];
  },
): { hash: string; displayName: string } | null;
```

Case-insensitive exact match on `displayName`. Lookup order: `remotePlayers` → `buddies` → `blocked`, depending on which sources the caller passed. First match wins. v1 assumes display names are unique in the player population; name-uniqueness enforcement is a future concern tracked as a separate namespace-registration effort.

### Shared emote firing helper

Extracted from `App.svelte`'s current `handleEmoteSelect` / `fireHiWithAnimation` logic:

```ts
async function fireEmoteWithFeedback(
  kind: EmoteKind,
  targetHash: string | null,
  ctx: { pushFeedback: (text: string) => void },
): Promise<void>;
```

Switches on `EmoteFireResult` (`success` / `cooldown` / `no_target` / `target_blocked`) and surfaces the same user-facing feedback paths the palette uses today. Palette, hotkeys (H-key, etc.), SocialPrompt, and command handlers all call this single helper — one place to maintain the result switch.

**Note on feedback surface.** Palette/hotkey callers pass `pushFeedback = pushEmoteFeedback` (GameNotification toast). Command handlers pass `pushFeedback = ctx.pushLocalBubble` (chat bubble). Same switch, different surface — chat-initiated emotes surface errors as chat bubbles; palette-initiated emotes stay on the toast pipeline users are already accustomed to.

## Commands

Resolution rule for target-taking emotes: **explicit name > `nearestSocialTarget` > command-specific fallback.** Emote handlers only resolve against `remotePlayers` — the backend's range check requires the target to be on-street.

### `/hi`

- Args: none (extras silently ignored).
- Behavior: `ctx.fireEmoteHi()`. Hi carries its own daily variant; no target plumbing here.

### `/dance`

- Args: none.
- Behavior: `ctx.fireEmote('dance', null)` — always broadcast. Rust receive-side already coerces Dance-with-target to broadcast, but we keep the sender-side clean too.

### `/applaud`

- Args: none.
- Behavior: `ctx.fireEmote('applaud', null)` — broadcast-only in v1, matches palette. Targeted applaud (nearby-witness path) is a future option if users ask.

### `/wave [name]`

- Args: optional.
- Resolution: if `name` present, `resolvePlayerName(name, { remotePlayers })`; else `nearestSocialTarget?.addressHash ?? null`.
- Failure cases:
  - Name given but not found → bubble `"No player named {name} nearby."`
  - Name resolves to self → bubble `"Can't wave at yourself."`
- Behavior: `ctx.fireEmote('wave', hashOrNull)`. Wave is dual-mode per the emote spec — `null` broadcasts, hash targets.

### `/hug [name]` / `/high5 [name]` (alias `/highfive`)

- Args: optional (name or falls back to `nearestSocialTarget`).
- Resolution: same as `/wave`.
- Failure cases:
  - No name and no `nearestSocialTarget` → bubble `"/hug needs a target nearby."` (or `"/high5 …"`).
  - Name given but not found → bubble `"No player named {name} nearby."`
  - Resolves to self → bubble `"Can't hug yourself."` / `"Can't high-five yourself."`
- Behavior: `ctx.fireEmote('hug' | 'high_five', hash)`. Out-of-range / cooldown / target-blocked feedback comes from `fireEmoteWithFeedback` as chat bubbles.

### `/block <name>`

- Args: required.
- Resolution: `resolvePlayerName(name, { remotePlayers, buddies })`.
- Failure cases:
  - Empty args → bubble `"Usage: /block <name>"`.
  - Name not found → bubble `"No player named {name}."`
  - Resolves to self → bubble `"Can't block yourself."`
- Behavior: `await ctx.blockPlayer(hash)`; on resolve, bubble `"Blocked {displayName}."`

### `/unblock <name>`

- Args: required.
- Resolution: lazy `ctx.getBlockedList()` on each invocation; `resolvePlayerName(name, { blocked })`. No caching — blocklist lookups are local and infrequent.
- Failure cases:
  - Empty args → bubble `"Usage: /unblock <name>"`.
  - Name not found in blocked list → bubble `"No blocked player named {name}."`
- Behavior: `await ctx.unblockPlayer(hash)`; on resolve, bubble `"Unblocked {displayName}."`

### `/me <action>`

- Args: required.
- Failure cases:
  - Empty args → bubble `"Usage: /me <action>"`.
- Behavior: format as `"* {ctx.localIdentity.displayName} {args} *"` and `ctx.sendChat(formatted)`. Rust's existing 200-byte truncation applies silently. Receivers see a normal chat bubble with the starred text. v1 does not extend `ChatMessage` with an action flag — receiver-side styling is a future option.

### `/help`

- Args: none.
- Behavior: emits four local bubbles ~80ms apart so they stack legibly and age together:

  ```
  * Commands:
  * /hi /dance /wave /hug /high5 /applaud
  * /block <name> /unblock <name>
  * /me <action>      /help
  ```

### Unknown command

- Not a handler — the executor itself detects a missing registry entry and bubbles `"Unknown command: /{cmd}. Type /help for the list."`.

### Literal `//text`

- Parser emits `{ kind: 'literal', text: '/text' }`.
- `ChatInput` treats this as `sendChat('/text')` — broadcasts a message whose first character is a literal slash.

## Error UX

- All user-input errors produce **exactly one local bubble** above the local player. Never a broadcast.
- All IPC-rejection errors produce **one local bubble** with `"Command failed: {error.message}"`.
- No GameNotification toasts from command handlers — chat-initiated actions stay in the chat medium. Palette-initiated emotes continue to use toasts (unchanged).
- Self-target guard (`"Can't hug yourself."` etc.) is applied on every command whose resolver could legitimately return the local player's hash (`/wave`, `/hug`, `/high5`, `/block`). `/unblock` doesn't need it — self can't be in the blocked list, so the not-found bubble covers the case.
- 200-byte chat truncation is silent (existing plain-chat behavior).

## Testing strategy

**`commands.test.ts` — parser unit tests (one per grammar rule):**

- Empty / whitespace-only input → `null`.
- Plain text → `null`.
- `//foo` → `{ kind: 'literal', text: '/foo' }`.
- `/dance` → `{ cmd: 'dance', args: '', ... }`.
- `/Dance` → `cmd === 'dance'` (case-insensitive).
- `/me waves  hello` → `args === 'waves  hello'` (intra-arg whitespace preserved).
- `/hug   alice` → `args === 'alice'` (leading whitespace stripped).
- `/` (bare) → `{ cmd: '', args: '', raw: '/' }`.
- `/highfive` alias maps to high-five handler.

**`commands.test.ts` — executor tests:**

- Unknown `cmd` → `pushLocalBubble` called with the unknown-command message, no handler invoked.
- Handler throws → executor catches and bubbles `"Command failed: …"`.

**`handlers.test.ts` — per-handler tests** (mock `CommandContext` with stub adapters):

- Each command: happy path + nearest fallback (where applicable) + empty-args error + name-not-found error + self-target guard (where applicable).
- `/me` formats with local display name correctly.
- `/help` emits four bubbles.
- `/unblock` calls `getBlockedList()` lazily (once per invocation).

**`ChatInput.test.ts`:**

- Plain text → `sendChat` called, `onCommand` not called.
- Slash text → `onCommand` called with parsed command, `sendChat` not called.
- Literal `//foo` → `sendChat('/foo')`.

**No new Rust tests.** The feature adds zero Rust surface.

**Manual acceptance checks:**

1. `/dance` → local dance fires, peers see it, palette cooldown updates.
2. `/hug alice` with Alice on-street → hug fires with the animation.
3. `/hug bob` with no Bob nearby → bubble `"No player named bob nearby."`.
4. `/hug` with no name and no nearest → bubble `"/hug needs a target nearby."`.
5. `/me waves` → peers see `"* {myName} waves *"`, local echo matches.
6. `/block alice` then `/unblock alice` → both surface success bubbles, buddy/blocklist state consistent.
7. `/help` → four bubbles stack above player.
8. `//dance` → peers see literal `"/dance"` chat text (no emote fires).
9. `/xyzzy` → unknown-command bubble.

## Non-goals (v1)

- Autocomplete / tab-completion / suggestion popup.
- Command history (up-arrow recall).
- Persistent chat scrollback.
- Fuzzy or partial-prefix name matching.
- Targeted `/applaud <name>` — v1 broadcast-only.
- Additional commands (`/roll`, `/shout`, `/whisper`, `/w`, `/dm`, `/join`, etc.).
- Server-side command validation or enforcement.
- Receiver-side `/me` styling (italic, color) — plain text in v1.
- Name uniqueness enforcement — assumed unique for v1; namespace registration is a future effort.

## Scope & risk

**Size:** ~3 new TS files (~400 LOC including tests), ~30 LOC touched in `ChatInput.svelte` and `App.svelte`. One medium PR.

**Risk:** Low. No Rust changes, no network-schema changes, no protocol versioning concerns. All touched IPC is already exercised by the emote, buddy, and chat subsystems. Failure modes are confined to the sender's UI surface.

**Dependencies:** None — emote palette (ZEB-76) and chat foundation are shipped on main.

**Unlocks:** Chat-driven emote triggers (power-user path without opening the palette), moderation affordances without dedicated UI, and a reusable parser/registry for future expressive commands (`/roll`, `/shout`, etc.).
