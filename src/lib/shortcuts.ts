export type Shortcut = 'search' | 'new-thread' | 'composer' | 'dismiss' | null;
export type ShortcutInput = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey' | 'isComposing'>;

export function resolveShortcut(event: ShortcutInput): Shortcut {
  if (event.isComposing) return null;
  if (event.key === 'Escape') return 'dismiss';
  if (!event.ctrlKey && !event.metaKey) return null;
  if (event.shiftKey || event.altKey) return null;
  const key = event.key.toLowerCase();
  if (key === 'k') return 'search';
  if (key === 'n') return 'new-thread';
  if (key === 'l') return 'composer';
  return null;
}
