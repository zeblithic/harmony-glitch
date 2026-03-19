import { Howl, Howler } from 'howler';
import type { AudioEvent } from '../types';

export interface SoundEntry {
  default: string;
  variants?: Record<string, string>;
}

export interface SoundKit {
  name: string;
  version: number;
  sfxVolume: number;
  ambientVolume: number;
  events: Record<string, SoundEntry>;
  ambient: SoundEntry;
}

export class AudioManager {
  private kit: SoundKit;
  private sounds: Map<string, Howl> = new Map();
  private currentAmbient: Howl | null = null;
  private sfxVolume: number;
  private ambientVolume: number;
  private audioBasePath: string;
  private fadingOut = false;

  constructor(kit: SoundKit, audioBasePath: string) {
    this.kit = kit;
    this.sfxVolume = kit.sfxVolume;
    this.ambientVolume = kit.ambientVolume;
    this.audioBasePath = audioBasePath;
    this.preloadSounds();
  }

  private preloadSounds(): void {
    const paths = new Set<string>();
    for (const entry of Object.values(this.kit.events)) {
      paths.add(entry.default);
      if (entry.variants) {
        for (const path of Object.values(entry.variants)) {
          paths.add(path);
        }
      }
    }
    paths.add(this.kit.ambient.default);
    if (this.kit.ambient.variants) {
      for (const path of Object.values(this.kit.ambient.variants)) {
        paths.add(path);
      }
    }

    for (const path of paths) {
      const fullPath = `${this.audioBasePath}${path}`;
      const howl = new Howl({
        src: [fullPath],
        preload: true,
        onloaderror: (_id: number, err: unknown) => {
          console.warn(`[AudioManager] Failed to load ${path}:`, err);
        },
      });
      this.sounds.set(path, howl);
    }
  }

  processEvents(events: AudioEvent[]): void {
    // Resume audio context if suspended (browser autoplay policy, tab switch, etc.)
    if (Howler.ctx?.state === 'suspended') {
      Howler.ctx.resume();
    }

    for (const event of events) {
      switch (event.type) {
        case 'itemPickup':
          this.playSfx('itemPickup', event.itemId);
          break;
        case 'craftSuccess':
          this.playSfx('craftSuccess', event.recipeId);
          break;
        case 'actionFailed':
          this.playSfx('actionFailed');
          break;
        case 'jump':
          this.playSfx('jump');
          break;
        case 'land':
          this.playSfx('land');
          break;
        case 'transitionStart':
          this.playSfx('transitionStart');
          this.fadeOutAmbient();
          break;
        case 'transitionComplete':
          this.playSfx('transitionComplete');
          if (this.fadingOut) {
            this.fadeInAmbient();
          }
          break;
        case 'entityInteract':
          this.playSfx('entityInteract', event.entityType);
          break;
        case 'streetChanged':
          this.handleStreetChanged(event.streetId);
          break;
      }
    }
  }

  private playSfx(eventType: string, variantKey?: string): void {
    const entry = this.kit.events[eventType];
    if (!entry) return;

    const path = (variantKey && entry.variants?.[variantKey]) || entry.default;
    const howl = this.sounds.get(path);
    if (howl) {
      howl.volume(this.sfxVolume);
      howl.play();
    }
  }

  private handleStreetChanged(streetId: string): void {
    const path =
      this.kit.ambient.variants?.[streetId] || this.kit.ambient.default;
    const howl = this.sounds.get(path);
    if (!howl) return;

    if (this.fadingOut) {
      if (this.currentAmbient) {
        this.currentAmbient.stop();
      }
      this.currentAmbient = howl;
      howl.loop(true);
      howl.volume(0);
      howl.play();
      return;
    }

    if (this.currentAmbient) {
      this.currentAmbient.stop();
    }
    this.currentAmbient = howl;
    howl.loop(true);
    howl.volume(this.ambientVolume);
    howl.play();
  }

  private fadeOutAmbient(): void {
    this.fadingOut = true;
    if (this.currentAmbient) {
      this.currentAmbient.fade(this.ambientVolume, 0, 1000);
    }
  }

  private fadeInAmbient(): void {
    this.fadingOut = false;
    if (this.currentAmbient) {
      this.currentAmbient.fade(0, this.ambientVolume, 1000);
    }
  }

  setVolume(channel: 'sfx' | 'ambient', volume: number): void {
    if (channel === 'sfx') {
      this.sfxVolume = volume;
    } else {
      this.ambientVolume = volume;
      if (this.currentAmbient && !this.fadingOut) {
        this.currentAmbient.volume(volume);
      }
    }
  }

  dispose(): void {
    for (const howl of this.sounds.values()) {
      howl.unload();
    }
    this.sounds.clear();
    this.currentAmbient = null;
  }
}

export async function loadSoundKit(basePath: string): Promise<SoundKit> {
  const response = await fetch(`${basePath}default-kit.json`);
  if (!response.ok) {
    throw new Error(`Failed to load sound kit: ${response.status}`);
  }
  return response.json();
}
