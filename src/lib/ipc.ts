import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { StreetData, InputState, RenderFrame } from './types';

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
