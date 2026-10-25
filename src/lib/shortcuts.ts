export type Shortcut = 'search' | 'new-thread' | 'composer' | 'dismiss' | null;
export type ShortcutInput = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey' | 'isComposing'> & { keyCode?: number };

/** Some Windows WebView2 IMEs report isComposing=false for the commit Enter (keyCode 229). */
export function isTextComposing(event: Pick<ShortcutInput, 'key' | 'isComposing' | 'keyCode'>, composing = false): boolean {
  return composing || event.isComposing || event.keyCode === 229 ||
    event.key === 'Process' || event.key === 'Dead';
}

export function shouldSendComposer(event: ShortcutInput, composing = false): boolean {
  return event.key === 'Enter' && !event.shiftKey && !event.ctrlKey && !event.metaKey &&
    !event.altKey && !isTextComposing(event, composing);
}

export function resolveShortcut(event: ShortcutInput, composing = false): Shortcut {
  if (isTextComposing(event, composing)) return null;
  if (event.key === 'Escape' && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey) return 'dismiss';
  if (!event.ctrlKey && !event.metaKey) return null;
  if (event.shiftKey || event.altKey) return null;
  const key = event.key.toLowerCase();
  if (key === 'k') return 'search';
  if (key === 'n') return 'new-thread';
  if (key === 'l') return 'composer';
  return null;
}
