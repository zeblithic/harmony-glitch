<script lang="ts">
  let { imagination = 0 }: { imagination: number } = $props();
</script>

<div class="imagination-hud" role="status" aria-label="Imagination: {imagination}">
  <span class="imagination-icon">✦</span>
  <span class="imagination-amount">{imagination}</span>
</div>

<style>
  .imagination-hud {
    position: fixed;
    top: 12px;
    left: 200px;
    background: rgba(26, 26, 46, 0.85);
    padding: 4px 10px;
    border-radius: 16px;
    display: flex;
    align-items: center;
    gap: 5px;
    z-index: 50;
    pointer-events: none;
    user-select: none;
  }

  .imagination-icon {
    font-size: 11px;
    color: #c084fc;
  }

  .imagination-amount {
    font-size: 11px;
    font-weight: bold;
    color: #c084fc;
    min-width: 20px;
    text-align: right;
  }
</style>
