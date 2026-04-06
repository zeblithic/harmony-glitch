import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { StreetData, InputState, RenderFrame, NetworkStatus, PlayerIdentity, ChatEvent, RecipeDef, SavedState, SoundKitMeta, JukeboxInfo, AvatarAppearance, StoreState, EatResult, TradeFrame, TradeEvent, SaveItemStack, SkillDef } from './types';
import type { SoundKit } from './engine/audio';

export async function listStreets(): Promise<string[]> {
  return invoke<string[]>('list_streets');
}

export async function loadStreet(name: string, saveState?: SavedState | null): Promise<StreetData> {
  return invoke<StreetData>('load_street', {
    name,
    saveState: saveState ?? null,
  });
}

export async function getSavedState(): Promise<SavedState | null> {
  return invoke<SavedState | null>('get_saved_state');
}

export async function sendInput(input: InputState): Promise<void> {
  return invoke('send_input', { input });
}

export async function startGame(): Promise<void> {
  return invoke('start_game');
}

export async function stopGame(): Promise<void> {
  return invoke('stop_game');
}

export async function onRenderFrame(
  callback: (frame: RenderFrame) => void
): Promise<UnlistenFn> {
  return listen<RenderFrame>('render_frame', (event) => {
    callback(event.payload);
  });
}

export async function sendChat(message: string): Promise<void> {
  return invoke('send_chat', { message });
}

export async function getNetworkStatus(): Promise<NetworkStatus> {
  return invoke('get_network_status');
}

export async function getIdentity(): Promise<PlayerIdentity> {
  return invoke('get_identity');
}

export async function setDisplayName(name: string): Promise<void> {
  return invoke('set_display_name', { name });
}

export async function onChatMessage(
  callback: (event: ChatEvent) => void
): Promise<UnlistenFn> {
  return listen<ChatEvent>('chat_message', (event) => {
    callback(event.payload);
  });
}

export async function dropItem(slot: number): Promise<void> {
  return invoke('drop_item', { slot });
}

export async function streetTransitionReady(generation: number): Promise<void> {
  return invoke('street_transition_ready', { generation });
}

export async function getRecipes(): Promise<RecipeDef[]> {
  return invoke<RecipeDef[]>('get_recipes');
}

export async function craftRecipe(recipeId: string): Promise<void> {
  return invoke('craft_recipe', { recipeId });
}

export async function listSoundKits(): Promise<SoundKitMeta[]> {
  return invoke<SoundKitMeta[]>('list_sound_kits');
}

export async function readSoundKit(kitId: string): Promise<SoundKit> {
  return invoke<SoundKit>('read_sound_kit', { kitId });
}

export async function jukeboxPlay(entityId: string): Promise<void> {
  return invoke('jukebox_play', { entityId });
}

export async function jukeboxPause(entityId: string): Promise<void> {
  return invoke('jukebox_pause', { entityId });
}

export async function jukeboxSelectTrack(entityId: string, trackIndex: number): Promise<void> {
  return invoke('jukebox_select_track', { entityId, trackIndex });
}

export async function getJukeboxState(entityId: string): Promise<JukeboxInfo> {
  return invoke<JukeboxInfo>('get_jukebox_state', { entityId });
}

export async function getAvatar(): Promise<AvatarAppearance> {
  return invoke<AvatarAppearance>('get_avatar');
}

export async function setAvatar(appearance: AvatarAppearance): Promise<AvatarAppearance> {
  return invoke<AvatarAppearance>('set_avatar', { appearance });
}

export async function getStoreState(entityId: string): Promise<StoreState> {
  return invoke<StoreState>('get_store_state', { entityId });
}

export async function vendorBuy(entityId: string, itemId: string, count: number): Promise<number> {
  return invoke<number>('vendor_buy', { entityId, itemId, count });
}

export async function vendorSell(entityId: string, itemId: string, count: number): Promise<number> {
  return invoke<number>('vendor_sell', { entityId, itemId, count });
}

export async function eatItem(itemId: string): Promise<EatResult> {
  return invoke<EatResult>('eat_item', { itemId });
}

// ── Trade ───────────────────────────────────────────────────────────────

export async function tradeInitiate(peerHash: string): Promise<void> {
  return invoke('trade_initiate', { peerHash });
}
export async function tradeAccept(): Promise<void> {
  return invoke('trade_accept');
}
export async function tradeDecline(): Promise<void> {
  return invoke('trade_decline');
}
export async function tradeUpdateOffer(items: SaveItemStack[], currants: number): Promise<void> {
  return invoke('trade_update_offer', { items, currants });
}
export async function tradeLock(): Promise<void> {
  return invoke('trade_lock');
}
export async function tradeUnlock(): Promise<void> {
  return invoke('trade_unlock');
}
export async function tradeCancel(): Promise<void> {
  return invoke('trade_cancel');
}
export async function tradeGetState(): Promise<TradeFrame | null> {
  return invoke<TradeFrame | null>('trade_get_state');
}
export function onTradeEvent(callback: (event: TradeEvent) => void): Promise<UnlistenFn> {
  return listen<TradeEvent>('trade_event', (event) => {
    callback(event.payload);
  });
}

// ── Skills ─────────────────────────────────────────────────────────────

export async function getSkills(): Promise<SkillDef[]> {
  return invoke<SkillDef[]>('get_skills');
}

export async function learnSkill(skillId: string): Promise<void> {
  return invoke('learn_skill', { skillId });
}

export async function cancelLearning(): Promise<void> {
  return invoke('cancel_learning');
}
