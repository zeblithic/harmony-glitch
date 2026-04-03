// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import JukeboxPanel from './JukeboxPanel.svelte';
import type { JukeboxInfo } from '../types';

// Mock dialog showModal/close (jsdom doesn't support)
HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) {
  this.setAttribute('open', '');
});
HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) {
  this.removeAttribute('open');
});

function makeJukeboxInfo(overrides?: Partial<JukeboxInfo>): JukeboxInfo {
  return {
    entityId: 'jukebox-1',
    name: 'Street Jukebox',
    playlist: [
      { id: 'track-a', title: 'Song Alpha', artist: 'Artist A', durationSecs: 180 },
      { id: 'track-b', title: 'Song Beta', artist: 'Artist B', durationSecs: 240 },
      { id: 'track-c', title: 'Song Gamma', artist: 'Artist C', durationSecs: 120 },
    ],
    currentTrackIndex: 1,
    playing: true,
    elapsedSecs: 45,
    ...overrides,
  };
}

describe('JukeboxPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders track list when visible with info', () => {
    const info = makeJukeboxInfo();
    render(JukeboxPanel, {
      props: { info, visible: true },
    });

    const listbox = screen.getByRole('listbox', { name: 'Track list' });
    expect(listbox).toBeDefined();

    const options = screen.getAllByRole('option');
    expect(options).toHaveLength(3);
    expect(options[0].textContent).toContain('Song Alpha');
    expect(options[1].textContent).toContain('Song Beta');
    expect(options[2].textContent).toContain('Song Gamma');
  });

  it('shows empty message when playlist is empty', () => {
    const info = makeJukeboxInfo({ playlist: [], currentTrackIndex: -1 });
    render(JukeboxPanel, {
      props: { info, visible: true },
    });

    expect(screen.queryByRole('listbox')).toBeNull();
    expect(screen.getByText('No tracks available')).toBeDefined();
  });

  it('highlights current track', () => {
    const info = makeJukeboxInfo({ currentTrackIndex: 1 });
    render(JukeboxPanel, {
      props: { info, visible: true },
    });

    const options = screen.getAllByRole('option');
    // The second track (index 1) should have aria-selected and the active class
    expect(options[1].getAttribute('aria-selected')).toBe('true');
    expect(options[1].classList.contains('active')).toBe(true);

    // Other tracks should not be active
    expect(options[0].getAttribute('aria-selected')).toBe('false');
    expect(options[0].classList.contains('active')).toBe(false);
  });

  it('does not render when not visible', () => {
    const info = makeJukeboxInfo();
    render(JukeboxPanel, {
      props: { info, visible: false },
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('has accessible dialog label', () => {
    const info = makeJukeboxInfo({ name: 'Groovy Jukebox' });
    render(JukeboxPanel, {
      props: { info, visible: true },
    });
    const dialog = screen.getByRole('dialog');
    expect(dialog.getAttribute('aria-label')).toBe('Jukebox: Groovy Jukebox');
  });

  it('does not render when info is null even if visible', () => {
    render(JukeboxPanel, {
      props: { info: null, visible: true },
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('renders play/pause button with correct label', () => {
    const info = makeJukeboxInfo({ playing: true });
    render(JukeboxPanel, {
      props: { info, visible: true },
    });
    expect(screen.getByRole('button', { name: 'Pause' })).toBeDefined();
  });

  it('renders play button when not playing', () => {
    const info = makeJukeboxInfo({ playing: false });
    render(JukeboxPanel, {
      props: { info, visible: true },
    });
    expect(screen.getByRole('button', { name: 'Play' })).toBeDefined();
  });

  it('renders progress bar', () => {
    const info = makeJukeboxInfo({ elapsedSecs: 60, currentTrackIndex: 0 });
    render(JukeboxPanel, {
      props: { info, visible: true },
    });
    const progressBar = screen.getByRole('progressbar', { name: 'Track progress' });
    expect(progressBar).toBeDefined();
    expect(progressBar.getAttribute('aria-valuenow')).toBe('60');
    expect(progressBar.getAttribute('aria-valuemax')).toBe('180');
  });
});
