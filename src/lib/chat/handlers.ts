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

export const hiHandler: CommandHandler = async (_args, ctx) => {
  await ctx.fireEmoteHi();
};

export const danceHandler: CommandHandler = async (_args, ctx) => {
  await ctx.fireEmote('dance', null);
};

export const applaudHandler: CommandHandler = async (_args, ctx) => {
  await ctx.fireEmote('applaud', null);
};

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
  // Resolve against visible + buddies to match /block and /unblock; if the
  // target isn't in range, the emote IPC is the final gate (returns no_target).
  const resolved = resolvePlayerName(name, {
    remotePlayers: ctx.remotePlayers,
    buddies: ctx.buddies,
  });
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
  const resolved = resolvePlayerName(name, {
    remotePlayers: ctx.remotePlayers,
    buddies: ctx.buddies,
  });
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
  const resolved = resolvePlayerName(name, {
    remotePlayers: ctx.remotePlayers,
    buddies: ctx.buddies,
  });
  if (!resolved) {
    ctx.pushLocalBubble(`No player named ${name} nearby.`);
    return;
  }
  await ctx.fireEmote('high_five', resolved.hash);
};

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

export const meHandler: CommandHandler = async (args, ctx) => {
  // trim() only strips leading/trailing whitespace; internal spacing
  // (e.g., "waves  hello") is preserved so peers see what was typed.
  const action = args.trim();
  if (action === '') {
    ctx.pushLocalBubble('Usage: /me <action>');
    return;
  }
  await ctx.sendChat(`* ${ctx.localIdentity.displayName} ${action} *`);
};

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
