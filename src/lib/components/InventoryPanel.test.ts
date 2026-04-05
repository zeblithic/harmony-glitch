// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import InventoryPanel from './InventoryPanel.svelte';
import type { InventoryFrame, RecipeDef } from '../types';

// Mock ipc module
vi.mock('../ipc', () => ({
  dropItem: vi.fn().mockResolvedValue(undefined),
  craftRecipe: vi.fn().mockResolvedValue(undefined),
}));

// Mock dialog showModal/close (jsdom doesn't support)
HTMLDialogElement.prototype.showModal = vi.fn(function(this: HTMLDialogElement) {
  this.setAttribute('open', '');
});
HTMLDialogElement.prototype.close = vi.fn(function(this: HTMLDialogElement) {
  this.removeAttribute('open');
});

function makeInventory(items: { itemId: string; name: string; count: number }[]): InventoryFrame {
  const slots: (null | { itemId: string; name: string; description: string; icon: string; count: number; stackLimit: number; energyValue: number | null })[] =
    items.map(i => ({
      itemId: i.itemId,
      name: i.name,
      description: '',
      icon: i.itemId,
      count: i.count,
      stackLimit: 50,
      energyValue: null,
    }));
  while (slots.length < 16) slots.push(null);
  return { slots, capacity: 16 };
}

function makeRecipes(): RecipeDef[] {
  return [
    {
      id: 'bread',
      name: 'Bread',
      description: 'Simple bread.',
      inputs: [{ item: 'grain', count: 4 }],
      tools: [{ item: 'pot', count: 1 }],
      outputs: [{ item: 'bread', count: 1 }],
      durationSecs: 8.0,
      category: 'food',
    },
    {
      id: 'plank',
      name: 'Plank',
      description: 'Wood plank.',
      inputs: [{ item: 'wood', count: 3 }],
      tools: [],
      outputs: [{ item: 'plank', count: 2 }],
      durationSecs: 4.0,
      category: 'material',
    },
  ];
}

describe('InventoryPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders tab bar with Items and Recipes tabs', () => {
    const inv = makeInventory([]);
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const tablist = screen.getByRole('tablist');
    expect(tablist).toBeDefined();

    const tabs = screen.getAllByRole('tab');
    expect(tabs).toHaveLength(2);
    expect(tabs[0].textContent).toBe('Items');
    expect(tabs[1].textContent).toBe('Recipes');
  });

  it('shows recipes tab when clicked', async () => {
    const inv = makeInventory([]);
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const recipesTab = screen.getByRole('tab', { name: /recipes/i });
    await fireEvent.click(recipesTab);

    const panel = screen.getByRole('tabpanel');
    expect(panel.id).toBe('panel-recipes');
  });

  it('sorts craftable recipes before uncraftable', async () => {
    // Has wood for plank but not grain+pot for bread
    const inv = makeInventory([{ itemId: 'wood', name: 'Wood', count: 5 }]);
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const recipesTab = screen.getByRole('tab', { name: /recipes/i });
    await fireEvent.click(recipesTab);

    const options = screen.getAllByRole('option');
    // Plank (craftable) should come before Bread (not craftable)
    expect(options[0].textContent).toContain('Plank');
    expect(options[1].textContent).toContain('Bread');
  });

  it('disables craft button when missing ingredients', async () => {
    const inv = makeInventory([]); // empty inventory
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const recipesTab = screen.getByRole('tab', { name: /recipes/i });
    await fireEvent.click(recipesTab);

    // Select first recipe
    const options = screen.getAllByRole('option');
    await fireEvent.click(options[0]);

    const craftBtn = screen.getByRole('button', { name: /craft/i });
    expect((craftBtn as HTMLButtonElement).disabled).toBe(true);
  });

  it('has correct ARIA tab structure', () => {
    const inv = makeInventory([]);
    render(InventoryPanel, {
      props: { inventory: inv, recipes: makeRecipes(), visible: true },
    });

    const tabs = screen.getAllByRole('tab');
    // Active tab has aria-selected=true
    expect(tabs[0].getAttribute('aria-selected')).toBe('true');
    expect(tabs[1].getAttribute('aria-selected')).toBe('false');

    // Active tab controls the visible panel
    expect(tabs[0].getAttribute('aria-controls')).toBe('panel-items');
    expect(tabs[1].getAttribute('aria-controls')).toBe('panel-recipes');

    // Tab panel exists and is labeled
    const panel = screen.getByRole('tabpanel');
    expect(panel.getAttribute('aria-labelledby')).toBe('tab-items');
  });
});
