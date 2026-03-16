import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { StreetData, InputState, RenderFrame, NetworkStatus, PlayerIdentity, ChatEvent } from './types';

export async function listStreets(): Promise<string[]> {
  return invoke<string[]>('list_streets');
}

export async function loadStreet(name: string): Promise<StreetData> {
  return invoke<StreetData>('load_street', { name });
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
