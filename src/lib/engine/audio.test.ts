import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { AudioEvent } from '../types';

// Mock howler before importing AudioManager
vi.mock('howler', () => {
  const mockHowl = vi.fn().mockImplementation(function () {
    return {
      play: vi.fn(),
      stop: vi.fn(),
      fade: vi.fn(),
      volume: vi.fn().mockReturnValue(1.0),
      loop: vi.fn(),
      unload: vi.fn(),
    };
  });
  return {
    Howl: mockHowl,
    Howler: { ctx: { state: 'running', resume: vi.fn() } },
  };
});

import { AudioManager, kitBasePath } from './audio';
import type { SoundKit } from './audio';
import { Howl, Howler } from 'howler';

function makeKit(): SoundKit {
  return {
    name: 'Test',
    version: 1,
    sfxVolume: 1.0,
    ambientVolume: 0.5,
    events: {
      itemPickup: {
        default: 'sfx/pick-up.mp3',
        variants: { cherry: 'sfx/cherry-pick.mp3' },
      },
      jump: { default: 'sfx/jump.mp3' },
      actionFailed: { default: 'sfx/fail.mp3' },
      transitionStart: { default: 'sfx/transition-start.mp3' },
      transitionComplete: { default: 'sfx/transition-complete.mp3' },
      entityInteract: { default: 'sfx/interact.mp3' },
    },
    ambient: {
      default: 'ambient/default.mp3',
      variants: { LADEMO001: 'ambient/meadow.mp3' },
    },
  };
}

/**
 * Find the Howl mock instance whose src[0] contains the given substring.
 * AudioManager preloads all sounds during construction, so instances are
 * created before processEvents is ever called.
 */
function findHowlBySrc(substr: string): ReturnType<typeof vi.fn> | undefined {
  const calls = vi.mocked(Howl).mock.calls;
  const results = vi.mocked(Howl).mock.results;
  const idx = calls.findIndex((c) => (c[0] as { src: string[] }).src[0].includes(substr));
  if (idx < 0) return undefined;
  return results[idx].value as ReturnType<typeof vi.fn>;
}

describe('AudioManager', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
  });

  it('resolves variant sound when available', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    const events: AudioEvent[] = [{ type: 'itemPickup', itemId: 'cherry' }];
    manager.processEvents(events);

    const cherryHowl = findHowlBySrc('cherry-pick');
    expect(cherryHowl).toBeDefined();
    expect(cherryHowl!.play).toHaveBeenCalled();
  });

  it('falls back to default when no variant matches', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    const events: AudioEvent[] = [{ type: 'itemPickup', itemId: 'wood' }];

    // Should not throw
    expect(() => manager.processEvents(events)).not.toThrow();

    // The default pick-up.mp3 howl should have play() called
    const pickupHowl = findHowlBySrc('pick-up');
    expect(pickupHowl).toBeDefined();
    expect(pickupHowl!.play).toHaveBeenCalled();
  });

  it('plays SFX on jump event', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    manager.processEvents([{ type: 'jump' }]);

    const jumpHowl = findHowlBySrc('jump');
    expect(jumpHowl).toBeDefined();
    expect(jumpHowl!.play).toHaveBeenCalled();
  });

  it('starts ambient on streetChanged without transition', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    manager.processEvents([{ type: 'streetChanged', streetId: 'LADEMO001' }]);

    // The meadow ambient Howl should have loop(true) and play() called
    const meadowHowl = findHowlBySrc('meadow');
    expect(meadowHowl).toBeDefined();
    expect(meadowHowl!.loop).toHaveBeenCalledWith(true);
    expect(meadowHowl!.play).toHaveBeenCalled();
  });

  it('fades out ambient on transitionStart', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    // Start ambient first
    manager.processEvents([{ type: 'streetChanged', streetId: 'LADEMO001' }]);
    // Then transition
    manager.processEvents([{ type: 'transitionStart' }]);

    const meadowHowl = findHowlBySrc('meadow');
    expect(meadowHowl).toBeDefined();
    expect(meadowHowl!.fade).toHaveBeenCalled();
  });

  it('dispose unloads all preloaded sounds', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    manager.dispose();

    const howlInstances = vi.mocked(Howl).mock.results
      .filter((r) => r.type === 'return')
      .map((r) => r.value as ReturnType<typeof vi.fn>);

    expect(howlInstances.length).toBeGreaterThan(0);
    const allUnloaded = howlInstances.every((h) => h.unload.mock.calls.length > 0);
    expect(allUnloaded).toBe(true);
  });

  it('setVolume adjusts sfx volume used on subsequent SFX plays', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    // Start ambient so we have a currentAmbient (verifies channel isolation)
    manager.processEvents([{ type: 'streetChanged', streetId: 'LADEMO001' }]);

    // Change SFX volume
    manager.setVolume('sfx', 0.3);

    // Play an SFX — it should use the new volume
    manager.processEvents([{ type: 'jump' }]);

    const jumpHowl = findHowlBySrc('jump');
    expect(jumpHowl).toBeDefined();
    // volume() is called by playSfx with the current sfxVolume
    expect(jumpHowl!.volume).toHaveBeenCalledWith(0.3);
  });

  it('handles missing event type gracefully', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    // entityInteract is in the kit but with no variant for "unknown_entity"
    // Should fall back to default without throwing
    expect(() => {
      manager.processEvents([{ type: 'entityInteract', entityType: 'unknown_entity' }]);
    }).not.toThrow();

    const interactHowl = findHowlBySrc('interact');
    expect(interactHowl).toBeDefined();
    expect(interactHowl!.play).toHaveBeenCalled();
  });

  it('stops previous ambient when street changes', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    // Start with meadow ambient
    manager.processEvents([{ type: 'streetChanged', streetId: 'LADEMO001' }]);
    // Switch to a street with default ambient
    manager.processEvents([{ type: 'streetChanged', streetId: 'UNKNOWN_STREET' }]);

    const meadowHowl = findHowlBySrc('meadow');
    expect(meadowHowl).toBeDefined();
    // stop() should have been called on the previous ambient
    expect(meadowHowl!.stop).toHaveBeenCalled();
  });

  it('does not stop ambient when switching to street with same audio file', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    // Start with an unknown street (falls back to default ambient)
    manager.processEvents([{ type: 'streetChanged', streetId: 'UNKNOWN_A' }]);
    const defaultHowl = findHowlBySrc('ambient/default');
    expect(defaultHowl).toBeDefined();
    expect(defaultHowl!.play).toHaveBeenCalledTimes(1);

    // Switch to another unknown street (also falls back to default ambient — same Howl)
    manager.processEvents([{ type: 'streetChanged', streetId: 'UNKNOWN_B' }]);

    // Same Howl instance — stop() should NOT have been called, play() not called again
    expect(defaultHowl!.stop).not.toHaveBeenCalled();
    expect(defaultHowl!.play).toHaveBeenCalledTimes(1);
  });

  it('cancels fade-out when transitioning to street with same ambient', () => {
    const manager = new AudioManager(makeKit(), '/audio/');
    // Start ambient
    manager.processEvents([{ type: 'streetChanged', streetId: 'UNKNOWN_A' }]);

    // Start transition (fade out)
    manager.processEvents([{ type: 'transitionStart' }]);

    const defaultHowl = findHowlBySrc('ambient/default');
    expect(defaultHowl!.fade).toHaveBeenCalledTimes(1); // fade out

    // Switch to another street with same ambient during transition
    manager.processEvents([{ type: 'streetChanged', streetId: 'UNKNOWN_B' }]);

    // Should NOT have called stop() — same Howl instance
    expect(defaultHowl!.stop).not.toHaveBeenCalled();
    // Should have called fade again (recovery fade back to ambient volume)
    expect(defaultHowl!.fade).toHaveBeenCalledTimes(2);
  });

  it('resumes audio context on first processEvents call', () => {
    // Override Howler ctx to simulate suspended state
    const resumeFn = vi.fn();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (Howler as any).ctx = { state: 'suspended', resume: resumeFn };

    const manager = new AudioManager(makeKit(), '/audio/');
    manager.processEvents([{ type: 'jump' }]);

    expect(resumeFn).toHaveBeenCalled();
  });

  describe('getVolume / setMuted / isMuted', () => {
    it('getVolume returns kit defaults initially', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      expect(manager.getVolume('sfx')).toBe(1.0);
      expect(manager.getVolume('ambient')).toBe(0.5);
    });

    it('getVolume reflects setVolume changes', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      manager.setVolume('sfx', 0.3);
      manager.setVolume('ambient', 0.8);
      expect(manager.getVolume('sfx')).toBe(0.3);
      expect(manager.getVolume('ambient')).toBe(0.8);
    });

    it('isMuted returns false initially', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      expect(manager.isMuted('sfx')).toBe(false);
      expect(manager.isMuted('ambient')).toBe(false);
    });

    it('setMuted toggles mute state', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      manager.setMuted('sfx', true);
      expect(manager.isMuted('sfx')).toBe(true);
      expect(manager.isMuted('ambient')).toBe(false);

      manager.setMuted('ambient', true);
      expect(manager.isMuted('ambient')).toBe(true);
    });

    it('muted SFX plays at volume 0', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      manager.setMuted('sfx', true);
      manager.processEvents([{ type: 'jump' }]);

      const jumpHowl = findHowlBySrc('jump');
      expect(jumpHowl!.volume).toHaveBeenCalledWith(0);
    });

    it('muted ambient sets currentAmbient volume to 0', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      manager.processEvents([{ type: 'streetChanged', streetId: 'LADEMO001' }]);

      const meadowHowl = findHowlBySrc('meadow');
      manager.setMuted('ambient', true);
      expect(meadowHowl!.volume).toHaveBeenCalledWith(0);
    });

    it('unmuting ambient restores volume', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      manager.setVolume('ambient', 0.6);
      manager.processEvents([{ type: 'streetChanged', streetId: 'LADEMO001' }]);

      manager.setMuted('ambient', true);
      manager.setMuted('ambient', false);

      const meadowHowl = findHowlBySrc('meadow');
      // Last volume call should be the restored ambient volume
      const volumeCalls = meadowHowl!.volume.mock.calls;
      expect(volumeCalls[volumeCalls.length - 1][0]).toBe(0.6);
    });

    it('getPreferences returns current state', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      manager.setVolume('sfx', 0.7);
      manager.setMuted('ambient', true);

      const prefs = manager.getPreferences();
      expect(prefs.sfxVolume).toBe(0.7);
      expect(prefs.ambientVolume).toBe(0.5);
      expect(prefs.sfxMuted).toBe(false);
      expect(prefs.ambientMuted).toBe(true);
    });
  });

  describe('localStorage persistence', () => {
    it('saves preferences to localStorage on setVolume', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      manager.setVolume('sfx', 0.4);

      const stored = JSON.parse(localStorage.getItem('audio-prefs')!);
      expect(stored.sfxVolume).toBe(0.4);
    });

    it('saves preferences to localStorage on setMuted', () => {
      const manager = new AudioManager(makeKit(), '/audio/');
      manager.setMuted('sfx', true);

      const stored = JSON.parse(localStorage.getItem('audio-prefs')!);
      expect(stored.sfxMuted).toBe(true);
    });

    it('restores preferences from localStorage on construction', () => {
      localStorage.setItem('audio-prefs', JSON.stringify({
        sfxVolume: 0.4,
        ambientVolume: 0.2,
        sfxMuted: true,
        ambientMuted: false,
      }));

      const manager = new AudioManager(makeKit(), '/audio/');
      expect(manager.getVolume('sfx')).toBe(0.4);
      expect(manager.getVolume('ambient')).toBe(0.2);
      expect(manager.isMuted('sfx')).toBe(true);
      expect(manager.isMuted('ambient')).toBe(false);
    });

    it('clamps out-of-range volume values from localStorage', () => {
      localStorage.setItem('audio-prefs', JSON.stringify({
        sfxVolume: 2.0,
        ambientVolume: -0.5,
        sfxMuted: false,
        ambientMuted: false,
      }));

      const manager = new AudioManager(makeKit(), '/audio/');
      expect(manager.getVolume('sfx')).toBe(1.0);
      expect(manager.getVolume('ambient')).toBe(0.0);
    });

    it('falls back to defaults for invalid localStorage data', () => {
      localStorage.setItem('audio-prefs', JSON.stringify({
        sfxVolume: 'not a number',
        ambientVolume: null,
        sfxMuted: 'false',
        ambientMuted: 42,
      }));

      const manager = new AudioManager(makeKit(), '/audio/');
      expect(manager.getVolume('sfx')).toBe(1.0);
      expect(manager.getVolume('ambient')).toBe(0.5);
      expect(manager.isMuted('sfx')).toBe(false);
      expect(manager.isMuted('ambient')).toBe(false);
    });

    it('handles corrupt JSON in localStorage gracefully', () => {
      localStorage.setItem('audio-prefs', '{not valid json');

      let manager!: AudioManager;
      expect(() => {
        manager = new AudioManager(makeKit(), '/audio/');
      }).not.toThrow();
      expect(manager.getVolume('sfx')).toBe(1.0);
    });
  });
});

describe('kitBasePath', () => {
  it('returns /assets/audio/ for default kit', () => {
    expect(kitBasePath('default')).toBe('/assets/audio/');
  });

  it('returns soundkit:// URL for custom kit', () => {
    expect(kitBasePath('retro-kit')).toBe('soundkit://localhost/retro-kit/');
  });
});
