<script lang="ts">
  import type { PlayerUpgrades, UpgradePathDef } from '../types';
  import { buyUpgrade, getUpgradeDefs } from '../ipc';

  let {
    visible = false,
    imagination,
    upgrades,
    maxEnergy,
    onClose,
  }: {
    visible?: boolean;
    imagination: number;
    upgrades: PlayerUpgrades;
    maxEnergy: number;
    onClose?: () => void;
  } = $props();

  let dialogEl: HTMLDialogElement | undefined = $state();
  let isPurchasing = $state(false);
  let purchaseError = $state<string | null>(null);
  let upgradeDefs = $state<UpgradePathDef[]>([]);

  let energyTankDef = $derived(upgradeDefs.find(d => d.id === 'energy_tank'));
  let hagglingDef = $derived(upgradeDefs.find(d => d.id === 'haggling'));
  let energyTankMaxTiers = $derived(energyTankDef?.tiers.length ?? 4);
  let hagglingMaxTiers = $derived(hagglingDef?.tiers.length ?? 4);

  let energyTankMaxed = $derived(upgrades.energyTankTier >= energyTankMaxTiers);
  let hagglingMaxed = $derived(upgrades.hagglingTier >= hagglingMaxTiers);
  let nextEnergyTier = $derived(
    energyTankMaxed || !energyTankDef ? null : energyTankDef.tiers[upgrades.energyTankTier],
  );
  let nextHagglingTier = $derived(
    hagglingMaxed || !hagglingDef ? null : hagglingDef.tiers[upgrades.hagglingTier],
  );
  let currentDiscount = $derived(
    upgrades.hagglingTier > 0 && hagglingDef
      ? Math.round(hagglingDef.tiers[upgrades.hagglingTier - 1].effectValue * 100)
      : 0,
  );
  let nextEnergyTotal = $derived(nextEnergyTier ? maxEnergy + nextEnergyTier.effectValue : null);

  $effect(() => {
    if (visible && upgradeDefs.length === 0) {
      getUpgradeDefs()
        .then(defs => { upgradeDefs = defs; })
        .catch(e => { purchaseError = `Failed to load upgrades: ${e}`; });
    }
  });

  $effect(() => {
    if (visible && dialogEl && !dialogEl.open) {
      const prev = document.activeElement as HTMLElement | null;
      dialogEl.showModal();
      return () => {
        prev?.focus();
      };
    }
  });

  function handleClose() {
    purchaseError = null;
    onClose?.();
  }

  function handleCancel(e: Event) {
    e.preventDefault();
    handleClose();
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === dialogEl) {
      handleClose();
    }
  }

  async function handleBuy(upgradeId: string) {
    if (isPurchasing) return;
    isPurchasing = true;
    purchaseError = null;
    try {
      const result = await buyUpgrade(upgradeId);
      // Apply immediately so button state reflects the deduction
      // while awaiting the next render frame.
      imagination = result.imagination;
      upgrades = result.upgrades;
      maxEnergy = result.maxEnergy;
    } catch (e) {
      purchaseError = String(e);
    } finally {
      isPurchasing = false;
    }
  }

  function renderTierDots(tier: number, max: number = 4): string {
    return Array.from({ length: max }, (_, i) => (i < tier ? '●' : '○')).join('');
  }
</script>

{#if visible}
  <dialog
    class="upgrade-panel"
    aria-label="Imagination Upgrades"
    aria-modal="true"
    bind:this={dialogEl}
    oncancel={handleCancel}
    onclick={handleBackdropClick}
  >
    <div class="panel-inner">
      <div class="panel-header">
        <span class="panel-title">✦ Imagination</span>
        <span class="panel-balance">{imagination} iMG</span>
        <button type="button" class="close-btn" onclick={handleClose} aria-label="Close upgrades">✕</button>
      </div>

      <div class="panel-body">
        <!-- Energy Tank card -->
        <div class="upgrade-card">
          <div class="card-name">Energy Tank</div>
          <div class="card-tier">Tier {upgrades.energyTankTier} / {energyTankMaxTiers}</div>
          <div class="card-dots" aria-hidden="true">{renderTierDots(upgrades.energyTankTier, energyTankMaxTiers)}</div>
          <div class="card-effect">Max Energy: {maxEnergy}</div>
          {#if energyTankMaxed}
            <div class="max-badge">MAX</div>
          {:else if nextEnergyTier}
            <div class="card-next">Next: +{nextEnergyTier.effectValue} energy ({nextEnergyTotal} total)</div>
            <button
              type="button"
              class="buy-btn"
              disabled={imagination < nextEnergyTier.cost || isPurchasing}
              onclick={() => handleBuy('energy_tank')}
            >
              Buy — {nextEnergyTier.cost} iMG
            </button>
          {/if}
        </div>

        <!-- Vendor Haggling card -->
        <div class="upgrade-card">
          <div class="card-name">Vendor Haggling</div>
          <div class="card-tier">Tier {upgrades.hagglingTier} / {hagglingMaxTiers}</div>
          <div class="card-dots" aria-hidden="true">{renderTierDots(upgrades.hagglingTier, hagglingMaxTiers)}</div>
          <div class="card-effect">
            {#if currentDiscount > 0}
              Current discount: {currentDiscount}%
            {:else}
              No discount yet
            {/if}
          </div>
          {#if hagglingMaxed}
            <div class="max-badge">MAX</div>
          {:else if nextHagglingTier}
            <div class="card-next">Next: {Math.round(nextHagglingTier.effectValue * 100)}% vendor discount</div>
            <button
              type="button"
              class="buy-btn"
              disabled={imagination < nextHagglingTier.cost || isPurchasing}
              onclick={() => handleBuy('haggling')}
            >
              Buy — {nextHagglingTier.cost} iMG
            </button>
          {/if}
        </div>
      </div>

      {#if purchaseError}
        <div class="purchase-error" role="alert">{purchaseError}</div>
      {/if}
    </div>
  </dialog>
{/if}

<style>
  .upgrade-panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    left: 0;
    width: 100%;
    height: 100%;
    max-width: 100%;
    max-height: 100%;
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    z-index: 100;
  }

  .upgrade-panel::backdrop {
    background: rgba(0, 0, 0, 0.3);
  }

  .panel-inner {
    position: absolute;
    top: 80px;
    right: 12px;
    width: 320px;
    background: rgba(26, 26, 46, 0.95);
    border: 1px solid #4a3a6a;
    border-radius: 12px;
    padding: 16px;
    color: #e0e0e0;
  }

  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 16px;
  }

  .panel-title {
    font-size: 14px;
    font-weight: bold;
    color: #c084fc;
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 1px;
  }

  .panel-balance {
    color: #c084fc;
    font-weight: bold;
    font-size: 14px;
    margin-left: auto;
    margin-right: 8px;
  }

  .close-btn {
    background: none;
    border: none;
    color: #888;
    cursor: pointer;
    font-size: 14px;
    padding: 2px 6px;
    border-radius: 4px;
  }

  .close-btn:hover {
    color: #e0e0e0;
    background: rgba(255, 255, 255, 0.1);
  }

  .close-btn:focus-visible {
    outline: 2px solid #c084fc;
    outline-offset: 2px;
  }

  .panel-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .upgrade-card {
    background: rgba(40, 30, 60, 0.8);
    border: 1px solid #3a2a5a;
    border-radius: 8px;
    padding: 12px;
  }

  .card-name {
    font-weight: bold;
    font-size: 13px;
    color: #c084fc;
    margin-bottom: 4px;
  }

  .card-tier {
    font-size: 11px;
    color: #888;
    margin-bottom: 4px;
  }

  .card-dots {
    font-size: 12px;
    color: #c084fc;
    margin-bottom: 6px;
    letter-spacing: 2px;
  }

  .card-effect {
    font-size: 12px;
    color: #bbb;
    margin-bottom: 8px;
  }

  .card-next {
    font-size: 11px;
    color: #9ca3af;
    margin-bottom: 8px;
    font-style: italic;
  }

  .buy-btn {
    width: 100%;
    padding: 6px 12px;
    background: rgba(192, 132, 252, 0.15);
    color: #c084fc;
    border: 1px solid #7c3aed;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12px;
    font-weight: bold;
    transition: background 0.2s;
    font-family: inherit;
  }

  .buy-btn:hover:not(:disabled) {
    background: rgba(192, 132, 252, 0.3);
  }

  .buy-btn:focus-visible {
    outline: 2px solid #c084fc;
    outline-offset: 2px;
  }

  .buy-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .max-badge {
    display: inline-block;
    padding: 3px 10px;
    background: rgba(192, 132, 252, 0.2);
    color: #c084fc;
    border: 1px solid #7c3aed;
    border-radius: 12px;
    font-size: 11px;
    font-weight: bold;
    letter-spacing: 1px;
  }

  .purchase-error {
    margin-top: 12px;
    padding: 8px;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid #ef4444;
    border-radius: 6px;
    font-size: 12px;
    color: #fca5a5;
  }
</style>
