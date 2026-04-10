<script lang="ts">
  let {
    visible,
    targetName,
    canHi,
    canTrade,
    canInvite,
    canBuddy,
    onHi,
    onTrade,
    onInvite,
    onBuddy,
  }: {
    visible: boolean;
    targetName: string;
    canHi: boolean;
    canTrade: boolean;
    canInvite: boolean;
    canBuddy: boolean;
    onHi: () => void;
    onTrade: () => void;
    onInvite: () => void;
    onBuddy: () => void;
  } = $props();

  let hasAnyAction = $derived(canHi || canTrade || canInvite || canBuddy);
  let shouldRender = $derived(visible && hasAnyAction);

  const actions = $derived([
    canHi    && { key: 'H', label: 'Hi',     action: onHi },
    canTrade && { key: 'T', label: 'Trade',  action: onTrade },
    canInvite && { key: 'Y', label: 'Invite', action: onInvite },
    canBuddy && { key: 'B', label: 'Buddy',  action: onBuddy },
  ].filter(Boolean) as { key: string; label: string; action: () => void }[]);
</script>

{#if shouldRender}
  <div class="social-prompt" role="menu" aria-label="Social actions for {targetName}">
    <div class="social-prompt-name">{targetName}</div>
    <div class="social-actions">
      {#each actions as act (act.label)}
        <button
          type="button"
          class="social-action"
          role="menuitem"
          aria-label="{act.label} {targetName}"
          onclick={act.action}
        >
          <span class="social-action-key">{act.key}</span>
          <span class="social-action-label">{act.label}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .social-prompt {
    position: fixed;
    bottom: 80px;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(26, 26, 46, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 10px;
    padding: 8px 14px;
    z-index: 55;
    pointer-events: auto;
    user-select: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }

  .social-prompt-name {
    font-size: 13px;
    font-weight: bold;
    color: #e0e0e0;
    letter-spacing: 0.03em;
  }

  .social-actions {
    display: flex;
    gap: 6px;
  }

  .social-action {
    display: flex;
    align-items: center;
    gap: 4px;
    background: rgba(255, 255, 255, 0.07);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
    color: #e0e0e0;
    font-size: 12px;
    transition: background 0.1s, border-color 0.1s;
  }

  .social-action:hover {
    background: rgba(255, 255, 255, 0.15);
    border-color: rgba(255, 255, 255, 0.3);
  }

  .social-action-key {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 3px;
    font-size: 10px;
    font-weight: bold;
    color: #aaa;
    flex-shrink: 0;
  }

  .social-action-label {
    color: #e0e0e0;
  }
</style>
