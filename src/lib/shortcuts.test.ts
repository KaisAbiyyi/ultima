import { describe, expect, it } from 'vitest';
import { resolveShortcut, type ShortcutInput } from './shortcuts';

function input(key: string, extra: Partial<ShortcutInput> = {}): ShortcutInput {
  return { key, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, isComposing: false, ...extra };
}

describe('keyboard shortcuts', () => {
  it('supports Windows/Linux and macOS modifiers', () => {
    expect(resolveShortcut(input('k', { ctrlKey: true }))).toBe('search');
    expect(resolveShortcut(input('N', { metaKey: true }))).toBe('new-thread');
    expect(resolveShortcut(input('l', { ctrlKey: true }))).toBe('composer');
  });
  it('does not hijack ordinary text or IME composition', () => {
    expect(resolveShortcut(input('k'))).toBeNull();
    expect(resolveShortcut(input('k', { ctrlKey: true, isComposing: true }))).toBeNull();
    expect(resolveShortcut(input('n', { ctrlKey: true, shiftKey: true }))).toBeNull();
  });
});
