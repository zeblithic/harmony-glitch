<script lang="ts">
  interface BuffFrame {
    kind: string;
    icon: string;
    label: string;
    remainingSecs: number;
  }

  let { buffs = [] }: { buffs: BuffFrame[] } = $props();

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
  <div class="buff-hud" role="list" aria-label="Active buffs">
    {#each buffs as buff (buff.kind)}
      <div
        class="buff-icon"
        role="listitem"
        aria-label="{buff.label}: {formatRemaining(buff.remainingSecs)} remaining"
      >
        <span class="buff-icon-sprite">{buff.icon}</span>
        <span class="buff-timer">{formatRemaining(buff.remainingSecs)}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .buff-hud {
    position: fixed;
    top: 74px;
    left: 12px;
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
    font-size: 11px;
    color: #fbbf24;
  }

  .buff-timer {
    font-size: 10px;
    font-weight: bold;
    color: #fbbf24;
  }
</style>
