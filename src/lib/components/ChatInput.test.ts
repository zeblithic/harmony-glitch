// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import ChatInput from './ChatInput.svelte';

// Mock the IPC module — only sendChat is imported by ChatInput.
vi.mock('../ipc', () => ({
  sendChat: vi.fn(async () => {}),
}));

import { sendChat } from '../ipc';

describe('ChatInput', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function focusAndType(container: HTMLElement, text: string) {
    // Component is hidden until focused; focus opens the input.
    await fireEvent.keyDown(window, { key: 'Enter' });
    const input = container.querySelector('input')!;
    expect(input).toBeTruthy();
    await fireEvent.input(input, { target: { value: text } });
    return input;
  }

  it('plain text submits via sendChat, not onCommand', async () => {
    const onCommand = vi.fn();
    const { container } = render(ChatInput, {
      props: { onFocusChange: () => {}, onCommand },
    });
    const input = await focusAndType(container, 'hello world');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(sendChat).toHaveBeenCalledWith('hello world');
    expect(onCommand).not.toHaveBeenCalled();
  });

  it('slash command routes to onCommand, not sendChat', async () => {
    const onCommand = vi.fn(async () => {});
    const { container } = render(ChatInput, {
      props: { onFocusChange: () => {}, onCommand },
    });
    const input = await focusAndType(container, '/dance');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(onCommand).toHaveBeenCalledWith({
      kind: 'command',
      cmd: 'dance',
      args: '',
      raw: '/dance',
    });
    expect(sendChat).not.toHaveBeenCalled();
  });

  it('literal //text sends the stripped form via sendChat', async () => {
    const onCommand = vi.fn();
    const { container } = render(ChatInput, {
      props: { onFocusChange: () => {}, onCommand },
    });
    const input = await focusAndType(container, '//dance');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(sendChat).toHaveBeenCalledWith('/dance');
    expect(onCommand).not.toHaveBeenCalled();
  });

  it('clears the input after submit', async () => {
    const { container } = render(ChatInput, {
      props: { onFocusChange: () => {}, onCommand: async () => {} },
    });
    const input = await focusAndType(container, '/dance');
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect((input as HTMLInputElement).value).toBe('');
  });
});
