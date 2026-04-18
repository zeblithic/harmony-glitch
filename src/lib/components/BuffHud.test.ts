// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { describe, it, expect } from 'vitest';
import BuffHud from './BuffHud.svelte';

describe('BuffHud', () => {
  it('renders nothing when there are no active buffs', () => {
    const { container } = render(BuffHud, { props: { buffs: [] } });
    // Container should be empty (or contain only the root hud element with no icons).
    const icons = container.querySelectorAll('.buff-icon');
    expect(icons.length).toBe(0);
  });

  it('renders one buff icon with label and remaining time', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: 300 },
        ],
      },
    });
    expect(screen.getByLabelText(/Rookswort.*5:00 remaining/)).toBeInTheDocument();
  });

  it('formats remaining time as mm:ss when above 60 seconds', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: 125 },
        ],
      },
    });
    expect(screen.getByText('2:05')).toBeInTheDocument();
  });

  it('formats remaining time as Ns when below 60 seconds', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: 45 },
        ],
      },
    });
    expect(screen.getByText('45s')).toBeInTheDocument();
  });

  it('renders multiple buffs in given order', () => {
    const { container } = render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: 300 },
          { kind: 'campfire', icon: 'campfire', label: 'Campfire', remainingSecs: 30 },
        ],
      },
    });
    const icons = container.querySelectorAll('.buff-icon');
    expect(icons.length).toBe(2);
  });

  it('clamps negative remainingSecs to 0s in display', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: -5 },
        ],
      },
    });
    expect(screen.getByText('0s')).toBeInTheDocument();
  });

  it('renders rookswort with the 🌿 glyph', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'rookswort', icon: 'rookswort', label: 'Rookswort', remainingSecs: 300 },
        ],
      },
    });
    expect(screen.getByText('🌿')).toBeInTheDocument();
  });

  it('falls back to ✨ glyph for unknown buff kinds', () => {
    render(BuffHud, {
      props: {
        buffs: [
          { kind: 'some_unknown_kind', icon: 'whatever', label: 'Unknown', remainingSecs: 30 },
        ],
      },
    });
    expect(screen.getByText('✨')).toBeInTheDocument();
  });
});
