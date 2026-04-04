import { describe, it, expect } from 'vitest';
import { LocalMusicSource } from './music';

describe('LocalMusicSource', () => {
  it('resolves track URL from filename', () => {
    const source = new LocalMusicSource();
    expect(source.resolveTrackUrl('glitch-theme', 'glitch-theme.mp3'))
      .toBe('/assets/music/tracks/glitch-theme.mp3');
  });

  it('ignores trackId and uses filename', () => {
    const source = new LocalMusicSource();
    expect(source.resolveTrackUrl('any-id', 'some-file.ogg'))
      .toBe('/assets/music/tracks/some-file.ogg');
  });
});
