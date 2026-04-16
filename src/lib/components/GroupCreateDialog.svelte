<script lang="ts">
  let nameInput: HTMLInputElement | undefined = $state();

  let {
    visible = false,
    onCreate,
    onCancel,
  }: {
    visible: boolean;
    onCreate: (name: string, mode: string) => void;
    onCancel: () => void;
  } = $props();

  let name = $state('');
  let mode = $state('invite_only');

  $effect(() => {
    if (visible) {
      name = '';
      mode = 'invite_only';
      nameInput?.focus();
    }
  });

  function submit() {
    const trimmed = name.trim();
    if (!trimmed) return;
    onCreate(trimmed, mode);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.target as HTMLElement)?.tagName === 'INPUT') {
      e.preventDefault();
      submit();
    }
    if (e.key === 'Escape') onCancel();
  }
</script>

{#if visible}
  <dialog
    class="dialog-backdrop"
    open
    aria-label="Create group"
    onkeydown={onKeydown}
  >
    <div class="dialog-box">
      <h2 class="dialog-title">Create Group</h2>
      <label class="dialog-label" for="group-name-input">Name</label>
      <input
        id="group-name-input"
        type="text"
        class="dialog-input"
        placeholder="Group name"
        bind:this={nameInput}
        bind:value={name}
        maxlength={40}
        autocomplete="off"
        spellcheck={false}
      />
      <fieldset class="mode-fieldset">
        <legend class="dialog-label">Mode</legend>
        <div class="radio-group">
          <label class="radio-label">
            <input type="radio" name="group-mode" value="invite_only" bind:group={mode} />
            <span>Invite Only</span>
          </label>
          <label class="radio-label">
            <input type="radio" name="group-mode" value="open" bind:group={mode} />
            <span>Open</span>
          </label>
        </div>
      </fieldset>
      <div class="dialog-actions">
        <button
          type="button"
          class="dialog-btn create-btn"
          disabled={!name.trim()}
          onclick={submit}
        >Create</button>
        <button
          type="button"
          class="dialog-btn cancel-btn"
          onclick={onCancel}
        >Cancel</button>
      </div>
    </div>
  </dialog>
{/if}

<style>
  .dialog-backdrop {
    position: fixed;
    inset: 0;
    border: none;
    padding: 0;
    max-width: none;
    max-height: none;
    width: 100%;
    height: 100%;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 300;
  }

  .dialog-box {
    background: rgba(30, 30, 46, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 8px;
    padding: 20px 24px 16px;
    min-width: 260px;
    max-width: 340px;
    width: 100%;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.6);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .dialog-title {
    margin: 0 0 4px;
    font-size: 14px;
    font-weight: bold;
    color: #a78bfa;
    letter-spacing: 0.04em;
  }

  .dialog-label {
    font-size: 12px;
    color: #888;
    margin-bottom: 2px;
    display: block;
  }

  .dialog-input {
    width: 100%;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    color: #e0e0e0;
    font-size: 13px;
    padding: 6px 10px;
    box-sizing: border-box;
    outline: none;
  }

  .dialog-input:focus {
    border-color: #5865f2;
    background: rgba(88, 101, 242, 0.08);
  }

  .dialog-input:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }

  .mode-fieldset {
    border: none;
    margin: 0;
    padding: 0;
  }

  .radio-group {
    display: flex;
    gap: 16px;
    margin-top: 4px;
  }

  .radio-label {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 13px;
    color: #e0e0e0;
    cursor: pointer;
  }

  .radio-label input[type="radio"] {
    accent-color: #5865f2;
    cursor: pointer;
  }

  .dialog-actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
    justify-content: flex-end;
  }

  .dialog-btn {
    padding: 6px 16px;
    border-radius: 4px;
    font-size: 13px;
    cursor: pointer;
    border: none;
  }

  .create-btn {
    background: #5865f2;
    color: #fff;
  }

  .create-btn:hover:not(:disabled) {
    background: #4752c4;
  }

  .create-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .create-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }

  .cancel-btn {
    background: rgba(255, 255, 255, 0.1);
    color: #ccc;
  }

  .cancel-btn:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .cancel-btn:focus-visible {
    outline: 2px solid #fbbf24;
    outline-offset: 2px;
  }
</style>
