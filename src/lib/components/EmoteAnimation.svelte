<script lang="ts">
  import type { EmoteAnimationFrame } from '$lib/types';

  let { animation, x, y }: {
    animation: EmoteAnimationFrame;
    x: number;
    y: number;
  } = $props();

  const VARIANT_EMOJIS: Record<string, string> = {
    bats: '🦇', birds: '🐦', butterflies: '🦋', cubes: '🧊',
    flowers: '🌸', hands: '👋', hearts: '❤️', hi: '👋',
    pigs: '🐷', rocketships: '🚀', stars: '⭐',
  };

  let emoji = $derived(VARIANT_EMOJIS[animation.variant] ?? '👋');
</script>

<div class="emote-animation" style="left: {x}px; top: {y - 60}px;" aria-label="Emote: {animation.variant}">
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
