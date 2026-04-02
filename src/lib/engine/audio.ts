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

export interface AudioPreferences {
  sfxVolume: number;
  ambientVolume: number;
  sfxMuted: boolean;
  ambientMuted: boolean;
}

export class AudioManager {
  private kit: SoundKit;
  private sounds: Map<string, Howl> = new Map();
  private currentAmbient: Howl | null = null;
  private sfxVolume: number;
  private ambientVolume: number;
  private sfxMuted = false;
  private ambientMuted = false;
  private audioBasePath: string;
  private fadingOut = false;

  constructor(kit: SoundKit, audioBasePath: string) {
    this.kit = kit;
    this.sfxVolume = kit.sfxVolume;
    this.ambientVolume = kit.ambientVolume;
    this.audioBasePath = audioBasePath;

    const saved = AudioManager.loadPreferences();
    if (saved) {
      this.sfxVolume = saved.sfxVolume;
      this.ambientVolume = saved.ambientVolume;
      this.sfxMuted = saved.sfxMuted;
      this.ambientMuted = saved.ambientMuted;
    }

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

  private effectiveSfxVolume(): number {
    return this.sfxMuted ? 0 : this.sfxVolume;
  }

  private effectiveAmbientVolume(): number {
    return this.ambientMuted ? 0 : this.ambientVolume;
  }

  private playSfx(eventType: string, variantKey?: string): void {
    const entry = this.kit.events[eventType];
    if (!entry) return;

    const path = (variantKey && entry.variants?.[variantKey]) || entry.default;
    const howl = this.sounds.get(path);
    if (howl) {
      howl.volume(this.effectiveSfxVolume());
      howl.play();
    }
  }

  private handleStreetChanged(streetId: string): void {
    const path =
      this.kit.ambient.variants?.[streetId] || this.kit.ambient.default;
    const howl = this.sounds.get(path);
    if (!howl) return;

    // Same ambient file — no stop/restart needed (avoids audible gap)
    if (this.currentAmbient === howl) {
      if (this.fadingOut) {
        this.fadingOut = false;
        howl.fade(howl.volume(), this.effectiveAmbientVolume(), 500);
      }
      return;
    }

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
    howl.volume(this.effectiveAmbientVolume());
    howl.play();
  }

  private fadeOutAmbient(): void {
    this.fadingOut = true;
    if (this.currentAmbient) {
      this.currentAmbient.fade(this.currentAmbient.volume(), 0, 1000);
    }
  }

  private fadeInAmbient(): void {
    this.fadingOut = false;
    if (this.currentAmbient) {
      this.currentAmbient.fade(0, this.effectiveAmbientVolume(), 1000);
    }
  }

  setVolume(channel: 'sfx' | 'ambient', volume: number): void {
    if (channel === 'sfx') {
      this.sfxVolume = volume;
    } else {
      this.ambientVolume = volume;
      if (this.currentAmbient && !this.fadingOut) {
        this.currentAmbient.volume(this.effectiveAmbientVolume());
      }
    }
    this.savePreferences();
  }

  getVolume(channel: 'sfx' | 'ambient'): number {
    return channel === 'sfx' ? this.sfxVolume : this.ambientVolume;
  }

  setMuted(channel: 'sfx' | 'ambient', muted: boolean): void {
    if (channel === 'sfx') {
      this.sfxMuted = muted;
    } else {
      this.ambientMuted = muted;
      if (this.currentAmbient && !this.fadingOut) {
        this.currentAmbient.volume(this.effectiveAmbientVolume());
      }
    }
    this.savePreferences();
  }

  isMuted(channel: 'sfx' | 'ambient'): boolean {
    return channel === 'sfx' ? this.sfxMuted : this.ambientMuted;
  }

  getPreferences(): AudioPreferences {
    return {
      sfxVolume: this.sfxVolume,
      ambientVolume: this.ambientVolume,
      sfxMuted: this.sfxMuted,
      ambientMuted: this.ambientMuted,
    };
  }

  private savePreferences(): void {
    try {
      const prefs: AudioPreferences = this.getPreferences();
      localStorage.setItem('audio-prefs', JSON.stringify(prefs));
    } catch { /* localStorage unavailable */ }
  }

  private static loadPreferences(): AudioPreferences | null {
    try {
      const raw = localStorage.getItem('audio-prefs');
      if (!raw) return null;
      const parsed = JSON.parse(raw);
      const clamp = (v: unknown, fallback: number) =>
        typeof v === 'number' && isFinite(v) ? Math.max(0, Math.min(1, v)) : fallback;
      return {
        sfxVolume: clamp(parsed.sfxVolume, 1.0),
        ambientVolume: clamp(parsed.ambientVolume, 0.5),
        sfxMuted: typeof parsed.sfxMuted === 'boolean' ? parsed.sfxMuted : false,
        ambientMuted: typeof parsed.ambientMuted === 'boolean' ? parsed.ambientMuted : false,
      };
    } catch {
      return null;
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
