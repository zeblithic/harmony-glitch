<script lang="ts">
  import type { TradeFrame, InventoryFrame } from '../types';

  let {
    tradeFrame = null,
    inventory = null,
    currants = 0,
    visible = false,
    onClose = undefined,
    onAddItem = undefined,
    onRemoveItem = undefined,
    onSetCurrants = undefined,
    onLock = undefined,
    onUnlock = undefined,
    onCancel = undefined,
  }: {
    tradeFrame: TradeFrame | null;
    inventory: InventoryFrame | null;
    currants: number;
    visible: boolean;
    onClose?: () => void;
    onAddItem?: (itemId: string, count: number) => void;
    onRemoveItem?: (itemId: string, count: number) => void;
    onSetCurrants?: (amount: number) => void;
    onLock?: () => void;
    onUnlock?: () => void;
    onCancel?: () => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;
  let currantsInput = $state(0);

  $effect(() => {
    if (visible && dialogEl) {
      if (!dialogEl.open) {
        previousFocus = document.activeElement as HTMLElement | null;
        dialogEl.showModal();
        currantsInput = tradeFrame?.localOffer.currants ?? 0;
      }
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
    onCancel?.();
    onClose?.();
  }

  let statusText = $derived.by(() => {
    if (!tradeFrame) return '';
    switch (tradeFrame.phase) {
      case 'pending': return 'Waiting for response...';
      case 'negotiating': return 'Adjust your offer';
      case 'lockedLocal': return 'Waiting for partner to lock in...';
      case 'lockedRemote': return 'Partner locked in — review and lock yours';
      case 'executing': return 'Executing trade...';
      case 'completed': return 'Trade complete!';
      case 'cancelled': return 'Trade cancelled';
      default: return '';
    }
  });

  let canModify = $derived(
    tradeFrame?.phase === 'negotiating' || tradeFrame?.phase === 'lockedRemote'
  );

  let canLock = $derived(canModify);

  let isLocked = $derived(
    tradeFrame?.phase === 'lockedLocal' || tradeFrame?.phase === 'executing'
  );

  // Available items from inventory that can be added to the offer.
  let availableItems = $derived.by(() => {
    if (!inventory) return [];
    const offered = new Map<string, number>();
    for (const item of tradeFrame?.localOffer.items ?? []) {
      offered.set(item.itemId, (offered.get(item.itemId) ?? 0) + item.count);
    }
    const totals = new Map<string, { itemId: string; name: string; icon: string; total: number }>();
    for (const slot of inventory.slots) {
      if (!slot) continue;
      const existing = totals.get(slot.itemId);
      totals.set(slot.itemId, {
        itemId: slot.itemId,
        name: slot.name,
        icon: slot.icon,
        total: (existing?.total ?? 0) + slot.count,
      });
    }
    return [...totals.values()]
      .map(i => ({ ...i, available: i.total - (offered.get(i.itemId) ?? 0) }))
      .filter(i => i.available > 0);
  });

  function handleCurrantsChange() {
    const val = Math.max(0, Math.min(currantsInput, currants));
    currantsInput = val;
    onSetCurrants?.(val);
  }
</script>

{#if visible && tradeFrame}
  <dialog
    bind:this={dialogEl}
    class="trade-dialog"
    aria-label="Trading with {tradeFrame.peerName}"
    aria-modal="true"
    oncancel={handleCancel}
  >
    <header class="trade-header">
      <h2>Trading with {tradeFrame.peerName}</h2>
      <button class="close-btn" onclick={() => { onCancel?.(); onClose?.(); }} aria-label="Cancel trade">✕</button>
    </header>

    <div class="trade-columns">
      <!-- Your Offer -->
      <div class="trade-column">
        <h3>Your Offer</h3>
        <div class="offer-items">
          {#each tradeFrame.localOffer.items as item (item.itemId)}
            <div class="offer-item">
              <span class="item-icon">{item.icon.charAt(0)}</span>
              <span class="item-name">{item.name}</span>
              <span class="item-count">×{item.count}</span>
              {#if canModify}
                <button
                  class="remove-btn"
                  onclick={() => onRemoveItem?.(item.itemId, 1)}
                  aria-label="Remove 1 {item.name}"
                >−</button>
              {/if}
            </div>
          {/each}
          {#if tradeFrame.localOffer.currants > 0}
            <div class="offer-item currants-row">
              <span class="item-icon">💰</span>
              <span class="item-name">Currants</span>
              <span class="item-count">{tradeFrame.localOffer.currants}</span>
            </div>
          {/if}
          {#if tradeFrame.localOffer.items.length === 0 && tradeFrame.localOffer.currants === 0}
            <p class="empty-offer">No items offered</p>
          {/if}
        </div>
      </div>

      <!-- Their Offer -->
      <div class="trade-column">
        <h3>{tradeFrame.peerName}'s Offer</h3>
        <div class="offer-items">
          {#each tradeFrame.remoteOffer.items as item (item.itemId)}
            <div class="offer-item">
              <span class="item-icon">{item.icon.charAt(0)}</span>
              <span class="item-name">{item.name}</span>
              <span class="item-count">×{item.count}</span>
            </div>
          {/each}
          {#if tradeFrame.remoteOffer.currants > 0}
            <div class="offer-item currants-row">
              <span class="item-icon">💰</span>
              <span class="item-name">Currants</span>
              <span class="item-count">{tradeFrame.remoteOffer.currants}</span>
            </div>
          {/if}
          {#if tradeFrame.remoteOffer.items.length === 0 && tradeFrame.remoteOffer.currants === 0}
            <p class="empty-offer">Waiting for offer...</p>
          {/if}
        </div>
      </div>
    </div>

    <!-- Item picker from inventory -->
    {#if canModify}
      <div class="item-picker">
        <h4>Add from inventory</h4>
        <div class="picker-grid">
          {#each availableItems as item (item.itemId)}
            <button
              class="picker-item"
              onclick={() => onAddItem?.(item.itemId, 1)}
              aria-label="Add 1 {item.name} to offer ({item.available} available)"
            >
              <span class="picker-icon">{item.icon.charAt(0)}</span>
              <span class="picker-count">{item.available}</span>
            </button>
          {/each}
          {#if availableItems.length === 0}
            <p class="empty-picker">No items available</p>
          {/if}
        </div>
        <div class="currants-picker">
          <label for="trade-currants">Currants:</label>
          <input
            id="trade-currants"
            type="number"
            min="0"
            max={currants}
            bind:value={currantsInput}
            onchange={handleCurrantsChange}
            oninput={handleCurrantsChange}
            disabled={!canModify}
          />
          <span class="currants-max">(max {currants})</span>
        </div>
      </div>
    {/if}

    <!-- Status and actions -->
    <div class="trade-footer">
      <p class="trade-status" role="status" aria-live="polite">{statusText}</p>
      <div class="trade-actions">
        {#if canLock && !isLocked}
          <button class="action-btn lock" onclick={() => onLock?.()} aria-label="Lock in your offer">Lock In</button>
        {/if}
        {#if isLocked && tradeFrame.phase === 'lockedLocal'}
          <button class="action-btn unlock" onclick={() => onUnlock?.()} aria-label="Unlock your offer">Unlock</button>
        {/if}
        <button class="action-btn cancel" onclick={() => { onCancel?.(); onClose?.(); }} aria-label="Cancel trade">Cancel</button>
      </div>
      <div class="lock-indicators">
        {#if tradeFrame.localLocked}
          <span class="lock-badge you">You: Locked ✓</span>
        {/if}
        {#if tradeFrame.remoteLocked}
          <span class="lock-badge them">{tradeFrame.peerName}: Locked ✓</span>
        {/if}
      </div>
    </div>
  </dialog>
{/if}

<style>
  .trade-dialog {
    background: rgba(30, 30, 46, 0.97);
    color: #e0e0e0;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 12px;
    padding: 0;
    width: min(600px, 90vw);
    max-height: 80vh;
    overflow-y: auto;
  }

  .trade-dialog::backdrop {
    background: rgba(0, 0, 0, 0.6);
  }

  .trade-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px 12px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .trade-header h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }

  .close-btn {
    background: none;
    border: none;
    color: #888;
    font-size: 18px;
    cursor: pointer;
    padding: 4px 8px;
  }

  .close-btn:hover {
    color: #fff;
  }

  .trade-columns {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    background: rgba(255, 255, 255, 0.08);
  }

  .trade-column {
    padding: 12px 16px;
    background: rgba(30, 30, 46, 0.97);
  }

  .trade-column h3 {
    margin: 0 0 8px;
    font-size: 13px;
    font-weight: 600;
    text-transform: uppercase;
    color: #888;
  }

  .offer-items {
    min-height: 60px;
  }

  .offer-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    font-size: 13px;
  }

  .item-icon {
    width: 20px;
    text-align: center;
  }

  .item-name {
    flex: 1;
  }

  .item-count {
    color: #888;
    font-size: 12px;
  }

  .remove-btn {
    background: rgba(255, 80, 80, 0.2);
    border: none;
    color: #f88;
    border-radius: 3px;
    width: 20px;
    height: 20px;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 0;
  }

  .remove-btn:hover {
    background: rgba(255, 80, 80, 0.4);
  }

  .empty-offer {
    color: #555;
    font-size: 12px;
    font-style: italic;
    margin: 8px 0;
  }

  .item-picker {
    padding: 12px 16px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
  }

  .item-picker h4 {
    margin: 0 0 8px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    color: #888;
  }

  .picker-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .picker-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    padding: 6px;
    min-width: 44px;
    cursor: pointer;
    color: #e0e0e0;
  }

  .picker-item:hover {
    background: rgba(88, 101, 242, 0.2);
    border-color: #5865f2;
  }

  .picker-icon {
    font-size: 18px;
  }

  .picker-count {
    font-size: 10px;
    color: #888;
  }

  .empty-picker {
    color: #555;
    font-size: 12px;
    font-style: italic;
  }

  .currants-picker {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    font-size: 13px;
  }

  .currants-picker input {
    width: 80px;
    background: rgba(255, 255, 255, 0.06);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 4px;
    color: #e0e0e0;
    padding: 4px 8px;
    font-size: 13px;
  }

  .currants-max {
    color: #666;
    font-size: 11px;
  }

  .trade-footer {
    padding: 12px 16px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    text-align: center;
  }

  .trade-status {
    margin: 0 0 8px;
    font-size: 13px;
    color: #aaa;
  }

  .trade-actions {
    display: flex;
    justify-content: center;
    gap: 8px;
  }

  .action-btn {
    padding: 8px 20px;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .action-btn.lock {
    background: #5865f2;
    color: white;
  }

  .action-btn.lock:hover {
    background: #4752c4;
  }

  .action-btn.unlock {
    background: rgba(255, 255, 255, 0.1);
    color: #e0e0e0;
  }

  .action-btn.cancel {
    background: rgba(255, 80, 80, 0.15);
    color: #f88;
  }

  .action-btn.cancel:hover {
    background: rgba(255, 80, 80, 0.3);
  }

  .lock-indicators {
    display: flex;
    justify-content: center;
    gap: 12px;
    margin-top: 8px;
  }

  .lock-badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 10px;
  }

  .lock-badge.you {
    background: rgba(88, 101, 242, 0.2);
    color: #8a93f2;
  }

  .lock-badge.them {
    background: rgba(80, 200, 120, 0.2);
    color: #6dd898;
  }
</style>
