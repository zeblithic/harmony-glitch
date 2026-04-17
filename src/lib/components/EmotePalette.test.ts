// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import EmotePalette from './EmotePalette.svelte';

HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) {
  this.setAttribute('open', '');
});
HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) {
  this.removeAttribute('open');
});

describe('EmotePalette', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders all 6 emote buttons when visible', () => {
    const onSelect = vi.fn();
    render(EmotePalette, {
      props: {
        visible: true,
        onClose: vi.fn(),
        onSelect,
        cooldowns: {},
        nearestTarget: null,
        privacy: { hug: true, high_five: true },
      },
    });

    const buttons = screen.getAllByRole('button');
    expect(buttons.length).toBeGreaterThanOrEqual(6);
    expect(screen.getByText(/\bHi\b/i)).toBeDefined();
    expect(screen.getByText(/Dance/i)).toBeDefined();
    expect(screen.getByText(/Wave/i)).toBeDefined();
    expect(screen.getByText(/Hug/i)).toBeDefined();
    expect(screen.getByText(/High.?Five/i)).toBeDefined();
    expect(screen.getByText(/Applaud/i)).toBeDefined();
  });

  it('calls onSelect with dance kind when button 2 is clicked', async () => {
    const onSelect = vi.fn();
    render(EmotePalette, {
      props: {
        visible: true,
        onClose: vi.fn(),
        onSelect,
        cooldowns: {},
        nearestTarget: 'abcd'.padEnd(32, '0'),
        privacy: { hug: true, high_five: true },
      },
    });

    const danceBtn = screen.getByRole('button', { name: /Dance/i });
    await fireEvent.click(danceBtn);
    expect(onSelect).toHaveBeenCalledWith('dance');
  });

  it('dims hug button when no target in range', () => {
    render(EmotePalette, {
      props: {
        visible: true,
        onClose: vi.fn(),
        onSelect: vi.fn(),
        cooldowns: {},
        nearestTarget: null,
        privacy: { hug: true, high_five: true },
      },
    });

    const hugBtn = screen.getByRole('button', { name: /Hug/i });
    expect(hugBtn.hasAttribute('disabled')).toBe(true);
  });

  it('does not disable high_five button when privacy is permissive', () => {
    render(EmotePalette, {
      props: {
        visible: true,
        onClose: vi.fn(),
        onSelect: vi.fn(),
        cooldowns: {},
        nearestTarget: 'abcd'.padEnd(32, '0'),
        privacy: { hug: true, high_five: true },
      },
    });

    const hiFiveBtn = screen.getByRole('button', { name: /High.?Five/i });
    expect(hiFiveBtn.hasAttribute('disabled')).toBe(false);
  });

  it('shows cooldown countdown text when cooldowns prop has a kind entry', () => {
    render(EmotePalette, {
      props: {
        visible: true,
        onClose: vi.fn(),
        onSelect: vi.fn(),
        cooldowns: { hug: 45_000 },
        nearestTarget: 'abcd'.padEnd(32, '0'),
        privacy: { hug: true, high_five: true },
      },
    });

    const hugBtn = screen.getByRole('button', { name: /Hug/i });
    expect(hugBtn.hasAttribute('disabled')).toBe(true);
    expect(hugBtn.textContent).toMatch(/45/);
  });

  it('calls onClose when Escape is pressed', async () => {
    const onClose = vi.fn();
    render(EmotePalette, {
      props: {
        visible: true,
        onClose,
        onSelect: vi.fn(),
        cooldowns: {},
        nearestTarget: null,
        privacy: { hug: true, high_five: true },
      },
    });

    const dialog = screen.getByRole('dialog');
    await fireEvent.keyDown(dialog, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('does not render when visible is false', () => {
    render(EmotePalette, {
      props: {
        visible: false,
        onClose: vi.fn(),
        onSelect: vi.fn(),
        cooldowns: {},
        nearestTarget: null,
        privacy: { hug: true, high_five: true },
      },
    });

    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
