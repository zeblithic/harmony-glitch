<script lang="ts">
  let { currants = 0 }: { currants: number } = $props();
</script>

<div class="currant-hud" aria-label="Currant balance: {currants}">
  <span class="currant-icon">●</span>
  <span class="currant-amount">{currants}</span>
</div>

<style>
  .currant-hud {
    position: fixed;
    top: 12px;
    right: 12px;
    background: rgba(26, 26, 46, 0.85);
    color: #ffd700;
    padding: 6px 12px;
    border-radius: 16px;
    font-weight: bold;
    font-size: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
    z-index: 50;
    pointer-events: none;
    user-select: none;
  }
  .currant-icon {
    font-size: 10px;
  }
</style>
