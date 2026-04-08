<script lang="ts">
  import { listStreets, loadStreet } from '../ipc';
  import type { StreetListEntry } from '../ipc';
  import type { StreetData } from '../types';
  import { onMount } from 'svelte';

  let { onStreetLoaded }: {
    onStreetLoaded: (street: StreetData) => void;
  } = $props();

  let streets = $state<StreetListEntry[]>([]);
  let search = $state('');
  let initialLoading = $state(true);
  let loading = $state(false);
  let error = $state<string | null>(null);

  let filtered = $derived(
    search.length > 0
      ? streets.filter(s => s.name.toLowerCase().includes(search.toLowerCase()))
      : streets
  );

  onMount(async () => {
    try {
      streets = await listStreets();
    } catch (e) {
      error = `Failed to list streets: ${e}`;
    } finally {
      initialLoading = false;
    }
  });

  async function handleSelect(tsid: string) {
    loading = true;
    error = null;
    try {
      const street = await loadStreet(tsid);
      onStreetLoaded(street);
    } catch (e) {
      error = `Failed to load street: ${e}`;
    } finally {
      loading = false;
    }
  }
</script>

<div class="street-picker">
  <h1>Harmony Glitch</h1>
  <p class="subtitle">Choose a street to explore</p>

  <p class="error" class:sr-only={!error} role="alert">{error ?? ''}</p>

  <div role="status" aria-live="polite" class="sr-only">
    {#if initialLoading}Loading streets…{:else if loading}Loading street, please wait…{/if}
  </div>

  {#if streets.length > 4}
    <input
      type="search"
      class="search-input"
      placeholder="Search {streets.length} streets…"
      bind:value={search}
      aria-label="Search streets"
    />
  {/if}

  <div class="street-list">
    {#each filtered as entry}
      <button
        type="button"
        class="street-btn"
        onclick={() => handleSelect(entry.tsid)}
        disabled={loading}
      >
        {entry.name}
      </button>
    {/each}

    {#if initialLoading}
      <p class="empty">Loading streets…</p>
    {:else if filtered.length === 0 && search.length > 0}
      <p class="empty">No streets matching "{search}"</p>
    {:else if streets.length === 0 && !error}
      <p class="empty">No streets available</p>
    {/if}
  </div>
</div>

<style>
  .street-picker {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 16px;
    padding: 32px;
  }

  h1 {
    font-size: 2rem;
    color: #e0e0e0;
    margin: 0;
  }

  .subtitle {
    color: #888;
    font-size: 0.9rem;
    margin: 0;
  }

  .error {
    color: #e74c3c;
    font-size: 0.85rem;
  }

  .search-input {
    padding: 8px 16px;
    border: 1px solid #444;
    border-radius: 8px;
    background: #1a1a3a;
    color: #e0e0e0;
    font-size: 0.9rem;
    width: 280px;
    outline: none;
  }

  .search-input:focus {
    border-color: #5865f2;
  }

  .street-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 16px;
    max-height: 60vh;
    overflow-y: auto;
  }

  .street-btn {
    padding: 12px 32px;
    border: 1px solid #444;
    border-radius: 8px;
    background: #2a2a4a;
    color: #e0e0e0;
    font-size: 1rem;
    cursor: pointer;
  }

  .street-btn:hover:not(:disabled) {
    background: #5865f2;
    border-color: #5865f2;
  }

  .street-btn:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: 2px;
  }

  .street-btn:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .empty {
    color: #666;
    font-size: 0.85rem;
  }
</style>
