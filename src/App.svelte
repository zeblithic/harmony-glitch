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
  import EnergyHud from './lib/components/EnergyHud.svelte';
  import MoodHud from './lib/components/MoodHud.svelte';
  import ImaginationHud from './lib/components/ImaginationHud.svelte';
  import UpgradePanel from './lib/components/UpgradePanel.svelte';
  import AvatarEditor from './lib/components/AvatarEditor.svelte';
  import TradePanel from './lib/components/TradePanel.svelte';
  import TradePrompt from './lib/components/TradePrompt.svelte';
  import StreetNameHud from './lib/components/StreetNameHud.svelte';
  import SkillsPanel from './lib/components/SkillsPanel.svelte';
  import DialoguePanel from './lib/components/DialoguePanel.svelte';
  import QuestLogPanel from './lib/components/QuestLogPanel.svelte';
  import EmoteAnimation from './lib/components/EmoteAnimation.svelte';
  import EmotePalette from './lib/components/EmotePalette.svelte';
  import PartyPanel from './lib/components/PartyPanel.svelte';
  import BuddyListPanel from './lib/components/BuddyListPanel.svelte';
  import SocialPrompt from './lib/components/SocialPrompt.svelte';
  import BuddyRequestPrompt from './lib/components/BuddyRequestPrompt.svelte';
  import PartyInvitePrompt from './lib/components/PartyInvitePrompt.svelte';
  import { stopGame, loadStreet, getIdentity, streetTransitionReady, getRecipes, getSavedState, listSoundKits, jukeboxPlay, jukeboxPause, jukeboxSelectTrack, getJukeboxState, getStoreState, vendorBuy, vendorSell, tradeInitiate, tradeAccept, tradeDecline, tradeUpdateOffer, tradeLock, tradeUnlock, tradeCancel, tradeGetState, onTradeEvent, getSkills, getDialogueState, closeDialogue, getQuestLog, emoteHi, emote as emoteFire, onEmoteReceived, partyLeave, partyKick, buddyRemove, blockPlayer, onBuddyEvent, onPartyEvent, getBuddyList, getPartyState, buddyRequest, buddyAccept, buddyDecline, partyInvite, partyAccept, partyDecline } from './lib/ipc';
  import type { PartyMemberInfo, BuddyEntry } from './lib/ipc';
  import type { StreetData, RenderFrame, RecipeDef, SkillDef, SoundKitMeta, JukeboxInfo, StoreState, AvatarManifest, TradeFrame, TradeEvent, SaveItemStack, DialogueFrame, QuestLogFrame, EmoteKind, EmoteFireResult, EmoteAnimationFrame, HiVariant } from './lib/types';
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
  let upgradePanelOpen = $state(false);
  let tradeOpen = $state(false);
  let tradeFrame = $state<TradeFrame | null>(null);
  let tradeStateVersion = 0;
  let tradeRequestVisible = $state(false);
  let tradeRequestName = $state('');
  let skillsOpen = $state(false);
  let skills = $state<SkillDef[]>([]);
  let dialogueOpen = $state(false);
  let dialogueFrame = $state<DialogueFrame | null>(null);
  let dialogueEntityId = $state<string | null>(null);
  let dialogueCloseFrames = 0;
  let dialogueClosing: Promise<void> | null = null;
  let questLogOpen = $state(false);
  let questLog = $state<QuestLogFrame | null>(null);

  // Emote palette state
  let emotePaletteOpen = $state(false);
  let emoteCooldowns = $state<Record<string, number>>({});
  let emotePrivacy = $state({ hug: true, high_five: true });

  /**
   * Active emote animations keyed by playerHash ("self" for us).
   * Each lives for ~2s then expires (matches CSS emote-float duration).
   */
  let activeEmotes = $state<Map<string, EmoteAnimationFrame>>(new Map());

  function spawnEmoteAnimation(playerKey: string, kind: EmoteKind, targetHash: string | null) {
    const kindStr: EmoteAnimationFrame['kind'] =
      typeof kind === 'object' && 'hi' in kind ? 'hi' : kind;
    const variant = typeof kind === 'object' && 'hi' in kind ? kind.hi : '';
    const next = new Map(activeEmotes);
    next.set(playerKey, {
      kind: kindStr,
      variant,
      targetHash,
      startedAt: performance.now(),
    });
    activeEmotes = next;
    setTimeout(() => {
      const pruned = new Map(activeEmotes);
      pruned.delete(playerKey);
      activeEmotes = pruned;
    }, 2000);
  }

  // Social state
  let buddyListOpen = $state(false);
  let buddies = $state<BuddyEntry[]>([]);
  let partyInParty = $state(false);
  let partyMembers = $state<PartyMemberInfo[]>([]);
  let partyIsLeader = $state(false);

  let ourAddressHash = $state('');
  let buddyRequestVisible = $state(false);
  let buddyRequestName = $state('');
  let buddyRequestHash = $state('');
  let partyInviteVisible = $state(false);
  let partyInviteName = $state('');
  let partyInviteCount = $state(0);

  async function refreshBuddyList() {
    try {
      const result = await getBuddyList();
      buddies = result.buddies;
    } catch (e) {
      console.error('Failed to refresh buddy list:', e);
    }
  }

  async function refreshPartyState() {
    try {
      const result = await getPartyState();
      partyInParty = result.inParty;
      partyMembers = result.members;
      partyIsLeader = result.leader === ourAddressHash;
    } catch (e) {
      console.error('Failed to refresh party state:', e);
    }
  }

  onMount(async () => {
    try {
      const identity = await getIdentity();
      identityReady = identity.setupComplete;
      ourAddressHash = identity.addressHash;
    } catch {
      identityReady = false;
    } finally {
      checkingIdentity = false;
    }

    // Load recipes and skills once at startup
    try {
      recipes = await getRecipes();
    } catch (e) {
      console.error('Failed to load recipes:', e);
    }
    try {
      skills = await getSkills();
    } catch (e) {
      console.error('Failed to load skills:', e);
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

    // Listen for trade events
    const unlistenTrade = await onTradeEvent((event: TradeEvent) => {
      switch (event.type) {
        case 'request':
          tradeRequestName = event.initiatorName;
          tradeRequestVisible = true;
          break;
        case 'accepted': {
          tradeRequestVisible = false;
          tradeOpen = true;
          inventoryOpen = false; shopOpen = false; volumeOpen = false; avatarEditorOpen = false;
          const v1 = ++tradeStateVersion;
          tradeGetState().then(f => { if (v1 === tradeStateVersion) tradeFrame = f; }).catch(console.error);
          break;
        }
        case 'declined':
          tradeRequestVisible = false;
          tradeOpen = false;
          tradeFrame = null;
          ++tradeStateVersion;
          break;
        case 'updated':
          tradeFrame = event.tradeFrame;
          ++tradeStateVersion;
          break;
        case 'locked':
        case 'unlocked': {
          const v2 = ++tradeStateVersion;
          tradeGetState().then(f => { if (v2 === tradeStateVersion) tradeFrame = f; }).catch(console.error);
          break;
        }
        case 'completed':
          tradeOpen = false;
          tradeFrame = null;
          ++tradeStateVersion;
          break;
        case 'cancelled':
          tradeOpen = false;
          tradeFrame = null;
          tradeRequestVisible = false;
          ++tradeStateVersion;
          break;
      }
    });

    // Listen for buddy events
    const unlistenBuddy = await onBuddyEvent((event) => {
      switch (event.type) {
        case 'request_received':
          buddyRequestName = event.fromName ?? 'Unknown';
          buddyRequestHash = event.fromHash;
          buddyRequestVisible = true;
          break;
        case 'accepted':
        case 'declined':
        case 'removed':
          refreshBuddyList();
          break;
      }
    });

    // Listen for party events
    const unlistenParty = await onPartyEvent((event) => {
      switch (event.type) {
        case 'invite_received':
          partyInviteName = event.leaderName;
          partyInviteCount = event.memberCount;
          partyInviteVisible = true;
          break;
        case 'joined':
          partyInviteVisible = false;
          refreshPartyState();
          break;
        case 'dissolved':
          partyInParty = false;
          partyMembers = [];
          partyIsLeader = false;
          break;
        default:
          refreshPartyState();
          break;
      }
    });

    // Initial social state fetch
    refreshBuddyList();
    refreshPartyState();

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
          // Re-fetch recipes now that skill_progress is restored from save
          try { recipes = await getRecipes(); } catch { /* ignore */ }
        }
      } catch (e) {
        console.error('Auto-resume failed, showing street picker:', e);
      } finally {
        resuming = false;
      }
    }

    return () => {
      unlistenTrade();
      unlistenBuddy();
      unlistenParty();
    };
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

    // Refresh recipes when a skill completes (locked status changes)
    if (frame.audioEvents?.some(e => e.type === 'skillLearned')) {
      getRecipes().then(r => { recipes = r; }).catch(console.error);
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

    // Detect NPC dialogue interaction via audio events
    if (frame.audioEvents?.length) {
      for (const event of frame.audioEvents) {
        if (event.type === 'entityInteract' && event.entityType === 'npc') {
          if (!dialogueOpen && !dialogueClosing && frame.interactionPrompt?.entityId) {
            const eid = frame.interactionPrompt.entityId;
            getDialogueState(eid).then(dialogFrame => {
              if (latestFrame?.interactionPrompt?.entityId !== eid) return;
              dialogueFrame = dialogFrame;
              dialogueEntityId = eid;
              dialogueOpen = true;
              dialogueCloseFrames = 0;
              inventoryOpen = false;
              volumeOpen = false;
              jukeboxOpen = false;
              jukeboxInfo = null;
              shopOpen = false;
              storeState = null;
              avatarEditorOpen = false;
              skillsOpen = false;
              questLogOpen = false;
            }).catch(e => console.error('Failed to get dialogue state:', e));
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

    // Close dialogue panel when the player walks out of interact_radius.
    if (dialogueOpen && dialogueEntityId) {
      if (frame.interactionPrompt?.entityId === dialogueEntityId) {
        dialogueCloseFrames = 0;
      } else {
        dialogueCloseFrames++;
        if (dialogueCloseFrames >= 2) {
          dialogueOpen = false;
          dialogueFrame = null;
          dialogueEntityId = null;
          dialogueCloseFrames = 0;
          dialogueClosing = closeDialogue().catch(console.error).then(() => { dialogueClosing = null; });
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

  async function handleEmoteSelect(kind: EmoteKind) {
    if (typeof kind === 'object' && 'hi' in kind) {
      emoteHi().catch(console.error);
      spawnEmoteAnimation('self', kind, null);
      return;
    }
    const target = latestFrame?.nearestSocialTarget?.addressHash ?? null;
    const result: EmoteFireResult = await emoteFire(kind, target);
    if (result.type === 'success') {
      spawnEmoteAnimation('self', kind, target);
    } else if (result.type === 'cooldown') {
      emoteCooldowns = { ...emoteCooldowns, [kind as string]: result.remaining_ms };
    }
  }

  // Countdown tick — decrements cooldowns every 250ms while palette is open
  $effect(() => {
    if (!emotePaletteOpen) return;
    const interval = setInterval(() => {
      const next: Record<string, number> = {};
      for (const [k, v] of Object.entries(emoteCooldowns)) {
        const remaining = v - 250;
        if (remaining > 0) next[k] = remaining;
      }
      emoteCooldowns = next;
    }, 250);
    return () => clearInterval(interval);
  });

  // Subscribe to emote_received events and spawn animations above the sender's avatar
  $effect(() => {
    let unlisten: (() => void) | undefined;
    onEmoteReceived((evt) => {
      const kind: EmoteKind = evt.kind === 'hi'
        ? { hi: (evt.variant ?? 'hi') as HiVariant }
        : evt.kind;
      spawnEmoteAnimation(evt.senderHash, kind, null);
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
  });
</script>

<svelte:window onkeydown={(e) => {
  if (e.key === 'F3') { e.preventDefault(); toggleDebug(); }
  if ((e.key === 'i' || e.key === 'I') && currentStreet && !chatFocused && !jukeboxOpen && !shopOpen && !dialogueOpen) {
    e.preventDefault();
    inventoryOpen = !inventoryOpen;
    if (inventoryOpen) { volumeOpen = false; avatarEditorOpen = false; shopOpen = false; storeState = null; shopCloseFrames = 0; }
  }
  if ((e.key === 'p' || e.key === 'P') && currentStreet && !chatFocused && !jukeboxOpen && !shopOpen && !dialogueOpen) {
    e.preventDefault();
    volumeOpen = !volumeOpen;
    if (volumeOpen) { inventoryOpen = false; avatarEditorOpen = false; }
  }
  if ((e.key === 'c' || e.key === 'C') && currentStreet && !chatFocused && !jukeboxOpen && !shopOpen && !dialogueOpen) {
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
  if ((e.key === 'k' || e.key === 'K') && currentStreet && !chatFocused && !jukeboxOpen && !shopOpen && !dialogueOpen) {
    e.preventDefault();
    skillsOpen = !skillsOpen;
    if (skillsOpen) { inventoryOpen = false; volumeOpen = false; avatarEditorOpen = false; shopOpen = false; storeState = null; shopCloseFrames = 0; questLogOpen = false; }
  }
  if ((e.key === 'q' || e.key === 'Q') && currentStreet && !chatFocused && !jukeboxOpen && !shopOpen && !dialogueOpen) {
    e.preventDefault();
    questLogOpen = !questLogOpen;
    if (questLogOpen) {
      getQuestLog().then(log => { questLog = log; }).catch(console.error);
      inventoryOpen = false; volumeOpen = false; avatarEditorOpen = false; shopOpen = false; storeState = null; shopCloseFrames = 0; skillsOpen = false;
    }
  }
  // H key: send Hi emote
  if ((e.key === 'h' || e.key === 'H') && currentStreet && !chatFocused && latestFrame) {
    e.preventDefault();
    emoteHi().catch(console.error);
  }
  // E key: toggle emote palette
  if ((e.key === 'e' || e.key === 'E') && currentStreet && !chatFocused && !jukeboxOpen && !shopOpen && !dialogueOpen && !tradeOpen && latestFrame) {
    e.preventDefault();
    emotePaletteOpen = !emotePaletteOpen;
    if (emotePaletteOpen) {
      inventoryOpen = false; volumeOpen = false; avatarEditorOpen = false; skillsOpen = false; questLogOpen = false;
    }
  }
  // T key: initiate trade with nearest remote player (computed by Rust)
  if ((e.key === 't' || e.key === 'T') && currentStreet && !chatFocused && !tradeOpen && !tradeRequestVisible && !shopOpen && latestFrame) {
    const target = latestFrame.nearestSocialTarget;
    if (target) {
      e.preventDefault();
      tradeInitiate(target.addressHash).then(() => {
        tradeOpen = true;
        inventoryOpen = false; shopOpen = false; volumeOpen = false; avatarEditorOpen = false;
        tradeGetState().then(f => { tradeFrame = f; }).catch(console.error);
      }).catch(console.error);
    }
  }
  // Y key: invite nearest player to party
  if ((e.key === 'y' || e.key === 'Y') && currentStreet && !chatFocused && !tradeOpen && !shopOpen && !dialogueOpen && latestFrame) {
    const target = latestFrame.nearestSocialTarget;
    if (target && !target.inParty && (partyIsLeader || !partyInParty)) {
      e.preventDefault();
      partyInvite(target.addressHash).then(refreshPartyState).catch(console.error);
    }
  }
  // B key: send buddy request to nearest player
  if ((e.key === 'b' || e.key === 'B') && currentStreet && !chatFocused && !tradeOpen && !shopOpen && !dialogueOpen && latestFrame) {
    const target = latestFrame.nearestSocialTarget;
    if (target && !target.isBuddy) {
      e.preventDefault();
      buddyRequest(target.addressHash).catch(console.error);
    }
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
    <GameCanvas street={currentStreet} {debugMode} {chatFocused} {inventoryOpen} uiOpen={volumeOpen || jukeboxOpen || shopOpen || avatarEditorOpen || tradeOpen} onFrame={handleFrame} onRendererReady={(r) => { gameRenderer = r; }} />
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
      energy={latestFrame?.energy ?? 600}
      maxEnergy={latestFrame?.maxEnergy ?? 600}
      activeCraft={latestFrame?.activeCraft ?? null}
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
    <TradePrompt
      initiatorName={tradeRequestName}
      visible={tradeRequestVisible}
      onAccept={async () => {
        try {
          await tradeAccept();
        } catch (e) {
          console.error('Trade accept failed:', e);
          tradeRequestVisible = false;
        }
      }}
      onDecline={async () => {
        try {
          await tradeDecline();
        } catch (e) {
          console.error('Trade decline failed:', e);
        }
        tradeRequestVisible = false;
      }}
    />
    <BuddyRequestPrompt
      senderName={buddyRequestName}
      visible={buddyRequestVisible}
      onAccept={async () => {
        try {
          await buddyAccept(buddyRequestHash);
          await refreshBuddyList();
        } catch (e) {
          console.error('Buddy accept failed:', e);
        }
        buddyRequestVisible = false;
      }}
      onDecline={async () => {
        try {
          await buddyDecline(buddyRequestHash);
        } catch (e) {
          console.error('Buddy decline failed:', e);
        }
        buddyRequestVisible = false;
      }}
    />
    <PartyInvitePrompt
      leaderName={partyInviteName}
      memberCount={partyInviteCount}
      visible={partyInviteVisible}
      onAccept={async () => {
        try {
          await partyAccept();
        } catch (e) {
          console.error('Party accept failed:', e);
        }
        partyInviteVisible = false;
      }}
      onDecline={async () => {
        try {
          await partyDecline();
        } catch (e) {
          console.error('Party decline failed:', e);
        }
        partyInviteVisible = false;
      }}
    />
    <TradePanel
      {tradeFrame}
      inventory={latestFrame?.inventory ?? null}
      currants={latestFrame?.currants ?? 0}
      visible={tradeOpen && tradeFrame !== null}
      onClose={() => { tradeOpen = false; tradeFrame = null; }}
      onAddItem={async (itemId, count) => {
        if (!tradeFrame) return;
        const existingIdx = tradeFrame.localOffer.items.findIndex(i => i.itemId === itemId);
        let items;
        if (existingIdx >= 0) {
          items = tradeFrame.localOffer.items.map((i, idx) =>
            idx === existingIdx ? { ...i, count: i.count + count } : i
          );
        } else {
          items = [...tradeFrame.localOffer.items, { itemId, name: itemId, icon: itemId, count }];
        }
        try {
          await tradeUpdateOffer(items.map(i => ({ itemId: i.itemId, count: i.count })), tradeFrame.localOffer.currants);
          tradeFrame = await tradeGetState();
        } catch (e) { console.error('Trade update failed:', e); }
      }}
      onRemoveItem={async (itemId, count) => {
        if (!tradeFrame) return;
        const items = tradeFrame.localOffer.items
          .map(i => i.itemId === itemId ? { ...i, count: i.count - count } : i)
          .filter(i => i.count > 0);
        try {
          await tradeUpdateOffer(items.map(i => ({ itemId: i.itemId, count: i.count })), tradeFrame.localOffer.currants);
          tradeFrame = await tradeGetState();
        } catch (e) { console.error('Trade update failed:', e); }
      }}
      onSetCurrants={async (amount) => {
        if (!tradeFrame) return;
        try {
          await tradeUpdateOffer(
            tradeFrame.localOffer.items.map(i => ({ itemId: i.itemId, count: i.count })),
            amount
          );
          tradeFrame = await tradeGetState();
        } catch (e) { console.error('Trade update failed:', e); }
      }}
      onLock={async () => {
        try { await tradeLock(); } catch (e) { console.error('Trade lock failed:', e); }
      }}
      onUnlock={async () => {
        try { await tradeUnlock(); } catch (e) { console.error('Trade unlock failed:', e); }
      }}
      onCancel={async () => {
        try { await tradeCancel(); } catch (e) { console.error('Trade cancel failed:', e); }
        tradeOpen = false;
        tradeFrame = null;
      }}
    />
    <StreetNameHud name={currentStreet.name} />
    <CurrantHud currants={latestFrame?.currants ?? 0} />
    <EnergyHud energy={latestFrame?.energy ?? 600} maxEnergy={latestFrame?.maxEnergy ?? 600} />
    <MoodHud mood={latestFrame?.mood ?? 100} maxMood={latestFrame?.maxMood ?? 100} />
    <ImaginationHud
      imagination={latestFrame?.imagination ?? 0}
      onOpen={() => { upgradePanelOpen = true; }}
    />
    <UpgradePanel
      visible={upgradePanelOpen}
      imagination={latestFrame?.imagination ?? 0}
      upgrades={latestFrame?.upgrades ?? { energyTankTier: 0, hagglingTier: 0 }}
      maxEnergy={latestFrame?.maxEnergy ?? 600}
      onClose={() => { upgradePanelOpen = false; }}
    />
    <SkillsPanel
      {skills}
      skillProgress={latestFrame?.skillProgress ?? null}
      imagination={latestFrame?.imagination ?? 0}
      visible={skillsOpen}
      onClose={async () => {
        skillsOpen = false;
        // Refresh recipes when closing skills panel (locked status may have changed)
        try { recipes = await getRecipes(); } catch { /* ignore */ }
      }}
    />
    <DialoguePanel
      {dialogueFrame}
      visible={dialogueOpen}
      onClose={() => {
        dialogueOpen = false;
        dialogueFrame = null;
        dialogueEntityId = null;
        dialogueCloseFrames = 0;
        dialogueClosing = closeDialogue().catch(console.error).then(() => { dialogueClosing = null; });
      }}
      onFrameUpdate={(frame) => { dialogueFrame = frame; }}
    />
    <QuestLogPanel
      {questLog}
      visible={questLogOpen}
      onClose={() => { questLogOpen = false; }}
    />
    <EmotePalette
      visible={emotePaletteOpen}
      onClose={() => { emotePaletteOpen = false; }}
      onSelect={handleEmoteSelect}
      cooldowns={emoteCooldowns}
      nearestTarget={latestFrame?.nearestSocialTarget?.addressHash ?? null}
      privacy={emotePrivacy}
    />
    <PartyPanel
      inParty={partyInParty}
      members={partyMembers}
      isLeader={partyIsLeader}
      onLeave={() => partyLeave().then(refreshPartyState).catch(console.error)}
      onKick={(hash) => partyKick(hash).then(refreshPartyState).catch(console.error)}
    />
    <BuddyListPanel
      {buddies}
      visible={buddyListOpen}
      onRemove={(hash) => buddyRemove(hash).then(refreshBuddyList).catch(console.error)}
      onBlock={(hash) => blockPlayer(hash).then(refreshBuddyList).catch(console.error)}
    />
    {#if latestFrame}
      <!-- Remote player emote animations driven by onEmoteReceived listener -->
      {#each latestFrame.remotePlayers as rp (rp.addressHash)}
        {#if activeEmotes.has(rp.addressHash)}
          <EmoteAnimation
            animation={activeEmotes.get(rp.addressHash)!}
            x={rp.x - latestFrame.camera.x}
            y={rp.y - latestFrame.camera.y}
          />
        {/if}
      {/each}
      <!-- Self emote animation (sender's own fire) -->
      {#if activeEmotes.has('self')}
        <EmoteAnimation
          animation={activeEmotes.get('self')!}
          x={latestFrame.player.x - latestFrame.camera.x}
          y={latestFrame.player.y - latestFrame.camera.y}
        />
      {/if}
    {/if}
    {#if latestFrame?.nearestSocialTarget}
      {@const target = latestFrame.nearestSocialTarget}
      <SocialPrompt
        visible={!chatFocused && !tradeOpen && !shopOpen && !dialogueOpen}
        targetName={target.displayName}
        canHi={true}
        canTrade={true}
        canInvite={!target.inParty && (partyIsLeader || !partyInParty)}
        canBuddy={!target.isBuddy}
        onHi={() => emoteHi().catch(console.error)}
        onTrade={() => tradeInitiate(target.addressHash).then(() => {
          tradeOpen = true;
          inventoryOpen = false; shopOpen = false; volumeOpen = false; avatarEditorOpen = false;
          tradeGetState().then(f => { tradeFrame = f; }).catch(console.error);
        }).catch(console.error)}
        onInvite={() => partyInvite(target.addressHash).then(refreshPartyState).catch(console.error)}
        onBuddy={() => buddyRequest(target.addressHash).catch(console.error)}
      />
    {/if}
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
