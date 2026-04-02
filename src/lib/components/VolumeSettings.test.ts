// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import VolumeSettings from './VolumeSettings.svelte';

// Mock dialog showModal/close (jsdom doesn't support)
HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) {
  this.setAttribute('open', '');
});
HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) {
  this.removeAttribute('open');
});

function makeAudioManager(overrides?: Record<string, unknown>) {
  return {
    getVolume: vi.fn((channel: string) => channel === 'sfx' ? 1.0 : 0.5),
    isMuted: vi.fn().mockReturnValue(false),
    setVolume: vi.fn(),
    setMuted: vi.fn(),
    ...overrides,
  };
}

describe('VolumeSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders nothing when not visible', () => {
    render(VolumeSettings, {
      props: { audioManager: makeAudioManager(), visible: false },
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('renders dialog with volume label when visible', () => {
    render(VolumeSettings, {
      props: { audioManager: makeAudioManager(), visible: true },
    });
    const dialog = screen.getByRole('dialog');
    expect(dialog).toBeDefined();
    expect(dialog.textContent).toContain('Volume');
  });

  it('renders SFX and Ambient sliders', () => {
    render(VolumeSettings, {
      props: { audioManager: makeAudioManager(), visible: true },
    });
    const sfxSlider = screen.getByLabelText('SFX');
    const ambientSlider = screen.getByLabelText('Ambient');
    expect(sfxSlider).toBeDefined();
    expect(ambientSlider).toBeDefined();
  });

  it('reads initial values from audioManager', () => {
    const am = makeAudioManager();
    render(VolumeSettings, {
      props: { audioManager: am, visible: true },
    });
    expect(am.getVolume).toHaveBeenCalledWith('sfx');
    expect(am.getVolume).toHaveBeenCalledWith('ambient');
    expect(am.isMuted).toHaveBeenCalledWith('sfx');
    expect(am.isMuted).toHaveBeenCalledWith('ambient');
  });

  it('calls setVolume when SFX slider changes', async () => {
    const am = makeAudioManager();
    render(VolumeSettings, {
      props: { audioManager: am, visible: true },
    });
    const sfxSlider = screen.getByLabelText('SFX') as HTMLInputElement;
    // Simulate slider change
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!
      .set!.call(sfxSlider, '0.7');
    await fireEvent.input(sfxSlider);
    expect(am.setVolume).toHaveBeenCalledWith('sfx', 0.7);
  });

  it('calls setVolume when Ambient slider changes', async () => {
    const am = makeAudioManager();
    render(VolumeSettings, {
      props: { audioManager: am, visible: true },
    });
    const ambientSlider = screen.getByLabelText('Ambient') as HTMLInputElement;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!
      .set!.call(ambientSlider, '0.3');
    await fireEvent.input(ambientSlider);
    expect(am.setVolume).toHaveBeenCalledWith('ambient', 0.3);
  });

  it('toggles SFX mute on button click', async () => {
    const am = makeAudioManager();
    render(VolumeSettings, {
      props: { audioManager: am, visible: true },
    });
    const muteBtn = screen.getByRole('button', { name: 'Mute SFX' });
    await fireEvent.click(muteBtn);
    expect(am.setMuted).toHaveBeenCalledWith('sfx', true);
  });

  it('toggles Ambient mute on button click', async () => {
    const am = makeAudioManager();
    render(VolumeSettings, {
      props: { audioManager: am, visible: true },
    });
    const muteBtn = screen.getByRole('button', { name: 'Mute ambient' });
    await fireEvent.click(muteBtn);
    expect(am.setMuted).toHaveBeenCalledWith('ambient', true);
  });

  it('shows mute buttons with aria-pressed', () => {
    const am = makeAudioManager();
    render(VolumeSettings, {
      props: { audioManager: am, visible: true },
    });
    const muteBtn = screen.getByRole('button', { name: /mute sfx/i });
    expect(muteBtn.getAttribute('aria-pressed')).toBe('false');
  });

  it('calls onClose when close button is clicked', async () => {
    const onClose = vi.fn();
    render(VolumeSettings, {
      props: { audioManager: makeAudioManager(), visible: true, onClose },
    });
    const closeBtn = screen.getByRole('button', { name: /close/i });
    await fireEvent.click(closeBtn);
    expect(onClose).toHaveBeenCalled();
  });

  it('handles null audioManager gracefully', () => {
    expect(() => {
      render(VolumeSettings, {
        props: { audioManager: null, visible: true },
      });
    }).not.toThrow();
  });
});
