<script lang="ts">
  import type { BuffFrame } from '../types';

  let { buffs = [] }: { buffs: BuffFrame[] } = $props();

  const BUFF_EMOJI: Record<string, string> = {
    rookswort: '🌿',
  };

  function iconGlyph(kind: string): string {
    return BUFF_EMOJI[kind] ?? '✨';
  }

  function formatRemaining(secs: number): string {
    const safe = Math.max(0, Math.floor(secs));
    if (safe >= 60) {
      const m = Math.floor(safe / 60);
      const s = safe % 60;
      return `${m}:${s.toString().padStart(2, '0')}`;
    }
    return `${safe}s`;
  }
</script>

{#if buffs.length > 0}
  <ul class="buff-hud" aria-label="Active buffs">
    {#each buffs as buff (buff.kind)}
      <li
        class="buff-icon"
        aria-label="{buff.label}: {formatRemaining(buff.remainingSecs)} remaining"
      >
        <span class="buff-icon-sprite">{iconGlyph(buff.kind)}</span>
        <span class="buff-timer">{formatRemaining(buff.remainingSecs)}</span>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .buff-hud {
    position: fixed;
    top: 74px;
    left: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: row;
    gap: 6px;
    z-index: 50;
    pointer-events: none;
    user-select: none;
  }

  .buff-icon {
    background: rgba(26, 26, 46, 0.85);
    padding: 4px 8px;
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    min-width: 28px;
  }

  .buff-icon-sprite {
    font-size: 16px;
  }

  .buff-timer {
    font-size: 10px;
    font-weight: bold;
    color: #fbbf24;
  }
</style>
