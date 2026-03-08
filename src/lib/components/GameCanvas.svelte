<script lang="ts">
  import { onMount } from 'svelte';
  import { GameRenderer } from '../engine/renderer';
  import { sendInput, onRenderFrame, startGame } from '../ipc';
  import type { StreetData, InputState, RenderFrame } from '../types';

  let { street, debugMode = false, onFrame }: {
    street: StreetData | null;
    debugMode?: boolean;
    onFrame?: (frame: RenderFrame) => void;
  } = $props();

  let canvasEl: HTMLCanvasElement;
  let renderer = $state<GameRenderer | null>(null);

  // Track key state
  let keys = $state<InputState>({ left: false, right: false, jump: false });

  function handleKeyDown(e: KeyboardEvent) {
    let changed = false;
    if (e.key === 'ArrowLeft' || e.key === 'a') { keys.left = true; changed = true; }
    if (e.key === 'ArrowRight' || e.key === 'd') { keys.right = true; changed = true; }
    if (e.key === ' ' || e.key === 'ArrowUp' || e.key === 'w') {
      e.preventDefault();
      keys.jump = true;
      changed = true;
    }
    if (changed) sendInput({ ...keys }).catch(console.error);
  }

  function handleKeyUp(e: KeyboardEvent) {
    let changed = false;
    if (e.key === 'ArrowLeft' || e.key === 'a') { keys.left = false; changed = true; }
    if (e.key === 'ArrowRight' || e.key === 'd') { keys.right = false; changed = true; }
    if (e.key === ' ' || e.key === 'ArrowUp' || e.key === 'w') { keys.jump = false; changed = true; }
    if (changed) sendInput({ ...keys }).catch(console.error);
  }

  onMount(async () => {
    const r = new GameRenderer();
    await r.init(canvasEl);
    renderer = r; // Set *after* init so $effect only fires with a ready instance

    const unlisten = await onRenderFrame((frame) => {
      r.updateFrame(frame);
      onFrame?.(frame);
    });

    if (street) {
      r.buildScene(street);
      startGame().catch(console.error);
    }

    return () => {
      unlisten();
      r.destroy();
    };
  });

  // Debug mode toggle — only redraws platform overlays, not the full scene.
  $effect(() => {
    renderer?.setDebugMode(debugMode);
  });
</script>

<svelte:window onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

<div class="canvas-container">
  <canvas
    bind:this={canvasEl}
    role="application"
    aria-label="Harmony Glitch game — use arrow keys or WASD to move, Space to jump, F3 for debug overlay"
  ></canvas>
</div>

<style>
  .canvas-container {
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  canvas {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
