<script lang="ts">
  import type { PartyMemberInfo } from '$lib/ipc';

  let { inParty, members, isLeader, onLeave, onKick }: {
    inParty: boolean;
    members: PartyMemberInfo[];
    isLeader: boolean;
    onLeave: () => void;
    onKick: (hash: string) => void;
  } = $props();

  let collapsed = $state(false);
</script>

{#if inParty}
  <div class="party-panel">
    <div
      class="party-header"
      role="button"
      tabindex="0"
      aria-expanded={!collapsed}
      onclick={() => { collapsed = !collapsed; }}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); collapsed = !collapsed; } }}
    >
      <span class="party-title">Party</span>
      <span class="party-toggle">{collapsed ? '▶' : '▼'}</span>
    </div>
    {#if !collapsed}
      <ul class="party-member-list">
        {#each members as member (member.addressHash)}
          <li class="party-member" class:leader={member.isLeader}>
            <span class="member-name">{member.displayName}</span>
            {#if member.isLeader}
              <span class="leader-badge" aria-label="Party leader">★</span>
            {/if}
            {#if isLeader && !member.isLeader}
              <button
                type="button"
                class="kick-btn"
                aria-label="Kick {member.displayName}"
                onclick={() => onKick(member.addressHash)}
              >
                ✕
              </button>
            {/if}
          </li>
        {/each}
      </ul>
      <button
        type="button"
        class="leave-btn"
        onclick={onLeave}
      >
        Leave Party
      </button>
    {/if}
  </div>
{/if}

<style>
  .party-panel {
    position: fixed;
    top: 80px;
    left: 12px;
    background: rgba(26, 26, 46, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    min-width: 160px;
    z-index: 50;
    overflow: hidden;
    user-select: none;
  }

  .party-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    cursor: pointer;
    background: rgba(255, 255, 255, 0.05);
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .party-header:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .party-title {
    font-size: 12px;
    font-weight: bold;
    color: #c084fc;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .party-toggle {
    font-size: 10px;
    color: #888;
  }

  .party-member-list {
    list-style: none;
    margin: 0;
    padding: 4px 0;
  }

  .party-member {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    font-size: 13px;
    color: #e0e0e0;
  }

  .party-member.leader {
    color: #fde047;
  }

  .member-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .leader-badge {
    color: #fbbf24;
    font-size: 12px;
    flex-shrink: 0;
  }

  .kick-btn {
    flex-shrink: 0;
    background: none;
    border: 1px solid rgba(239, 68, 68, 0.4);
    border-radius: 3px;
    color: #ef4444;
    font-size: 10px;
    cursor: pointer;
    padding: 1px 4px;
    line-height: 1;
  }

  .kick-btn:hover {
    background: rgba(239, 68, 68, 0.2);
    border-color: #ef4444;
  }

  .leave-btn {
    display: block;
    width: calc(100% - 20px);
    margin: 4px 10px 8px;
    padding: 5px 0;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.35);
    border-radius: 4px;
    color: #ef4444;
    font-size: 12px;
    cursor: pointer;
    text-align: center;
  }

  .leave-btn:hover {
    background: rgba(239, 68, 68, 0.3);
  }
</style>
