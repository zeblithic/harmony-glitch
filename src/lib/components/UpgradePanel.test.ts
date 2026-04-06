// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import UpgradePanel from './UpgradePanel.svelte';

vi.mock('../ipc', () => ({
  buyUpgrade: vi.fn().mockResolvedValue({
    imagination: 0,
    upgrades: { energyTankTier: 1, hagglingTier: 0 },
    energy: 650,
    maxEnergy: 650,
  }),
}));

// Mock dialog showModal/close (jsdom doesn't support)
HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) {
  this.setAttribute('open', '');
});
HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) {
  this.removeAttribute('open');
});

const defaultProps = {
  visible: true,
  imagination: 500,
  upgrades: { energyTankTier: 1, hagglingTier: 0 },
  maxEnergy: 650,
};

describe('UpgradePanel', () => {
  it('shows both upgrade paths', () => {
    render(UpgradePanel, { props: defaultProps });
    const cards = document.querySelectorAll('.upgrade-card');
    expect(cards.length).toBe(2);
  });

  it('shows correct tier for energy tank', () => {
    render(UpgradePanel, { props: defaultProps });
    const tiers = document.querySelectorAll('.card-tier');
    expect(tiers[0]?.textContent).toContain('Tier 1 / 4');
  });

  it('buy button enabled when sufficient iMG', () => {
    render(UpgradePanel, { props: defaultProps });
    const buttons = document.querySelectorAll('.buy-btn') as NodeListOf<HTMLButtonElement>;
    expect(buttons[0]?.disabled).toBe(false);
  });

  it('buy button disabled when insufficient iMG', () => {
    render(UpgradePanel, { props: { ...defaultProps, imagination: 50 } });
    const buttons = document.querySelectorAll('.buy-btn') as NodeListOf<HTMLButtonElement>;
    expect(buttons[0]?.disabled).toBe(true);
  });

  it('shows MAX when tier is 4', () => {
    render(UpgradePanel, {
      props: { ...defaultProps, upgrades: { energyTankTier: 4, hagglingTier: 0 }, maxEnergy: 950 },
    });
    const maxBadge = document.querySelector('.max-badge');
    expect(maxBadge?.textContent).toContain('MAX');
  });

  it('calls buyUpgrade IPC on click', async () => {
    const { buyUpgrade: mockBuy } = await import('../ipc');
    render(UpgradePanel, { props: defaultProps });
    const buttons = document.querySelectorAll('.buy-btn') as NodeListOf<HTMLButtonElement>;
    await fireEvent.click(buttons[0]);
    expect(mockBuy).toHaveBeenCalledWith('energy_tank');
  });

  it('has dialog with aria-label', () => {
    render(UpgradePanel, { props: defaultProps });
    const dialog = document.querySelector('dialog');
    expect(dialog?.getAttribute('aria-label')).toBe('Imagination Upgrades');
  });
});
