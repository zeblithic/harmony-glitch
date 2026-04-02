<script lang="ts">
  import type { InventoryFrame, RecipeDef } from '../types';
  import { dropItem, craftRecipe } from '../ipc';

  let { inventory, recipes = [], visible = false, onClose }: {
    inventory: InventoryFrame | null;
    recipes?: RecipeDef[];
    visible?: boolean;
    onClose?: () => void;
  } = $props();

  let selectedSlot = $state<number | null>(null);
  let activeTab = $state<'items' | 'recipes'>('items');
  let selectedRecipeId = $state<string | null>(null);
  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;
  let craftError = $state<string | null>(null);
  let isCrafting = $state(false);

  let selectedItem = $derived.by(() => {
    if (selectedSlot === null || !inventory) return null;
    return inventory.slots[selectedSlot] ?? null;
  });

  function displayName(itemId: string): string {
    return itemId.split('_').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ');
  }

  function countItem(itemId: string): number {
    if (!inventory) return 0;
    return inventory.slots.reduce((sum, slot) => {
      if (slot && slot.itemId === itemId) return sum + slot.count;
      return sum;
    }, 0);
  }

  function hasRoomForOutput(recipe: RecipeDef): boolean {
    if (!inventory) return false;
    const emptySlots = inventory.slots.filter(s => s === null).length;
    let emptySlotsUsed = 0;
    for (const output of recipe.outputs) {
      const matching = inventory.slots.filter(s => s && s.itemId === output.item);
      // Use stackLimit from existing stack, or assume output.count fits one fresh slot
      const stackLimit = matching[0]?.stackLimit ?? output.count;
      const existingRoom = matching.reduce((sum, s) => sum + (stackLimit - (s?.count ?? 0)), 0);
      let needed = output.count - existingRoom;
      if (needed > 0) {
        const slotsNeeded = Math.ceil(needed / stackLimit);
        if (emptySlotsUsed + slotsNeeded > emptySlots) return false;
        emptySlotsUsed += slotsNeeded;
      }
    }
    return true;
  }

  function isRecipeCraftable(recipe: RecipeDef): boolean {
    if (!hasRoomForOutput(recipe)) return false;
    for (const input of recipe.inputs) {
      if (countItem(input.item) < input.count) return false;
    }
    for (const tool of recipe.tools) {
      if (countItem(tool.item) < tool.count) return false;
    }
    return true;
  }

  let sortedRecipes = $derived.by(() => {
    return [...recipes].sort((a, b) => {
      const aCraftable = isRecipeCraftable(a);
      const bCraftable = isRecipeCraftable(b);
      if (aCraftable && !bCraftable) return -1;
      if (!aCraftable && bCraftable) return 1;
      return a.name.localeCompare(b.name);
    });
  });

  let selectedRecipe = $derived.by(() => {
    if (!selectedRecipeId) return null;
    return recipes.find(r => r.id === selectedRecipeId) ?? null;
  });

  $effect(() => {
    if (visible && dialogEl) {
      previousFocus = document.activeElement as HTMLElement | null;
      if (!dialogEl.open) {
        dialogEl.showModal();
      }
      dialogEl.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]')?.focus();
      return () => {
        if (dialogEl?.open) dialogEl.close();
      };
    } else if (!visible && previousFocus) {
      previousFocus.focus();
      previousFocus = null;
    }
  });

  function handleCancel(e: Event) {
    e.preventDefault();
    onClose?.();
  }

  function handleSlotClick(index: number) {
    selectedSlot = selectedSlot === index ? null : index;
  }

  async function handleDrop() {
    if (selectedSlot === null) return;
    try {
      await dropItem(selectedSlot);
      selectedSlot = null;
    } catch (e) {
      console.error('Drop failed:', e);
    }
  }

  async function handleCraft() {
    if (!selectedRecipeId || isCrafting) return;
    craftError = null;
    isCrafting = true;
    try {
      await craftRecipe(selectedRecipeId);
    } catch (e) {
      craftError = String(e);
    } finally {
      isCrafting = false;
    }
  }

  function switchTab(tab: 'items' | 'recipes') {
    activeTab = tab;
    selectedSlot = null;
    selectedRecipeId = null;
    requestAnimationFrame(() => {
      const panel = dialogEl?.querySelector<HTMLElement>(`[role="tabpanel"]`);
      const firstFocusable = panel?.querySelector<HTMLElement>('button, [tabindex="0"]');
      firstFocusable?.focus();
    });
  }

  function handleTabKey(e: KeyboardEvent) {
    if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
      e.preventDefault();
      switchTab(activeTab === 'items' ? 'recipes' : 'items');
    }
  }

  function handleItemsKeyDown(e: KeyboardEvent) {
    if (e.ctrlKey || e.altKey || e.metaKey) return;

    if (e.key === 'd' || e.key === 'D') {
      handleDrop();
      return;
    }

    if (!inventory) return;
    const cols = 4;
    const total = inventory.capacity;

    if (e.key === 'ArrowRight') {
      e.preventDefault();
      selectedSlot = selectedSlot === null ? 0 : Math.min(selectedSlot + 1, total - 1);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      selectedSlot = selectedSlot === null ? 0 : Math.max(selectedSlot - 1, 0);
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedSlot = selectedSlot === null ? 0 : Math.min(selectedSlot + cols, total - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedSlot = selectedSlot === null ? 0 : Math.max(selectedSlot - cols, 0);
    } else if (e.key === 'Enter') {
      if (selectedSlot !== null) handleSlotClick(selectedSlot);
    }

    if (selectedSlot !== null) {
      const buttons = (e.currentTarget as HTMLElement)
        .querySelectorAll<HTMLElement>('button.slot');
      buttons[selectedSlot]?.focus();
    }
  }

  function handleRecipeListKeyDown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const options = (e.currentTarget as HTMLElement).querySelectorAll<HTMLElement>('[role="option"]');
    if (options.length === 0) return;
    const ids = sortedRecipes.map(r => r.id);
    const currentIdx = selectedRecipeId ? ids.indexOf(selectedRecipeId) : -1;
    let nextIdx: number;
    if (e.key === 'ArrowDown') {
      nextIdx = currentIdx < ids.length - 1 ? currentIdx + 1 : 0;
    } else {
      nextIdx = currentIdx > 0 ? currentIdx - 1 : ids.length - 1;
    }
    selectedRecipeId = ids[nextIdx];
    craftError = null;
    options[nextIdx]?.focus();
  }

  function handleSpaceKey(e: KeyboardEvent) {
    if (e.key === ' ') {
      e.preventDefault();
      handleCraft();
    }
  }
</script>

{#if visible}
  <dialog
    class="inventory-panel"
    aria-label="Inventory"
    aria-modal="true"
    bind:this={dialogEl}
    oncancel={handleCancel}
  >
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <div class="tab-bar" role="tablist" aria-label="Inventory sections" onkeydown={handleTabKey}>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === 'items'}
        aria-controls="panel-items"
        id="tab-items"
        tabindex={activeTab === 'items' ? 0 : -1}
        class="tab"
        class:active={activeTab === 'items'}
        onclick={() => switchTab('items')}
      >Items</button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === 'recipes'}
        aria-controls="panel-recipes"
        id="tab-recipes"
        tabindex={activeTab === 'recipes' ? 0 : -1}
        class="tab"
        class:active={activeTab === 'recipes'}
        onclick={() => switchTab('recipes')}
      >Recipes</button>
    </div>

    {#if activeTab === 'items'}
      <div
        id="panel-items"
        role="tabpanel"
        aria-labelledby="tab-items"
        tabindex="0"
        onkeydown={handleItemsKeyDown}
      >
        <div class="slots" role="grid" aria-label="Inventory slots">
          {#each { length: Math.ceil((inventory?.capacity ?? 16) / 4) } as _, row}
            <div role="row" class="slot-row">
              {#each inventory?.slots?.slice(row * 4, row * 4 + 4) ?? [] as slot, col}
                {@const i = row * 4 + col}
                <div role="gridcell">
                  <button
                    type="button"
                    class="slot"
                    class:selected={selectedSlot === i}
                    class:filled={slot !== null}
                    tabindex={selectedSlot === i || (selectedSlot === null && i === 0) ? 0 : -1}
                    aria-label={slot ? `${slot.name} x${slot.count}` : `Empty slot ${i + 1}`}
                    onclick={() => handleSlotClick(i)}
                  >
                    {#if slot}
                      <span class="slot-icon">{slot.icon.charAt(0).toUpperCase()}</span>
                      <span class="slot-count">{slot.count}</span>
                    {/if}
                  </button>
                </div>
              {/each}
            </div>
          {/each}
        </div>

        {#if selectedItem}
          <div class="item-details">
            <div class="item-name">{selectedItem.name}</div>
            <div class="item-desc">{selectedItem.description}</div>
            <div class="item-count">{selectedItem.count} / {selectedItem.stackLimit}</div>
            <button type="button" class="drop-btn" onclick={handleDrop}>
              Drop
            </button>
          </div>
        {/if}
      </div>
    {:else}
      <div
        id="panel-recipes"
        role="tabpanel"
        aria-labelledby="tab-recipes"
        tabindex="0"
      >
        <div class="recipe-list" role="listbox" aria-label="Recipes" tabindex="-1" onkeydown={handleRecipeListKeyDown}>
          {#each sortedRecipes as recipe (recipe.id)}
            {@const craftable = isRecipeCraftable(recipe)}
            <button
              type="button"
              role="option"
              aria-selected={selectedRecipeId === recipe.id}
              class="recipe-row"
              class:craftable
              class:selected={selectedRecipeId === recipe.id}
              aria-label="{recipe.name}{craftable ? '' : ' (missing ingredients)'}"
              onclick={() => { selectedRecipeId = selectedRecipeId === recipe.id ? null : recipe.id; craftError = null; }}
            >
              <span class="recipe-name">{recipe.name}</span>
              {#if !craftable}
                <span class="recipe-badge">-</span>
              {/if}
            </button>
          {/each}
        </div>

        {#if selectedRecipe}
          <div class="recipe-details">
            <div class="recipe-detail-name">{selectedRecipe.name}</div>
            <div class="recipe-desc">{selectedRecipe.description}</div>

            {#if selectedRecipe.inputs.length > 0}
              <div class="ingredient-section">
                <div class="ingredient-label">Ingredients:</div>
                {#each selectedRecipe.inputs as input}
                  {@const have = countItem(input.item)}
                  <div
                    class="ingredient"
                    class:sufficient={have >= input.count}
                    aria-label="{displayName(input.item)}: have {have}, need {input.count}"
                  >
                    {displayName(input.item)} {have}/{input.count}
                  </div>
                {/each}
              </div>
            {/if}

            {#if selectedRecipe.tools.length > 0}
              <div class="ingredient-section">
                <div class="ingredient-label">Tools:</div>
                {#each selectedRecipe.tools as tool}
                  {@const have = countItem(tool.item)}
                  <div
                    class="ingredient"
                    class:sufficient={have >= tool.count}
                    aria-label="{displayName(tool.item)}: have {have}, need {tool.count}"
                  >
                    {displayName(tool.item)} {have >= tool.count ? '✓' : '✗'}
                  </div>
                {/each}
              </div>
            {/if}

            <div class="ingredient-section">
              <div class="ingredient-label">Produces:</div>
              {#each selectedRecipe.outputs as output}
                <div class="ingredient">{displayName(output.item)} x{output.count}</div>
              {/each}
            </div>

            <button
              type="button"
              class="craft-btn"
              disabled={!isRecipeCraftable(selectedRecipe) || isCrafting}
              onclick={handleCraft}
              onkeydown={handleSpaceKey}
            >
              Craft
            </button>
            {#if craftError}
              <div class="craft-error" role="alert">{craftError}</div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </dialog>
{/if}

<style>
  .inventory-panel {
    position: fixed;
    top: 0;
    right: 0;
    left: auto;
    width: 220px;
    height: 100%;
    max-height: 100%;
    max-width: 220px;
    margin: 0;
    background: rgba(20, 20, 40, 0.92);
    border: none;
    border-left: 1px solid #444;
    padding: 12px;
    z-index: 100;
    color: #e0e0e0;
    display: flex;
    flex-direction: column;
  }

  .inventory-panel::backdrop {
    background: transparent;
  }

  .tab-bar {
    display: flex;
    gap: 2px;
    margin-bottom: 12px;
  }

  .tab {
    flex: 1;
    padding: 6px 0;
    background: rgba(40, 40, 70, 0.6);
    border: 1px solid #3a3a5a;
    border-radius: 4px 4px 0 0;
    color: #888;
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 1px;
    cursor: pointer;
  }

  .tab.active {
    background: rgba(50, 50, 90, 0.9);
    color: #e0e0e0;
    border-bottom-color: transparent;
  }

  .tab:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: -2px;
  }

  .slots {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .slot-row {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
  }

  .slot {
    width: 40px;
    height: 40px;
    background: rgba(40, 40, 70, 0.8);
    border: 1px solid #3a3a5a;
    border-radius: 4px;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 0;
    color: #e0e0e0;
    position: relative;
    font-size: 0.7rem;
  }

  .slot:hover { border-color: #6a6a9a; }
  .slot:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }
  .slot.selected { border-color: #5865f2; box-shadow: 0 0 6px rgba(88, 101, 242, 0.4); }
  .slot.filled { background: rgba(50, 50, 80, 0.9); }

  .slot-icon { font-size: 1rem; line-height: 1; }
  .slot-count { font-size: 0.6rem; color: #aaa; position: absolute; bottom: 1px; right: 3px; }

  .item-details, .recipe-details {
    margin-top: 12px;
    padding: 8px;
    background: rgba(40, 40, 70, 0.6);
    border-radius: 4px;
  }

  .item-name, .recipe-detail-name { font-weight: bold; font-size: 0.85rem; margin-bottom: 2px; }
  .item-desc, .recipe-desc { font-size: 0.7rem; color: #999; margin-bottom: 4px; font-style: italic; }
  .item-count { font-size: 0.75rem; color: #aaa; margin-bottom: 8px; }

  .drop-btn {
    background: rgba(80, 60, 40, 0.8);
    color: #e8c170;
    border: 1px solid #6a5a3a;
    border-radius: 3px;
    padding: 4px 12px;
    cursor: pointer;
    font-size: 0.75rem;
  }

  .drop-btn:hover { background: rgba(100, 80, 50, 0.9); }
  .drop-btn:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }

  .recipe-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 300px;
    overflow-y: auto;
  }

  .recipe-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 8px;
    background: rgba(40, 40, 70, 0.6);
    border: 1px solid transparent;
    border-radius: 3px;
    cursor: pointer;
    color: #888;
    font-size: 0.75rem;
    text-align: left;
    width: 100%;
  }

  .recipe-row.craftable { color: #e0e0e0; }
  .recipe-row.selected { border-color: #5865f2; background: rgba(50, 50, 90, 0.8); }
  .recipe-row:hover { background: rgba(50, 50, 80, 0.7); }
  .recipe-row:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }
  .recipe-name { flex: 1; }
  .recipe-badge { color: #666; font-size: 0.7rem; }

  .ingredient-section { margin: 6px 0; }
  .ingredient-label { font-size: 0.65rem; color: #888; text-transform: uppercase; margin-bottom: 2px; }
  .ingredient { font-size: 0.75rem; color: #c66; padding: 1px 0; }
  .ingredient.sufficient { color: #6c6; }

  .craft-btn {
    margin-top: 8px;
    width: 100%;
    padding: 6px;
    background: rgba(40, 80, 60, 0.8);
    color: #8cd48c;
    border: 1px solid #4a7a4a;
    border-radius: 3px;
    cursor: pointer;
    font-size: 0.8rem;
  }

  .craft-btn:hover:not(:disabled) { background: rgba(50, 100, 70, 0.9); }
  .craft-btn:focus-visible { outline: 2px solid #5865f2; outline-offset: -2px; }
  .craft-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .craft-error {
    margin-top: 4px;
    font-size: 0.7rem;
    color: #e88;
  }
</style>
