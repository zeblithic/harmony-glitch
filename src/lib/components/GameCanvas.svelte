<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { GameRenderer } from '../engine/renderer';
  import { sendInput, onRenderFrame, onChatMessage, startGame, getAvatar } from '../ipc';
  import type { StreetData, InputState, RenderFrame } from '../types';

  let { street, debugMode = false, chatFocused = false, inventoryOpen = false, uiOpen = false, onFrame }: {
    street: StreetData | null;
    debugMode?: boolean;
    chatFocused?: boolean;
    inventoryOpen?: boolean;
    uiOpen?: boolean;
    onFrame?: (frame: RenderFrame) => void;
  } = $props();

  let canvasEl: HTMLCanvasElement;
  let renderer = $state<GameRenderer | null>(null);

  // Track key state
  let keys = $state<InputState>({ left: false, right: false, jump: false, interact: false });

  // Clear held keys when chat opens so the player stops moving while typing.
  // Pass the literal to sendInput to avoid reading `keys` (which would create
  // a reactive dependency and cause an infinite re-run loop).
  $effect(() => {
    if (chatFocused || inventoryOpen || uiOpen) {
      keys = { left: false, right: false, jump: false, interact: false };
      sendInput({ left: false, right: false, jump: false, interact: false }).catch(console.error);
    }
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (chatFocused || inventoryOpen || uiOpen) return;
    let changed = false;
    if (e.key === 'ArrowLeft' || e.key === 'a') { keys.left = true; changed = true; }
    if (e.key === 'ArrowRight' || e.key === 'd') { keys.right = true; changed = true; }
    if (e.key === ' ' || e.key === 'ArrowUp' || e.key === 'w') {
      e.preventDefault();
      keys.jump = true;
      changed = true;
    }
    if (e.key === 'e' || e.key === 'E') { keys.interact = true; changed = true; }
    if (changed) sendInput({ ...keys }).catch(console.error);
  }

  function handleKeyUp(e: KeyboardEvent) {
    if (chatFocused || inventoryOpen || uiOpen) return;
    let changed = false;
    if (e.key === 'ArrowLeft' || e.key === 'a') { keys.left = false; changed = true; }
    if (e.key === 'ArrowRight' || e.key === 'd') { keys.right = false; changed = true; }
    if (e.key === ' ' || e.key === 'ArrowUp' || e.key === 'w') { keys.jump = false; changed = true; }
    if (e.key === 'e' || e.key === 'E') { keys.interact = false; changed = true; }
    if (changed) sendInput({ ...keys }).catch(console.error);
  }

  let cleanupFns: (() => void)[] = [];

  onMount(async () => {
    const r = new GameRenderer();
    await r.init(canvasEl);
    renderer = r; // Set *after* init so $effect only fires with a ready instance

    const unlisten = await onRenderFrame((frame) => {
      r.updateFrame(frame);
      onFrame?.(frame);
    });

    const unlistenChat = await onChatMessage((event) => {
      r.addChatBubble(event.senderHash, event.text);
    });

    cleanupFns.push(unlisten, unlistenChat, () => r.destroy());

    if (street) {
      await r.buildScene(street);
      try {
        const appearance = await getAvatar();
        await r.applyAppearance(appearance);
      } catch (e) {
        console.warn('[GameCanvas] Failed to load avatar appearance:', e);
      }
      startGame().catch(console.error);
    }
  });

  onDestroy(() => {
    for (const fn of cleanupFns) fn();
  });

  // Debug mode toggle — only redraws platform overlays, not the full scene.
  $effect(() => {
    renderer?.setDebugMode(debugMode);
  });
</script>

<svelte:window onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

<div
  class="canvas-container"
  role="application"
  aria-label="Harmony Glitch game — arrow keys or WASD to move, Space to jump, E to interact, I for inventory, P for volume settings, F3 for debug overlay"
>
  <canvas bind:this={canvasEl} aria-hidden="true"></canvas>
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
