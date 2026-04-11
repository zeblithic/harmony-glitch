<script lang="ts">
  let {
    leaderName = '',
    memberCount = 0,
    visible = false,
    onAccept = undefined,
    onDecline = undefined,
  }: {
    leaderName: string;
    memberCount: number;
    visible: boolean;
    onAccept?: () => void;
    onDecline?: () => void;
  } = $props();
</script>

{#if visible}
  <div class="party-prompt" role="alertdialog" aria-label="Party invite from {leaderName}">
    <p class="party-prompt-text"><strong>{leaderName}</strong> invited you to a party ({memberCount} {memberCount === 1 ? 'member' : 'members'})</p>
    <div class="party-prompt-actions">
      <button class="party-prompt-btn accept" onclick={() => onAccept?.()} aria-label="Join {leaderName}'s party">Join</button>
      <button class="party-prompt-btn decline" onclick={() => onDecline?.()} aria-label="Decline party invite from {leaderName}">Decline</button>
    </div>
  </div>
{/if}

<style>
  .party-prompt {
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

  .party-prompt-text {
    margin: 0;
    color: #e0e0e0;
    font-size: 14px;
  }

  .party-prompt-actions {
    display: flex;
    gap: 8px;
  }

  .party-prompt-btn {
    padding: 6px 14px;
    border: none;
    border-radius: 4px;
    font-size: 13px;
    cursor: pointer;
  }

  .party-prompt-btn.accept {
    background: #5865f2;
    color: white;
  }

  .party-prompt-btn.accept:hover {
    background: #4752c4;
  }

  .party-prompt-btn.decline {
    background: rgba(255, 255, 255, 0.1);
    color: #ccc;
  }

  .party-prompt-btn.decline:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .party-prompt-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }
</style>
