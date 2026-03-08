<script lang="ts">
  import type { RenderFrame } from '../types';

  let { frame, visible = false }: {
    frame: RenderFrame | null;
    visible?: boolean;
  } = $props();
</script>

{#if visible && frame}
  <div class="debug-overlay" aria-hidden="true">
    <p>pos: ({frame.player.x.toFixed(0)}, {frame.player.y.toFixed(0)})</p>
    <p>cam: ({frame.camera.x.toFixed(0)}, {frame.camera.y.toFixed(0)})</p>
    <p>state: {frame.player.animation} | {frame.player.facing}</p>
    <p>street: {frame.streetId}</p>
  </div>
{/if}

<style>
  .debug-overlay {
    position: fixed;
    top: 8px;
    left: 8px;
    background: rgba(0, 0, 0, 0.7);
    color: #0f0;
    font-family: monospace;
    font-size: 12px;
    padding: 8px;
    border-radius: 4px;
    z-index: 50;
    pointer-events: none;
  }

  p {
    margin: 0;
    line-height: 1.4;
  }
</style>
