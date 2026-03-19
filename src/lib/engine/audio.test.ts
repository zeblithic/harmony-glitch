import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { AudioEvent } from '../types';

// Mock howler before importing AudioManager
vi.mock('howler', () => {
  const mockHowl = vi.fn().mockImplementation(function () {
    return {
      play: vi.fn(),
      stop: vi.fn(),
      fade: vi.fn(),
      volume: vi.fn(),
      loop: vi.fn(),
      unload: vi.fn(),
    };
  });
  return {
    Howl: mockHowl,
    Howler: { ctx: { state: 'running', resume: vi.fn() } },
  };
});

import { AudioManager } from './audio';
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

  it('resumes audio context on first processEvents call', () => {
    // Override Howler ctx to simulate suspended state
    const resumeFn = vi.fn();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (Howler as any).ctx = { state: 'suspended', resume: resumeFn };

    const manager = new AudioManager(makeKit(), '/audio/');
    manager.processEvents([{ type: 'jump' }]);

    expect(resumeFn).toHaveBeenCalled();
  });
});
