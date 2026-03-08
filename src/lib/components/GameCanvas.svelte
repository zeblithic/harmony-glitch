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
  let renderer: GameRenderer | null = null;

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
    if (changed) sendInput({ ...keys });
  }

  function handleKeyUp(e: KeyboardEvent) {
    let changed = false;
    if (e.key === 'ArrowLeft' || e.key === 'a') { keys.left = false; changed = true; }
    if (e.key === 'ArrowRight' || e.key === 'd') { keys.right = false; changed = true; }
    if (e.key === ' ' || e.key === 'ArrowUp' || e.key === 'w') { keys.jump = false; changed = true; }
    if (changed) sendInput({ ...keys });
  }

  onMount(async () => {
    renderer = new GameRenderer();
    await renderer.init(canvasEl);

    const unlisten = await onRenderFrame((frame) => {
      renderer?.updateFrame(frame);
      onFrame?.(frame);
    });

    return () => {
      unlisten();
      renderer?.destroy();
    };
  });

  $effect(() => {
    if (renderer && street) {
      renderer.buildScene(street);
      renderer.setDebugMode(debugMode);
      startGame();
    }
  });

  $effect(() => {
    renderer?.setDebugMode(debugMode);
  });
</script>

<svelte:window onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

<div class="canvas-container">
  <canvas bind:this={canvasEl}></canvas>
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
