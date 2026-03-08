// Types matching Rust DTOs — must stay in sync with src-tauri/src/

export interface StreetData {
  tsid: string;
  name: string;
  left: number;
  right: number;
  top: number;
  bottom: number;
  groundY: number;
  gradient: Gradient | null;
  layers: Layer[];
  signposts: Signpost[];
}

export interface Gradient {
  top: string;
  bottom: string;
}

export interface Layer {
  name: string;
  z: number;
  w: number;
  h: number;
  isMiddleground: boolean;
  decos: Deco[];
  platformLines: PlatformLine[];
  walls: Wall[];
  ladders: Ladder[];
  filters: LayerFilters | null;
}

export interface PlatformLine {
  id: string;
  start: Point;
  end: Point;
  pcPerm: number | null;
  itemPerm: number | null;
}

export interface Point {
  x: number;
  y: number;
}

export interface Wall {
  id: string;
  x: number;
  y: number;
  h: number;
  pcPerm: number | null;
  itemPerm: number | null;
}

export interface Ladder {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Deco {
  id: string;
  name: string;
  spriteClass: string;
  x: number;
  y: number;
  w: number;
  h: number;
  z: number;
  r: number;
  hFlip: boolean;
}

export interface LayerFilters {
  brightness: number | null;
  contrast: number | null;
  saturation: number | null;
  blur: number | null;
  tintColor: number | null;
  tintAmount: number | null;
}

export interface Signpost {
  id: string;
  x: number;
  y: number;
  connects: SignpostConnection[];
}

export interface SignpostConnection {
  targetTsid: string;
  targetLabel: string;
}

export type Direction = 'left' | 'right';
export type AnimationState = 'idle' | 'walking' | 'jumping' | 'falling';

export interface PlayerFrame {
  x: number;
  y: number;
  facing: Direction;
  animation: AnimationState;
}

export interface CameraFrame {
  x: number;
  y: number;
}

export interface RemotePlayerFrame {
  addressHash: string;
  displayName: string;
  x: number;
  y: number;
  facing: string;
  onGround: boolean;
}

export interface TransitionInfo {
  progress: number;
  direction: 'left' | 'right';
  toStreet: string;
}

export interface RenderFrame {
  player: PlayerFrame;
  remotePlayers: RemotePlayerFrame[];
  camera: CameraFrame;
  streetId: string;
  transition?: TransitionInfo | null;
}

export interface NetworkStatus {
  peerCount: number;
}

export interface PlayerIdentity {
  displayName: string;
  addressHash: string;
}

export interface ChatEvent {
  text: string;
  senderHash: string;
  senderName: string;
}

export interface InputState {
  left: boolean;
  right: boolean;
  jump: boolean;
}
