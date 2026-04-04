import { Howl } from 'howler';

export interface MusicSource {
  resolveTrackUrl(trackId: string, filename: string): string;
}

export class LocalMusicSource implements MusicSource {
  resolveTrackUrl(_trackId: string, filename: string): string {
    return `/assets/music/tracks/${filename}`;
  }
}

export interface TrackCatalogEntry {
  title: string;
  artist: string;
  durationSecs: number;
  file: string;
}

export interface TrackCatalog {
  tracks: Record<string, TrackCatalogEntry>;
}

interface ActiveJukebox {
  howl: Howl;
  trackId: string;
  lastSeenAt: number;
  playing: boolean;
  elapsedSecs: number;
}

export class JukeboxController {
  private musicSource: MusicSource;
  private catalog: TrackCatalog;
  private getVolume: () => number;
  private active: Map<string, ActiveJukebox> = new Map();

  constructor(
    musicSource: MusicSource,
    catalog: TrackCatalog,
    getVolume: () => number,
  ) {
    this.musicSource = musicSource;
    this.catalog = catalog;
    this.getVolume = getVolume;
  }

  update(
    entityId: string,
    trackId: string,
    playing: boolean,
    distanceFactor: number,
    elapsedSecs: number,
  ): void {
    const now = performance.now();
    const existing = this.active.get(entityId);

    if (existing && existing.trackId === trackId) {
      // Same track — update volume, playing state, and stored position
      existing.lastSeenAt = now;
      existing.playing = playing;
      existing.elapsedSecs = elapsedSecs;
      existing.howl.volume(distanceFactor * this.getVolume());
      if (existing.howl.state() === 'loaded') {
        if (playing && !existing.howl.playing()) {
          existing.howl.play();
          existing.howl.seek(elapsedSecs);
        } else if (!playing && existing.howl.playing()) {
          existing.howl.pause();
        }
      }
      // While loading, onload will read the latest playing/elapsedSecs from the entry
    } else {
      // New jukebox or track changed — stop old Howl if any
      if (existing) {
        existing.howl.stop();
        existing.howl.unload();
      }

      const entry = this.catalog.tracks[trackId];
      if (!entry) {
        this.active.delete(entityId);
        console.warn(`[JukeboxController] Unknown track: ${trackId}`);
        return;
      }

      const url = this.musicSource.resolveTrackUrl(trackId, entry.file);
      const activeEntry: ActiveJukebox = {
        howl: null!,  // assigned immediately below
        trackId,
        lastSeenAt: now,
        playing,
        elapsedSecs,
      };

      const howl = new Howl({
        src: [url],
        volume: distanceFactor * this.getVolume(),
        onloaderror: (_id: number, err: unknown) => {
          console.warn(`[JukeboxController] Failed to load track ${trackId}:`, err);
        },
        onload: () => {
          // Read current state from the entry, not the stale closure captures
          if (activeEntry.playing) {
            howl.play();
            howl.seek(activeEntry.elapsedSecs);
          }
        },
      });
      activeEntry.howl = howl;

      this.active.set(entityId, activeEntry);
    }
  }

  cleanup(): void {
    const now = performance.now();
    const staleThresholdMs = 600;
    const stale: string[] = [];

    for (const [entityId, jukebox] of this.active.entries()) {
      if (now - jukebox.lastSeenAt > staleThresholdMs) {
        jukebox.howl.fade(jukebox.howl.volume() as number, 0, 500);
        const howl = jukebox.howl;
        setTimeout(() => {
          howl.stop();
          howl.unload();
        }, 500);
        stale.push(entityId);
      }
    }

    for (const entityId of stale) {
      this.active.delete(entityId);
    }
  }

  dispose(): void {
    for (const jukebox of this.active.values()) {
      jukebox.howl.stop();
      jukebox.howl.unload();
    }
    this.active.clear();
  }
}
