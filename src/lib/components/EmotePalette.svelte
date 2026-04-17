<script lang="ts">
  import type { EmoteKind, EmotePrivacy } from '$lib/types';

  let {
    visible,
    onClose,
    onSelect,
    cooldowns,
    nearestTarget,
    privacy,
  }: {
    visible: boolean;
    onClose: () => void;
    onSelect: (kind: EmoteKind) => void;
    /** Keyed by EmoteKindTag string ("hi","dance","wave","hug","high_five","applaud"). Value = remaining ms. */
    cooldowns: Record<string, number>;
    /** Hex address hash of nearest targetable remote player, or null. */
    nearestTarget: string | null;
    /** OUR privacy settings — used for future local defensive gating. v1 just displays. */
    privacy: EmotePrivacy;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();

  interface EmoteEntry {
    tag: string;
    label: string;
    emoji: string;
    kind: EmoteKind;
    needsTarget: boolean;
  }

  const entries: EmoteEntry[] = [
    // Hi is special — the palette sends a Hi with the caller's
    // daily variant; for now pass a placeholder and let the handler
    // resolve (alternatively wire emoteHi() directly).
    { tag: 'hi', label: 'Hi', emoji: '👋', kind: { hi: 'hi' }, needsTarget: false },
    { tag: 'dance', label: 'Dance', emoji: '💃', kind: 'dance', needsTarget: false },
    { tag: 'wave', label: 'Wave', emoji: '👋', kind: 'wave', needsTarget: true },
    { tag: 'hug', label: 'Hug', emoji: '🤗', kind: 'hug', needsTarget: true },
    { tag: 'high_five', label: 'High-Five', emoji: '🖐️', kind: 'high_five', needsTarget: true },
    { tag: 'applaud', label: 'Applaud', emoji: '👏', kind: 'applaud', needsTarget: false },
  ];

  function isDisabled(entry: EmoteEntry): boolean {
    if (entry.needsTarget && !nearestTarget) return true;
    if ((cooldowns[entry.tag] ?? 0) > 0) return true;
    return false;
  }

  function disabledReason(entry: EmoteEntry): string | null {
    if (entry.needsTarget && !nearestTarget) return 'No target in range';
    const ms = cooldowns[entry.tag] ?? 0;
    if (ms > 0) return `${Math.ceil(ms / 1000)}s`;
    return null;
  }

  $effect(() => {
    if (visible && dialogEl && !dialogEl.open) {
      dialogEl.showModal();
      requestAnimationFrame(() => {
        const first = dialogEl?.querySelector<HTMLButtonElement>('button:not([disabled])');
        first?.focus();
      });
    } else if (!visible && dialogEl?.open) {
      dialogEl.close();
    }
  });

  function handleSelect(entry: EmoteEntry) {
    if (isDisabled(entry)) return;
    onSelect(entry.kind);
    onClose();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
      return;
    }
    const num = parseInt(e.key);
    if (num >= 1 && num <= entries.length) {
      e.preventDefault();
      handleSelect(entries[num - 1]);
    }
  }
</script>

{#if visible}
  <dialog
    class="emote-palette"
    aria-label="Emote palette"
    bind:this={dialogEl}
    oncancel={(e) => { e.preventDefault(); onClose(); }}
    onkeydown={handleKeyDown}
  >
    <div class="palette-row">
      {#each entries as entry, i (entry.tag)}
        <button
          type="button"
          class="emote-button"
          class:disabled={isDisabled(entry)}
          disabled={isDisabled(entry)}
          title={disabledReason(entry) ?? ''}
          onclick={() => handleSelect(entry)}
        >
          <span class="emote-emoji">{entry.emoji}</span>
          <span class="emote-label">{i + 1} {entry.label}</span>
          {#if disabledReason(entry)}
            <span class="emote-reason">{disabledReason(entry)}</span>
          {/if}
        </button>
      {/each}
    </div>
  </dialog>
{/if}

<style>
  .emote-palette {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    margin: 0;
    padding: 10px 14px;
    background: rgba(26, 26, 46, 0.95);
    border: 1px solid rgba(192, 132, 252, 0.25);
    border-radius: 8px;
    color: #e0e0e0;
    z-index: 100;
  }
  .emote-palette::backdrop { background: transparent; }

  .palette-row { display: flex; gap: 8px; }

  .emote-button {
    display: flex;
    flex-direction: column;
    align-items: center;
    min-width: 72px;
    padding: 6px 10px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    color: #ccc;
    font-size: 11px;
    cursor: pointer;
    gap: 2px;
  }
  .emote-button:hover:not(.disabled), .emote-button:focus:not(.disabled) {
    background: rgba(192, 132, 252, 0.12);
    border-color: rgba(192, 132, 252, 0.3);
    color: #fff;
    outline: none;
  }
  .emote-button.disabled { opacity: 0.4; cursor: not-allowed; }

  .emote-emoji { font-size: 20px; }
  .emote-label { font-weight: 500; }
  .emote-reason { font-size: 10px; color: #888; }
</style>
