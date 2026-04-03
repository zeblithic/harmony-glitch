<script lang="ts">
  import type { JukeboxInfo } from '../types';

  let {
    info,
    visible = false,
    onClose,
    onPlay,
    onPause,
    onSelectTrack,
  }: {
    info: JukeboxInfo | null;
    visible: boolean;
    onClose?: () => void;
    onPlay?: (entityId: string) => void;
    onPause?: (entityId: string) => void;
    onSelectTrack?: (entityId: string, trackIndex: number) => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;

  let dialogLabel = $derived(info ? `Jukebox: ${info.name}` : 'Jukebox');

  let progressPercent = $derived.by(() => {
    if (!info || info.currentTrackIndex < 0) return 0;
    const track = info.playlist[info.currentTrackIndex];
    if (!track || track.durationSecs <= 0) return 0;
    return Math.min(100, (info.elapsedSecs / track.durationSecs) * 100);
  });

  let currentTrackDuration = $derived.by(() => {
    if (!info || info.currentTrackIndex < 0) return 0;
    const track = info.playlist[info.currentTrackIndex];
    return track?.durationSecs ?? 0;
  });

  function formatTime(secs: number): string {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  }

  $effect(() => {
    if (visible && dialogEl) {
      if (!dialogEl.open) {
        previousFocus = document.activeElement as HTMLElement | null;
        dialogEl.showModal();
        // Focus the first track item or the close button
        const firstFocusable = dialogEl.querySelector<HTMLElement>('[role="option"], .close-btn');
        firstFocusable?.focus();
      }
      return () => {
        if (dialogEl?.open) dialogEl.close();
      };
    } else if (!visible && previousFocus) {
      previousFocus.focus();
      previousFocus = null;
    }
  });

  function handleCancel(e: Event) {
    e.preventDefault();
    onClose?.();
  }

  function handlePrevious() {
    if (!info || info.playlist.length === 0) return;
    const prev = info.currentTrackIndex > 0
      ? info.currentTrackIndex - 1
      : info.playlist.length - 1;
    onSelectTrack?.(info.entityId, prev);
  }

  function handleNext() {
    if (!info || info.playlist.length === 0) return;
    const next = info.currentTrackIndex < info.playlist.length - 1
      ? info.currentTrackIndex + 1
      : 0;
    onSelectTrack?.(info.entityId, next);
  }

  function handlePlayPause() {
    if (!info) return;
    if (info.playing) {
      onPause?.(info.entityId);
    } else {
      onPlay?.(info.entityId);
    }
  }

  function handleTrackSelect(index: number) {
    if (!info) return;
    onSelectTrack?.(info.entityId, index);
  }
</script>

{#if visible && info}
  <dialog
    class="jukebox-panel"
    aria-label={dialogLabel}
    aria-modal="true"
    bind:this={dialogEl}
    oncancel={handleCancel}
  >
    <div class="panel-header">
      <h2>{info.name}</h2>
      <button type="button" class="close-btn" aria-label="Close jukebox" onclick={() => onClose?.()}>
        &times;
      </button>
    </div>

    {#if info.playlist.length === 0}
      <p class="empty-message">No tracks available</p>
    {:else}
      <ul role="listbox" aria-label="Track list" class="track-list">
        {#each info.playlist as track, i (track.id)}
          <li
            role="option"
            tabindex="0"
            class="track-item"
            class:active={i === info.currentTrackIndex}
            aria-selected={i === info.currentTrackIndex}
            onclick={() => handleTrackSelect(i)}
            onkeydown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                handleTrackSelect(i);
              }
            }}
          >
            <span class="track-title">{track.title}</span>
            <span class="track-artist">{track.artist}</span>
            <span class="track-duration">{formatTime(track.durationSecs)}</span>
          </li>
        {/each}
      </ul>
    {/if}

    <div class="controls">
      <button
        type="button"
        class="control-btn"
        aria-label="Previous track"
        onclick={handlePrevious}
        disabled={info.playlist.length === 0}
      >
        &#9198;
      </button>
      <button
        type="button"
        class="control-btn play-btn"
        aria-label={info.playing ? 'Pause' : 'Play'}
        onclick={handlePlayPause}
        disabled={info.playlist.length === 0}
      >
        {info.playing ? '\u23F8' : '\u25B6'}
      </button>
      <button
        type="button"
        class="control-btn"
        aria-label="Next track"
        onclick={handleNext}
        disabled={info.playlist.length === 0}
      >
        &#9197;
      </button>
    </div>

    <div class="progress-section">
      <span class="time-label">{formatTime(info.elapsedSecs)}</span>
      <div
        class="progress-bar"
        role="progressbar"
        aria-label="Track progress"
        aria-valuenow={Math.round(info.elapsedSecs)}
        aria-valuemin={0}
        aria-valuemax={Math.round(currentTrackDuration)}
      >
        <div class="progress-fill" style="width: {progressPercent}%"></div>
      </div>
      <span class="time-label">{formatTime(currentTrackDuration)}</span>
    </div>
  </dialog>
{/if}

<style>
  .jukebox-panel {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 320px;
    max-height: 80vh;
    padding: 16px;
    background: rgba(20, 20, 40, 0.95);
    border: 1px solid #444;
    border-radius: 8px;
    color: #e0e0e0;
    z-index: 100;
    display: flex;
    flex-direction: column;
  }

  .jukebox-panel::backdrop {
    background: transparent;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .panel-header h2 {
    margin: 0;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #ccc;
  }

  .close-btn {
    background: none;
    border: none;
    color: #888;
    font-size: 1.2rem;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
  }

  .close-btn:hover {
    color: #e0e0e0;
  }

  .close-btn:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: 2px;
  }

  .empty-message {
    text-align: center;
    color: #888;
    font-size: 0.8rem;
    padding: 24px 0;
    margin: 0;
  }

  .track-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 200px;
    overflow-y: auto;
    border: 1px solid #333;
    border-radius: 4px;
  }

  .track-item {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 8px 10px;
    cursor: pointer;
    font-size: 0.75rem;
    border-bottom: 1px solid #2a2a40;
  }

  .track-item:last-child {
    border-bottom: none;
  }

  .track-item:hover {
    background: rgba(88, 101, 242, 0.15);
  }

  .track-item.active {
    background: rgba(88, 101, 242, 0.3);
    color: #fff;
  }

  .track-item:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: -2px;
  }

  .track-title {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .track-artist {
    color: #999;
    font-size: 0.65rem;
    white-space: nowrap;
  }

  .track-duration {
    color: #888;
    font-size: 0.65rem;
    white-space: nowrap;
  }

  .controls {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
  }

  .control-btn {
    background: rgba(40, 40, 70, 0.6);
    border: 1px solid #555;
    border-radius: 4px;
    color: #ccc;
    font-size: 1rem;
    padding: 6px 12px;
    cursor: pointer;
    line-height: 1;
  }

  .control-btn:hover:not(:disabled) {
    background: rgba(88, 101, 242, 0.4);
    color: #fff;
  }

  .control-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .control-btn:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: 2px;
  }

  .play-btn {
    font-size: 1.2rem;
    padding: 6px 16px;
  }

  .progress-section {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
  }

  .time-label {
    font-size: 0.6rem;
    color: #888;
    min-width: 30px;
    text-align: center;
  }

  .progress-bar {
    flex: 1;
    height: 4px;
    background: rgba(255, 255, 255, 0.15);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: #5865f2;
    border-radius: 2px;
    transition: width 0.3s linear;
  }
</style>
