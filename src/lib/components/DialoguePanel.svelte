<script lang="ts">
  import type { DialogueFrame, DialogueChoiceResult } from '../types';
  import { dialogueChoose, closeDialogue } from '../ipc';

  let {
    dialogueFrame = null,
    visible = false,
    onClose,
    onFrameUpdate,
  }: {
    dialogueFrame: DialogueFrame | null;
    visible: boolean;
    onClose?: () => void;
    onFrameUpdate?: (frame: DialogueFrame) => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;
  let feedbackMessages = $state<string[]>([]);
  let showingFeedback = $state(false);

  $effect(() => {
    if (visible && dialogEl) {
      if (!dialogEl.open) {
        previousFocus = document.activeElement as HTMLElement | null;
        dialogEl.showModal();
      }
      // Focus first option
      requestAnimationFrame(() => {
        const first = dialogEl?.querySelector<HTMLElement>('.dialogue-option');
        first?.focus();
      });
    } else if (!visible && dialogEl?.open) {
      dialogEl.close();
      previousFocus?.focus();
      previousFocus = null;
      feedbackMessages = [];
      showingFeedback = false;
    }
  });

  function handleCancel(e: Event) {
    e.preventDefault();
    closeDialogue().catch(console.error);
    onClose?.();
  }

  async function handleChoose(optionIndex: number) {
    try {
      const result: DialogueChoiceResult = await dialogueChoose(optionIndex);
      if (result.type === 'continue') {
        onFrameUpdate?.(result.frame);
      } else {
        // Show feedback briefly, then close
        if (result.feedback.length > 0) {
          feedbackMessages = result.feedback;
          showingFeedback = true;
          setTimeout(() => {
            showingFeedback = false;
            feedbackMessages = [];
            onClose?.();
          }, 1500);
        } else {
          onClose?.();
        }
      }
    } catch (e) {
      console.error('Dialogue choice failed:', e);
      onClose?.();
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeDialogue().catch(console.error);
      onClose?.();
      return;
    }
    // Number keys 1-9 to select options
    const num = parseInt(e.key);
    if (num >= 1 && num <= 9 && dialogueFrame && !showingFeedback) {
      const opts = dialogueFrame.options;
      if (num <= opts.length) {
        e.preventDefault();
        handleChoose(opts[num - 1].index);
      }
    }
  }

  function handleOptionKeyDown(e: KeyboardEvent) {
    const options = Array.from(
      dialogEl?.querySelectorAll<HTMLElement>('.dialogue-option') ?? []
    );
    const idx = options.findIndex(el => el === document.activeElement);
    if (e.key === 'ArrowDown' && idx < options.length - 1) {
      e.preventDefault();
      options[idx + 1].focus();
    } else if (e.key === 'ArrowUp' && idx > 0) {
      e.preventDefault();
      options[idx - 1].focus();
    }
  }
</script>

{#if visible}
  <dialog
    class="dialogue-panel"
    aria-label="Dialogue"
    aria-modal="true"
    bind:this={dialogEl}
    oncancel={handleCancel}
    onkeydown={handleKeyDown}
  >
    {#if showingFeedback}
      <div class="feedback-overlay" role="status">
        {#each feedbackMessages as msg}
          <div class="feedback-msg">{msg}</div>
        {/each}
      </div>
    {:else if dialogueFrame}
      <div class="speaker-name">{dialogueFrame.speaker}</div>
      <div class="dialogue-text">{dialogueFrame.text}</div>

      {#if dialogueFrame.options.length > 0}
        <div class="options-list" role="list" onkeydown={handleOptionKeyDown}>
          {#each dialogueFrame.options as option, i (option.index)}
            <button
              type="button"
              class="dialogue-option"
              role="listitem"
              onclick={() => handleChoose(option.index)}
            >
              <span class="option-number">{i + 1}.</span>
              <span class="option-text">{option.text}</span>
            </button>
          {/each}
        </div>
      {:else}
        <button
          type="button"
          class="dialogue-option dialogue-close"
          onclick={() => { closeDialogue().catch(console.error); onClose?.(); }}
        >
          <span class="option-text">Close</span>
        </button>
      {/if}
    {/if}
  </dialog>
{/if}

<style>
  .dialogue-panel {
    position: fixed;
    bottom: 80px;
    left: 50%;
    transform: translateX(-50%);
    width: 500px;
    max-width: 90vw;
    margin: 0;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    background: rgba(26, 26, 46, 0.95);
    color: #e0e0e0;
    font-size: 13px;
    z-index: 100;
  }

  .dialogue-panel::backdrop {
    background: transparent;
  }

  .speaker-name {
    padding: 10px 16px 0;
    font-size: 14px;
    font-weight: bold;
    color: #c084fc;
  }

  .dialogue-text {
    padding: 8px 16px 12px;
    line-height: 1.5;
    color: #ddd;
  }

  .options-list {
    padding: 0 8px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .dialogue-option {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 4px;
    color: #ccc;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }

  .dialogue-option:hover,
  .dialogue-option:focus {
    background: rgba(192, 132, 252, 0.12);
    border-color: rgba(192, 132, 252, 0.3);
    color: #fff;
    outline: none;
  }

  .dialogue-close {
    justify-content: center;
    margin: 0 8px 8px;
    width: calc(100% - 16px);
  }

  .option-number {
    color: #c084fc;
    font-weight: bold;
    min-width: 16px;
  }

  .option-text {
    flex: 1;
  }

  .feedback-overlay {
    padding: 16px;
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .feedback-msg {
    font-size: 14px;
    font-weight: bold;
    color: #86efac;
    padding: 4px 0;
  }
</style>
