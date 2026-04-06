// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import UpgradePanel from './UpgradePanel.svelte';

const { mockGetUpgradeDefs, mockBuyUpgrade } = vi.hoisted(() => {
  const mockUpgradeDefs = [
    {
      id: 'energy_tank',
      name: 'Energy Tank',
      description: 'Increase your maximum energy capacity.',
      tiers: [
        { cost: 100, effectValue: 50 },
        { cost: 200, effectValue: 75 },
        { cost: 400, effectValue: 100 },
        { cost: 800, effectValue: 125 },
      ],
    },
    {
      id: 'haggling',
      name: 'Vendor Haggling',
      description: 'Negotiate better prices at vendor shops.',
      tiers: [
        { cost: 100, effectValue: 0.05 },
        { cost: 200, effectValue: 0.10 },
        { cost: 400, effectValue: 0.15 },
        { cost: 800, effectValue: 0.20 },
      ],
    },
  ];
  return {
    mockGetUpgradeDefs: vi.fn().mockResolvedValue(mockUpgradeDefs),
    mockBuyUpgrade: vi.fn().mockResolvedValue({
      imagination: 0,
      upgrades: { energyTankTier: 1, hagglingTier: 0 },
      energy: 650,
      maxEnergy: 650,
    }),
  };
});

vi.mock('../ipc', () => ({
  getUpgradeDefs: mockGetUpgradeDefs,
  buyUpgrade: mockBuyUpgrade,
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
  it('shows both upgrade paths after loading defs', async () => {
    render(UpgradePanel, { props: defaultProps });
    await waitFor(() => {
      const cards = document.querySelectorAll('.upgrade-card');
      expect(cards.length).toBe(2);
    });
  });

  it('shows correct tier for energy tank', async () => {
    render(UpgradePanel, { props: defaultProps });
    await waitFor(() => {
      const tiers = document.querySelectorAll('.card-tier');
      expect(tiers[0]?.textContent).toContain('Tier 1 / 4');
    });
  });

  it('buy button enabled when sufficient iMG', async () => {
    render(UpgradePanel, { props: defaultProps });
    await waitFor(() => {
      const buttons = document.querySelectorAll('.buy-btn') as NodeListOf<HTMLButtonElement>;
      expect(buttons.length).toBeGreaterThan(0);
      expect(buttons[0]?.disabled).toBe(false);
    });
  });

  it('buy button disabled when insufficient iMG', async () => {
    render(UpgradePanel, { props: { ...defaultProps, imagination: 50 } });
    await waitFor(() => {
      const buttons = document.querySelectorAll('.buy-btn') as NodeListOf<HTMLButtonElement>;
      expect(buttons.length).toBeGreaterThan(0);
      expect(buttons[0]?.disabled).toBe(true);
    });
  });

  it('shows MAX when tier is 4', async () => {
    render(UpgradePanel, {
      props: { ...defaultProps, upgrades: { energyTankTier: 4, hagglingTier: 0 }, maxEnergy: 950 },
    });
    await waitFor(() => {
      const maxBadge = document.querySelector('.max-badge');
      expect(maxBadge?.textContent).toContain('MAX');
    });
  });

  it('calls buyUpgrade IPC on click', async () => {
    render(UpgradePanel, { props: defaultProps });
    await waitFor(() => {
      expect(document.querySelectorAll('.buy-btn').length).toBeGreaterThan(0);
    });
    const buttons = document.querySelectorAll('.buy-btn') as NodeListOf<HTMLButtonElement>;
    await fireEvent.click(buttons[0]);
    expect(mockBuyUpgrade).toHaveBeenCalledWith('energy_tank');
  });

  it('has dialog with aria-label', () => {
    render(UpgradePanel, { props: defaultProps });
    const dialog = document.querySelector('dialog');
    expect(dialog?.getAttribute('aria-label')).toBe('Imagination Upgrades');
  });

  it('fetches upgrade defs from IPC on open', async () => {
    render(UpgradePanel, { props: defaultProps });
    await waitFor(() => {
      expect(mockGetUpgradeDefs).toHaveBeenCalled();
    });
  });
});
