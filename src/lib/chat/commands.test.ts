// @vitest-environment node
import { describe, it, expect } from 'vitest';
import { parseCommand } from './commands';

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
