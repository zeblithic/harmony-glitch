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
