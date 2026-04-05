// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import EnergyHud from './EnergyHud.svelte';

describe('EnergyHud', () => {
  it('renders bar with correct fill percentage', () => {
    render(EnergyHud, { props: { energy: 300, maxEnergy: 600 } });
    const fill = document.querySelector('.energy-fill') as HTMLElement;
    expect(fill).toBeDefined();
    expect(fill.style.width).toBe('50%');
  });

  it('shows numeric energy value', () => {
    render(EnergyHud, { props: { energy: 432, maxEnergy: 600 } });
    const amount = document.querySelector('.energy-amount');
    expect(amount?.textContent).toBe('432');
  });

  it('has low-energy class when below 150', () => {
    render(EnergyHud, { props: { energy: 100, maxEnergy: 600 } });
    const hud = document.querySelector('.energy-hud');
    expect(hud?.classList.contains('low')).toBe(true);
  });

  it('does not have low-energy class when above 150', () => {
    render(EnergyHud, { props: { energy: 300, maxEnergy: 600 } });
    const hud = document.querySelector('.energy-hud');
    expect(hud?.classList.contains('low')).toBe(false);
  });

  it('has accessible role="status"', () => {
    render(EnergyHud, { props: { energy: 432, maxEnergy: 600 } });
    const hud = document.querySelector('[role="status"]');
    expect(hud).toBeDefined();
    expect(hud?.getAttribute('aria-label')).toContain('432');
  });
});
