import { describe, expect, it } from 'vitest';
import { isTextComposing, resolveShortcut, shouldSendComposer, type ShortcutInput } from './shortcuts';

function input(key: string, extra: Partial<ShortcutInput> = {}): ShortcutInput {
  return { key, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, isComposing: false, ...extra };
}

describe('keyboard shortcuts and Windows IME safety', () => {
  it('supports Windows/Linux and macOS modifiers', () => {
    expect(resolveShortcut(input('k', { ctrlKey: true }))).toBe('search');
    expect(resolveShortcut(input('N', { metaKey: true }))).toBe('new-thread');
    expect(resolveShortcut(input('l', { ctrlKey: true }))).toBe('composer');
    expect(resolveShortcut(input('Escape', { altKey: true }))).toBeNull();
    expect(resolveShortcut(input('Escape'))).toBe('dismiss');
  });
  it('does not hijack ordinary text or IME composition', () => {
    expect(resolveShortcut(input('k'))).toBeNull();
    expect(resolveShortcut(input('k', { ctrlKey: true, isComposing: true }))).toBeNull();
    expect(resolveShortcut(input('n', { ctrlKey: true, shiftKey: true }))).toBeNull();
    expect(resolveShortcut(input('k', { ctrlKey: true, keyCode: 229 }))).toBeNull();
    expect(resolveShortcut(input('k', { ctrlKey: true }), true)).toBeNull();
    expect(isTextComposing(input('Dead'))).toBe(true);
    expect(isTextComposing(input('Process'))).toBe(true);
  });
  it('sends on unmodified Enter only, never on composition commit or newline', () => {
    expect(shouldSendComposer(input('Enter'))).toBe(true);
    expect(shouldSendComposer(input('Enter', { shiftKey: true }))).toBe(false);
    expect(shouldSendComposer(input('Enter', { ctrlKey: true }))).toBe(false);
    expect(shouldSendComposer(input('Enter', { metaKey: true }))).toBe(false);
    expect(shouldSendComposer(input('Enter', { altKey: true }))).toBe(false);
    expect(shouldSendComposer(input('Enter', { isComposing: true }))).toBe(false);
    expect(shouldSendComposer(input('Enter', { keyCode: 229 }))).toBe(false);
    expect(shouldSendComposer(input('Enter'), true)).toBe(false);
  });
});
