<script lang="ts">
  let { energy = 0, maxEnergy = 600 }: { energy: number; maxEnergy: number } = $props();

  let percent = $derived(maxEnergy > 0 ? Math.min(100, (energy / maxEnergy) * 100) : 0);
  let isLow = $derived(energy < 150);
  let displayEnergy = $derived(Math.floor(energy));
</script>

<div class="energy-hud" class:low={isLow} role="status" aria-label="Energy: {displayEnergy} of {maxEnergy}">
  <span class="energy-icon">⚡</span>
  <div class="energy-bar">
    <div class="energy-fill" style="width: {percent}%"></div>
  </div>
  <span class="energy-amount">{displayEnergy}</span>
</div>

<style>
  .energy-hud {
    position: fixed;
    top: 12px;
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

  .energy-icon {
    font-size: 10px;
    color: #4ade80;
  }

  .energy-bar {
    width: 60px;
    height: 8px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    overflow: hidden;
  }

  .energy-fill {
    height: 100%;
    background: linear-gradient(90deg, #22c55e, #4ade80);
    border-radius: 4px;
    transition: width 0.3s ease;
  }

  .energy-amount {
    font-size: 11px;
    font-weight: bold;
    color: #4ade80;
    min-width: 24px;
    text-align: right;
  }

  .energy-hud.low .energy-fill {
    background: linear-gradient(90deg, #ef4444, #f59e0b);
  }

  .energy-hud.low .energy-icon,
  .energy-hud.low .energy-amount {
    color: #f59e0b;
  }
</style>
