<script lang="ts">
  import './app.css';
  import GameCanvas from './lib/components/GameCanvas.svelte';
  import StreetPicker from './lib/components/StreetPicker.svelte';
  import DebugOverlay from './lib/components/DebugOverlay.svelte';
  import ChatInput from './lib/components/ChatInput.svelte';
  import IdentitySetup from './lib/components/IdentitySetup.svelte';
  import NetworkStatus from './lib/components/NetworkStatus.svelte';
  import InventoryPanel from './lib/components/InventoryPanel.svelte';
  import { stopGame, loadStreet, getIdentity, streetTransitionReady } from './lib/ipc';
  import type { StreetData, RenderFrame } from './lib/types';
  import { onMount } from 'svelte';

  let currentStreet = $state<StreetData | null>(null);
  let latestFrame = $state<RenderFrame | null>(null);
  let debugMode = $state(false);
  let chatFocused = $state(false);
  let inventoryOpen = $state(false);
  let transitionPending = $state(false);
  let identityReady = $state(false);
  let checkingIdentity = $state(true);

  onMount(async () => {
    try {
      const identity = await getIdentity();
      identityReady = identity.setupComplete;
    } catch {
      identityReady = false;
    } finally {
      checkingIdentity = false;
    }
  });

  function handleStreetLoaded(street: StreetData) {
    currentStreet = street;
  }

  function handleFrame(frame: RenderFrame) {
    latestFrame = frame;

    // When a transition appears, pre-load the target street immediately.
    // The TransitionState stalls at progress 0.9 until we signal ready.
    // transitionPending stays true until frame.transition disappears (swoop
    // completes) — clearing it earlier causes repeated loadStreet/mark_street_ready
    // calls that push target_duration forward indefinitely, stalling the swoop.
    if (frame.transition && !transitionPending) {
      transitionPending = true;
      loadStreet(frame.transition.toStreet)
        .then((street) => {
          currentStreet = street;
          return streetTransitionReady();
        })
        .catch((e) => {
          console.error('Street transition failed:', e);
          transitionPending = false;
        });
    }
    if (!frame.transition && transitionPending) {
      transitionPending = false;
    }
  }

  function toggleDebug() {
    debugMode = !debugMode;
  }
</script>

<svelte:window onkeydown={(e) => {
  if (e.key === 'F3') { e.preventDefault(); toggleDebug(); }
  if ((e.key === 'i' || e.key === 'I') && currentStreet && !chatFocused) {
    e.preventDefault();
    inventoryOpen = !inventoryOpen;
  }
}} />

<main>
  {#if checkingIdentity}
    <!-- Wait for identity check before showing anything -->
  {:else if !identityReady}
    <IdentitySetup onComplete={() => { identityReady = true; }} />
  {:else if currentStreet}
    <GameCanvas street={currentStreet} {debugMode} {chatFocused} {inventoryOpen} onFrame={handleFrame} />
    <DebugOverlay frame={latestFrame} visible={debugMode} />
    <ChatInput onFocusChange={(focused) => { chatFocused = focused; }} />
    <NetworkStatus />
    <InventoryPanel
      inventory={latestFrame?.inventory ?? null}
      visible={inventoryOpen}
      onClose={() => { inventoryOpen = false; }}
    />
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
