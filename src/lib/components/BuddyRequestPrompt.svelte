<script lang="ts">
  let acceptBtn: HTMLButtonElement | undefined = $state();

  let {
    senderName = '',
    visible = false,
    onAccept = undefined,
    onDecline = undefined,
  }: {
    senderName: string;
    visible: boolean;
    onAccept?: () => void;
    onDecline?: () => void;
  } = $props();

  $effect(() => {
    if (visible) acceptBtn?.focus();
  });
</script>

{#if visible}
  <div class="buddy-prompt" role="alertdialog" aria-modal="true" aria-label="Buddy request from {senderName}">
    <p class="buddy-prompt-text"><strong>{senderName}</strong> wants to be buddies</p>
    <div class="buddy-prompt-actions">
      <button bind:this={acceptBtn} class="buddy-prompt-btn accept" onclick={() => onAccept?.()} aria-label="Accept buddy request from {senderName}">Accept</button>
      <button class="buddy-prompt-btn decline" onclick={() => onDecline?.()} aria-label="Decline buddy request from {senderName}">Decline</button>
    </div>
  </div>
{/if}

<style>
  .buddy-prompt {
    position: fixed;
    top: 80px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(30, 30, 46, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    padding: 12px 20px;
    z-index: 200;
    display: flex;
    align-items: center;
    gap: 16px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }

  .buddy-prompt-text {
    margin: 0;
    color: #e0e0e0;
    font-size: 14px;
  }

  .buddy-prompt-actions {
    display: flex;
    gap: 8px;
  }

  .buddy-prompt-btn {
    padding: 6px 14px;
    border: none;
    border-radius: 4px;
    font-size: 13px;
    cursor: pointer;
  }

  .buddy-prompt-btn.accept {
    background: #5865f2;
    color: white;
  }

  .buddy-prompt-btn.accept:hover {
    background: #4752c4;
  }

  .buddy-prompt-btn.decline {
    background: rgba(255, 255, 255, 0.1);
    color: #ccc;
  }

  .buddy-prompt-btn.decline:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .buddy-prompt-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }
</style>
