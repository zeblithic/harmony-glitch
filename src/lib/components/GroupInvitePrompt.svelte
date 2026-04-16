<script lang="ts">
  let joinBtn: HTMLButtonElement | undefined = $state();

  let {
    inviterName = '',
    groupName = '',
    visible = false,
    onAccept = undefined,
    onDecline = undefined,
  }: {
    inviterName: string;
    groupName: string;
    visible: boolean;
    onAccept?: () => void;
    onDecline?: () => void;
  } = $props();

  $effect(() => {
    if (visible) joinBtn?.focus();
  });
</script>

{#if visible}
  <div class="group-prompt" role="alertdialog" aria-modal="true" aria-label="Group invite from {inviterName}">
    <p class="group-prompt-text"><strong>{inviterName}</strong> invited you to <strong>{groupName}</strong></p>
    <div class="group-prompt-actions">
      <button
        bind:this={joinBtn}
        class="group-prompt-btn accept"
        onclick={() => onAccept?.()}
        aria-label="Join {groupName}"
      >Join</button>
      <button
        class="group-prompt-btn decline"
        onclick={() => onDecline?.()}
        aria-label="Decline invite to {groupName}"
      >Decline</button>
    </div>
  </div>
{/if}

<style>
  .group-prompt {
    position: fixed;
    top: 120px;
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

  .group-prompt-text {
    margin: 0;
    color: #e0e0e0;
    font-size: 14px;
  }

  .group-prompt-actions {
    display: flex;
    gap: 8px;
  }

  .group-prompt-btn {
    padding: 6px 14px;
    border: none;
    border-radius: 4px;
    font-size: 13px;
    cursor: pointer;
  }

  .group-prompt-btn.accept {
    background: #5865f2;
    color: white;
  }

  .group-prompt-btn.accept:hover {
    background: #4752c4;
  }

  .group-prompt-btn.decline {
    background: rgba(255, 255, 255, 0.1);
    color: #ccc;
  }

  .group-prompt-btn.decline:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .group-prompt-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }
</style>
