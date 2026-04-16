<script lang="ts">
  import type { GroupStateResult } from '$lib/ipc';

  let { groups, visible, onSelect, onCreate }: {
    groups: GroupStateResult[];
    visible: boolean;
    onSelect: (groupId: string) => void;
    onCreate: () => void;
  } = $props();
</script>

{#if visible}
  <div class="group-list-panel">
    <div class="group-list-header">
      <span class="group-list-title">Groups</span>
      <button
        type="button"
        class="create-btn"
        aria-label="Create new group"
        onclick={onCreate}
      >+</button>
    </div>
    {#if groups.length === 0}
      <div class="group-empty">No groups yet</div>
    {:else}
      <ul class="group-list">
        {#each groups as group (group.groupId)}
          <li class="group-entry">
            <button
              type="button"
              class="group-entry-btn"
              aria-label="Open group {group.name}"
              onclick={() => onSelect(group.groupId)}
            >
              <span class="group-name">{group.name}</span>
              <span class="group-count">{group.memberCount} {group.memberCount === 1 ? 'member' : 'members'}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .group-list-panel {
    position: fixed;
    top: 80px;
    right: 12px;
    background: rgba(26, 26, 46, 0.92);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 8px;
    min-width: 200px;
    max-height: 400px;
    overflow-y: auto;
    z-index: 50;
    user-select: none;
  }

  .group-list-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: rgba(255, 255, 255, 0.05);
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .group-list-title {
    font-size: 12px;
    font-weight: bold;
    color: #a78bfa;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .create-btn {
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    color: #e0e0e0;
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    padding: 1px 6px;
  }

  .create-btn:hover {
    background: rgba(88, 101, 242, 0.25);
    border-color: #5865f2;
    color: #fff;
  }

  .create-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }

  .group-empty {
    padding: 16px 12px;
    font-size: 13px;
    color: #888;
    text-align: center;
  }

  .group-list {
    list-style: none;
    margin: 0;
    padding: 4px 0;
  }

  .group-entry {
    display: flex;
  }

  .group-entry-btn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 6px 12px;
    background: none;
    border: none;
    font: inherit;
    color: inherit;
    cursor: pointer;
    gap: 8px;
    text-align: left;
  }

  .group-entry-btn:hover {
    background: rgba(255, 255, 255, 0.06);
  }

  .group-entry-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: -2px;
  }

  .group-name {
    font-size: 13px;
    color: #e0e0e0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-count {
    font-size: 11px;
    color: #888;
    flex-shrink: 0;
  }
</style>
