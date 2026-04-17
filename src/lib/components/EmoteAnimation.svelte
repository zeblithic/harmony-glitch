<script lang="ts">
  import type { EmoteAnimationFrame } from '$lib/types';

  let { animation, x, y }: {
    animation: EmoteAnimationFrame;
    x: number;
    y: number;
  } = $props();

  const HI_VARIANT_EMOJIS: Record<string, string> = {
    bats: '🦇', birds: '🐦', butterflies: '🦋', cubes: '🧊',
    flowers: '🌸', hands: '👋', hearts: '❤️', hi: '👋',
    pigs: '🐷', rocketships: '🚀', stars: '⭐',
  };

  const KIND_EMOJIS: Record<string, string> = {
    dance: '💃',
    wave: '👋',
    hug: '🤗',
    high_five: '🖐️',
    applaud: '👏',
  };

  let emoji = $derived.by(() => {
    const kind = animation.kind ?? 'hi'; // default to 'hi' for legacy payloads
    if (kind === 'hi') {
      return HI_VARIANT_EMOJIS[animation.variant] ?? '👋';
    }
    return KIND_EMOJIS[kind] ?? '👋';
  });

  let ariaLabel = $derived(animation.kind ?? 'hi');
</script>

<div class="emote-animation" style="left: {x}px; top: {y - 60}px;" aria-label="Emote: {ariaLabel}">
  <span class="emote-sprite">{emoji}</span>
</div>

<style>
  .emote-animation {
    position: absolute;
    pointer-events: none;
    z-index: 60;
    animation: emote-float 2s ease-out forwards;
  }
  .emote-sprite { font-size: 28px; filter: drop-shadow(0 0 4px rgba(255,255,255,0.5)); }
  @keyframes emote-float {
    0% { opacity: 1; transform: translateY(0) scale(1); }
    100% { opacity: 0; transform: translateY(-80px) scale(1.3); }
  }
</style>
