// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import PartyPanel from './PartyPanel.svelte';
import type { PartyMemberInfo } from '$lib/ipc';

const members: PartyMemberInfo[] = [
  { addressHash: 'hash1', displayName: 'Alice', isLeader: true },
  { addressHash: 'hash2', displayName: 'Bob', isLeader: false },
];

describe('PartyPanel', () => {
  it('renders member list (2 members → 2 .party-member elements)', () => {
    render(PartyPanel, {
      props: {
        inParty: true,
        members,
        isLeader: false,
        onLeave: vi.fn(),
        onKick: vi.fn(),
      },
    });
    const items = document.querySelectorAll('.party-member');
    expect(items.length).toBe(2);
  });

  it('shows leader badge (.party-member.leader exists)', () => {
    render(PartyPanel, {
      props: {
        inParty: true,
        members,
        isLeader: false,
        onLeave: vi.fn(),
        onKick: vi.fn(),
      },
    });
    const leader = document.querySelector('.party-member.leader');
    expect(leader).not.toBeNull();
  });

  it('shows kick button only for leader (1 kick button for 2 members when isLeader)', () => {
    render(PartyPanel, {
      props: {
        inParty: true,
        members,
        isLeader: true,
        onLeave: vi.fn(),
        onKick: vi.fn(),
      },
    });
    const kickBtns = document.querySelectorAll('.kick-btn');
    expect(kickBtns.length).toBe(1);
  });

  it('hides kick button for non-leader (0 kick buttons)', () => {
    render(PartyPanel, {
      props: {
        inParty: true,
        members,
        isLeader: false,
        onLeave: vi.fn(),
        onKick: vi.fn(),
      },
    });
    const kickBtns = document.querySelectorAll('.kick-btn');
    expect(kickBtns.length).toBe(0);
  });

  it('does not render when not in party (.party-panel is null)', () => {
    render(PartyPanel, {
      props: {
        inParty: false,
        members,
        isLeader: false,
        onLeave: vi.fn(),
        onKick: vi.fn(),
      },
    });
    const panel = document.querySelector('.party-panel');
    expect(panel).toBeNull();
  });
});
