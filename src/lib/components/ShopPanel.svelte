<script lang="ts">
  import type { StoreState } from '../types';

  let {
    storeState = null,
    visible = false,
    onClose = undefined,
    onBuy = undefined,
    onSell = undefined,
  }: {
    storeState: StoreState | null;
    visible: boolean;
    onClose?: () => void;
    onBuy?: (itemId: string, count: number) => void;
    onSell?: (itemId: string, count: number) => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let previousFocus: HTMLElement | null = null;
  let activeTab = $state<'buy' | 'sell'>('buy');

  $effect(() => {
    if (visible && storeState && dialogEl) {
      if (!dialogEl.open) {
        previousFocus = document.activeElement as HTMLElement | null;
        dialogEl.showModal();
        const firstFocusable = dialogEl.querySelector<HTMLElement>('button');
        firstFocusable?.focus();
      }
      return () => {
        if (dialogEl?.open) dialogEl.close();
        if (previousFocus) {
          previousFocus.focus();
          previousFocus = null;
        }
        activeTab = 'buy';
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

  function handleBuyClick(itemId: string, baseCost: number, e: MouseEvent) {
    if (!storeState) return;
    if (e.shiftKey) {
      const maxAffordable = Math.floor(storeState.currants / baseCost);
      if (maxAffordable > 0) onBuy?.(itemId, maxAffordable);
    } else {
      onBuy?.(itemId, 1);
    }
  }

  function handleSellClick(itemId: string, count: number, e: MouseEvent) {
    if (e.shiftKey) {
      onSell?.(itemId, count);
    } else {
      onSell?.(itemId, 1);
    }
  }
</script>

{#if visible && storeState}
  <dialog
    class="shop-panel"
    aria-label="Shop: {storeState.name}"
    aria-modal="true"
    bind:this={dialogEl}
    oncancel={handleCancel}
  >
    <div class="panel-header">
      <h2>{storeState.name}</h2>
      <span class="currant-balance" aria-label="Currant balance: {storeState.currants}">
        <span class="currant-icon">✦</span>{storeState.currants}
      </span>
      <button
        type="button"
        class="close-btn"
        aria-label="Close shop"
        onclick={() => onClose?.()}
      >
        &times;
      </button>
    </div>

    <div class="tab-bar" role="tablist" aria-label="Shop tabs">
      <button
        type="button"
        role="tab"
        id="tab-buy"
        aria-selected={activeTab === 'buy'}
        aria-controls="panel-buy"
        class="tab-btn"
        class:active={activeTab === 'buy'}
        onclick={() => (activeTab = 'buy')}
      >
        Buy
      </button>
      <button
        type="button"
        role="tab"
        id="tab-sell"
        aria-selected={activeTab === 'sell'}
        aria-controls="panel-sell"
        class="tab-btn"
        class:active={activeTab === 'sell'}
        onclick={() => (activeTab = 'sell')}
      >
        Sell
      </button>
    </div>

    {#if activeTab === 'buy'}
      <div
        role="tabpanel"
        id="panel-buy"
        aria-labelledby="tab-buy"
        class="tab-panel"
      >
        <ul class="item-list">
          {#each storeState.vendorInventory as item (item.itemId)}
            <li class="item-row">
              <span class="item-name">{item.name}</span>
              <span class="item-price gold">{item.baseCost} ✦</span>
              <button
                type="button"
                class="action-btn buy-btn"
                aria-label="Buy {item.name}"
                disabled={storeState.currants < item.baseCost}
                onclick={(e) => handleBuyClick(item.itemId, item.baseCost, e)}
              >
                Buy
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {:else}
      <div
        role="tabpanel"
        id="panel-sell"
        aria-labelledby="tab-sell"
        class="tab-panel"
      >
        {#if storeState.playerInventory.length === 0}
          <p class="empty-message">No items to sell</p>
        {:else}
          <ul class="item-list">
            {#each storeState.playerInventory as item (item.itemId)}
              <li class="item-row">
                <span class="item-name">{item.name}</span>
                <span class="item-count">×{item.count}</span>
                <span class="item-price gold">{item.sellPrice} ✦</span>
                <button
                  type="button"
                  class="action-btn sell-btn"
                  aria-label="Sell {item.name}"
                  onclick={(e) => handleSellClick(item.itemId, item.count, e)}
                >
                  Sell
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}

    <p class="footer-hint">Click = 1 · Shift+click = stack</p>
  </dialog>
{/if}

<style>
  .shop-panel {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 340px;
    max-height: 80vh;
    padding: 16px;
    background: #1a1a2e;
    border: none;
    border-radius: 8px;
    color: #e0e0e0;
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .shop-panel::backdrop {
    background: rgba(0, 0, 0, 0.5);
  }

  .panel-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .panel-header h2 {
    flex: 1;
    margin: 0;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #ccc;
  }

  .currant-balance {
    font-size: 0.8rem;
    color: #ffd700;
    font-weight: bold;
    white-space: nowrap;
  }

  .currant-icon {
    margin-right: 2px;
  }

  .close-btn {
    background: none;
    border: none;
    color: #888;
    font-size: 1.2rem;
    cursor: pointer;
    padding: 0 4px;
    line-height: 1;
  }

  .close-btn:hover {
    color: #e0e0e0;
  }

  .close-btn:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: 2px;
  }

  .tab-bar {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid #333;
    padding-bottom: 6px;
  }

  .tab-btn {
    background: none;
    border: none;
    border-radius: 4px 4px 0 0;
    color: #888;
    font-size: 0.8rem;
    padding: 5px 14px;
    cursor: pointer;
    transition: color 0.15s, background 0.15s;
  }

  .tab-btn:hover {
    color: #e0e0e0;
    background: rgba(255, 255, 255, 0.07);
  }

  .tab-btn.active {
    color: #e0e0e0;
    background: rgba(124, 111, 224, 0.2);
    border-bottom: 2px solid #7c6fe0;
  }

  .tab-btn:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: 2px;
  }

  .tab-panel {
    flex: 1;
    overflow-y: auto;
    min-height: 60px;
  }

  .item-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .item-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.03);
    font-size: 0.8rem;
  }

  .item-row:hover {
    background: rgba(255, 255, 255, 0.07);
  }

  .item-name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .item-count {
    color: #aaa;
    font-size: 0.75rem;
    white-space: nowrap;
  }

  .item-price {
    font-size: 0.75rem;
    white-space: nowrap;
  }

  .gold {
    color: #ffd700;
  }

  .action-btn {
    border: none;
    border-radius: 4px;
    font-size: 0.7rem;
    padding: 3px 10px;
    cursor: pointer;
    font-weight: 600;
    transition: opacity 0.15s, background 0.15s;
  }

  .action-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .action-btn:focus-visible {
    outline: 2px solid #5865f2;
    outline-offset: 2px;
  }

  .buy-btn {
    background: #7c6fe0;
    color: #fff;
  }

  .buy-btn:hover:not(:disabled) {
    background: #9d93e8;
  }

  .sell-btn {
    background: #4a8c5c;
    color: #fff;
  }

  .sell-btn:hover:not(:disabled) {
    background: #5aa872;
  }

  .empty-message {
    text-align: center;
    color: #888;
    font-size: 0.8rem;
    padding: 24px 0;
    margin: 0;
  }

  .footer-hint {
    margin: 0;
    font-size: 0.65rem;
    color: #555;
    text-align: center;
    border-top: 1px solid #2a2a3e;
    padding-top: 6px;
  }
</style>
