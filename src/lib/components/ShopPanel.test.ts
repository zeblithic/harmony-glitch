// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import ShopPanel from './ShopPanel.svelte';
import type { StoreState } from '../types';

// Mock dialog showModal/close (jsdom doesn't support)
HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) {
  this.setAttribute('open', '');
});
HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) {
  this.removeAttribute('open');
});

const mockStoreState: StoreState = {
  entityId: 'vendor_1',
  name: 'Grocery Vendor',
  vendorInventory: [
    { itemId: 'cherry', name: 'Cherry', baseCost: 3, stackLimit: 50 },
    { itemId: 'grain', name: 'Grain', baseCost: 3, stackLimit: 50 },
  ],
  playerInventory: [
    { itemId: 'cherry', name: 'Cherry', count: 12, sellPrice: 2 },
    { itemId: 'wood', name: 'Wood', count: 8, sellPrice: 2 },
  ],
  currants: 50,
};

describe('ShopPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders nothing when not visible', () => {
    render(ShopPanel, {
      props: { storeState: mockStoreState, visible: false },
    });
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('renders store name and currant balance', () => {
    render(ShopPanel, {
      props: { storeState: mockStoreState, visible: true },
    });
    expect(screen.getByText('Grocery Vendor')).toBeDefined();
    expect(screen.getByText(/50/)).toBeDefined();
  });

  it('renders buy tab with vendor inventory', () => {
    render(ShopPanel, {
      props: { storeState: mockStoreState, visible: true },
    });
    // Buy tab is active by default
    expect(screen.getByText('Cherry')).toBeDefined();
    expect(screen.getByText('Grain')).toBeDefined();
  });

  it('shows sell tab with player inventory', async () => {
    render(ShopPanel, {
      props: { storeState: mockStoreState, visible: true },
    });
    const sellTab = screen.getByRole('tab', { name: 'Sell' });
    await fireEvent.click(sellTab);

    // Wood ×8 should be visible
    expect(screen.getByText('Wood')).toBeDefined();
    expect(screen.getByText('×8')).toBeDefined();
  });

  it('shows empty state on sell tab with no items', async () => {
    const emptyState: StoreState = {
      ...mockStoreState,
      playerInventory: [],
    };
    render(ShopPanel, {
      props: { storeState: emptyState, visible: true },
    });
    const sellTab = screen.getByRole('tab', { name: 'Sell' });
    await fireEvent.click(sellTab);

    expect(screen.getByText('No items to sell')).toBeDefined();
  });

  it('calls onBuy when buy button clicked', async () => {
    const onBuy = vi.fn();
    render(ShopPanel, {
      props: { storeState: mockStoreState, visible: true, onBuy },
    });
    const buyBtn = screen.getByRole('button', { name: 'Buy Cherry' });
    await fireEvent.click(buyBtn);
    expect(onBuy).toHaveBeenCalledWith('cherry', 1);
  });

  it('calls onSell when sell button clicked', async () => {
    const onSell = vi.fn();
    render(ShopPanel, {
      props: { storeState: mockStoreState, visible: true, onSell },
    });
    const sellTab = screen.getByRole('tab', { name: 'Sell' });
    await fireEvent.click(sellTab);

    const sellBtn = screen.getByRole('button', { name: 'Sell Wood' });
    await fireEvent.click(sellBtn);
    expect(onSell).toHaveBeenCalledWith('wood', 1);
  });

  it('has accessible dialog label', () => {
    render(ShopPanel, {
      props: { storeState: mockStoreState, visible: true },
    });
    const dialog = screen.getByRole('dialog');
    expect(dialog.getAttribute('aria-label')).toBe('Shop: Grocery Vendor');
  });
});
