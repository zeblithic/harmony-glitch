<script lang="ts">
  import { onMount } from 'svelte';
  import { setDisplayName } from '../ipc';

  let { onComplete }: { onComplete: (displayName: string) => void } = $props();
  let name = $state('');
  let submitting = $state(false);
  let error = $state('');
  let inputEl = $state<HTMLInputElement>();

  onMount(() => { inputEl?.focus(); });

  async function handleSubmit() {
    const trimmed = name.trim();
    if (trimmed.length < 1 || submitting) return;
    submitting = true;
    error = '';
    try {
      await setDisplayName(trimmed);
      onComplete(trimmed);
    } catch (e) {
      console.error('Failed to set display name:', e);
      error = 'Could not save your name. Please try again.';
    } finally {
      submitting = false;
    }
  }
</script>

<div class="identity-setup" role="dialog" aria-label="Choose your display name">
  <h2>Welcome to Ur</h2>
  <p>Choose a name for your Glitchen:</p>
  <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
    <label>
      <span class="sr-only">Display name</span>
      <input
        bind:this={inputEl}
        bind:value={name}
        placeholder="Enter display name"
        maxlength="30"
      />
    </label>
    <button type="submit" disabled={name.trim().length < 1 || submitting}>
      Enter the World
    </button>
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </form>
</div>

<style>
  .identity-setup {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 16px;
    padding: 32px;
  }

  h2 {
    font-size: 2rem;
    color: #e0e0e0;
    margin: 0;
  }

  p {
    color: #888;
    font-size: 0.9rem;
    margin: 0;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    align-items: center;
    margin-top: 8px;
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
    padding: 10px 16px;
    font-size: 1rem;
    border: 2px solid #444;
    border-radius: 8px;
    background: #2a2a4a;
    color: #e0e0e0;
    width: 260px;
    text-align: center;
  }

  input:focus {
    border-color: #5865f2;
  }

  input:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: 2px;
  }

  button {
    padding: 10px 24px;
    border: none;
    border-radius: 8px;
    background: #5865f2;
    color: white;
    font-size: 1rem;
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    background: #4752c4;
  }

  button:focus-visible {
    outline: 2px solid white;
    outline-offset: 2px;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .error {
    color: #ff6b6b;
    font-size: 0.85rem;
    margin: 0;
  }
</style>
