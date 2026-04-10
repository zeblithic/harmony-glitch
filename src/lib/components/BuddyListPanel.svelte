<script lang="ts">
  import type { BuddyEntry } from '$lib/ipc';

  let { buddies, visible, onRemove, onBlock }: {
    buddies: BuddyEntry[];
    visible: boolean;
    onRemove: (hash: string) => void;
    onBlock: (hash: string) => void;
  } = $props();

  function formatCoPresence(secs: number): string {
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m`;
    return `${Math.floor(secs / 3600)}h`;
  }
</script>

{#if visible}
  <div class="buddy-list-panel">
    <div class="buddy-list-header">
      <span class="buddy-list-title">Buddies</span>
    </div>
    {#if buddies.length === 0}
      <div class="buddy-empty">No buddies yet</div>
    {:else}
      <ul class="buddy-list">
        {#each buddies as buddy (buddy.addressHash)}
          <li class="buddy-entry">
            <div class="buddy-info">
              <span class="buddy-name">{buddy.displayName}</span>
              <span class="buddy-copresence">{formatCoPresence(buddy.coPresenceTotal)}</span>
            </div>
            <div class="buddy-actions">
              <button
                type="button"
                class="buddy-action-btn remove-btn"
                aria-label="Remove {buddy.displayName}"
                onclick={() => onRemove(buddy.addressHash)}
              >
                Remove
              </button>
              <button
                type="button"
                class="buddy-action-btn block-btn"
                aria-label="Block {buddy.displayName}"
                onclick={() => onBlock(buddy.addressHash)}
              >
                Block
              </button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .buddy-list-panel {
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

  .buddy-list-header {
    padding: 6px 12px;
    background: rgba(255, 255, 255, 0.05);
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .buddy-list-title {
    font-size: 12px;
    font-weight: bold;
    color: #fbbf24;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .buddy-empty {
    padding: 16px 12px;
    font-size: 13px;
    color: #888;
    text-align: center;
  }

  .buddy-list {
    list-style: none;
    margin: 0;
    padding: 4px 0;
  }

  .buddy-entry {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 5px 12px;
    gap: 8px;
  }

  .buddy-entry:hover .buddy-actions,
  .buddy-entry:focus-within .buddy-actions {
    opacity: 1;
  }

  .buddy-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }

  .buddy-name {
    font-size: 13px;
    color: #fbbf24;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .buddy-copresence {
    font-size: 10px;
    color: #888;
  }

  .buddy-actions {
    display: flex;
    gap: 4px;
    opacity: 0;
    transition: opacity 0.15s;
    flex-shrink: 0;
  }

  .buddy-action-btn {
    background: none;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 3px;
    color: #aaa;
    font-size: 10px;
    cursor: pointer;
    padding: 2px 6px;
    line-height: 1;
  }

  .buddy-action-btn:hover {
    color: #fff;
    border-color: rgba(255, 255, 255, 0.4);
    background: rgba(255, 255, 255, 0.08);
  }

  .buddy-action-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
    color: #fff;
    border-color: rgba(255, 255, 255, 0.4);
    background: rgba(255, 255, 255, 0.08);
  }

  .block-btn:hover {
    color: #ef4444;
    border-color: rgba(239, 68, 68, 0.5);
    background: rgba(239, 68, 68, 0.1);
  }
</style>
