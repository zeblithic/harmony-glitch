<script lang="ts">
  let { name }: { name: string } = $props();
</script>

<div class="street-name-hud" role="status" aria-label="Current street: {name}">
  {name}
</div>

<style>
  .street-name-hud {
    position: fixed;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(26, 26, 46, 0.75);
    color: #e0e0e0;
    padding: 4px 16px;
    border-radius: 12px;
    font-size: 14px;
    z-index: 50;
    pointer-events: none;
    user-select: none;
  }
</style>
