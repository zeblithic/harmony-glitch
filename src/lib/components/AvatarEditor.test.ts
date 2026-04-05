// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import AvatarEditor from './AvatarEditor.svelte';
import type { AvatarManifest } from '../types';

// Mock dialog showModal/close (jsdom doesn't support)
HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) {
  this.setAttribute('open', '');
});
HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) {
  this.removeAttribute('open');
});

// Mock IPC
vi.mock('../ipc', () => ({
  getAvatar: vi.fn().mockResolvedValue({
    eyes: 'eyes_01', ears: 'ears_0001', nose: 'nose_0001',
    mouth: 'mouth_01', hair: 'Buzzcut',
    skinColor: 'D4C159', hairColor: '4A3728',
    hat: null, coat: null, shirt: 'Bandana_Tank',
    pants: 'Boardwalk_Empire_ladies_pants', dress: null,
    skirt: null, shoes: 'Men_DressShoes', bracelet: null,
  }),
  setAvatar: vi.fn().mockResolvedValue({}),
}));

const testManifest: AvatarManifest = {
  categories: {
    eyes: { items: [
      { id: 'eyes_01', name: 'Classic', sheet: 'eyes/eyes_01.json' },
      { id: 'eyes_02', name: 'Round', sheet: 'eyes/eyes_02.json' },
    ]},
    ears: { items: [
      { id: 'ears_0001', name: 'Default', sheet: 'ears/ears_0001.json' },
    ]},
    nose: { items: [
      { id: 'nose_0001', name: 'Default', sheet: 'nose/nose_0001.json' },
    ]},
    mouth: { items: [
      { id: 'mouth_01', name: 'Default', sheet: 'mouth/mouth_01.json' },
    ]},
    hair: { items: [
      { id: 'Buzzcut', name: 'Buzzcut', sheet: 'hair/Buzzcut.json' },
    ]},
    hat: { items: [
      { id: 'top_hat', name: 'Top Hat', sheet: 'hat/top_hat.json' },
    ]},
    shirt: { items: [
      { id: 'Bandana_Tank', name: 'Bandana Tank', sheet: 'shirt/Bandana_Tank.json' },
    ]},
  },
  defaults: { eyes: 'eyes_01', ears: 'ears_0001' },
};

function makeRenderer() {
  return {
    applyAppearance: vi.fn().mockResolvedValue(undefined),
  };
}

describe('AvatarEditor', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders nothing when not visible', () => {
    render(AvatarEditor, {
      props: { visible: false, manifest: testManifest, renderer: null },
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('renders dialog with Avatar Editor label when visible', () => {
    render(AvatarEditor, {
      props: { visible: true, manifest: testManifest, renderer: makeRenderer() as any },
    });
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeDefined();
    expect(dialog.getAttribute('aria-labelledby')).toBe('avatar-editor-title');
    expect(screen.getByText('Avatar Editor')).toBeDefined();
  });

  it('renders 4 top-level tabs', () => {
    render(AvatarEditor, {
      props: { visible: true, manifest: testManifest, renderer: makeRenderer() as any },
    });
    const tabs = screen.getAllByRole('tab');
    const groupTabs = tabs.filter(t => ['Face', 'Hair', 'Body', 'Clothes'].includes(t.textContent ?? ''));
    expect(groupTabs.length).toBe(4);
  });

  it('defaults to Face tab with eyes items visible', async () => {
    render(AvatarEditor, {
      props: { visible: true, manifest: testManifest, renderer: makeRenderer() as any },
    });
    await new Promise(r => setTimeout(r, 0));
    expect(screen.getByText('Classic')).toBeDefined();
    expect(screen.getByText('Round')).toBeDefined();
  });

  it('shows None option for wardrobe categories', async () => {
    render(AvatarEditor, {
      props: { visible: true, manifest: testManifest, renderer: makeRenderer() as any },
    });
    // Switch to Clothes tab, then hat sub-category
    await fireEvent.click(screen.getByText('Clothes'));
    expect(screen.getByText('None')).toBeDefined();
  });

  it('does not show None option for vanity categories', async () => {
    render(AvatarEditor, {
      props: { visible: true, manifest: testManifest, renderer: makeRenderer() as any },
    });
    // Default is Face > eyes — vanity slot, no None
    await new Promise(r => setTimeout(r, 0));
    expect(screen.queryByText('None')).toBeNull();
  });

  it('renders skin color swatches in Body tab', async () => {
    render(AvatarEditor, {
      props: { visible: true, manifest: testManifest, renderer: makeRenderer() as any },
    });
    await fireEvent.click(screen.getByText('Body'));
    const swatches = screen.getAllByRole('radio');
    expect(swatches.length).toBeGreaterThan(5);
  });

  it('renders hair color swatches in Hair tab', async () => {
    render(AvatarEditor, {
      props: { visible: true, manifest: testManifest, renderer: makeRenderer() as any },
    });
    await fireEvent.click(screen.getByText('Hair'));
    const swatches = screen.getAllByRole('radio');
    expect(swatches.length).toBeGreaterThan(5);
  });

  it('calls setAvatar on save', async () => {
    const { setAvatar } = await import('../ipc');
    const onClose = vi.fn();
    render(AvatarEditor, {
      props: { visible: true, manifest: testManifest, renderer: makeRenderer() as any, onClose },
    });
    await new Promise(r => setTimeout(r, 0));
    await fireEvent.click(screen.getByText('Save'));
    await new Promise(r => setTimeout(r, 0));
    expect(setAvatar).toHaveBeenCalled();
  });

  it('handles null manifest gracefully', () => {
    render(AvatarEditor, {
      props: { visible: true, manifest: null, renderer: makeRenderer() as any },
    });
    expect(screen.getByRole('dialog')).toBeDefined();
  });

  it('handles null renderer gracefully', () => {
    render(AvatarEditor, {
      props: { visible: true, manifest: testManifest, renderer: null },
    });
    expect(screen.getByRole('dialog')).toBeDefined();
  });

  it('shows first-run UI when firstRun is true', async () => {
    render(AvatarEditor, {
      props: { visible: true, firstRun: true, manifest: testManifest, renderer: null },
    });
    const heading = screen.getByText('Customize Your Glitchen');
    expect(heading).toBeDefined();
    expect(heading.id).toBe('avatar-editor-title');
    expect(screen.getByText('Continue')).toBeDefined();
    expect(screen.getByText('Skip')).toBeDefined();
    // No close button in first-run mode
    expect(screen.queryByLabelText('Close avatar editor')).toBeNull();
  });

  it('skip advances even before avatar loads in first-run mode', async () => {
    const { getAvatar } = await import('../ipc');
    // Make getAvatar hang forever to simulate pending load
    (getAvatar as ReturnType<typeof vi.fn>).mockReturnValue(new Promise(() => {}));
    const onClose = vi.fn();
    render(AvatarEditor, {
      props: { visible: true, firstRun: true, manifest: testManifest, renderer: null, onClose },
    });
    // pendingAppearance is null because getAvatar hasn't resolved
    await fireEvent.click(screen.getByText('Skip'));
    expect(onClose).toHaveBeenCalled();
  });

  it('shows standard UI when firstRun is false', () => {
    render(AvatarEditor, {
      props: { visible: true, firstRun: false, manifest: testManifest, renderer: makeRenderer() as any },
    });
    expect(screen.queryByText('Customize Your Glitchen')).toBeNull();
    expect(screen.getByText('Save')).toBeDefined();
    expect(screen.getByText('Cancel')).toBeDefined();
    expect(screen.getByLabelText('Close avatar editor')).toBeDefined();
  });
});
