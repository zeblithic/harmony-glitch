// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import ImaginationHud from './ImaginationHud.svelte';

describe('ImaginationHud', () => {
  it('renders iMG amount', () => {
    render(ImaginationHud, { props: { imagination: 156 } });
    const amount = document.querySelector('.img-amount');
    expect(amount?.textContent).toBe('156 iMG');
  });

  it('has accessible role="status"', () => {
    render(ImaginationHud, { props: { imagination: 42 } });
    const hud = document.querySelector('[role="status"]');
    expect(hud).toBeDefined();
    expect(hud?.getAttribute('aria-label')).toContain('42');
  });

  it('calls onOpen when clicked', async () => {
    const onOpen = vi.fn();
    render(ImaginationHud, { props: { imagination: 100, onOpen } });
    const btn = document.querySelector('.imagination-hud') as HTMLElement;
    await fireEvent.click(btn);
    expect(onOpen).toHaveBeenCalledOnce();
  });
});
