// @vitest-environment node
import { describe, it, expect } from 'vitest';
import { resolvePlayerName, hiHandler, danceHandler, applaudHandler, waveHandler, hugHandler, high5Handler, blockHandler, unblockHandler, meHandler } from './handlers';
import type { RemotePlayerFrame, NearestSocialTarget } from '$lib/types';
import type { BuddyEntry } from '$lib/ipc';
import type { CommandContext } from './commands';

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

  it('resolves explicit name from remotePlayers', async () => {
    const calls: Array<[string, string | null]> = [];
    await high5Handler(
      'Alice',
      makeContext({
        remotePlayers: [remote('Alice', '11'.repeat(16))],
        fireEmote: async (kind, target) => calls.push([kind as string, target]),
      }),
    );
    expect(calls).toEqual([['high_five', '11'.repeat(16)]]);
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
