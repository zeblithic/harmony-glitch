// @vitest-environment node
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
