// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import BuddyListPanel from './BuddyListPanel.svelte';
import type { BuddyEntry } from '$lib/ipc';

const buddies: BuddyEntry[] = [
  {
    addressHash: 'hash1',
    displayName: 'Alice',
    addedDate: '2024-01-01',
    coPresenceTotal: 120,
    lastSeenDate: '2024-01-10',
  },
  {
    addressHash: 'hash2',
    displayName: 'Bob',
    addedDate: '2024-01-02',
    coPresenceTotal: 3600,
    lastSeenDate: null,
  },
];

describe('BuddyListPanel', () => {
  it('renders buddy list (2 buddies → 2 .buddy-entry elements)', () => {
    render(BuddyListPanel, {
      props: {
        buddies,
        visible: true,
        onRemove: vi.fn(),
        onBlock: vi.fn(),
      },
    });
    const entries = document.querySelectorAll('.buddy-entry');
    expect(entries.length).toBe(2);
  });

  it('shows buddy display names', () => {
    render(BuddyListPanel, {
      props: {
        buddies,
        visible: true,
        onRemove: vi.fn(),
        onBlock: vi.fn(),
      },
    });
    const names = document.querySelectorAll('.buddy-name');
    const nameTexts = Array.from(names).map(n => n.textContent);
    expect(nameTexts).toContain('Alice');
    expect(nameTexts).toContain('Bob');
  });

  it('does not render when not visible', () => {
    render(BuddyListPanel, {
      props: {
        buddies,
        visible: false,
        onRemove: vi.fn(),
        onBlock: vi.fn(),
      },
    });
    const panel = document.querySelector('.buddy-list-panel');
    expect(panel).toBeNull();
  });

  it('shows empty state when no buddies', () => {
    render(BuddyListPanel, {
      props: {
        buddies: [],
        visible: true,
        onRemove: vi.fn(),
        onBlock: vi.fn(),
      },
    });
    const empty = document.querySelector('.buddy-empty');
    expect(empty).not.toBeNull();
    expect(empty?.textContent).toContain('No buddies yet');
  });

  it('formats co-presence time (3600s → "1h")', () => {
    render(BuddyListPanel, {
      props: {
        buddies,
        visible: true,
        onRemove: vi.fn(),
        onBlock: vi.fn(),
      },
    });
    const copresences = document.querySelectorAll('.buddy-copresence');
    const texts = Array.from(copresences).map(el => el.textContent);
    expect(texts).toContain('1h');
  });
});
