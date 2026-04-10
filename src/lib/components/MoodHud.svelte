<script lang="ts">
  let { mood = 0, maxMood = 100 }: { mood: number; maxMood: number } = $props();
  let percent = $derived(maxMood > 0 ? Math.max(0, Math.min(100, (mood / maxMood) * 100)) : 0);
  let isLow = $derived(mood < maxMood * 0.5);
  let displayMood = $derived(Math.floor(mood));
</script>

<div class="mood-hud" class:low={isLow} role="status" aria-label="Mood: {displayMood} of {maxMood}">
  <span class="mood-icon">😊</span>
  <div class="mood-bar">
    <div class="mood-fill" style="width: {percent}%"></div>
  </div>
  <span class="mood-amount">{displayMood}</span>
</div>

<style>
  .mood-hud {
    position: fixed;
    top: 42px;
    left: 12px;
    background: rgba(26, 26, 46, 0.85);
    padding: 4px 10px;
    border-radius: 16px;
    display: flex;
    align-items: center;
    gap: 6px;
    z-index: 50;
    pointer-events: none;
    user-select: none;
  }

  .mood-icon {
    font-size: 10px;
    color: #c084fc;
  }

  .mood-bar {
    width: 60px;
    height: 8px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    overflow: hidden;
  }

  .mood-fill {
    height: 100%;
    background: linear-gradient(90deg, #c084fc, #e879a8);
    border-radius: 4px;
    transition: width 0.3s ease;
  }

  .mood-amount {
    font-size: 11px;
    font-weight: bold;
    color: #c084fc;
    min-width: 24px;
    text-align: right;
  }

  .mood-hud.low .mood-fill {
    background: linear-gradient(90deg, #6b7280, #9ca3af);
  }

  .mood-hud.low .mood-icon,
  .mood-hud.low .mood-amount {
    color: #9ca3af;
  }
</style>
