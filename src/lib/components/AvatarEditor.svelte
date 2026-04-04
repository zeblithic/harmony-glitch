<script lang="ts">
  import { getAvatar, setAvatar } from '../ipc';
  import type { GameRenderer } from '../engine/renderer';
  import type { AvatarAppearance, AvatarManifest } from '../types';

  let { visible = false, manifest, renderer, onClose }: {
    visible: boolean;
    manifest: AvatarManifest | null;
    renderer: GameRenderer | null;
    onClose?: () => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;
  let savedAppearance = $state<AvatarAppearance | null>(null);
  let pendingAppearance = $state<AvatarAppearance | null>(null);
  let activeTabGroup = $state<'face' | 'hair' | 'body' | 'clothes'>('face');
  let activeCategory = $state<string>('eyes');
  let saving = $state(false);
  let error = $state<string | null>(null);
  let applyGeneration = 0;
  let loadGeneration = 0;

  const VANITY_SLOTS = new Set(['eyes', 'ears', 'nose', 'mouth']);

  const TAB_GROUPS: Record<string, string[]> = {
    face: ['eyes', 'ears', 'nose', 'mouth'],
    hair: ['hair'],
    body: [],
    clothes: ['hat', 'coat', 'shirt', 'pants', 'dress', 'skirt', 'shoes', 'bracelet'],
  };

  const SKIN_PALETTE = [
    'FFDFC4', 'F0D5BE', 'EECEB3', 'E1B899', 'E5C298',
    'D4A574', 'C68642', 'A0522D', '8D5524', '6B3A2A',
    'D4C159', 'A8D8A8', '9BB8D3', 'D8A8D8', 'FFB6C1',
  ];

  const HAIR_PALETTE = [
    '090806', '2C222B', '3B3024', '4E433F', '504444',
    '6A4E42', '8D4A43', 'B55239', 'D6C4C2', 'CABFB1',
    'FFF5E1', 'E6CEA8', '977961', 'A55728', 'B7410E',
    'CD853F', '4A3728', '91A3B0', '3B7DD8', '7B2D8B',
  ];

  // Dialog open/close — matches VolumeSettings pattern
  $effect(() => {
    if (visible && dialogEl) {
      if (!dialogEl.open) {
        previousFocus = document.activeElement as HTMLElement | null;
        error = null;
        const gen = ++loadGeneration;
        getAvatar().then(a => {
          if (gen !== loadGeneration) return; // stale — editor was reopened
          savedAppearance = a;
          pendingAppearance = { ...a };
        }).catch(e => {
          if (gen !== loadGeneration) return;
          console.error('[AvatarEditor] Failed to load avatar:', e);
          error = 'Failed to load avatar. Try closing and reopening.';
        });
        dialogEl.showModal();
      }
      return () => {
        if (dialogEl?.open) dialogEl.close();
        // Revert unsaved changes when closing — unless a save is in-flight,
        // in which case the save's .then() will update savedAppearance and
        // the renderer already has the correct state.
        if (!saving && savedAppearance && renderer) {
          pendingAppearance = { ...savedAppearance };
        }
      };
    } else if (!visible && previousFocus) {
      previousFocus.focus();
      previousFocus = null;
    }
  });

  // Live preview — apply pending appearance to the renderer.
  // Uses a generation counter so stale async calls from rapid selections
  // don't corrupt the compositor's layer state.
  $effect(() => {
    if (pendingAppearance && renderer) {
      const gen = ++applyGeneration;
      const snapshot = pendingAppearance;
      renderer.applyAppearance(snapshot).catch(e => {
        if (gen === applyGeneration) {
          console.error('[AvatarEditor] applyAppearance failed:', e);
        }
      });
    }
  });

  function getSlotValue(category: string): string | null {
    if (!pendingAppearance) return null;
    return (pendingAppearance as Record<string, string | null>)[category] ?? null;
  }

  function selectItem(category: string, itemId: string | null) {
    if (!pendingAppearance) return;
    pendingAppearance = { ...pendingAppearance, [category]: itemId };
  }

  function selectColor(field: 'skinColor' | 'hairColor', hex: string) {
    if (!pendingAppearance) return;
    pendingAppearance = { ...pendingAppearance, [field]: hex };
  }

  async function handleSave() {
    if (!pendingAppearance || saving) return;
    saving = true;
    error = null;
    // Snapshot before await — if the editor closes mid-flight, the cleanup
    // reverts pendingAppearance, so reading it after await would capture
    // the reverted value instead of what was actually saved.
    const snapshot = { ...pendingAppearance };
    try {
      const confirmed = await setAvatar(snapshot);
      savedAppearance = confirmed;
      onClose?.();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function handleCancel() {
    if (saving) return; // Don't revert while save is in-flight
    pendingAppearance = savedAppearance ? { ...savedAppearance } : null;
    onClose?.();
  }

  function handleDialogCancel(e: Event) {
    e.preventDefault();
    if (saving) return;
    handleCancel();
  }

  function switchTabGroup(group: string) {
    activeTabGroup = group as typeof activeTabGroup;
    const cats = TAB_GROUPS[group];
    if (cats.length > 0) {
      activeCategory = cats[0];
    }
  }

  function handleGroupTabKey(e: KeyboardEvent) {
    const groups = Object.keys(TAB_GROUPS);
    const idx = groups.indexOf(activeTabGroup);
    let nextIdx = idx;
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      nextIdx = (idx + 1) % groups.length;
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      nextIdx = (idx - 1 + groups.length) % groups.length;
    } else {
      return;
    }
    switchTabGroup(groups[nextIdx]);
    // Focus by ID — avoids stale DOM from querying aria-selected before flush
    const target = (e.currentTarget as HTMLElement)?.querySelector<HTMLElement>(`#tab-${groups[nextIdx]}`);
    target?.focus();
  }

  function handleSubTabKey(e: KeyboardEvent) {
    const cats = TAB_GROUPS[activeTabGroup];
    const idx = cats.indexOf(activeCategory);
    let nextIdx = idx;
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      nextIdx = (idx + 1) % cats.length;
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      nextIdx = (idx - 1 + cats.length) % cats.length;
    } else {
      return;
    }
    activeCategory = cats[nextIdx];
    const target = (e.currentTarget as HTMLElement)?.querySelector<HTMLElement>(`#subtab-${cats[nextIdx]}`);
    target?.focus();
  }

  function displayName(s: string): string {
    return s.charAt(0).toUpperCase() + s.slice(1).replace(/_/g, ' ');
  }
</script>

{#if visible}
  <dialog class="avatar-editor" aria-label="Avatar Editor" aria-modal="true"
    bind:this={dialogEl} oncancel={handleDialogCancel}>

    <div class="panel-header">
      <h2>Avatar Editor</h2>
      <button type="button" class="close-btn" aria-label="Close avatar editor"
        onclick={handleCancel}>&times;</button>
    </div>

    <div class="tab-bar" role="tablist" aria-label="Avatar sections"
      onkeydown={handleGroupTabKey}>
      {#each Object.keys(TAB_GROUPS) as group}
        <button type="button" role="tab"
          aria-selected={activeTabGroup === group}
          aria-controls="panel-{group}"
          id="tab-{group}"
          tabindex={activeTabGroup === group ? 0 : -1}
          class="tab" class:active={activeTabGroup === group}
          onclick={() => switchTabGroup(group)}>
          {displayName(group)}
        </button>
      {/each}
    </div>

    {#if TAB_GROUPS[activeTabGroup].length > 1}
      <div class="sub-tab-bar" role="tablist" aria-label="{displayName(activeTabGroup)} categories"
        onkeydown={handleSubTabKey}>
        {#each TAB_GROUPS[activeTabGroup] as cat}
          <button type="button" role="tab"
            id="subtab-{cat}"
            aria-selected={activeCategory === cat}
            tabindex={activeCategory === cat ? 0 : -1}
            class="sub-tab" class:active={activeCategory === cat}
            onclick={() => { activeCategory = cat; }}>
            {displayName(cat)}
          </button>
        {/each}
      </div>
    {/if}

    <div id="panel-{activeTabGroup}" role="tabpanel" tabindex="0"
      aria-labelledby="tab-{activeTabGroup}" class="panel-content">

      {#if activeTabGroup === 'body'}
        <div class="color-section">
          <span class="section-label">Skin Color</span>
          <div class="color-swatches" role="radiogroup" aria-label="Skin color">
            {#each SKIN_PALETTE as hex}
              <button type="button" role="radio"
                class="swatch" class:selected={pendingAppearance?.skinColor === hex}
                aria-checked={pendingAppearance?.skinColor === hex}
                aria-label="Skin color #{hex}"
                style="background-color: #{hex};"
                onclick={() => selectColor('skinColor', hex)}>
              </button>
            {/each}
          </div>
        </div>

      {:else if activeTabGroup === 'hair'}
        {#if manifest?.categories.hair}
          <div class="item-list" role="listbox" aria-label="Hair styles">
            {#each manifest.categories.hair.items as item (item.id)}
              <button type="button" role="option"
                class="item-option" class:selected={pendingAppearance?.hair === item.id}
                aria-selected={pendingAppearance?.hair === item.id}
                onclick={() => selectItem('hair', item.id)}>
                {item.name}
              </button>
            {/each}
          </div>
        {/if}
        <div class="color-section">
          <span class="section-label">Hair Color</span>
          <div class="color-swatches" role="radiogroup" aria-label="Hair color">
            {#each HAIR_PALETTE as hex}
              <button type="button" role="radio"
                class="swatch" class:selected={pendingAppearance?.hairColor === hex}
                aria-checked={pendingAppearance?.hairColor === hex}
                aria-label="Hair color #{hex}"
                style="background-color: #{hex};"
                onclick={() => selectColor('hairColor', hex)}>
              </button>
            {/each}
          </div>
        </div>

      {:else}
        {#if manifest?.categories[activeCategory]}
          <div class="item-list" role="listbox" aria-label="{displayName(activeCategory)} options">
            {#if !VANITY_SLOTS.has(activeCategory)}
              <button type="button" role="option"
                class="item-option" class:selected={getSlotValue(activeCategory) === null}
                aria-selected={getSlotValue(activeCategory) === null}
                onclick={() => selectItem(activeCategory, null)}>
                None
              </button>
            {/if}
            {#each manifest.categories[activeCategory].items as item (item.id)}
              <button type="button" role="option"
                class="item-option" class:selected={getSlotValue(activeCategory) === item.id}
                aria-selected={getSlotValue(activeCategory) === item.id}
                onclick={() => selectItem(activeCategory, item.id)}>
                {item.name}
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    </div>

    <div class="editor-footer">
      <button type="button" class="cancel-btn" onclick={handleCancel}>Cancel</button>
      <button type="button" class="save-btn" disabled={saving} onclick={handleSave}>
        {saving ? 'Saving...' : 'Save'}
      </button>
    </div>
    {#if error}
      <div class="error-msg" role="alert">{error}</div>
    {/if}
  </dialog>
{/if}

<style>
  .avatar-editor {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 340px;
    max-height: 80vh;
    padding: 16px;
    background: rgba(20, 20, 40, 0.95);
    border: 1px solid #444;
    border-radius: 8px;
    color: #e0e0e0;
    z-index: 100;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .avatar-editor::backdrop {
    background: transparent;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .panel-header h2 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }

  .close-btn {
    background: none;
    border: none;
    color: #888;
    font-size: 1.4rem;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
  }

  .close-btn:hover { color: #e0e0e0; }
  .close-btn:focus-visible { outline: 2px solid #5865f2; outline-offset: 2px; }

  .tab-bar {
    display: flex;
    gap: 2px;
    margin-bottom: 8px;
  }

  .tab {
    flex: 1;
    padding: 6px 4px;
    background: rgba(40, 40, 70, 0.6);
    border: 1px solid transparent;
    border-radius: 4px 4px 0 0;
    color: #999;
    font-size: 0.75rem;
    cursor: pointer;
    text-align: center;
  }

  .tab.active {
    background: rgba(50, 50, 90, 0.8);
    color: #e0e0e0;
    border-color: #5865f2;
    border-bottom-color: transparent;
  }

  .tab:hover:not(.active) { color: #ccc; }
  .tab:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }

  .sub-tab-bar {
    display: flex;
    gap: 2px;
    margin-bottom: 8px;
    flex-wrap: wrap;
  }

  .sub-tab {
    padding: 4px 8px;
    background: rgba(40, 40, 70, 0.4);
    border: 1px solid transparent;
    border-radius: 3px;
    color: #888;
    font-size: 0.7rem;
    cursor: pointer;
  }

  .sub-tab.active {
    background: rgba(50, 50, 90, 0.6);
    color: #e0e0e0;
    border-color: #5865f2;
  }

  .sub-tab:hover:not(.active) { color: #ccc; }
  .sub-tab:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }

  .panel-content {
    flex: 1;
    overflow-y: auto;
    min-height: 120px;
  }

  .item-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 200px;
    overflow-y: auto;
  }

  .item-option {
    text-align: left;
    padding: 6px 10px;
    background: rgba(40, 40, 70, 0.6);
    border: 1px solid transparent;
    border-radius: 3px;
    color: #e0e0e0;
    font-size: 0.75rem;
    cursor: pointer;
    width: 100%;
  }

  .item-option.selected {
    border-color: #5865f2;
    background: rgba(50, 50, 90, 0.8);
  }

  .item-option:hover:not(.selected) { background: rgba(50, 50, 80, 0.7); }
  .item-option:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }

  .color-section {
    margin-top: 12px;
  }

  .section-label {
    display: block;
    font-size: 0.75rem;
    color: #999;
    margin-bottom: 6px;
  }

  .color-swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .swatch {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
  }

  .swatch.selected {
    border-color: #5865f2;
    box-shadow: 0 0 6px rgba(88, 101, 242, 0.6);
  }

  .swatch:hover:not(.selected) { border-color: #666; }
  .swatch:focus-visible { outline: 2px solid #5865f2; outline-offset: 2px; }

  .editor-footer {
    display: flex;
    gap: 8px;
    margin-top: 12px;
    justify-content: flex-end;
  }

  .save-btn {
    padding: 6px 16px;
    background: rgba(40, 80, 60, 0.8);
    color: #8cd48c;
    border: 1px solid #4a7a4a;
    border-radius: 3px;
    cursor: pointer;
    font-size: 0.8rem;
  }

  .save-btn:hover:not(:disabled) { background: rgba(50, 100, 70, 0.9); }
  .save-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .save-btn:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }

  .cancel-btn {
    padding: 6px 16px;
    background: rgba(60, 40, 40, 0.8);
    color: #ccc;
    border: 1px solid #555;
    border-radius: 3px;
    cursor: pointer;
    font-size: 0.8rem;
  }

  .cancel-btn:hover { background: rgba(80, 50, 50, 0.9); }
  .cancel-btn:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }

  .error-msg {
    margin-top: 8px;
    padding: 6px 10px;
    background: rgba(120, 30, 30, 0.6);
    border-radius: 3px;
    font-size: 0.75rem;
    color: #ff9999;
  }
</style>
