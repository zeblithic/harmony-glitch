<script lang="ts">
  let { imagination = 0, onOpen }: { imagination: number; onOpen?: () => void } = $props();
</script>

<div class="imagination-hud-wrapper" role="status" aria-label="Imagination: {imagination} iMG">
  <button
    type="button"
    class="imagination-hud"
    aria-label="Open upgrades"
    onclick={onOpen}
  >
    <span class="img-icon">✦</span>
    <span class="img-amount">{imagination} iMG</span>
  </button>
</div>

<style>
  .imagination-hud-wrapper {
    pointer-events: none;
  }
  .imagination-hud {
    pointer-events: auto;
    position: fixed;
    top: 44px;
    right: 12px;
    background: rgba(26, 26, 46, 0.85);
    color: #c084fc;
    padding: 6px 12px;
    border-radius: 16px;
    font-weight: bold;
    font-size: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
    z-index: 50;
    user-select: none;
    border: 1px solid transparent;
    cursor: pointer;
    transition: border-color 0.2s;
    font-family: inherit;
  }
  .imagination-hud:hover {
    border-color: #c084fc;
  }
  .imagination-hud:focus-visible {
    outline: 2px solid #c084fc;
    outline-offset: 2px;
  }
  .img-icon {
    font-size: 10px;
  }
</style>
