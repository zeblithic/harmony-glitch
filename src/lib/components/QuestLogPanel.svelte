<script lang="ts">
  import type { QuestLogFrame } from '../types';

  let {
    questLog = null,
    visible = false,
    onClose,
  }: {
    questLog: QuestLogFrame | null;
    visible: boolean;
    onClose?: () => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;

  $effect(() => {
    if (visible && dialogEl) {
      previousFocus = document.activeElement as HTMLElement | null;
      if (!dialogEl.open) dialogEl.showModal();
    } else if (!visible && dialogEl?.open) {
      dialogEl.close();
      previousFocus?.focus();
      previousFocus = null;
    }
  });

  function handleCancel(e: Event) {
    e.preventDefault();
    onClose?.();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose?.();
    }
  }
</script>

{#if visible}
  <dialog
    class="quest-log-panel"
    aria-label="Quest Log"
    aria-modal="true"
    bind:this={dialogEl}
    oncancel={handleCancel}
    onkeydown={handleKeyDown}
  >
    <div class="panel-header">
      <h2>Quest Log</h2>
      <button type="button" class="close-btn" onclick={() => onClose?.()} aria-label="Close">
        &times;
      </button>
    </div>

    {#if questLog}
      {#if questLog.active.length > 0}
        <div class="section">
          <h3 class="section-title">Active Quests</h3>
          {#each questLog.active as quest (quest.questId)}
            <div class="quest-card">
              <div class="quest-name">{quest.name}</div>
              <div class="quest-desc">{quest.description}</div>
              {#each quest.objectives as obj}
                <div class="objective" class:complete={obj.complete}>
                  <span class="obj-text">{obj.description}</span>
                  <span class="obj-count">{obj.current}/{obj.target}</span>
                  <div
                    class="obj-bar"
                    role="progressbar"
                    aria-label={obj.description}
                    aria-valuenow={obj.current}
                    aria-valuemin={0}
                    aria-valuemax={obj.target}
                  >
                    <div
                      class="obj-fill"
                      style="width: {Math.min(obj.current / obj.target, 1) * 100}%"
                    ></div>
                  </div>
                </div>
              {/each}
            </div>
          {/each}
        </div>
      {/if}

      {#if questLog.completed.length > 0}
        <div class="section">
          <h3 class="section-title">Completed</h3>
          {#each questLog.completed as quest (quest.questId)}
            <div class="completed-quest">
              <span class="check">{'\u2713'}</span>
              <span>{quest.name}</span>
            </div>
          {/each}
        </div>
      {/if}

      {#if questLog.active.length === 0 && questLog.completed.length === 0}
        <div class="empty-state">No quests yet. Talk to NPCs to get started!</div>
      {/if}
    {:else}
      <div class="empty-state">Loading...</div>
    {/if}
  </dialog>
{/if}

<style>
  .quest-log-panel {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 360px;
    max-width: 90vw;
    max-height: 80vh;
    margin: 0;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    background: rgba(26, 26, 46, 0.95);
    color: #e0e0e0;
    font-size: 12px;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    z-index: 100;
  }

  .quest-log-panel::backdrop {
    background: rgba(0, 0, 0, 0.3);
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 12px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  }

  .panel-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: bold;
    color: #c084fc;
  }

  .close-btn {
    background: none;
    border: none;
    color: #999;
    font-size: 18px;
    cursor: pointer;
    padding: 0 4px;
  }

  .close-btn:hover {
    color: #fff;
  }

  .section {
    padding: 8px 12px;
  }

  .section-title {
    margin: 0 0 6px;
    font-size: 10px;
    color: #999;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .quest-card {
    padding: 8px;
    margin-bottom: 8px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 4px;
  }

  .quest-name {
    font-size: 13px;
    font-weight: bold;
    color: #fff;
    margin-bottom: 4px;
  }

  .quest-desc {
    font-size: 11px;
    color: #aaa;
    margin-bottom: 8px;
  }

  .objective {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 4px;
    margin-bottom: 6px;
  }

  .objective.complete .obj-text {
    color: #86efac;
  }

  .obj-text {
    font-size: 11px;
    color: #ccc;
  }

  .obj-count {
    font-size: 11px;
    color: #999;
    text-align: right;
  }

  .obj-bar {
    grid-column: 1 / -1;
    height: 4px;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 2px;
    overflow: hidden;
  }

  .obj-fill {
    height: 100%;
    background: linear-gradient(90deg, #a855f7, #c084fc);
    border-radius: 2px;
    transition: width 0.3s ease;
  }

  .objective.complete .obj-fill {
    background: #86efac;
  }

  .completed-quest {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 0;
    color: #666;
    font-size: 11px;
  }

  .check {
    color: #86efac;
  }

  .empty-state {
    padding: 24px 16px;
    text-align: center;
    color: #666;
    font-size: 12px;
  }
</style>
