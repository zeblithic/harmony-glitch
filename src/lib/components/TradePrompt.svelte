<script lang="ts">
  let {
    initiatorName = '',
    visible = false,
    onAccept = undefined,
    onDecline = undefined,
  }: {
    initiatorName: string;
    visible: boolean;
    onAccept?: () => void;
    onDecline?: () => void;
  } = $props();
</script>

{#if visible}
  <div class="trade-prompt" role="alertdialog" aria-label="Trade request from {initiatorName}">
    <p class="trade-prompt-text">{initiatorName} wants to trade</p>
    <div class="trade-prompt-actions">
      <button class="trade-prompt-btn accept" onclick={() => onAccept?.()} aria-label="Accept trade with {initiatorName}">Accept</button>
      <button class="trade-prompt-btn decline" onclick={() => onDecline?.()} aria-label="Decline trade with {initiatorName}">Decline</button>
    </div>
  </div>
{/if}

<style>
  .trade-prompt {
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

  .trade-prompt-text {
    margin: 0;
    color: #e0e0e0;
    font-size: 14px;
  }

  .trade-prompt-actions {
    display: flex;
    gap: 8px;
  }

  .trade-prompt-btn {
    padding: 6px 14px;
    border: none;
    border-radius: 4px;
    font-size: 13px;
    cursor: pointer;
  }

  .trade-prompt-btn.accept {
    background: #5865f2;
    color: white;
  }

  .trade-prompt-btn.accept:hover {
    background: #4752c4;
  }

  .trade-prompt-btn.decline {
    background: rgba(255, 255, 255, 0.1);
    color: #ccc;
  }

  .trade-prompt-btn.decline:hover {
    background: rgba(255, 255, 255, 0.2);
  }
</style>
