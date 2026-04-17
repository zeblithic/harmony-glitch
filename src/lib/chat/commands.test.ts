// @vitest-environment node
import { describe, it, expect } from 'vitest';
import { parseCommand, executeCommand } from './commands';
import type { CommandContext, CommandRegistry, CommandHandler } from './commands';

describe('parseCommand', () => {
  it('returns null for empty input', () => {
    expect(parseCommand('')).toBeNull();
    expect(parseCommand('   ')).toBeNull();
  });

  it('returns null for plain text', () => {
    expect(parseCommand('hello world')).toBeNull();
    expect(parseCommand('  hi there')).toBeNull();
  });

  it('returns literal escape for //-prefixed input', () => {
    expect(parseCommand('//dance')).toEqual({ kind: 'literal', text: '/dance' });
    expect(parseCommand('//hello world')).toEqual({ kind: 'literal', text: '/hello world' });
  });

  it('parses a bare command with no args', () => {
    expect(parseCommand('/dance')).toEqual({
      kind: 'command',
      cmd: 'dance',
      args: '',
      raw: '/dance',
    });
  });

  it('lowercases the command name', () => {
    const result = parseCommand('/Dance');
    expect(result).toEqual({ kind: 'command', cmd: 'dance', args: '', raw: '/Dance' });
  });

  it('splits command from args on first whitespace', () => {
    expect(parseCommand('/hug alice')).toEqual({
      kind: 'command',
      cmd: 'hug',
      args: 'alice',
      raw: '/hug alice',
    });
  });

  it('preserves intra-arg whitespace', () => {
    const result = parseCommand('/me waves  hello');
    expect(result).toEqual({
      kind: 'command',
      cmd: 'me',
      args: 'waves  hello',
      raw: '/me waves  hello',
    });
  });

  it('strips leading whitespace from args', () => {
    const result = parseCommand('/hug   alice');
    expect(result?.kind === 'command' && result.args).toBe('alice');
  });

  it('produces bare-command shape for just "/"', () => {
    expect(parseCommand('/')).toEqual({ kind: 'command', cmd: '', args: '', raw: '/' });
  });

  it('trims surrounding whitespace once before parsing', () => {
    expect(parseCommand('  /dance  ')).toEqual({
      kind: 'command',
      cmd: 'dance',
      args: '',
      raw: '/dance',
    });
  });
});

function makeContext(overrides: Partial<CommandContext> = {}): CommandContext {
  return {
    remotePlayers: [],
    nearestSocialTarget: null,
    buddies: [],
    localIdentity: { displayName: 'Me', addressHash: 'aa'.repeat(16), setupComplete: true },
    pushLocalBubble: () => {},
    fireEmote: async () => {},
    fireEmoteHi: async () => {},
    sendChat: async () => {},
    blockPlayer: async () => {},
    unblockPlayer: async () => {},
    getBlockedList: async () => [],
    ...overrides,
  };
}

describe('executeCommand', () => {
  it('dispatches to the handler for a known command', async () => {
    let receivedArgs = '';
    const handler: CommandHandler = async (args) => {
      receivedArgs = args;
    };
    const registry: CommandRegistry = new Map([['dance', handler]]);
    await executeCommand(
      { kind: 'command', cmd: 'dance', args: 'extra', raw: '/dance extra' },
      registry,
      makeContext(),
    );
    expect(receivedArgs).toBe('extra');
  });

  it('bubbles an unknown-command message for a missing entry', async () => {
    const bubbles: string[] = [];
    const registry: CommandRegistry = new Map();
    await executeCommand(
      { kind: 'command', cmd: 'xyzzy', args: '', raw: '/xyzzy' },
      registry,
      makeContext({ pushLocalBubble: (t) => bubbles.push(t) }),
    );
    expect(bubbles).toEqual(['Unknown command: /xyzzy. Type /help for the list.']);
  });

  it('bubbles a specific message for the bare "/" case', async () => {
    const bubbles: string[] = [];
    await executeCommand(
      { kind: 'command', cmd: '', args: '', raw: '/' },
      new Map(),
      makeContext({ pushLocalBubble: (t) => bubbles.push(t) }),
    );
    expect(bubbles).toEqual(['Unknown command: /. Type /help for the list.']);
  });

  it('catches handler errors and bubbles "Command failed: <message>"', async () => {
    const bubbles: string[] = [];
    const handler: CommandHandler = async () => {
      throw new Error('boom');
    };
    const registry: CommandRegistry = new Map([['fail', handler]]);
    await executeCommand(
      { kind: 'command', cmd: 'fail', args: '', raw: '/fail' },
      registry,
      makeContext({ pushLocalBubble: (t) => bubbles.push(t) }),
    );
    expect(bubbles).toEqual(['Command failed: boom']);
  });

  it('catches non-Error throws and bubbles the stringified value', async () => {
    const bubbles: string[] = [];
    const handler: CommandHandler = async () => {
      throw 'raw string error';
    };
    const registry: CommandRegistry = new Map([['fail', handler]]);
    await executeCommand(
      { kind: 'command', cmd: 'fail', args: '', raw: '/fail' },
      registry,
      makeContext({ pushLocalBubble: (t) => bubbles.push(t) }),
    );
    expect(bubbles).toEqual(['Command failed: raw string error']);
  });
});
