# Chat Slash-Command System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a frontend-only slash-command layer that intercepts chat input starting with `/` and routes it to local handlers instead of broadcasting as plain text. Ships `/hi /dance /wave /hug /high5 /applaud /block /unblock /me /help`.

**Architecture:** Pure TypeScript/Svelte — zero Rust or network-schema changes. A new `src/lib/chat/` module holds a pure `parseCommand` function, an `executeCommand` dispatcher, a `resolvePlayerName` helper, and ten `CommandHandler` functions that call existing IPC (`emote`, `emoteHi`, `blockPlayer`, `unblockPlayer`, `sendChat`). `ChatInput.svelte` gains an `onCommand` prop; `App.svelte` builds a fresh `CommandContext` at dispatch time and refactors its emote-firing path into a shared `fireEmoteWithFeedback` helper so palette, hotkeys, and commands share one `EmoteFireResult` switch.

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest, Tauri IPC (existing). No new dependencies.

**Spec:** `docs/superpowers/specs/2026-04-17-chat-slash-commands-design.md` (read before starting).

---

## File Map

**Created:**
- `src/lib/chat/commands.ts` — types, `parseCommand`, `executeCommand`
- `src/lib/chat/handlers.ts` — `resolvePlayerName`, default handlers, `createDefaultHandlers()`
- `src/lib/chat/commands.test.ts` — parser + executor tests
- `src/lib/chat/handlers.test.ts` — resolver + per-handler tests
- `src/lib/components/ChatInput.test.ts` — component routing tests

**Modified:**
- `src/lib/components/ChatInput.svelte` — add `onCommand` prop, route slash input through parser
- `src/App.svelte` — extract `fireEmoteWithFeedback`, refactor `fireHiWithAnimation` to accept feedback sink, build `CommandContext`, wire `onCommand` prop

**Unchanged:** All Rust files (`src-tauri/`), all other IPC, all other components.

---

## Conventions

- **Test runner:** `npm run test` (maps to `vitest run`). For a single test file: `npm run test -- src/lib/chat/commands.test.ts`. Watch mode: `npx vitest src/lib/chat/`.
- **Frontend build check:** `npm run build` (Vite type-checks + bundles).
- **Test file env:** vitest files that render Svelte components start with `// @vitest-environment jsdom` as the first line. Pure-TS test files don't need that directive. See `src/lib/components/EmotePalette.test.ts` for a component-test example.
- **Commit style:** short imperative subject, optional body. Examples in existing history: `feat(chat): add slash-command parser`, `refactor(emote): extract fireEmoteWithFeedback helper`.
- **Svelte 5 idioms:** `$props()`, `$state()`, `$effect()`. Component props are destructured from `$props()`. See `ChatInput.svelte` for a small reference component.

---

## Task 1: Scaffold chat module — types and parser

**Files:**
- Create: `src/lib/chat/commands.ts`
- Create: `src/lib/chat/commands.test.ts`

- [ ] **Step 1: Create the test file with parser tests (RED)**

Create `src/lib/chat/commands.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { parseCommand } from './commands';

describe('parseCommand', () => {
  it('returns null for empty input', () => {
    expect(parseCommand('')).toBeNull();
    expect(parseCommand('   ')).toBeNull();
  });

  it('returns null for plain text', () => {
    expect(parseCommand('hello world')).toBeNull();
    expect(parseCommand('  hi there')).toBeNull();
  });

  it('returns literal escape for //-prefixed input', () => {
    expect(parseCommand('//dance')).toEqual({ kind: 'literal', text: '/dance' });
    expect(parseCommand('//hello world')).toEqual({ kind: 'literal', text: '/hello world' });
  });

  it('parses a bare command with no args', () => {
    expect(parseCommand('/dance')).toEqual({
      kind: 'command',
      cmd: 'dance',
      args: '',
      raw: '/dance',
    });
  });

  it('lowercases the command name', () => {
    const result = parseCommand('/Dance');
    expect(result).toEqual({ kind: 'command', cmd: 'dance', args: '', raw: '/Dance' });
  });

  it('splits command from args on first whitespace', () => {
    expect(parseCommand('/hug alice')).toEqual({
      kind: 'command',
      cmd: 'hug',
      args: 'alice',
      raw: '/hug alice',
    });
  });

  it('preserves intra-arg whitespace', () => {
    const result = parseCommand('/me waves  hello');
    expect(result).toEqual({
      kind: 'command',
      cmd: 'me',
      args: 'waves  hello',
      raw: '/me waves  hello',
    });
  });

  it('strips leading whitespace from args', () => {
    const result = parseCommand('/hug   alice');
    expect(result?.kind === 'command' && result.args).toBe('alice');
  });

  it('produces bare-command shape for just "/"', () => {
    expect(parseCommand('/')).toEqual({ kind: 'command', cmd: '', args: '', raw: '/' });
  });

  it('trims surrounding whitespace once before parsing', () => {
    expect(parseCommand('  /dance  ')).toEqual({
      kind: 'command',
      cmd: 'dance',
      args: '',
      raw: '/dance',
    });
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- src/lib/chat/commands.test.ts`
Expected: All tests fail with "Failed to resolve import './commands'" or similar — file does not exist yet.

- [ ] **Step 3: Create commands.ts with types and parseCommand**

Create `src/lib/chat/commands.ts`:

```ts
import type {
  RemotePlayerFrame,
  NearestSocialTarget,
  EmoteKind,
  PlayerIdentity,
} from '$lib/types';
import type { BuddyEntry } from '$lib/ipc';

/** Result of parsing a chat input. Null means "treat as plain chat". */
export type ParsedCommand =
  | { kind: 'command'; cmd: string; args: string; raw: string }
  | { kind: 'literal'; text: string };

/** Read-only state snapshots + side-effect adapters injected into handlers. */
export interface CommandContext {
  remotePlayers: RemotePlayerFrame[];
  nearestSocialTarget: NearestSocialTarget | null;
  buddies: BuddyEntry[];
  localIdentity: PlayerIdentity;

  pushLocalBubble: (text: string) => void;
  fireEmote: (kind: EmoteKind, targetHash: string | null) => Promise<void>;
  fireEmoteHi: () => Promise<void>;
  sendChat: (text: string) => Promise<void>;
  blockPlayer: (peerHash: string) => Promise<void>;
  unblockPlayer: (peerHash: string) => Promise<void>;
  getBlockedList: () => Promise<string[]>;
}

export type CommandHandler = (args: string, ctx: CommandContext) => Promise<void>;

export type CommandRegistry = Map<string, CommandHandler>;

/**
 * Parse a chat input line into a structured command, a literal-escape, or null
 * (meaning "not a command — send as plain chat"). See the design spec's
 * Parser section for grammar details.
 */
export function parseCommand(input: string): ParsedCommand | null {
  const trimmed = input.trim();
  if (trimmed === '') return null;
  if (!trimmed.startsWith('/')) return null;

  // //foo escapes a literal leading slash.
  if (trimmed.startsWith('//')) {
    return { kind: 'literal', text: trimmed.slice(1) };
  }

  // Strip leading '/', split on first whitespace run.
  const body = trimmed.slice(1);
  const match = body.match(/^(\S*)(\s+(.*))?$/);
  const cmd = (match?.[1] ?? '').toLowerCase();
  const args = (match?.[3] ?? '').trimStart();

  return { kind: 'command', cmd, args, raw: trimmed };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test -- src/lib/chat/commands.test.ts`
Expected: 10 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/chat/commands.ts src/lib/chat/commands.test.ts
git commit -m "feat(chat): add slash-command parser and types"
```

---

## Task 2: Executor with unknown-command handling

**Files:**
- Modify: `src/lib/chat/commands.ts` (append `executeCommand` function)
- Modify: `src/lib/chat/commands.test.ts` (append executor tests)

- [ ] **Step 1: Append executor tests to commands.test.ts (RED)**

Append to `src/lib/chat/commands.test.ts`:

```ts
import { executeCommand } from './commands';
import type { CommandContext, CommandRegistry, CommandHandler } from './commands';

function makeContext(overrides: Partial<CommandContext> = {}): CommandContext {
  return {
    remotePlayers: [],
    nearestSocialTarget: null,
    buddies: [],
    localIdentity: { displayName: 'Me', addressHash: 'aa'.repeat(16), setupComplete: true },
    pushLocalBubble: () => {},
    fireEmote: async () => {},
    fireEmoteHi: async () => {},
    sendChat: async () => {},
    blockPlayer: async () => {},
    unblockPlayer: async () => {},
    getBlockedList: async () => [],
    ...overrides,
  };
}

describe('executeCommand', () => {
  it('dispatches to the handler for a known command', async () => {
    let receivedArgs = '';
    const handler: CommandHandler = async (args) => {
      receivedArgs = args;
    };
    const registry: CommandRegistry = new Map([['dance', handler]]);
    await executeCommand(
      { kind: 'command', cmd: 'dance', args: 'extra', raw: '/dance extra' },
      registry,
      makeContext(),
    );
    expect(receivedArgs).toBe('extra');
  });

  it('bubbles an unknown-command message for a missing entry', async () => {
    const bubbles: string[] = [];
    const registry: CommandRegistry = new Map();
    await executeCommand(
      { kind: 'command', cmd: 'xyzzy', args: '', raw: '/xyzzy' },
      registry,
      makeContext({ pushLocalBubble: (t) => bubbles.push(t) }),
    );
    expect(bubbles).toEqual(['Unknown command: /xyzzy. Type /help for the list.']);
  });

  it('bubbles a specific message for the bare "/" case', async () => {
    const bubbles: string[] = [];
    await executeCommand(
      { kind: 'command', cmd: '', args: '', raw: '/' },
      new Map(),
      makeContext({ pushLocalBubble: (t) => bubbles.push(t) }),
    );
    expect(bubbles).toEqual(['Unknown command: /. Type /help for the list.']);
  });

  it('catches handler errors and bubbles "Command failed: <message>"', async () => {
    const bubbles: string[] = [];
    const handler: CommandHandler = async () => {
      throw new Error('boom');
    };
    const registry: CommandRegistry = new Map([['fail', handler]]);
    await executeCommand(
      { kind: 'command', cmd: 'fail', args: '', raw: '/fail' },
      registry,
      makeContext({ pushLocalBubble: (t) => bubbles.push(t) }),
    );
    expect(bubbles).toEqual(['Command failed: boom']);
  });

  it('catches non-Error throws and bubbles the stringified value', async () => {
    const bubbles: string[] = [];
    const handler: CommandHandler = async () => {
      throw 'raw string error';
    };
    const registry: CommandRegistry = new Map([['fail', handler]]);
    await executeCommand(
      { kind: 'command', cmd: 'fail', args: '', raw: '/fail' },
      registry,
      makeContext({ pushLocalBubble: (t) => bubbles.push(t) }),
    );
    expect(bubbles).toEqual(['Command failed: raw string error']);
  });
});
```

- [ ] **Step 2: Run tests to verify new tests fail**

Run: `npm run test -- src/lib/chat/commands.test.ts`
Expected: Parser tests pass, executor tests fail with "executeCommand is not a function" or import error.

- [ ] **Step 3: Append executeCommand to commands.ts**

Append to `src/lib/chat/commands.ts`:

```ts
/**
 * Dispatch a parsed command to its handler. Unknown commands and handler
 * errors are reported as local bubbles; nothing propagates to the caller.
 */
export async function executeCommand(
  parsed: Extract<ParsedCommand, { kind: 'command' }>,
  registry: CommandRegistry,
  ctx: CommandContext,
): Promise<void> {
  const handler = registry.get(parsed.cmd);
  if (!handler) {
    ctx.pushLocalBubble(`Unknown command: /${parsed.cmd}. Type /help for the list.`);
    return;
  }
  try {
    await handler(parsed.args, ctx);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    ctx.pushLocalBubble(`Command failed: ${msg}`);
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test -- src/lib/chat/commands.test.ts`
Expected: All 15 tests pass (10 parser + 5 executor).

- [ ] **Step 5: Commit**

```bash
git add src/lib/chat/commands.ts src/lib/chat/commands.test.ts
git commit -m "feat(chat): add slash-command executor with error handling"
```

---

## Task 3: resolvePlayerName helper

**Files:**
- Create: `src/lib/chat/handlers.ts`
- Create: `src/lib/chat/handlers.test.ts`

- [ ] **Step 1: Create handlers.test.ts with resolver tests (RED)**

Create `src/lib/chat/handlers.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { resolvePlayerName } from './handlers';
import type { RemotePlayerFrame } from '$lib/types';
import type { BuddyEntry } from '$lib/ipc';

function remote(name: string, hash: string): RemotePlayerFrame {
  return {
    addressHash: hash,
    displayName: name,
    x: 0,
    y: 0,
    facing: 'right',
    onGround: true,
    animation: 'idle',
    avatar: null,
    epoch: '',
    isBuddy: false,
    partyRole: null,
    emoteAnimation: null,
  };
}

function buddy(name: string, hash: string): BuddyEntry {
  return {
    addressHash: hash,
    displayName: name,
    addedDate: '2026-01-01',
    coPresenceTotal: 0,
    lastSeenDate: null,
  };
}

describe('resolvePlayerName', () => {
  it('matches case-insensitively on displayName from remotePlayers', () => {
    const rp = [remote('Alice', '11'.repeat(16))];
    const result = resolvePlayerName('alice', { remotePlayers: rp });
    expect(result).toEqual({ hash: '11'.repeat(16), displayName: 'Alice' });
  });

  it('falls back to buddies when not found in remotePlayers', () => {
    const rp = [remote('Charlie', '33'.repeat(16))];
    const buds = [buddy('Bob', '22'.repeat(16))];
    const result = resolvePlayerName('bob', { remotePlayers: rp, buddies: buds });
    expect(result).toEqual({ hash: '22'.repeat(16), displayName: 'Bob' });
  });

  it('prefers remotePlayers when a name appears in both sources', () => {
    const rp = [remote('Alice', 'aa'.repeat(16))];
    const buds = [buddy('Alice', 'bb'.repeat(16))];
    const result = resolvePlayerName('Alice', { remotePlayers: rp, buddies: buds });
    expect(result?.hash).toBe('aa'.repeat(16));
  });

  it('returns null when no source contains the name', () => {
    const rp = [remote('Alice', '11'.repeat(16))];
    expect(resolvePlayerName('Bob', { remotePlayers: rp })).toBeNull();
  });

  it('returns null for empty name', () => {
    const rp = [remote('Alice', '11'.repeat(16))];
    expect(resolvePlayerName('', { remotePlayers: rp })).toBeNull();
  });

  it('handles sources being absent', () => {
    expect(resolvePlayerName('alice', {})).toBeNull();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: All tests fail with module-not-found error.

- [ ] **Step 3: Create handlers.ts with resolvePlayerName**

Create `src/lib/chat/handlers.ts`:

```ts
import type { RemotePlayerFrame } from '$lib/types';
import type { BuddyEntry } from '$lib/ipc';
import type { CommandHandler, CommandRegistry } from './commands';

/**
 * Case-insensitive exact match on displayName. Lookup order:
 * remotePlayers → buddies. First hit wins.
 */
export function resolvePlayerName(
  name: string,
  sources: {
    remotePlayers?: RemotePlayerFrame[];
    buddies?: BuddyEntry[];
  },
): { hash: string; displayName: string } | null {
  if (name === '') return null;
  const lc = name.toLowerCase();

  for (const rp of sources.remotePlayers ?? []) {
    if (rp.displayName.toLowerCase() === lc) {
      return { hash: rp.addressHash, displayName: rp.displayName };
    }
  }
  for (const b of sources.buddies ?? []) {
    if (b.displayName.toLowerCase() === lc) {
      return { hash: b.addressHash, displayName: b.displayName };
    }
  }
  return null;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: All 6 resolver tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/chat/handlers.ts src/lib/chat/handlers.test.ts
git commit -m "feat(chat): add resolvePlayerName helper"
```

---

## Task 4: Untargeted emote handlers (/hi, /dance, /applaud)

**Files:**
- Modify: `src/lib/chat/handlers.ts` (append handlers)
- Modify: `src/lib/chat/handlers.test.ts` (append tests)

- [ ] **Step 1: Append tests to handlers.test.ts (RED)**

Append to `src/lib/chat/handlers.test.ts`:

```ts
import { hiHandler, danceHandler, applaudHandler } from './handlers';
import type { CommandContext } from './commands';

function makeContext(overrides: Partial<CommandContext> = {}): CommandContext {
  return {
    remotePlayers: [],
    nearestSocialTarget: null,
    buddies: [],
    localIdentity: { displayName: 'Me', addressHash: 'ff'.repeat(16), setupComplete: true },
    pushLocalBubble: () => {},
    fireEmote: async () => {},
    fireEmoteHi: async () => {},
    sendChat: async () => {},
    blockPlayer: async () => {},
    unblockPlayer: async () => {},
    getBlockedList: async () => [],
    ...overrides,
  };
}

describe('hiHandler', () => {
  it('calls ctx.fireEmoteHi regardless of args', async () => {
    let called = 0;
    await hiHandler('', makeContext({ fireEmoteHi: async () => { called++; } }));
    expect(called).toBe(1);
    await hiHandler('ignored junk', makeContext({ fireEmoteHi: async () => { called++; } }));
    expect(called).toBe(2);
  });
});

describe('danceHandler', () => {
  it('fires dance with null target (broadcast)', async () => {
    const calls: Array<[string, string | null]> = [];
    await danceHandler(
      '',
      makeContext({
        fireEmote: async (kind, target) => {
          calls.push([kind as string, target]);
        },
      }),
    );
    expect(calls).toEqual([['dance', null]]);
  });
});

describe('applaudHandler', () => {
  it('fires applaud with null target (broadcast)', async () => {
    const calls: Array<[string, string | null]> = [];
    await applaudHandler(
      '',
      makeContext({
        fireEmote: async (kind, target) => {
          calls.push([kind as string, target]);
        },
      }),
    );
    expect(calls).toEqual([['applaud', null]]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: New tests fail — handlers don't exist yet.

- [ ] **Step 3: Append untargeted emote handlers to handlers.ts**

Append to `src/lib/chat/handlers.ts`:

```ts
export const hiHandler: CommandHandler = async (_args, ctx) => {
  await ctx.fireEmoteHi();
};

export const danceHandler: CommandHandler = async (_args, ctx) => {
  await ctx.fireEmote('dance', null);
};

export const applaudHandler: CommandHandler = async (_args, ctx) => {
  await ctx.fireEmote('applaud', null);
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: All tests pass (resolver + 3 untargeted handler tests = 9 total).

- [ ] **Step 5: Commit**

```bash
git add src/lib/chat/handlers.ts src/lib/chat/handlers.test.ts
git commit -m "feat(chat): add /hi, /dance, /applaud handlers"
```

---

## Task 5: Target-taking emote handlers (/wave, /hug, /high5)

**Files:**
- Modify: `src/lib/chat/handlers.ts` (append handlers)
- Modify: `src/lib/chat/handlers.test.ts` (append tests)

- [ ] **Step 1: Append tests to handlers.test.ts (RED)**

Append to `src/lib/chat/handlers.test.ts`:

```ts
import { waveHandler, hugHandler, high5Handler } from './handlers';
import type { NearestSocialTarget } from '$lib/types';

function nearestTarget(hash: string, name = 'Nearest'): NearestSocialTarget {
  return { addressHash: hash, displayName: name, isBuddy: false, inParty: false };
}

describe('waveHandler', () => {
  it('with no args and no nearest, broadcasts (null target)', async () => {
    const calls: Array<[string, string | null]> = [];
    await waveHandler(
      '',
      makeContext({
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(calls).toEqual([['wave', null]]);
  });

  it('with no args and a nearest target, targets nearest', async () => {
    const calls: Array<[string, string | null]> = [];
    await waveHandler(
      '',
      makeContext({
        nearestSocialTarget: nearestTarget('aa'.repeat(16)),
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(calls).toEqual([['wave', 'aa'.repeat(16)]]);
  });

  it('resolves explicit name from remotePlayers', async () => {
    const calls: Array<[string, string | null]> = [];
    await waveHandler(
      'Alice',
      makeContext({
        remotePlayers: [remote('Alice', '11'.repeat(16))],
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(calls).toEqual([['wave', '11'.repeat(16)]]);
  });

  it('name-not-found bubbles error and does NOT fire', async () => {
    const bubbles: string[] = [];
    const calls: Array<[string, string | null]> = [];
    await waveHandler(
      'ghost',
      makeContext({
        pushLocalBubble: (t) => bubbles.push(t),
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(bubbles).toEqual(['No player named ghost nearby.']);
    expect(calls).toEqual([]);
  });

  it('self-target by name bubbles error and does NOT fire', async () => {
    const bubbles: string[] = [];
    const calls: Array<[string, string | null]> = [];
    await waveHandler(
      'me',
      makeContext({
        localIdentity: { displayName: 'Me', addressHash: 'ff'.repeat(16), setupComplete: true },
        pushLocalBubble: (t) => bubbles.push(t),
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(bubbles).toEqual(["Can't wave at yourself."]);
    expect(calls).toEqual([]);
  });
});

describe('hugHandler', () => {
  it('with no args and no nearest, bubbles error and does NOT fire', async () => {
    const bubbles: string[] = [];
    const calls: Array<[string, string | null]> = [];
    await hugHandler(
      '',
      makeContext({
        pushLocalBubble: (t) => bubbles.push(t),
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(bubbles).toEqual(['/hug needs a target nearby.']);
    expect(calls).toEqual([]);
  });

  it('with no args and a nearest target, targets nearest', async () => {
    const calls: Array<[string, string | null]> = [];
    await hugHandler(
      '',
      makeContext({
        nearestSocialTarget: nearestTarget('cc'.repeat(16)),
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(calls).toEqual([['hug', 'cc'.repeat(16)]]);
  });

  it('resolves explicit name from remotePlayers', async () => {
    const calls: Array<[string, string | null]> = [];
    await hugHandler(
      'Alice',
      makeContext({
        remotePlayers: [remote('Alice', '11'.repeat(16))],
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(calls).toEqual([['hug', '11'.repeat(16)]]);
  });

  it('name-not-found bubbles error', async () => {
    const bubbles: string[] = [];
    await hugHandler(
      'ghost',
      makeContext({ pushLocalBubble: (t) => bubbles.push(t) }),
    );
    expect(bubbles).toEqual(['No player named ghost nearby.']);
  });

  it('self-target by name bubbles error', async () => {
    const bubbles: string[] = [];
    await hugHandler(
      'me',
      makeContext({
        localIdentity: { displayName: 'Me', addressHash: 'ff'.repeat(16), setupComplete: true },
        pushLocalBubble: (t) => bubbles.push(t),
      }),
    );
    expect(bubbles).toEqual(["Can't hug yourself."]);
  });
});

describe('high5Handler', () => {
  it('fires with nearest target when no arg', async () => {
    const calls: Array<[string, string | null]> = [];
    await high5Handler(
      '',
      makeContext({
        nearestSocialTarget: nearestTarget('dd'.repeat(16)),
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(calls).toEqual([['high_five', 'dd'.repeat(16)]]);
  });

  it('bubbles error with no nearest and no name', async () => {
    const bubbles: string[] = [];
    await high5Handler('', makeContext({ pushLocalBubble: (t) => bubbles.push(t) }));
    expect(bubbles).toEqual(['/high5 needs a target nearby.']);
  });

  it('self-target bubbles "Can\'t high-five yourself."', async () => {
    const bubbles: string[] = [];
    await high5Handler(
      'me',
      makeContext({
        localIdentity: { displayName: 'Me', addressHash: 'ff'.repeat(16), setupComplete: true },
        pushLocalBubble: (t) => bubbles.push(t),
      }),
    );
    expect(bubbles).toEqual(["Can't high-five yourself."]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: New tests fail — handlers not defined.

- [ ] **Step 3: Append target-taking emote handlers to handlers.ts**

Append to `src/lib/chat/handlers.ts`:

```ts
/** Returns true iff `name` matches the local player's displayName (case-insensitive, trimmed). */
function isSelfName(name: string, localDisplayName: string): boolean {
  return name.trim().toLowerCase() === localDisplayName.toLowerCase();
}

export const waveHandler: CommandHandler = async (args, ctx) => {
  const name = args.trim();
  if (name === '') {
    const target = ctx.nearestSocialTarget?.addressHash ?? null;
    await ctx.fireEmote('wave', target);
    return;
  }
  if (isSelfName(name, ctx.localIdentity.displayName)) {
    ctx.pushLocalBubble("Can't wave at yourself.");
    return;
  }
  const resolved = resolvePlayerName(name, { remotePlayers: ctx.remotePlayers });
  if (!resolved) {
    ctx.pushLocalBubble(`No player named ${name} nearby.`);
    return;
  }
  await ctx.fireEmote('wave', resolved.hash);
};

export const hugHandler: CommandHandler = async (args, ctx) => {
  const name = args.trim();
  if (name === '') {
    const target = ctx.nearestSocialTarget?.addressHash ?? null;
    if (target === null) {
      ctx.pushLocalBubble('/hug needs a target nearby.');
      return;
    }
    await ctx.fireEmote('hug', target);
    return;
  }
  if (isSelfName(name, ctx.localIdentity.displayName)) {
    ctx.pushLocalBubble("Can't hug yourself.");
    return;
  }
  const resolved = resolvePlayerName(name, { remotePlayers: ctx.remotePlayers });
  if (!resolved) {
    ctx.pushLocalBubble(`No player named ${name} nearby.`);
    return;
  }
  await ctx.fireEmote('hug', resolved.hash);
};

export const high5Handler: CommandHandler = async (args, ctx) => {
  const name = args.trim();
  if (name === '') {
    const target = ctx.nearestSocialTarget?.addressHash ?? null;
    if (target === null) {
      ctx.pushLocalBubble('/high5 needs a target nearby.');
      return;
    }
    await ctx.fireEmote('high_five', target);
    return;
  }
  if (isSelfName(name, ctx.localIdentity.displayName)) {
    ctx.pushLocalBubble("Can't high-five yourself.");
    return;
  }
  const resolved = resolvePlayerName(name, { remotePlayers: ctx.remotePlayers });
  if (!resolved) {
    ctx.pushLocalBubble(`No player named ${name} nearby.`);
    return;
  }
  await ctx.fireEmote('high_five', resolved.hash);
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/chat/handlers.ts src/lib/chat/handlers.test.ts
git commit -m "feat(chat): add /wave, /hug, /high5 handlers with nearest fallback"
```

---

## Task 6: Moderation handlers (/block, /unblock)

**Files:**
- Modify: `src/lib/chat/handlers.ts` (append handlers)
- Modify: `src/lib/chat/handlers.test.ts` (append tests)

- [ ] **Step 1: Append tests to handlers.test.ts (RED)**

Append to `src/lib/chat/handlers.test.ts`:

```ts
import { blockHandler, unblockHandler } from './handlers';

describe('blockHandler', () => {
  it('empty args bubbles usage', async () => {
    const bubbles: string[] = [];
    await blockHandler('', makeContext({ pushLocalBubble: (t) => bubbles.push(t) }));
    expect(bubbles).toEqual(['Usage: /block <name>']);
  });

  it('resolves from remotePlayers, calls blockPlayer, bubbles success', async () => {
    const blocks: string[] = [];
    const bubbles: string[] = [];
    await blockHandler(
      'Alice',
      makeContext({
        remotePlayers: [remote('Alice', '11'.repeat(16))],
        pushLocalBubble: (t) => bubbles.push(t),
        blockPlayer: async (h) => { blocks.push(h); },
      }),
    );
    expect(blocks).toEqual(['11'.repeat(16)]);
    expect(bubbles).toEqual(['Blocked Alice.']);
  });

  it('falls back to buddies when not in remotePlayers', async () => {
    const blocks: string[] = [];
    await blockHandler(
      'Bob',
      makeContext({
        remotePlayers: [],
        buddies: [buddy('Bob', '22'.repeat(16))],
        blockPlayer: async (h) => { blocks.push(h); },
      }),
    );
    expect(blocks).toEqual(['22'.repeat(16)]);
  });

  it('name not found in either source bubbles error', async () => {
    const blocks: string[] = [];
    const bubbles: string[] = [];
    await blockHandler(
      'ghost',
      makeContext({
        pushLocalBubble: (t) => bubbles.push(t),
        blockPlayer: async (h) => { blocks.push(h); },
      }),
    );
    expect(bubbles).toEqual(['No player named ghost.']);
    expect(blocks).toEqual([]);
  });

  it('self-target bubbles error and does NOT call blockPlayer', async () => {
    const blocks: string[] = [];
    const bubbles: string[] = [];
    await blockHandler(
      'me',
      makeContext({
        localIdentity: { displayName: 'Me', addressHash: 'ff'.repeat(16), setupComplete: true },
        pushLocalBubble: (t) => bubbles.push(t),
        blockPlayer: async (h) => { blocks.push(h); },
      }),
    );
    expect(bubbles).toEqual(["Can't block yourself."]);
    expect(blocks).toEqual([]);
  });
});

describe('unblockHandler', () => {
  it('empty args bubbles usage', async () => {
    const bubbles: string[] = [];
    await unblockHandler('', makeContext({ pushLocalBubble: (t) => bubbles.push(t) }));
    expect(bubbles).toEqual(['Usage: /unblock <name>']);
  });

  it('name resolves, hash is in blocked list, unblocks', async () => {
    const unblocks: string[] = [];
    const bubbles: string[] = [];
    const hash = '22'.repeat(16);
    await unblockHandler(
      'Bob',
      makeContext({
        buddies: [buddy('Bob', hash)],
        pushLocalBubble: (t) => bubbles.push(t),
        unblockPlayer: async (h) => { unblocks.push(h); },
        getBlockedList: async () => [hash, 'ab'.repeat(16)],
      }),
    );
    expect(unblocks).toEqual([hash]);
    expect(bubbles).toEqual(['Unblocked Bob.']);
  });

  it('name resolves but hash not in blocked list bubbles "not blocked"', async () => {
    const unblocks: string[] = [];
    const bubbles: string[] = [];
    await unblockHandler(
      'Bob',
      makeContext({
        buddies: [buddy('Bob', '22'.repeat(16))],
        pushLocalBubble: (t) => bubbles.push(t),
        unblockPlayer: async (h) => { unblocks.push(h); },
        getBlockedList: async () => [],
      }),
    );
    expect(bubbles).toEqual(['Bob is not blocked.']);
    expect(unblocks).toEqual([]);
  });

  it('name not found in visible+buddies bubbles "No player named X"', async () => {
    const bubbles: string[] = [];
    await unblockHandler(
      'ghost',
      makeContext({
        pushLocalBubble: (t) => bubbles.push(t),
        getBlockedList: async () => [],
      }),
    );
    expect(bubbles).toEqual(['No player named ghost.']);
  });

  it('self-target bubbles error', async () => {
    const bubbles: string[] = [];
    await unblockHandler(
      'me',
      makeContext({
        localIdentity: { displayName: 'Me', addressHash: 'ff'.repeat(16), setupComplete: true },
        pushLocalBubble: (t) => bubbles.push(t),
      }),
    );
    expect(bubbles).toEqual(["Can't unblock yourself."]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: New tests fail — handlers not defined.

- [ ] **Step 3: Append moderation handlers to handlers.ts**

Append to `src/lib/chat/handlers.ts`:

```ts
export const blockHandler: CommandHandler = async (args, ctx) => {
  const name = args.trim();
  if (name === '') {
    ctx.pushLocalBubble('Usage: /block <name>');
    return;
  }
  if (isSelfName(name, ctx.localIdentity.displayName)) {
    ctx.pushLocalBubble("Can't block yourself.");
    return;
  }
  const resolved = resolvePlayerName(name, {
    remotePlayers: ctx.remotePlayers,
    buddies: ctx.buddies,
  });
  if (!resolved) {
    ctx.pushLocalBubble(`No player named ${name}.`);
    return;
  }
  await ctx.blockPlayer(resolved.hash);
  ctx.pushLocalBubble(`Blocked ${resolved.displayName}.`);
};

export const unblockHandler: CommandHandler = async (args, ctx) => {
  const name = args.trim();
  if (name === '') {
    ctx.pushLocalBubble('Usage: /unblock <name>');
    return;
  }
  if (isSelfName(name, ctx.localIdentity.displayName)) {
    ctx.pushLocalBubble("Can't unblock yourself.");
    return;
  }
  const resolved = resolvePlayerName(name, {
    remotePlayers: ctx.remotePlayers,
    buddies: ctx.buddies,
  });
  if (!resolved) {
    ctx.pushLocalBubble(`No player named ${name}.`);
    return;
  }
  const blocked = await ctx.getBlockedList();
  if (!blocked.includes(resolved.hash)) {
    ctx.pushLocalBubble(`${resolved.displayName} is not blocked.`);
    return;
  }
  await ctx.unblockPlayer(resolved.hash);
  ctx.pushLocalBubble(`Unblocked ${resolved.displayName}.`);
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/chat/handlers.ts src/lib/chat/handlers.test.ts
git commit -m "feat(chat): add /block and /unblock handlers"
```

---

## Task 7: /me handler

**Files:**
- Modify: `src/lib/chat/handlers.ts` (append handler)
- Modify: `src/lib/chat/handlers.test.ts` (append tests)

- [ ] **Step 1: Append tests to handlers.test.ts (RED)**

Append to `src/lib/chat/handlers.test.ts`:

```ts
import { meHandler } from './handlers';

describe('meHandler', () => {
  it('empty args bubbles usage', async () => {
    const bubbles: string[] = [];
    const sends: string[] = [];
    await meHandler(
      '',
      makeContext({
        pushLocalBubble: (t) => bubbles.push(t),
        sendChat: async (t) => { sends.push(t); },
      }),
    );
    expect(bubbles).toEqual(['Usage: /me <action>']);
    expect(sends).toEqual([]);
  });

  it('whitespace-only args treated as empty', async () => {
    const bubbles: string[] = [];
    await meHandler('   ', makeContext({ pushLocalBubble: (t) => bubbles.push(t) }));
    expect(bubbles).toEqual(['Usage: /me <action>']);
  });

  it('formats as "* {name} {action} *" and calls sendChat', async () => {
    const sends: string[] = [];
    await meHandler(
      'waves hello',
      makeContext({
        localIdentity: { displayName: 'Alice', addressHash: 'aa'.repeat(16), setupComplete: true },
        sendChat: async (t) => { sends.push(t); },
      }),
    );
    expect(sends).toEqual(['* Alice waves hello *']);
  });

  it('preserves intra-arg whitespace', async () => {
    const sends: string[] = [];
    await meHandler(
      'waves  hello  world',
      makeContext({
        localIdentity: { displayName: 'Me', addressHash: 'ff'.repeat(16), setupComplete: true },
        sendChat: async (t) => { sends.push(t); },
      }),
    );
    expect(sends).toEqual(['* Me waves  hello  world *']);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: New tests fail — meHandler not defined.

- [ ] **Step 3: Append meHandler to handlers.ts**

Append to `src/lib/chat/handlers.ts`:

```ts
export const meHandler: CommandHandler = async (args, ctx) => {
  const action = args.trim();
  if (action === '') {
    ctx.pushLocalBubble('Usage: /me <action>');
    return;
  }
  const formatted = `* ${ctx.localIdentity.displayName} ${args} *`;
  await ctx.sendChat(formatted);
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/chat/handlers.ts src/lib/chat/handlers.test.ts
git commit -m "feat(chat): add /me handler (client-side pre-format)"
```

---

## Task 8: /help handler

**Files:**
- Modify: `src/lib/chat/handlers.ts` (append handler + constant)
- Modify: `src/lib/chat/handlers.test.ts` (append tests)

- [ ] **Step 1: Append tests to handlers.test.ts (RED)**

Append to `src/lib/chat/handlers.test.ts`:

```ts
import { helpHandler, HELP_LINES } from './handlers';

describe('helpHandler', () => {
  it('emits HELP_LINES as local bubbles in order', async () => {
    const bubbles: string[] = [];
    // Use fake timers so the sequenced 80ms dispatch completes synchronously.
    vi.useFakeTimers();
    try {
      const promise = helpHandler('', makeContext({ pushLocalBubble: (t) => bubbles.push(t) }));
      await vi.runAllTimersAsync();
      await promise;
    } finally {
      vi.useRealTimers();
    }
    expect(bubbles).toEqual(HELP_LINES);
    expect(HELP_LINES.length).toBe(4);
  });

  it('first bubble is the header', () => {
    expect(HELP_LINES[0]).toBe('* Commands:');
  });

  it('includes every v1 command somewhere in the list', () => {
    const joined = HELP_LINES.join(' ');
    for (const cmd of ['/hi', '/dance', '/wave', '/hug', '/high5', '/applaud',
                       '/block', '/unblock', '/me', '/help']) {
      expect(joined).toContain(cmd);
    }
  });
});
```

Add `import { vi }` to the existing `import { describe, it, expect }` statement at the top of the file if not already present.

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: New tests fail — helpHandler and HELP_LINES not defined.

- [ ] **Step 3: Append helpHandler and HELP_LINES to handlers.ts**

Append to `src/lib/chat/handlers.ts`:

```ts
export const HELP_LINES: readonly string[] = [
  '* Commands:',
  '* /hi /dance /wave /hug /high5 /applaud',
  '* /block <name> /unblock <name>',
  '* /me <action>      /help',
];

/** Emits help lines ~80ms apart so they stack legibly and age together. */
export const helpHandler: CommandHandler = async (_args, ctx) => {
  for (let i = 0; i < HELP_LINES.length; i++) {
    if (i > 0) {
      await new Promise<void>((resolve) => setTimeout(resolve, 80));
    }
    ctx.pushLocalBubble(HELP_LINES[i]);
  }
};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/chat/handlers.ts src/lib/chat/handlers.test.ts
git commit -m "feat(chat): add /help handler with sequenced bubble dispatch"
```

---

## Task 9: Default handler registry

**Files:**
- Modify: `src/lib/chat/handlers.ts` (append `createDefaultHandlers`)
- Modify: `src/lib/chat/handlers.test.ts` (append registry tests)

- [ ] **Step 1: Append registry tests to handlers.test.ts (RED)**

Append to `src/lib/chat/handlers.test.ts`:

```ts
import { createDefaultHandlers } from './handlers';

describe('createDefaultHandlers', () => {
  it('registers all 10 v1 commands', () => {
    const reg = createDefaultHandlers();
    for (const cmd of ['hi', 'dance', 'wave', 'hug', 'high5', 'applaud',
                       'block', 'unblock', 'me', 'help']) {
      expect(reg.has(cmd)).toBe(true);
    }
  });

  it('aliases /highfive to the /high5 handler', () => {
    const reg = createDefaultHandlers();
    expect(reg.get('highfive')).toBe(reg.get('high5'));
  });

  it('is integration-wired: /dance fires dance broadcast through the registry lookup', async () => {
    const calls: Array<[string, string | null]> = [];
    const reg = createDefaultHandlers();
    const handler = reg.get('dance')!;
    await handler(
      '',
      makeContext({
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(calls).toEqual([['dance', null]]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: New tests fail — createDefaultHandlers not defined.

- [ ] **Step 3: Append createDefaultHandlers to handlers.ts**

Append to `src/lib/chat/handlers.ts`:

```ts
export function createDefaultHandlers(): CommandRegistry {
  return new Map<string, CommandHandler>([
    ['hi', hiHandler],
    ['dance', danceHandler],
    ['applaud', applaudHandler],
    ['wave', waveHandler],
    ['hug', hugHandler],
    ['high5', high5Handler],
    ['highfive', high5Handler],
    ['block', blockHandler],
    ['unblock', unblockHandler],
    ['me', meHandler],
    ['help', helpHandler],
  ]);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test -- src/lib/chat/handlers.test.ts`
Expected: All tests pass (full file: resolver + 10 handlers + registry, ~40 tests).

Also run the full chat test suite to confirm no regressions:
Run: `npm run test -- src/lib/chat/`
Expected: commands.test.ts (15) + handlers.test.ts (~40) all pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/chat/handlers.ts src/lib/chat/handlers.test.ts
git commit -m "feat(chat): add default handler registry with /highfive alias"
```

---

## Task 10: Extract `fireEmoteWithFeedback` in App.svelte

**Files:**
- Modify: `src/App.svelte` (refactor `fireHiWithAnimation` and `handleEmoteSelect` around lines 647–708)

This task is a pure refactor — behavior unchanged, but the emote-firing logic becomes reusable by command handlers. Existing tests must still pass.

- [ ] **Step 1: Read the current implementation to confirm line numbers**

Run: `grep -n 'fireHiWithAnimation\|handleEmoteSelect' src/App.svelte`
Expected: `fireHiWithAnimation` around line 647, `handleEmoteSelect` around line 667, referenced from palette onSelect (~1050), H-key (~774), SocialPrompt onHi (~1097).

- [ ] **Step 2: Refactor `fireHiWithAnimation` to accept a feedback sink**

In `src/App.svelte`, replace the function (currently at ~647–665):

**Before:**
```ts
async function fireHiWithAnimation() {
  try {
    const result = await emoteHi();
    spawnEmoteAnimation('self', { hi: result.variant as HiVariant }, null);
    emoteCooldownExpiries = {
      ...emoteCooldownExpiries,
      hi: Date.now() + result.cooldown_ms,
    };
  } catch (err) {
    const msg = typeof err === 'string' ? err : 'Hi failed';
    pushEmoteFeedback(msg);
  }
}
```

**After:**
```ts
async function fireHiWithAnimation(pushFeedback: (msg: string) => void = pushEmoteFeedback) {
  try {
    const result = await emoteHi();
    spawnEmoteAnimation('self', { hi: result.variant as HiVariant }, null);
    emoteCooldownExpiries = {
      ...emoteCooldownExpiries,
      hi: Date.now() + result.cooldown_ms,
    };
  } catch (err) {
    const msg = typeof err === 'string' ? err : 'Hi failed';
    pushFeedback(msg);
  }
}
```

- [ ] **Step 3: Extract `fireEmoteWithFeedback` from `handleEmoteSelect`**

Still in `src/App.svelte`, replace the `handleEmoteSelect` function (currently at ~667–708) with two functions:

**Before (the single `handleEmoteSelect`):**
```ts
async function handleEmoteSelect(kind: EmoteKind) {
  if (typeof kind === 'object' && 'hi' in kind) {
    await fireHiWithAnimation();
    return;
  }

  const nearest = latestFrame?.nearestSocialTarget?.addressHash ?? null;
  const target = (kind === 'hug' || kind === 'high_five' || kind === 'wave')
    ? nearest
    : null;
  const result: EmoteFireResult = await emoteFire(kind, target);
  switch (result.type) {
    case 'success':
      spawnEmoteAnimation('self', kind, target);
      emoteCooldownExpiries = {
        ...emoteCooldownExpiries,
        [kind as string]: Date.now() + result.cooldown_ms,
      };
      break;
    case 'cooldown':
      emoteCooldownExpiries = {
        ...emoteCooldownExpiries,
        [kind as string]: Date.now() + result.remaining_ms,
      };
      break;
    case 'no_target':
      pushEmoteFeedback('No target in range');
      break;
    case 'target_blocked':
      pushEmoteFeedback('Player is blocked');
      break;
  }
}
```

**After (two functions — the extracted helper + a thin wrapper that preserves palette behavior):**
```ts
/**
 * Fire an emote with explicit target and route feedback through the caller's
 * sink. Shared by the palette, hotkeys, and chat command handlers — one place
 * to maintain the EmoteFireResult switch.
 */
async function fireEmoteWithFeedback(
  kind: EmoteKind,
  target: string | null,
  pushFeedback: (msg: string) => void = pushEmoteFeedback,
) {
  if (typeof kind === 'object' && 'hi' in kind) {
    await fireHiWithAnimation(pushFeedback);
    return;
  }
  const result: EmoteFireResult = await emoteFire(kind, target);
  switch (result.type) {
    case 'success':
      spawnEmoteAnimation('self', kind, target);
      emoteCooldownExpiries = {
        ...emoteCooldownExpiries,
        [kind as string]: Date.now() + result.cooldown_ms,
      };
      break;
    case 'cooldown':
      emoteCooldownExpiries = {
        ...emoteCooldownExpiries,
        [kind as string]: Date.now() + result.remaining_ms,
      };
      break;
    case 'no_target':
      pushFeedback('No target in range');
      break;
    case 'target_blocked':
      pushFeedback('Player is blocked');
      break;
  }
}

/**
 * Palette onSelect adapter: computes target from nearestSocialTarget and
 * delegates. Preserves the palette's existing "auto-pick nearest for
 * targeted-only kinds, broadcast otherwise" behavior.
 */
async function handleEmoteSelect(kind: EmoteKind) {
  const nearest = latestFrame?.nearestSocialTarget?.addressHash ?? null;
  const target = (kind === 'hug' || kind === 'high_five' || kind === 'wave')
    ? nearest
    : null;
  await fireEmoteWithFeedback(kind, target);
}
```

- [ ] **Step 4: Run full test suite to confirm no regressions**

Run: `npm run test`
Expected: All existing tests pass (EmotePalette, App behavior via SocialPrompt, etc.). No new tests yet — this is pure refactor.

Run: `npm run build`
Expected: TypeScript compilation succeeds.

- [ ] **Step 5: Commit**

```bash
git add src/App.svelte
git commit -m "refactor(emote): extract fireEmoteWithFeedback for chat reuse"
```

---

## Task 11: Add `onCommand` prop to ChatInput + component tests

**Files:**
- Modify: `src/lib/components/ChatInput.svelte`
- Create: `src/lib/components/ChatInput.test.ts`

- [ ] **Step 1: Create ChatInput.test.ts (RED)**

Create `src/lib/components/ChatInput.test.ts`:

```ts
// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import ChatInput from './ChatInput.svelte';

// Mock the IPC module — only sendChat is imported by ChatInput.
vi.mock('../ipc', () => ({
  sendChat: vi.fn(async () => {}),
}));

import { sendChat } from '../ipc';

describe('ChatInput', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function focusAndType(container: HTMLElement, text: string) {
    // Component is hidden until focused; focus opens the input.
    await fireEvent.keyDown(window, { key: 'Enter' });
    const input = container.querySelector('input')!;
    expect(input).toBeTruthy();
    await fireEvent.input(input, { target: { value: text } });
    return input;
  }

  it('plain text submits via sendChat, not onCommand', async () => {
    const onCommand = vi.fn();
    const { container } = render(ChatInput, {
      props: { onFocusChange: () => {}, onCommand },
    });
    const input = await focusAndType(container, 'hello world');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(sendChat).toHaveBeenCalledWith('hello world');
    expect(onCommand).not.toHaveBeenCalled();
  });

  it('slash command routes to onCommand, not sendChat', async () => {
    const onCommand = vi.fn(async () => {});
    const { container } = render(ChatInput, {
      props: { onFocusChange: () => {}, onCommand },
    });
    const input = await focusAndType(container, '/dance');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommand).toHaveBeenCalledWith({
      kind: 'command',
      cmd: 'dance',
      args: '',
      raw: '/dance',
    });
    expect(sendChat).not.toHaveBeenCalled();
  });

  it('literal //text sends the stripped form via sendChat', async () => {
    const onCommand = vi.fn();
    const { container } = render(ChatInput, {
      props: { onFocusChange: () => {}, onCommand },
    });
    const input = await focusAndType(container, '//dance');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(sendChat).toHaveBeenCalledWith('/dance');
    expect(onCommand).not.toHaveBeenCalled();
  });

  it('clears the input after submit', async () => {
    const { container } = render(ChatInput, {
      props: { onFocusChange: () => {}, onCommand: async () => {} },
    });
    const input = await focusAndType(container, '/dance');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect((input as HTMLInputElement).value).toBe('');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test -- src/lib/components/ChatInput.test.ts`
Expected: Tests fail — ChatInput does not accept an `onCommand` prop and does not call it.

- [ ] **Step 3: Modify ChatInput.svelte to parse slash input and call onCommand**

Replace the `<script>` block in `src/lib/components/ChatInput.svelte` (currently lines 1–44) with:

```svelte
<script lang="ts">
  import { sendChat } from '../ipc';
  import { parseCommand, type ParsedCommand } from '$lib/chat/commands';

  let {
    onFocusChange,
    onCommand,
  }: {
    onFocusChange: (focused: boolean) => void;
    onCommand: (parsed: Extract<ParsedCommand, { kind: 'command' }>) => Promise<void>;
  } = $props();

  let inputEl = $state<HTMLInputElement>();
  let text = $state('');
  let focused = $state(false);

  function handleGlobalKeyDown(e: KeyboardEvent) {
    if (!focused && (e.key === 'Enter' || e.key === '/')) {
      e.preventDefault();
      focused = true;
      onFocusChange(true);
      requestAnimationFrame(() => inputEl?.focus());
    }
  }

  function handleSubmit() {
    const raw = text.trim();
    text = '';
    handleBlur();
    if (raw === '') return;

    const parsed = parseCommand(raw);
    if (parsed === null) {
      sendChat(raw).catch(console.error);
    } else if (parsed.kind === 'literal') {
      sendChat(parsed.text).catch(console.error);
    } else {
      onCommand(parsed).catch(console.error);
    }
  }

  function handleBlur() {
    focused = false;
    onFocusChange(false);
    inputEl?.blur();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      text = '';
      handleBlur();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      handleSubmit();
    }
    e.stopPropagation();
  }
</script>
```

The rest of the component (template + styles, lines 46–99) is unchanged.

- [ ] **Step 4: Run component tests to verify they pass**

Run: `npm run test -- src/lib/components/ChatInput.test.ts`
Expected: All 4 tests pass.

Run full test suite to confirm no regressions:
Run: `npm run test`
Expected: Full suite passes (handlers, commands, ChatInput, all existing tests).

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/ChatInput.svelte src/lib/components/ChatInput.test.ts
git commit -m "feat(chat): route slash input through parseCommand to onCommand prop"
```

---

## Task 12: Wire command dispatch in App.svelte

**Files:**
- Modify: `src/App.svelte` (add imports, context builder, onCommand handler; pass prop to ChatInput)

- [ ] **Step 1: Add imports at the top of the `<script>` block**

Find the existing `./lib/ipc` import line (around line 33 of `src/App.svelte`) and the `./lib/types` import block. Add these two new imports after the existing imports (preserve the existing lines):

```ts
import { getBlockedList } from './lib/ipc';
import { executeCommand, type CommandContext, type ParsedCommand } from '$lib/chat/commands';
import { createDefaultHandlers } from '$lib/chat/handlers';
```

Note: `getBlockedList` may already be imported via the long existing import — if it is, don't duplicate. Verify with:
Run: `grep -n 'getBlockedList' src/App.svelte`

- [ ] **Step 2: Cache the local display name alongside the address hash**

`App.svelte` currently stores the local `addressHash` in `ourAddressHash = $state('')` (~line 155) and sets it from `identity.addressHash` in the `onMount` (~line 187). It does NOT currently cache the local display name, but `getIdentity()` returns it as `identity.displayName`.

Add a parallel state declaration immediately after `let ourAddressHash = $state('');`:

```ts
let ourDisplayName = $state('');
```

In the `onMount` block where `ourAddressHash = identity.addressHash;` is set, also set:

```ts
ourDisplayName = identity.displayName;
```

Verify both assignments live in the same `try` block so they always set together.

- [ ] **Step 3: Define the command registry and context builder**

Add these declarations in the `<script>` block, near the existing `pushEmoteFeedback` definition (~line 115). The registry is a constant for the component's lifetime; `buildCommandContext` captures current state via closure and is called fresh on each command dispatch.

Note: `sendChat` and `blockPlayer` are already imported from `./lib/ipc` in the long import on line 33. We need to add `unblockPlayer` and `getBlockedList` if they aren't already imported. Check with:

Run: `grep -n 'unblockPlayer\|getBlockedList' src/App.svelte`

If either is missing, add them to the existing `./lib/ipc` import. No renaming needed — the IPC names are fine to use directly.

Now add the registry, context builder, and dispatcher:

```ts
// Chat slash-command registry — constant for the component's lifetime.
const commandRegistry = createDefaultHandlers();

function buildCommandContext(): CommandContext {
  const pushBubble = (text: string) => {
    if (!ourAddressHash) return;
    window.dispatchEvent(
      new CustomEvent('harmony:local-bubble', {
        detail: { addressHash: ourAddressHash, text },
      }),
    );
  };

  return {
    remotePlayers: latestFrame?.remotePlayers ?? [],
    nearestSocialTarget: latestFrame?.nearestSocialTarget ?? null,
    buddies,
    localIdentity: {
      displayName: ourDisplayName,
      addressHash: ourAddressHash,
      setupComplete: true,
    },
    pushLocalBubble: pushBubble,
    fireEmote: (kind, target) => fireEmoteWithFeedback(kind, target, pushBubble),
    fireEmoteHi: () => fireHiWithAnimation(pushBubble),
    sendChat: (t) => sendChat(t),
    blockPlayer: (h) => blockPlayer(h),
    unblockPlayer: (h) => unblockPlayer(h),
    getBlockedList: async () => {
      const result = await getBlockedList();
      return result.blocked;
    },
  };
}

// The `pushBubble` adapter above dispatches a window CustomEvent that
// GameCanvas listens for (see Step 4). This mirrors how GameCanvas already
// receives Tauri chat_message events and keeps App.svelte decoupled from
// the renderer's lifecycle.
declare global {
  interface WindowEventMap {
    'harmony:local-bubble': CustomEvent<{ addressHash: string; text: string }>;
  }
}

async function handleChatCommand(parsed: Extract<ParsedCommand, { kind: 'command' }>) {
  await executeCommand(parsed, commandRegistry, buildCommandContext());
}
```

**No import renaming needed.** The `ctx.sendChat`, `ctx.blockPlayer`, `ctx.unblockPlayer` fields are distinct namespaces (properties of a local object) and do not shadow the top-level IPC imports. The arrow-function wrappers `(t) => sendChat(t)` call the imported IPC directly.

- [ ] **Step 4: Subscribe GameCanvas to the `harmony:local-bubble` window event**

`pushLocalBubble` dispatches a window CustomEvent (see Step 3). GameCanvas listens for it alongside its existing Tauri `chat_message` listener and forwards the payload into `renderer.addChatBubble`.

In `src/lib/components/GameCanvas.svelte`, find the existing `onMount` block where `onChatMessage` is subscribed (search for `unlistenChat`). Add a parallel window-event listener immediately after the `unlistenChat` setup:

```ts
const handleLocalBubble = (e: CustomEvent<{ addressHash: string; text: string }>) => {
  r.addChatBubble(e.detail.addressHash, e.detail.text);
};
window.addEventListener('harmony:local-bubble', handleLocalBubble as EventListener);
```

In the component's cleanup (the existing `onDestroy` or returned-from-onMount teardown that removes `unlistenChat`), add:

```ts
window.removeEventListener('harmony:local-bubble', handleLocalBubble as EventListener);
```

Verify by grepping:
Run: `grep -n 'unlistenChat\|handleLocalBubble' src/lib/components/GameCanvas.svelte`
Expected: both names appear, each with a matching cleanup call.

- [ ] **Step 5: Pass `onCommand` to ChatInput**

Find the existing `<ChatInput ... />` usage in `src/App.svelte` (search for `ChatInput` tag) and add the `onCommand` prop:

```svelte
<ChatInput onFocusChange={(f) => { chatFocused = f; }} onCommand={handleChatCommand} />
```

(Preserve whatever `onFocusChange` is currently wired to — the real value may differ from the above placeholder. Only add the `onCommand={handleChatCommand}` attribute.)

- [ ] **Step 6: Run full test suite and manual-test the wiring**

Run: `npm run test`
Expected: Full suite passes. If ChatInput.test.ts's `onCommand` mock was previously called with a generic parsed command, those tests continue to pass.

Run: `npm run build`
Expected: TypeScript compiles with no errors.

Manual smoke test (requires a running app):
Run: `npm run tauri dev`

In the running app, type the following and verify each:
1. `/dance` — local dance animation fires, palette Dance button dims with cooldown.
2. `/hug` with nobody nearby — bubble: `/hug needs a target nearby.`
3. `/xyzzy` — bubble: `Unknown command: /xyzzy. Type /help for the list.`
4. `/help` — four bubbles stack above player.
5. `//dance` — peers see literal text `/dance` in a chat bubble (no emote fires locally or remotely).
6. `hello` — normal chat path, local bubble appears with "hello".

If any manual test fails, debug the wiring before committing.

- [ ] **Step 7: Commit**

```bash
git add src/App.svelte src/lib/components/GameCanvas.svelte
git commit -m "feat(chat): wire slash-command dispatch through CommandContext"
```

---

## Task 13: Final verification and cleanup

**Files:** none modified — verification only.

- [ ] **Step 1: Run the full frontend test suite**

Run: `npm run test`
Expected: All tests pass. Note the count — should be the previous total plus the new chat module tests (~60+ new tests from commands + handlers + ChatInput).

- [ ] **Step 2: Run the build**

Run: `npm run build`
Expected: Build succeeds with no TypeScript errors or Svelte warnings.

- [ ] **Step 3: Run the Rust test suite**

No Rust changes were made, but confirm the Rust side still compiles and all tests pass:

Run: `cd src-tauri && cargo test --quiet`
Expected: All Rust tests pass (no new ones added; confirms we didn't break anything inadvertently).

- [ ] **Step 4: Full manual acceptance pass**

Run: `npm run tauri dev`

Execute this checklist in the running app. Each line should produce the described result:

```
/dance              → local dance fires, peers see it, palette Dance cooldown updates
/hug <visible>      → hug animation fires on target
/hug <ghost>        → "No player named <ghost> nearby." bubble
/hug                → if nearest exists, targets nearest; else "/hug needs a target nearby."
/hug <myName>       → "Can't hug yourself." bubble (does not fire)
/high5 / /highfive  → both work identically
/applaud            → broadcast applaud fires
/wave               → broadcasts (or targets nearest if one exists)
/wave <ghost>       → "No player named <ghost> nearby." bubble
/hi                 → Hi emote fires with today's variant
/me waves hello     → peers see "* <myName> waves hello *"
/block <visible>    → "Blocked <name>." bubble, peer goes invisible to you
/block <buddy>      → resolves, blocks
/block <ghost>      → "No player named <ghost>."
/unblock <blocked>  → "Unblocked <name>."
/unblock <ghost>    → "No player named <ghost>."
/help               → four bubbles stack above player
/xyzzy              → "Unknown command: /xyzzy. Type /help for the list."
//dance             → peers see literal "/dance" as chat text (no emote)
hello world         → normal chat path
                    → (bare enter) does nothing
```

If any item fails, fix it before proceeding to commit closure.

- [ ] **Step 5: Summary commit (if any last fixes) and close**

If the manual pass revealed nothing, no commit is needed. If fixes were made, commit them with a descriptive message. Then:

Run: `git log --oneline origin/main..HEAD`
Expected: ~12 commits on the branch, covering the plan tasks.

Run: `git diff --stat origin/main..HEAD`
Expected: Roughly:
- `src/lib/chat/commands.ts`: new, ~90 lines
- `src/lib/chat/handlers.ts`: new, ~180 lines
- `src/lib/chat/commands.test.ts`: new, ~150 lines
- `src/lib/chat/handlers.test.ts`: new, ~450 lines
- `src/lib/components/ChatInput.svelte`: ~30 lines changed
- `src/lib/components/ChatInput.test.ts`: new, ~70 lines
- `src/lib/components/GameCanvas.svelte`: ~6 lines (window event listener + cleanup)
- `src/App.svelte`: ~50 lines changed (refactor + wiring)
- `docs/superpowers/specs/2026-04-17-chat-slash-commands-design.md`: committed earlier in brainstorming
- `docs/superpowers/plans/2026-04-17-chat-slash-commands.md`: this file

The feature is complete and ready for PR.
