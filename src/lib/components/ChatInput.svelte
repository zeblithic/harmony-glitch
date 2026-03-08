<script lang="ts">
  import { sendChat } from '../ipc';

  let { onFocusChange }: { onFocusChange: (focused: boolean) => void } = $props();

  let inputEl = $state<HTMLInputElement>();
  let text = $state('');
  let focused = $state(false);

  function handleGlobalKeyDown(e: KeyboardEvent) {
    if (!focused && (e.key === 'Enter' || e.key === '/')) {
      e.preventDefault();
      focused = true;
      onFocusChange(true);
      requestAnimationFrame(() => inputEl?.focus());
    }
  }

  function handleSubmit() {
    if (text.trim()) {
      sendChat(text.trim()).catch(console.error);
      text = '';
    }
    handleBlur();
  }

  function handleBlur() {
    focused = false;
    onFocusChange(false);
    inputEl?.blur();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      text = '';
      handleBlur();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      handleSubmit();
    }
    e.stopPropagation();
  }
</script>

<svelte:window onkeydown={focused ? undefined : handleGlobalKeyDown} />

{#if focused}
  <div class="chat-input">
    <label>
      <span class="sr-only">Chat message</span>
      <input
        bind:this={inputEl}
        bind:value={text}
        onkeydown={handleKeyDown}
        placeholder="Type a message..."
        maxlength="200"
      />
    </label>
  </div>
{/if}

<style>
  .chat-input {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 100;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  input {
    width: 400px;
    padding: 8px 12px;
    font-size: 14px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-radius: 20px;
    background: rgba(0, 0, 0, 0.6);
    color: white;
    outline: none;
  }
  input:focus {
    border-color: rgba(88, 101, 242, 0.8);
  }
</style>
