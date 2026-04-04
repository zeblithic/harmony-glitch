import { Howl, Howler } from 'howler';
import type { AudioEvent } from '../types';
import { JukeboxController, LocalMusicSource } from './music';
import type { MusicSource, TrackCatalog } from './music';

export interface SoundEntry {
  default: string;
  variants?: Record<string, string>;
}

export interface SoundKit {
  name: string;
  version: number;
  cas?: string | null;
  sfxVolume: number;
  ambientVolume: number;
  events: Record<string, SoundEntry>;
  ambient: SoundEntry;
}

export interface AudioPreferences {
  sfxVolume: number;
  ambientVolume: number;
  musicVolume: number;
  sfxMuted: boolean;
  ambientMuted: boolean;
  musicMuted: boolean;
}

export class AudioManager {
  private kit: SoundKit;
  private sounds: Map<string, Howl> = new Map();
  private currentAmbient: Howl | null = null;
  private sfxVolume: number;
  private ambientVolume: number;
  private musicVolume = 0.5;
  private sfxMuted = false;
  private ambientMuted = false;
  private musicMuted = false;
  private audioBasePath: string;
  private fadingOut = false;
  private jukeboxController: JukeboxController | null = null;

  constructor(
    kit: SoundKit,
    audioBasePath: string,
    musicSource?: MusicSource,
    catalog?: TrackCatalog,
  ) {
    this.kit = kit;
    this.sfxVolume = kit.sfxVolume;
    this.ambientVolume = kit.ambientVolume;
    this.audioBasePath = audioBasePath;

    const saved = AudioManager.loadPreferences();
    if (saved) {
      this.sfxVolume = saved.sfxVolume;
      this.ambientVolume = saved.ambientVolume;
      this.musicVolume = saved.musicVolume;
      this.sfxMuted = saved.sfxMuted;
      this.ambientMuted = saved.ambientMuted;
      this.musicMuted = saved.musicMuted;
    }

    if (catalog) {
      const source = musicSource ?? new LocalMusicSource();
      this.jukeboxController = new JukeboxController(
        source,
        catalog,
        () => this.effectiveMusicVolume(),
      );
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
        case 'footstep':
          this.playSfx('footstep', event.surface);
          break;
        case 'jukeboxUpdate':
          if (this.jukeboxController) {
            this.jukeboxController.update(
              event.entityId,
              event.trackId,
              event.playing,
              event.distanceFactor,
              event.elapsedSecs,
            );
          }
          break;
      }
    }

    if (this.jukeboxController) {
      this.jukeboxController.cleanup();
    }
  }

  private effectiveSfxVolume(): number {
    return this.sfxMuted ? 0 : this.sfxVolume;
  }

  private effectiveAmbientVolume(): number {
    return this.ambientMuted ? 0 : this.ambientVolume;
  }

  private effectiveMusicVolume(): number {
    return this.musicMuted ? 0 : this.musicVolume;
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

  setVolume(channel: 'sfx' | 'ambient' | 'music', volume: number): void {
    if (channel === 'sfx') {
      this.sfxVolume = volume;
    } else if (channel === 'ambient') {
      this.ambientVolume = volume;
      if (this.currentAmbient && !this.fadingOut) {
        this.currentAmbient.volume(this.effectiveAmbientVolume());
      }
    } else {
      this.musicVolume = volume;
    }
    this.savePreferences();
  }

  getVolume(channel: 'sfx' | 'ambient' | 'music'): number {
    if (channel === 'sfx') return this.sfxVolume;
    if (channel === 'ambient') return this.ambientVolume;
    return this.musicVolume;
  }

  setMuted(channel: 'sfx' | 'ambient' | 'music', muted: boolean): void {
    if (channel === 'sfx') {
      this.sfxMuted = muted;
    } else if (channel === 'ambient') {
      this.ambientMuted = muted;
      if (this.currentAmbient && !this.fadingOut) {
        this.currentAmbient.volume(this.effectiveAmbientVolume());
      }
    } else {
      this.musicMuted = muted;
    }
    this.savePreferences();
  }

  isMuted(channel: 'sfx' | 'ambient' | 'music'): boolean {
    if (channel === 'sfx') return this.sfxMuted;
    if (channel === 'ambient') return this.ambientMuted;
    return this.musicMuted;
  }

  getPreferences(): AudioPreferences {
    return {
      sfxVolume: this.sfxVolume,
      ambientVolume: this.ambientVolume,
      musicVolume: this.musicVolume,
      sfxMuted: this.sfxMuted,
      ambientMuted: this.ambientMuted,
      musicMuted: this.musicMuted,
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
        musicVolume: clamp(parsed.musicVolume, 0.5),
        sfxMuted: typeof parsed.sfxMuted === 'boolean' ? parsed.sfxMuted : false,
        ambientMuted: typeof parsed.ambientMuted === 'boolean' ? parsed.ambientMuted : false,
        musicMuted: typeof parsed.musicMuted === 'boolean' ? parsed.musicMuted : false,
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
    if (this.jukeboxController) {
      this.jukeboxController.dispose();
    }
  }
}

export function kitBasePath(kitId: string): string {
  if (kitId === 'default') return '/assets/audio/';
  return `soundkit://localhost/${kitId}/`;
}

export async function loadSoundKit(kitId: string): Promise<SoundKit> {
  if (kitId === 'default') {
    const response = await fetch('/assets/audio/default-kit.json');
    if (!response.ok) {
      throw new Error(`Failed to load default sound kit: ${response.status}`);
    }
    return response.json();
  }
  const { readSoundKit } = await import('../ipc');
  return readSoundKit(kitId);
}
