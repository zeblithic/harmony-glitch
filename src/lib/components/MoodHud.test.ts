// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import MoodHud from './MoodHud.svelte';

describe('MoodHud', () => {
  it('renders bar with correct fill percentage', () => {
    render(MoodHud, { props: { mood: 50, maxMood: 100 } });
    const fill = document.querySelector('.mood-fill') as HTMLElement;
    expect(fill.style.width).toBe('50%');
  });

  it('renders numeric mood value floored', () => {
    render(MoodHud, { props: { mood: 72.8, maxMood: 100 } });
    const amount = document.querySelector('.mood-amount') as HTMLElement;
    expect(amount.textContent).toBe('72');
  });

  it('applies low class when mood below 50%', () => {
    render(MoodHud, { props: { mood: 40, maxMood: 100 } });
    const hud = document.querySelector('.mood-hud') as HTMLElement;
    expect(hud.classList.contains('low')).toBe(true);
  });

  it('does not apply low class when mood at 50%', () => {
    render(MoodHud, { props: { mood: 50, maxMood: 100 } });
    const hud = document.querySelector('.mood-hud') as HTMLElement;
    expect(hud.classList.contains('low')).toBe(false);
  });

  it('caps fill at 100%', () => {
    render(MoodHud, { props: { mood: 120, maxMood: 100 } });
    const fill = document.querySelector('.mood-fill') as HTMLElement;
    expect(fill.style.width).toBe('100%');
  });

  it('handles zero maxMood gracefully', () => {
    render(MoodHud, { props: { mood: 0, maxMood: 0 } });
    const fill = document.querySelector('.mood-fill') as HTMLElement;
    expect(fill.style.width).toBe('0%');
  });

  it('has correct aria label', () => {
    render(MoodHud, { props: { mood: 72, maxMood: 100 } });
    const hud = document.querySelector('.mood-hud') as HTMLElement;
    expect(hud.getAttribute('aria-label')).toBe('Mood: 72 of 100');
  });
});
