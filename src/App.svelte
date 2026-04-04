<script lang="ts">
  import './app.css';
  import GameCanvas from './lib/components/GameCanvas.svelte';
  import StreetPicker from './lib/components/StreetPicker.svelte';
  import DebugOverlay from './lib/components/DebugOverlay.svelte';
  import ChatInput from './lib/components/ChatInput.svelte';
  import IdentitySetup from './lib/components/IdentitySetup.svelte';
  import NetworkStatus from './lib/components/NetworkStatus.svelte';
  import GameNotification from './lib/components/GameNotification.svelte';
  import VolumeSettings from './lib/components/VolumeSettings.svelte';
  import InventoryPanel from './lib/components/InventoryPanel.svelte';
  import JukeboxPanel from './lib/components/JukeboxPanel.svelte';
  import { stopGame, loadStreet, getIdentity, streetTransitionReady, getRecipes, getSavedState, listSoundKits, jukeboxPlay, jukeboxPause, jukeboxSelectTrack, getJukeboxState } from './lib/ipc';
  import type { StreetData, RenderFrame, RecipeDef, SoundKitMeta, JukeboxInfo } from './lib/types';
  import { onMount } from 'svelte';
  import { AudioManager, loadSoundKit, kitBasePath, type SoundKit } from './lib/engine/audio';
  import { LocalMusicSource, type TrackCatalog } from './lib/engine/music';

  let audioManager = $state<AudioManager | null>(null);
  let cachedKit: SoundKit | null = null;
  let soundKits = $state<SoundKitMeta[]>([]);
  let selectedKitId = $state('default');
  let currentStreet = $state<StreetData | null>(null);
  let latestFrame = $state<RenderFrame | null>(null);
  let debugMode = $state(false);
  let chatFocused = $state(false);
  let inventoryOpen = $state(false);
  let volumeOpen = $state(false);
  let transitionPending = $state(false);
  let transitionTarget = $state<string | null>(null);
  let transitionAttempts = $state(0);
  const MAX_TRANSITION_ATTEMPTS = 3;
  let identityReady = $state(false);
  let checkingIdentity = $state(true);
  let resuming = $state(false);
  let recipes = $state<RecipeDef[]>([]);
  let jukeboxOpen = $state(false);
  let jukeboxInfo = $state<JukeboxInfo | null>(null);
  let musicCatalog = $state<TrackCatalog>({ tracks: {} });

  onMount(async () => {
    try {
      const identity = await getIdentity();
      identityReady = identity.setupComplete;
    } catch {
      identityReady = false;
    } finally {
      checkingIdentity = false;
    }

    // Load recipes once at startup
    try {
      recipes = await getRecipes();
    } catch (e) {
      console.error('Failed to load recipes:', e);
    }

    // Load available sound kits
    try {
      soundKits = await listSoundKits();
    } catch (e) {
      console.error('Failed to list sound kits:', e);
      soundKits = [{ id: 'default', name: 'Default' }];
    }

    // Load music catalog
    try {
      const response = await fetch('/assets/music/catalog.json');
      if (response.ok) {
        musicCatalog = await response.json();
      }
    } catch (e) {
      console.error('Failed to load music catalog:', e);
    }

    // Restore saved kit selection
    try {
      const savedKit = localStorage.getItem('selected-sound-kit');
      if (savedKit && soundKits.some((k) => k.id === savedKit)) {
        selectedKitId = savedKit;
      }
    } catch { /* localStorage unavailable */ }

    // Initialize audio eagerly so handleStreetLoaded stays synchronous
    // (avoids race where StreetPicker re-enables before currentStreet is set)
    try {
      cachedKit = await loadSoundKit(selectedKitId);
      audioManager = new AudioManager(cachedKit, kitBasePath(selectedKitId), new LocalMusicSource(), musicCatalog);
    } catch (e) {
      console.error('Failed to initialize audio:', e);
      if (selectedKitId !== 'default') {
        selectedKitId = 'default';
        try {
          localStorage.setItem('selected-sound-kit', 'default');
        } catch { /* localStorage unavailable */ }
        try {
          cachedKit = await loadSoundKit('default');
          audioManager = new AudioManager(cachedKit, kitBasePath('default'), new LocalMusicSource(), musicCatalog);
        } catch (e2) {
          console.error('Fallback to default kit also failed:', e2);
        }
      }
    }

    // Auto-resume from save file if available.
    // Only runs if identity was already configured before this launch.
    // First-time users who complete identity setup via IdentitySetup component
    // will see the street picker (no save file exists for them anyway).
    if (identityReady) {
      // Set resuming BEFORE any async calls to suppress street picker flash.
      resuming = true;
      try {
        const saved = await getSavedState();
        if (saved) {
          const street = await loadStreet(saved.streetId, saved);
          // Set currentStreet to mount GameCanvas. GameCanvas.onMount calls
          // buildScene then startGame — we don't call startGame here to
          // ensure the scene is built and listeners registered first.
          currentStreet = street;
        }
      } catch (e) {
        console.error('Auto-resume failed, showing street picker:', e);
      } finally {
        resuming = false;
      }
    }
  });

  function handleStreetLoaded(street: StreetData) {
    // Recreate AudioManager if it was disposed (Back button)
    if (!audioManager && cachedKit) {
      try {
        audioManager = new AudioManager(cachedKit, kitBasePath(selectedKitId), new LocalMusicSource(), musicCatalog);
      } catch (e) {
        console.error('Failed to recreate audio:', e);
      }
    }
    currentStreet = street;
  }

  function handleFrame(frame: RenderFrame) {
    latestFrame = frame;

    // When a transition appears, pre-load the target street immediately.
    // The TransitionState stalls at progress 0.9 until we signal ready.
    // transitionPending stays true until frame.transition disappears (swoop
    // completes) — clearing it earlier causes repeated loadStreet/mark_street_ready
    // calls that push target_duration forward indefinitely, stalling the swoop.
    if (frame.transition && !transitionPending && transitionAttempts < MAX_TRANSITION_ATTEMPTS) {
      transitionPending = true;
      transitionTarget = frame.transition.toStreet.replace(/_/g, ' ');
      transitionAttempts++;
      // Capture the generation at the time we start loading — if the swoop
      // times out and a new one starts, the stale promise will pass the old
      // generation, and the backend will ignore it.
      const gen = frame.transition.generation;
      loadStreet(frame.transition.toStreet)
        .then((street) => {
          currentStreet = street;
          // streetTransitionReady failure is non-retryable — repeated
          // mark_street_ready calls push target_duration forward, stalling the
          // swoop. Let the backend timeout (MAX_SWOOP_SECS) handle recovery.
          return streetTransitionReady(gen).catch((e) => {
            console.error('streetTransitionReady failed (backend will timeout):', e);
          });
        })
        .catch((e) => {
          // Only loadStreet failed — allow retry up to MAX_TRANSITION_ATTEMPTS.
          console.error('Street transition failed:', e);
          transitionPending = false;
        });
    }
    if (!frame.transition) {
      transitionPending = false;
      transitionTarget = null;
      transitionAttempts = 0;
    }

    // Process audio events — always call processEvents so cleanup() runs
    // even when no events arrive (player walked out of all jukebox ranges)
    if (audioManager) {
      audioManager.processEvents(frame.audioEvents ?? []);
    }

    // Detect jukebox interaction via audio events
    if (frame.audioEvents?.length) {
      for (const event of frame.audioEvents) {
        if (event.type === 'entityInteract' && event.entityType === 'jukebox') {
          if (jukeboxOpen) {
            jukeboxOpen = false;
            jukeboxInfo = null;
          } else if (frame.interactionPrompt?.entityId) {
            const eid = frame.interactionPrompt.entityId;
            getJukeboxState(eid).then(info => {
              // Guard: player may have walked away while the IPC was in flight
              if (latestFrame?.interactionPrompt?.entityId !== eid) return;
              jukeboxInfo = info;
              jukeboxOpen = true;
              inventoryOpen = false;
              volumeOpen = false;
            }).catch(e => console.error('Failed to get jukebox state:', e));
          }
        }
      }
    }

    // Update jukebox panel state from JukeboxUpdate events
    if (jukeboxOpen && jukeboxInfo && frame.audioEvents?.length) {
      for (const event of frame.audioEvents) {
        if (event.type === 'jukeboxUpdate' && event.entityId === jukeboxInfo.entityId) {
          const trackIndex = jukeboxInfo.playlist.findIndex(t => t.id === event.trackId);
          jukeboxInfo = {
            ...jukeboxInfo,
            currentTrackIndex: trackIndex >= 0 ? trackIndex : jukeboxInfo.currentTrackIndex,
            playing: event.playing,
            elapsedSecs: event.elapsedSecs,
          };
        }
      }
    }

    // Close jukebox panel when the jukebox is no longer the interaction target.
    // The interaction prompt is only present within interact_radius, so this
    // closes the panel at the same boundary the IPC commands enforce.
    if (jukeboxOpen && jukeboxInfo && frame.interactionPrompt?.entityId !== jukeboxInfo.entityId) {
      jukeboxOpen = false;
      jukeboxInfo = null;
    }
  }

  let switchingKit = false;
  async function switchKit(kitId: string) {
    if (switchingKit) {
      // Force the <select> back to the current kit so it doesn't desync
      const current = selectedKitId;
      selectedKitId = '';
      selectedKitId = current;
      return;
    }
    switchingKit = true;
    selectedKitId = kitId;
    try {
      localStorage.setItem('selected-sound-kit', kitId);
    } catch { /* localStorage unavailable */ }

    try {
      const kit = await loadSoundKit(kitId);
      audioManager?.dispose();
      cachedKit = kit;
      audioManager = new AudioManager(kit, kitBasePath(kitId), new LocalMusicSource(), musicCatalog);
    } catch (e) {
      console.error(`Failed to load kit '${kitId}':`, e);
      if (kitId !== 'default') {
        selectedKitId = 'default';
        try {
          localStorage.setItem('selected-sound-kit', 'default');
        } catch { /* localStorage unavailable */ }
        try {
          const fallback = await loadSoundKit('default');
          audioManager?.dispose();
          cachedKit = fallback;
          audioManager = new AudioManager(fallback, kitBasePath('default'), new LocalMusicSource(), musicCatalog);
        } catch (e2) {
          console.error('Fallback to default kit also failed:', e2);
        }
      }
    } finally {
      switchingKit = false;
    }
  }

  function toggleDebug() {
    debugMode = !debugMode;
  }
</script>

<svelte:window onkeydown={(e) => {
  if (e.key === 'F3') { e.preventDefault(); toggleDebug(); }
  if ((e.key === 'i' || e.key === 'I') && currentStreet && !chatFocused && !jukeboxOpen) {
    e.preventDefault();
    inventoryOpen = !inventoryOpen;
    if (inventoryOpen) volumeOpen = false;
  }
  if ((e.key === 'p' || e.key === 'P') && currentStreet && !chatFocused && !jukeboxOpen) {
    e.preventDefault();
    volumeOpen = !volumeOpen;
    if (volumeOpen) inventoryOpen = false;
  }
  if ((e.key === 'j' || e.key === 'J') && jukeboxOpen && !chatFocused) {
    e.preventDefault();
    jukeboxOpen = false;
    jukeboxInfo = null;
  }
}} />

<main>
  <div role="status" aria-live="polite" class="sr-only">
    {#if checkingIdentity || resuming}Loading, please wait…{/if}
  </div>
  {#if checkingIdentity || resuming}
    <!-- visual placeholder while loading -->
  {:else if !identityReady}
    <IdentitySetup onComplete={() => { identityReady = true; }} />
  {:else if currentStreet}
    <GameCanvas street={currentStreet} {debugMode} {chatFocused} {inventoryOpen} uiOpen={volumeOpen || jukeboxOpen} onFrame={handleFrame} />
    <DebugOverlay frame={latestFrame} visible={debugMode} />
    <ChatInput onFocusChange={(focused) => { chatFocused = focused; }} />
    <NetworkStatus />
    <GameNotification feedback={latestFrame?.pickupFeedback ?? []} />
    <VolumeSettings
      {audioManager}
      visible={volumeOpen}
      {soundKits}
      {selectedKitId}
      onClose={() => { volumeOpen = false; }}
      onKitChange={switchKit}
    />
    <JukeboxPanel
      info={jukeboxInfo}
      visible={jukeboxOpen}
      onClose={() => { jukeboxOpen = false; jukeboxInfo = null; }}
      onPlay={(eid) => jukeboxPlay(eid).catch(e => console.error('jukebox play:', e))}
      onPause={(eid) => jukeboxPause(eid).catch(e => console.error('jukebox pause:', e))}
      onSelectTrack={(eid, idx) => jukeboxSelectTrack(eid, idx).catch(e => console.error('jukebox select:', e))}
    />
    <InventoryPanel
      inventory={latestFrame?.inventory ?? null}
      {recipes}
      visible={inventoryOpen}
      onClose={() => { inventoryOpen = false; }}
    />
    <div role="status" aria-live="polite" class="sr-only">
      {#if transitionPending && transitionTarget}Travelling to {transitionTarget}…{/if}
    </div>
    <button type="button" class="back-btn" onclick={async () => {
      try {
        await stopGame();
      } catch (e) {
        console.error('stopGame failed:', e);
      } finally {
        audioManager?.dispose();
        audioManager = null;
        currentStreet = null;
        latestFrame = null;
        jukeboxOpen = false;
        jukeboxInfo = null;
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

  .back-btn:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: 2px;
  }

  :global(.sr-only) {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
