<script lang="ts">
  import type { InventoryFrame } from '../types';
  import { dropItem } from '../ipc';

  let { inventory, visible = false, onClose }: {
    inventory: InventoryFrame | null;
    visible?: boolean;
    onClose?: () => void;
  } = $props();

  let selectedSlot = $state<number | null>(null);
  let selectedItem = $derived.by(() => {
    if (selectedSlot === null || !inventory) return null;
    return inventory.slots[selectedSlot] ?? null;
  });

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

  function handleKeyDown(e: KeyboardEvent) {
    if (!visible) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      onClose?.();
      return;
    }

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
      if (selectedSlot !== null) {
        handleSlotClick(selectedSlot);
      }
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if visible}
  <div class="inventory-panel" role="dialog" aria-label="Inventory">
    <h3>Inventory</h3>
    <div class="slots" role="grid" aria-label="Inventory slots">
      {#each { length: Math.ceil((inventory?.capacity ?? 16) / 4) } as _, row}
        <div role="row" class="slot-row">
          {#each inventory?.slots?.slice(row * 4, row * 4 + 4) ?? [] as slot, col}
            {@const i = row * 4 + col}
            <button
              type="button"
              class="slot"
              class:selected={selectedSlot === i}
              class:filled={slot !== null}
              role="gridcell"
              aria-label={slot ? `${slot.name} x${slot.count}` : `Empty slot ${i + 1}`}
              onclick={() => handleSlotClick(i)}
            >
              {#if slot}
                <span class="slot-icon">{slot.icon.charAt(0).toUpperCase()}</span>
                <span class="slot-count">{slot.count}</span>
              {/if}
            </button>
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
{/if}

<style>
  .inventory-panel {
    position: fixed;
    top: 0;
    right: 0;
    width: 200px;
    height: 100%;
    background: rgba(20, 20, 40, 0.92);
    border-left: 1px solid #444;
    padding: 12px;
    z-index: 100;
    color: #e0e0e0;
    display: flex;
    flex-direction: column;
  }

  h3 {
    margin: 0 0 12px 0;
    font-size: 0.9rem;
    text-transform: uppercase;
    color: #888;
    letter-spacing: 1px;
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

  .slot:hover {
    border-color: #6a6a9a;
  }

  .slot.selected {
    border-color: #5865f2;
    box-shadow: 0 0 6px rgba(88, 101, 242, 0.4);
  }

  .slot.filled {
    background: rgba(50, 50, 80, 0.9);
  }

  .slot-icon {
    font-size: 1rem;
    line-height: 1;
  }

  .slot-count {
    font-size: 0.6rem;
    color: #aaa;
    position: absolute;
    bottom: 1px;
    right: 3px;
  }

  .item-details {
    margin-top: 12px;
    padding: 8px;
    background: rgba(40, 40, 70, 0.6);
    border-radius: 4px;
  }

  .item-name {
    font-weight: bold;
    font-size: 0.85rem;
    margin-bottom: 2px;
  }

  .item-desc {
    font-size: 0.7rem;
    color: #999;
    margin-bottom: 4px;
    font-style: italic;
  }

  .item-count {
    font-size: 0.75rem;
    color: #aaa;
    margin-bottom: 8px;
  }

  .drop-btn {
    background: rgba(80, 60, 40, 0.8);
    color: #e8c170;
    border: 1px solid #6a5a3a;
    border-radius: 3px;
    padding: 4px 12px;
    cursor: pointer;
    font-size: 0.75rem;
  }

  .drop-btn:hover {
    background: rgba(100, 80, 50, 0.9);
  }
</style>
