// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import GameNotification from './GameNotification.svelte';
import type { PickupFeedback } from '../types';

function makeFeedback(overrides?: Partial<PickupFeedback>): PickupFeedback {
  return {
    id: 1,
    text: 'Inventory full!',
    success: false,
    x: 100,
    y: 200,
    ageSecs: 0.1,
    ...overrides,
  };
}

describe('GameNotification', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders nothing when feedback is empty', () => {
    render(GameNotification, { props: { feedback: [] } });
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('renders nothing when all feedback is successful', () => {
    const fb = makeFeedback({ success: true, text: '+3 x Cherry' });
    render(GameNotification, { props: { feedback: [fb] } });
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('renders alert container for failure feedback', () => {
    const fb = makeFeedback();
    render(GameNotification, { props: { feedback: [fb] } });
    const alert = screen.getByRole('alert');
    expect(alert).toBeDefined();
    expect(alert.textContent).toContain('Inventory full!');
  });

  it('renders multiple failure messages', () => {
    const fb1 = makeFeedback({ id: 1, text: 'Inventory full!' });
    const fb2 = makeFeedback({ id: 2, text: 'Cannot pick up' });
    render(GameNotification, { props: { feedback: [fb1, fb2] } });

    const alert = screen.getByRole('alert');
    expect(alert.textContent).toContain('Inventory full!');
    expect(alert.textContent).toContain('Cannot pick up');
  });

  it('filters out successful feedback from display', () => {
    const success = makeFeedback({ id: 1, success: true, text: '+1 x Wood' });
    const failure = makeFeedback({ id: 2, success: false, text: 'Inventory full!' });
    render(GameNotification, { props: { feedback: [success, failure] } });

    const alert = screen.getByRole('alert');
    expect(alert.textContent).not.toContain('+1 x Wood');
    expect(alert.textContent).toContain('Inventory full!');
  });

  it('applies opacity based on ageSecs', () => {
    const fb = makeFeedback({ ageSecs: 0.75 });
    render(GameNotification, { props: { feedback: [fb] } });

    const alert = screen.getByRole('alert');
    const notification = alert.querySelector('.notification') as HTMLElement;
    expect(notification).toBeDefined();
    // opacity = max(0, 1 - 0.75/1.5) = 0.5
    expect(notification.style.opacity).toBe('0.5');
  });
});
