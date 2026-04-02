<script lang="ts">
  import type { PickupFeedback } from '../types';

  let { feedback = [] }: { feedback: PickupFeedback[] } = $props();

  let failureMessages = $derived(
    feedback.filter((fb) => !fb.success)
  );
</script>

{#if failureMessages.length > 0}
  <div class="notification-container" role="alert">
    {#each failureMessages as msg (msg.id)}
      <div class="notification" style="opacity: {Math.max(0, 1 - msg.ageSecs / 1.5)}">
        {msg.text}
      </div>
    {/each}
  </div>
{/if}

<style>
  .notification-container {
    position: fixed;
    top: 48px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 60;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    pointer-events: none;
  }

  .notification {
    padding: 6px 16px;
    border-radius: 4px;
    background: rgba(40, 0, 0, 0.85);
    border: 1px solid #e88;
    color: #e88;
    font-size: 0.85rem;
    white-space: nowrap;
  }
</style>
