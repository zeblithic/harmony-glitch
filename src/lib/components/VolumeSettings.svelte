<script lang="ts">
  import type { AudioManager } from '../engine/audio';

  let {
    audioManager,
    visible = false,
    onClose,
  }: {
    audioManager: AudioManager | null;
    visible: boolean;
    onClose?: () => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;

  let sfxVolume = $state(1.0);
  let ambientVolume = $state(0.5);
  let sfxMuted = $state(false);
  let ambientMuted = $state(false);

  $effect(() => {
    if (visible && dialogEl) {
      if (!dialogEl.open) {
        // Only capture focus when transitioning from closed → open
        previousFocus = document.activeElement as HTMLElement | null;
      }
      // Sync state from AudioManager on open and when it changes
      if (audioManager) {
        sfxVolume = audioManager.getVolume('sfx');
        ambientVolume = audioManager.getVolume('ambient');
        sfxMuted = audioManager.isMuted('sfx');
        ambientMuted = audioManager.isMuted('ambient');
      }
      if (!dialogEl.open) {
        dialogEl.showModal();
      }
      dialogEl.querySelector<HTMLElement>('input[type="range"]')?.focus();
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

  function handleSfxVolume(e: Event) {
    const value = parseFloat((e.target as HTMLInputElement).value);
    sfxVolume = value;
    audioManager?.setVolume('sfx', value);
  }

  function handleAmbientVolume(e: Event) {
    const value = parseFloat((e.target as HTMLInputElement).value);
    ambientVolume = value;
    audioManager?.setVolume('ambient', value);
  }

  function toggleSfxMute() {
    sfxMuted = !sfxMuted;
    audioManager?.setMuted('sfx', sfxMuted);
  }

  function toggleAmbientMute() {
    ambientMuted = !ambientMuted;
    audioManager?.setMuted('ambient', ambientMuted);
  }
</script>

{#if visible}
  <dialog
    class="volume-panel"
    aria-label="Volume Settings"
    aria-modal="true"
    bind:this={dialogEl}
    oncancel={handleCancel}
  >
    <div class="panel-header">
      <h2>Volume</h2>
      <button type="button" class="close-btn" aria-label="Close volume settings" onclick={() => onClose?.()}>
        &times;
      </button>
    </div>

    <div class="channels">
      <div class="channel">
        <div class="channel-header">
          <label for="sfx-slider">SFX</label>
          <button
            type="button"
            class="mute-btn"
            class:muted={sfxMuted}
            aria-label={sfxMuted ? 'Unmute SFX' : 'Mute SFX'}
            aria-pressed={sfxMuted}
            onclick={toggleSfxMute}
          >
            {sfxMuted ? 'Muted' : Math.round(sfxVolume * 100) + '%'}
          </button>
        </div>
        <input
          id="sfx-slider"
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={sfxVolume}
          oninput={handleSfxVolume}
          class:muted={sfxMuted}
        />
      </div>

      <div class="channel">
        <div class="channel-header">
          <label for="ambient-slider">Ambient</label>
          <button
            type="button"
            class="mute-btn"
            class:muted={ambientMuted}
            aria-label={ambientMuted ? 'Unmute ambient' : 'Mute ambient'}
            aria-pressed={ambientMuted}
            onclick={toggleAmbientMute}
          >
            {ambientMuted ? 'Muted' : Math.round(ambientVolume * 100) + '%'}
          </button>
        </div>
        <input
          id="ambient-slider"
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={ambientVolume}
          oninput={handleAmbientVolume}
          class:muted={ambientMuted}
        />
      </div>
    </div>
  </dialog>
{/if}

<style>
  .volume-panel {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 280px;
    padding: 16px;
    background: rgba(20, 20, 40, 0.95);
    border: 1px solid #444;
    border-radius: 8px;
    color: #e0e0e0;
    z-index: 100;
  }

  .volume-panel::backdrop {
    background: transparent;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
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

  .channels {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .channel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .channel-header label {
    font-size: 0.75rem;
    color: #ccc;
  }

  .mute-btn {
    background: rgba(40, 40, 70, 0.6);
    border: 1px solid #555;
    border-radius: 3px;
    color: #ccc;
    font-size: 0.65rem;
    padding: 2px 8px;
    cursor: pointer;
    min-width: 48px;
    text-align: center;
  }

  .mute-btn:hover {
    background: rgba(40, 40, 70, 0.9);
  }

  .mute-btn.muted {
    color: #e88;
    border-color: #e88;
  }

  .mute-btn:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: -2px;
  }

  input[type="range"] {
    width: 100%;
    height: 4px;
    appearance: none;
    background: rgba(255, 255, 255, 0.15);
    border-radius: 2px;
    outline: none;
    cursor: pointer;
  }

  input[type="range"]::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #5865f2;
    cursor: pointer;
  }

  input[type="range"]::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #5865f2;
    border: none;
    cursor: pointer;
  }

  input[type="range"].muted::-webkit-slider-thumb {
    background: #666;
  }

  input[type="range"].muted::-moz-range-thumb {
    background: #666;
  }

  input[type="range"]:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: 2px;
  }
</style>
