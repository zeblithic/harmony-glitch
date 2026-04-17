<script lang="ts">
  import type { GroupStateResult } from '$lib/ipc';

  let { group, ourHash, onLeave, onKick, onPromote, onDemote, onDissolve, onBack }: {
    group: GroupStateResult;
    ourHash: string;
    onLeave: () => void;
    onKick: (peerHash: string) => void;
    onPromote: (peerHash: string) => void;
    onDemote: (peerHash: string) => void;
    onDissolve: () => void;
    onBack: () => void;
  } = $props();

  let ourMember = $derived(group.members.find((m: { addressHash: string }) => m.addressHash === ourHash));
  let isFounder = $derived(ourMember?.isFounder ?? false);
  let isOfficer = $derived(ourMember?.role === 'officer');

  function roleBadgeClass(role: string): string {
    if (role === 'founder') return 'badge-founder';
    if (role === 'officer') return 'badge-officer';
    return 'badge-member';
  }
</script>

<div class="group-detail-panel">
  <div class="group-detail-header">
    <button
      type="button"
      class="back-btn"
      aria-label="Back to group list"
      onclick={onBack}
    >‹</button>
    <span class="group-detail-title">{group.name}</span>
  </div>

  <ul class="member-list">
    {#each group.members as member (member.addressHash)}
      <li class="member-entry">
        <span class="member-hash">{member.addressHash.slice(0, 8)}</span>
        <span class="role-badge {roleBadgeClass(member.role)}">{member.role}</span>
        {#if member.addressHash !== ourHash}
          <div class="member-actions">
            {#if isFounder}
              <button
                type="button"
                class="action-btn kick-btn"
                aria-label="Kick {member.addressHash.slice(0, 8)}"
                onclick={() => onKick(member.addressHash)}
              >Kick</button>
              {#if member.role === 'member'}
                <button
                  type="button"
                  class="action-btn promote-btn"
                  aria-label="Promote {member.addressHash.slice(0, 8)}"
                  onclick={() => onPromote(member.addressHash)}
                >Promote</button>
              {/if}
              {#if member.role === 'officer'}
                <button
                  type="button"
                  class="action-btn demote-btn"
                  aria-label="Demote {member.addressHash.slice(0, 8)}"
                  onclick={() => onDemote(member.addressHash)}
                >Demote</button>
              {/if}
            {:else if isOfficer && member.role === 'member'}
              <button
                type="button"
                class="action-btn kick-btn"
                aria-label="Kick {member.addressHash.slice(0, 8)}"
                onclick={() => onKick(member.addressHash)}
              >Kick</button>
            {/if}
          </div>
        {/if}
      </li>
    {/each}
  </ul>

  <div class="group-detail-footer">
    {#if isFounder}
      <button
        type="button"
        class="dissolve-btn"
        onclick={onDissolve}
      >Dissolve</button>
    {:else}
      <button
        type="button"
        class="leave-btn"
        onclick={onLeave}
      >Leave Group</button>
    {/if}
  </div>
</div>

<style>
  .group-detail-panel {
    position: fixed;
    top: 80px;
    right: 12px;
    background: rgba(26, 26, 46, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    min-width: 220px;
    max-height: 480px;
    overflow-y: auto;
    z-index: 50;
    user-select: none;
    display: flex;
    flex-direction: column;
  }

  .group-detail-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: rgba(255, 255, 255, 0.05);
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    flex-shrink: 0;
  }

  .back-btn {
    background: none;
    border: none;
    color: #888;
    font-size: 18px;
    line-height: 1;
    cursor: pointer;
    padding: 0 4px 0 0;
  }

  .back-btn:hover {
    color: #e0e0e0;
  }

  .back-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }

  .group-detail-title {
    font-size: 12px;
    font-weight: bold;
    color: #a78bfa;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .member-list {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    flex: 1;
    overflow-y: auto;
  }

  .member-entry {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    font-size: 13px;
    color: #e0e0e0;
  }

  .member-entry:hover .member-actions,
  .member-entry:focus-within .member-actions {
    opacity: 1;
  }

  .member-hash {
    flex: 1;
    font-family: monospace;
    font-size: 12px;
    color: #ccc;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .role-badge {
    font-size: 10px;
    border-radius: 3px;
    padding: 1px 5px;
    flex-shrink: 0;
    font-weight: bold;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .badge-founder {
    background: rgba(251, 191, 36, 0.2);
    color: #fbbf24;
    border: 1px solid rgba(251, 191, 36, 0.4);
  }

  .badge-officer {
    background: rgba(88, 101, 242, 0.2);
    color: #818cf8;
    border: 1px solid rgba(88, 101, 242, 0.4);
  }

  .badge-member {
    background: rgba(255, 255, 255, 0.07);
    color: #888;
    border: 1px solid rgba(255, 255, 255, 0.12);
  }

  .member-actions {
    display: flex;
    gap: 3px;
    opacity: 0;
    transition: opacity 0.15s;
    flex-shrink: 0;
  }

  .action-btn {
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 3px;
    color: #aaa;
    font-size: 10px;
    cursor: pointer;
    padding: 1px 5px;
    line-height: 1;
  }

  .action-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }

  .kick-btn:hover {
    color: #ef4444;
    border-color: rgba(239, 68, 68, 0.5);
    background: rgba(239, 68, 68, 0.1);
  }

  .promote-btn:hover {
    color: #a78bfa;
    border-color: rgba(167, 139, 250, 0.5);
    background: rgba(167, 139, 250, 0.1);
  }

  .demote-btn:hover {
    color: #f59e0b;
    border-color: rgba(245, 158, 11, 0.5);
    background: rgba(245, 158, 11, 0.1);
  }

  .group-detail-footer {
    display: flex;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    flex-shrink: 0;
  }

  .leave-btn {
    flex: 1;
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

  .leave-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }

  .dissolve-btn {
    padding: 5px 10px;
    background: rgba(237, 66, 69, 0.2);
    border: 1px solid rgba(237, 66, 69, 0.5);
    border-radius: 4px;
    color: #f87171;
    font-size: 12px;
    cursor: pointer;
    text-align: center;
  }

  .dissolve-btn:hover {
    background: rgba(237, 66, 69, 0.4);
  }

  .dissolve-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }
</style>
