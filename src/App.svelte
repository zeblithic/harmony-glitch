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
  import ShopPanel from './lib/components/ShopPanel.svelte';
  import CurrantHud from './lib/components/CurrantHud.svelte';
  import AvatarEditor from './lib/components/AvatarEditor.svelte';
  import { stopGame, loadStreet, getIdentity, streetTransitionReady, getRecipes, getSavedState, listSoundKits, jukeboxPlay, jukeboxPause, jukeboxSelectTrack, getJukeboxState, getStoreState, vendorBuy, vendorSell } from './lib/ipc';
  import type { StreetData, RenderFrame, RecipeDef, SoundKitMeta, JukeboxInfo, StoreState, AvatarManifest } from './lib/types';
  import type { GameRenderer } from './lib/engine/renderer';
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
  let jukeboxCloseFrames = 0; // frames since jukebox lost interaction prompt
  let shopOpen = $state(false);
  let storeState = $state<StoreState | null>(null);
  let shopCloseFrames = 0;
  let musicCatalog = $state<TrackCatalog>({ tracks: {} });
  let avatarEditorOpen = $state(false);
  let avatarManifest = $state<AvatarManifest | null>(null);
  let gameRenderer = $state<GameRenderer | null>(null);
  let needsAvatarSetup = $state(false);

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

    // Load avatar manifest
    try {
      const avatarResponse = await fetch('/assets/sprites/avatar/manifest.json');
      if (avatarResponse.ok) {
        avatarManifest = await avatarResponse.json();
      }
    } catch (e) {
      console.error('Failed to load avatar manifest:', e);
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
            jukeboxCloseFrames = 0;
          } else if (frame.interactionPrompt?.entityId) {
            const eid = frame.interactionPrompt.entityId;
            getJukeboxState(eid).then(info => {
              // Guard: player may have walked away while the IPC was in flight
              if (latestFrame?.interactionPrompt?.entityId !== eid) return;
              jukeboxInfo = info;
              jukeboxOpen = true;
              jukeboxCloseFrames = 0;
              inventoryOpen = false;
              volumeOpen = false;
              shopOpen = false;
              storeState = null;
              avatarEditorOpen = false;
            }).catch(e => console.error('Failed to get jukebox state:', e));
          }
        }
      }
    }

    // Detect vendor interaction via audio events
    if (frame.audioEvents?.length) {
      for (const event of frame.audioEvents) {
        if (event.type === 'entityInteract' && event.entityType === 'vendor') {
          if (shopOpen) {
            shopOpen = false;
            storeState = null;
            shopCloseFrames = 0;
          } else if (frame.interactionPrompt?.entityId) {
            const eid = frame.interactionPrompt.entityId;
            getStoreState(eid).then(state => {
              // Guard: player may have walked away while the IPC was in flight
              if (latestFrame?.interactionPrompt?.entityId !== eid) return;
              storeState = state;
              shopOpen = true;
              shopCloseFrames = 0;
              inventoryOpen = false;
              volumeOpen = false;
              jukeboxOpen = false;
              jukeboxInfo = null;
              avatarEditorOpen = false;
            }).catch(e => console.error('Failed to get store state:', e));
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

    // Close jukebox panel when the player walks out of interact_radius.
    // Uses the interaction prompt as the signal — present when the jukebox
    // is the nearest interactable within interact_radius. Debounced by 2
    // frames to ride through the one-frame null gap after ground item pickup.
    if (jukeboxOpen && jukeboxInfo) {
      if (frame.interactionPrompt?.entityId === jukeboxInfo.entityId) {
        jukeboxCloseFrames = 0;
      } else {
        jukeboxCloseFrames++;
        if (jukeboxCloseFrames >= 2) {
          jukeboxOpen = false;
          jukeboxInfo = null;
          jukeboxCloseFrames = 0;
        }
      }
    }

    // Close shop panel when the player walks out of interact_radius.
    // Same 2-frame debounce as jukebox to ride through one-frame null gaps.
    if (shopOpen && storeState) {
      if (frame.interactionPrompt?.entityId === storeState.entityId) {
        shopCloseFrames = 0;
      } else {
        shopCloseFrames++;
        if (shopCloseFrames >= 2) {
          shopOpen = false;
          storeState = null;
          shopCloseFrames = 0;
        }
      }
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
  if ((e.key === 'i' || e.key === 'I') && currentStreet && !chatFocused && !jukeboxOpen && !shopOpen) {
    e.preventDefault();
    inventoryOpen = !inventoryOpen;
    if (inventoryOpen) { volumeOpen = false; avatarEditorOpen = false; shopOpen = false; storeState = null; shopCloseFrames = 0; }
  }
  if ((e.key === 'p' || e.key === 'P') && currentStreet && !chatFocused && !jukeboxOpen && !shopOpen) {
    e.preventDefault();
    volumeOpen = !volumeOpen;
    if (volumeOpen) { inventoryOpen = false; avatarEditorOpen = false; }
  }
  if ((e.key === 'c' || e.key === 'C') && currentStreet && !chatFocused && !jukeboxOpen && !shopOpen) {
    e.preventDefault();
    avatarEditorOpen = !avatarEditorOpen;
    if (avatarEditorOpen) { inventoryOpen = false; volumeOpen = false; shopOpen = false; storeState = null; shopCloseFrames = 0; }
  }
  if ((e.key === 'j' || e.key === 'J') && jukeboxOpen && !chatFocused) {
    e.preventDefault();
    jukeboxOpen = false;
    jukeboxInfo = null;
    jukeboxCloseFrames = 0;
  }
}} />

<main>
  <div role="status" aria-live="polite" class="sr-only">
    {#if checkingIdentity || resuming}Loading, please wait…{/if}
  </div>
  {#if checkingIdentity || resuming}
    <!-- visual placeholder while loading -->
  {:else if !identityReady}
    <IdentitySetup onComplete={() => { identityReady = true; needsAvatarSetup = true; }} />
  {:else if needsAvatarSetup}
    <div class="first-run-avatar">
      <AvatarEditor
        visible={true}
        firstRun={true}
        manifest={avatarManifest}
        renderer={null}
        onClose={() => { needsAvatarSetup = false; }}
      />
    </div>
  {:else if currentStreet}
    <GameCanvas street={currentStreet} {debugMode} {chatFocused} {inventoryOpen} uiOpen={volumeOpen || jukeboxOpen || shopOpen || avatarEditorOpen} onFrame={handleFrame} onRendererReady={(r) => { gameRenderer = r; }} />
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
      onClose={() => { jukeboxOpen = false; jukeboxInfo = null; jukeboxCloseFrames = 0; }}
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
    <ShopPanel
      {storeState}
      visible={shopOpen}
      onClose={() => { shopOpen = false; storeState = null; shopCloseFrames = 0; }}
      onBuy={async (itemId, count) => {
        if (!storeState) return;
        const eid = storeState.entityId;
        try {
          await vendorBuy(eid, itemId, count);
          storeState = await getStoreState(eid);
        } catch (e) {
          console.error('Buy failed:', e);
        }
      }}
      onSell={async (itemId, count) => {
        if (!storeState) return;
        const eid = storeState.entityId;
        try {
          await vendorSell(eid, itemId, count);
          storeState = await getStoreState(eid);
        } catch (e) {
          console.error('Sell failed:', e);
        }
      }}
    />
    <CurrantHud currants={latestFrame?.currants ?? 0} />
    <AvatarEditor
      visible={avatarEditorOpen}
      manifest={avatarManifest}
      renderer={gameRenderer}
      onClose={() => { avatarEditorOpen = false; }}
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
        gameRenderer = null;
        currentStreet = null;
        latestFrame = null;
        avatarEditorOpen = false;
        jukeboxOpen = false;
        jukeboxInfo = null;
        jukeboxCloseFrames = 0;
        shopOpen = false;
        storeState = null;
        shopCloseFrames = 0;
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

  .first-run-avatar {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
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
