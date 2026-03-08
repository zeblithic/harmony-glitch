<script lang="ts">
  import './app.css';
  import GameCanvas from './lib/components/GameCanvas.svelte';
  import StreetPicker from './lib/components/StreetPicker.svelte';
  import DebugOverlay from './lib/components/DebugOverlay.svelte';
  import ChatInput from './lib/components/ChatInput.svelte';
  import { stopGame, loadStreet } from './lib/ipc';
  import type { StreetData, RenderFrame } from './lib/types';

  let currentStreet = $state<StreetData | null>(null);
  let latestFrame = $state<RenderFrame | null>(null);
  let debugMode = $state(false);
  let chatFocused = $state(false);
  let transitionPending = $state(false);

  function handleStreetLoaded(street: StreetData) {
    currentStreet = street;
  }

  function handleFrame(frame: RenderFrame) {
    latestFrame = frame;

    // When swoop transition completes, load the target street
    if (frame.transition && frame.transition.progress >= 1.0 && !transitionPending) {
      transitionPending = true;
      loadStreet(frame.transition.toStreet)
        .then((street) => {
          currentStreet = street;
        })
        .catch(console.error)
        .finally(() => {
          transitionPending = false;
        });
    }
  }

  function toggleDebug() {
    debugMode = !debugMode;
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'F3') { e.preventDefault(); toggleDebug(); }}} />

<main>
  {#if currentStreet}
    <GameCanvas street={currentStreet} {debugMode} {chatFocused} onFrame={handleFrame} />
    <DebugOverlay frame={latestFrame} visible={debugMode} />
    <ChatInput onFocusChange={(focused) => { chatFocused = focused; }} />
    <button type="button" class="back-btn" onclick={async () => {
      try {
        await stopGame();
      } catch (e) {
        console.error('stopGame failed:', e);
      } finally {
        currentStreet = null;
        latestFrame = null;
      }
    }}>
      Back
    </button>
  {:else}
    <StreetPicker onStreetLoaded={handleStreetLoaded} />
  {/if}
</main>

<style>
  main {
    height: 100%;
    width: 100%;
  }

  .back-btn {
    position: fixed;
    top: 8px;
    right: 8px;
    padding: 6px 16px;
    border: 1px solid #444;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.6);
    color: #e0e0e0;
    font-size: 0.8rem;
    cursor: pointer;
    z-index: 50;
  }

  .back-btn:hover {
    background: rgba(88, 101, 242, 0.8);
  }
</style>
