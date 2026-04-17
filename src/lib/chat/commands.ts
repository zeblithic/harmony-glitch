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
