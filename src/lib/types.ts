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
  surface: string;
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

export interface AvatarAppearance {
  eyes: string;
  ears: string;
  nose: string;
  mouth: string;
  hair: string;
  skinColor: string;
  hairColor: string;
  hat: string | null;
  coat: string | null;
  shirt: string | null;
  pants: string | null;
  dress: string | null;
  skirt: string | null;
  shoes: string | null;
  bracelet: string | null;
}

export interface PlayerFrame {
  x: number;
  y: number;
  vx: number;
  vy: number;
  facing: Direction;
  animation: AnimationState;
  onGround: boolean;
}

export interface CameraFrame {
  x: number;
  y: number;
}

export interface EmoteAnimationFrame {
  variant: string;
  targetHash: string | null;
  startedAt: number;
}

export interface RemotePlayerFrame {
  addressHash: string;
  displayName: string;
  x: number;
  y: number;
  facing: string;
  onGround: boolean;
  animation: AnimationState;
  avatar: AvatarAppearance | null;
  epoch: string;
  isBuddy: boolean;
  partyRole: string | null;
  emoteAnimation: EmoteAnimationFrame | null;
}

export interface TransitionInfo {
  progress: number;
  direction: 'left' | 'right';
  toStreet: string;
  generation: number;
}

export interface ActiveCraftFrame {
  recipeId: string;
  progress: number;
  remainingSecs: number;
}

export interface SkillDef {
  id: string;
  name: string;
  description: string;
  prerequisites: string[];
  imaginationCost: number;
  learnTimeSecs: number;
  unlocksRecipes: string[];
}

export interface SkillProgressFrame {
  learned: string[];
  learning: LearningFrame | null;
}

export interface LearningFrame {
  skillId: string;
  remainingSecs: number;
  progress: number;
}

export interface NearestSocialTarget {
  addressHash: string;
  displayName: string;
  isBuddy: boolean;
  inParty: boolean;
}

export interface RenderFrame {
  player: PlayerFrame;
  remotePlayers: RemotePlayerFrame[];
  camera: CameraFrame;
  streetId: string;
  transition?: TransitionInfo | null;
  inventory: InventoryFrame;
  worldEntities: WorldEntityFrame[];
  worldItems: WorldItemFrame[];
  interactionPrompt: InteractionPrompt | null;
  pickupFeedback: PickupFeedback[];
  audioEvents: AudioEvent[];
  currants: number;
  energy: number;
  maxEnergy: number;
  activeCraft?: ActiveCraftFrame | null;
  imagination: number;
  skillProgress: SkillProgressFrame;
  upgrades: PlayerUpgrades;
  questProgress: QuestProgressFrame;
  mood: number;
  maxMood: number;
  nearestSocialTarget: NearestSocialTarget | null;
}

export interface NetworkStatus {
  peerCount: number;
}

export interface PlayerIdentity {
  displayName: string;
  addressHash: string;
  setupComplete: boolean;
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
  interact: boolean;
}

export interface InventoryFrame {
  slots: (ItemStackFrame | null)[];
  capacity: number;
}

export interface ItemStackFrame {
  itemId: string;
  name: string;
  description: string;
  icon: string;
  count: number;
  stackLimit: number;
  energyValue: number | null;
  moodValue: number | null;
}

export interface WorldEntityFrame {
  id: string;
  entityType: string;
  name: string;
  spriteClass: string;
  x: number;
  y: number;
  cooldownRemaining: number | null;
  depleted: boolean;
  facing: Direction;
}

export interface WorldItemFrame {
  id: string;
  itemId: string;
  name: string;
  icon: string;
  count: number;
  x: number;
  y: number;
}

export interface InteractionPrompt {
  verb: string;
  targetName: string;
  targetX: number;
  targetY: number;
  actionable: boolean;
  entityId?: string | null;
}

export interface PickupFeedback {
  id: number;
  text: string;
  success: boolean;
  x: number;
  y: number;
  ageSecs: number;
  color?: string;
}

export interface RecipeDef {
  id: string;
  name: string;
  description: string;
  inputs: RecipeItem[];
  tools: RecipeItem[];
  outputs: RecipeItem[];
  durationSecs: number;
  energyCost: number;
  category: string;
  requiredSkill?: string | null;
  locked?: boolean;
}

export interface RecipeItem {
  item: string;
  count: number;
}

/** Minimal item stack for save/load (not ItemStackFrame). */
export interface SaveItemStack {
  itemId: string;
  count: number;
}

export interface SavedState {
  streetId: string;
  x: number;
  y: number;
  facing: string;
  inventory: (SaveItemStack | null)[];
  currants?: number;
  energy?: number;
  maxEnergy?: number;
  imagination?: number;
  upgrades?: PlayerUpgrades;
}

export interface EatResult {
  energy: number;
  maxEnergy: number;
}

export interface PlayerUpgrades {
  energyTankTier: number;
  hagglingTier: number;
}

export interface BuyUpgradeResult {
  imagination: number;
  upgrades: PlayerUpgrades;
  energy: number;
  maxEnergy: number;
}

export interface UpgradeTierDef {
  cost: number;
  effectValue: number;
}

export interface UpgradePathDef {
  id: string;
  name: string;
  description: string;
  tiers: UpgradeTierDef[];
}

export type AudioEvent =
  | { type: 'itemPickup'; itemId: string }
  | { type: 'craftSuccess'; recipeId: string }
  | { type: 'actionFailed' }
  | { type: 'jump' }
  | { type: 'land' }
  | { type: 'transitionStart' }
  | { type: 'transitionComplete' }
  | { type: 'entityInteract'; entityType: string }
  | { type: 'streetChanged'; streetId: string }
  | { type: 'footstep'; surface: string }
  | { type: 'jukeboxUpdate'; entityId: string; trackId: string; playing: boolean; distanceFactor: number; elapsedSecs: number }
  | { type: 'skillLearned'; skillId: string };

export interface SoundKitMeta {
  id: string;
  name: string;
}

export interface TrackInfo {
  id: string;
  title: string;
  artist: string;
  durationSecs: number;
}

export interface JukeboxInfo {
  entityId: string;
  name: string;
  playlist: TrackInfo[];
  currentTrackIndex: number;
  playing: boolean;
  elapsedSecs: number;
}

export interface StoreState {
  entityId: string;
  name: string;
  vendorInventory: StoreItem[];
  playerInventory: SellableItem[];
  currants: number;
}

export interface StoreItem {
  itemId: string;
  name: string;
  baseCost: number;
  stackLimit: number;
}

export interface SellableItem {
  itemId: string;
  name: string;
  count: number;
  sellPrice: number;
}

export interface TradeFrame {
  tradeId: number;
  phase: 'pending' | 'negotiating' | 'lockedLocal' | 'lockedRemote' | 'executing' | 'completed' | 'cancelled';
  peerName: string;
  localOffer: TradeOfferFrame;
  remoteOffer: TradeOfferFrame;
  localLocked: boolean;
  remoteLocked: boolean;
}

export interface TradeOfferFrame {
  items: TradeItemFrame[];
  currants: number;
}

export interface TradeItemFrame {
  itemId: string;
  name: string;
  icon: string;
  count: number;
}

export type TradeEvent =
  | { type: 'request'; tradeId: number; initiatorHash: string; initiatorName: string }
  | { type: 'accepted' }
  | { type: 'declined' }
  | { type: 'updated'; tradeFrame: TradeFrame }
  | { type: 'locked'; who: 'local' | 'remote' }
  | { type: 'unlocked'; who: 'local' | 'remote' }
  | { type: 'completed' }
  | { type: 'cancelled'; reason: string }
  | { type: 'error'; message: string };

export interface AvatarManifestItem {
  id: string;
  name: string;
  sheet?: string;
  parts?: string[];
}

export interface AvatarManifest {
  categories: Record<string, { items: AvatarManifestItem[] }>;
  defaults: Record<string, string>;
}

export interface DialogueFrame {
  speaker: string;
  text: string;
  options: DialogueOptionFrame[];
  entityId: string;
}

export interface DialogueOptionFrame {
  text: string;
  index: number;
}

export type DialogueChoiceResult =
  | { type: 'continue'; frame: DialogueFrame; feedback: string[] }
  | { type: 'end'; feedback: string[] };

export interface QuestLogFrame {
  active: QuestEntry[];
  completed: QuestCompletedEntry[];
}

export interface QuestEntry {
  questId: string;
  name: string;
  description: string;
  objectives: ObjectiveEntry[];
}

export interface ObjectiveEntry {
  description: string;
  current: number;
  target: number;
  complete: boolean;
}

export interface QuestCompletedEntry {
  questId: string;
  name: string;
}

export interface QuestProgressFrame {
  activeCount: number;
}

/**
 * Discriminated union mirroring Rust's EmoteKind. Hi carries its own
 * variant payload; other kinds are string-tagged.
 */
export type EmoteKind =
  | { hi: HiVariant }
  | 'dance'
  | 'wave'
  | 'hug'
  | 'high_five'
  | 'applaud';

/** Cosmetic variant for Hi emotes. */
export type HiVariant =
  | 'bats' | 'birds' | 'butterflies' | 'cubes' | 'flowers'
  | 'hands' | 'hearts' | 'hi' | 'pigs' | 'rocketships' | 'stars';

/** Result of firing an emote via the unified IPC. */
export type EmoteFireResult =
  | { type: 'success' }
  | { type: 'cooldown'; remaining_ms: number }
  | { type: 'no_target' }
  | { type: 'target_blocked' };

/** Privacy flags per emote kind. */
export interface EmotePrivacy {
  hug: boolean;
  high_five: boolean;
}
