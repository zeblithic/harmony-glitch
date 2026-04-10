// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import SocialPrompt from './SocialPrompt.svelte';

const defaultProps = {
  visible: true,
  targetName: 'Alice',
  canHi: true,
  canTrade: true,
  canInvite: true,
  canBuddy: true,
  onHi: vi.fn(),
  onTrade: vi.fn(),
  onInvite: vi.fn(),
  onBuddy: vi.fn(),
};

describe('SocialPrompt', () => {
  it('renders action buttons when visible (all 4 enabled → 4 .social-action)', () => {
    render(SocialPrompt, { props: defaultProps });
    const actions = document.querySelectorAll('.social-action');
    expect(actions.length).toBe(4);
  });

  it('hides when not visible', () => {
    render(SocialPrompt, {
      props: { ...defaultProps, visible: false },
    });
    const prompt = document.querySelector('.social-prompt');
    expect(prompt).toBeNull();
  });

  it('filters actions by availability (only canHi → 1 button)', () => {
    render(SocialPrompt, {
      props: {
        ...defaultProps,
        canHi: true,
        canTrade: false,
        canInvite: false,
        canBuddy: false,
      },
    });
    const actions = document.querySelectorAll('.social-action');
    expect(actions.length).toBe(1);
  });

  it('shows target name', () => {
    render(SocialPrompt, { props: defaultProps });
    const nameEl = document.querySelector('.social-prompt-name');
    expect(nameEl?.textContent).toBe('Alice');
  });

  it('has correct aria labels ([aria-label="Hi Alice"])', () => {
    render(SocialPrompt, { props: defaultProps });
    const hiBtn = document.querySelector('[aria-label="Hi Alice"]');
    expect(hiBtn).not.toBeNull();
  });
});
