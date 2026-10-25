import type { UIMessage } from './history';
const PROJECT_KEY = 'ultima-projects-v1';
const THREAD_KEY = 'ultima-threads-v1';
export const MAX_PROJECTS = 12;
export const MAX_THREADS = 50;
export const MAX_RENDER_HISTORY = 5000;
export const MAX_TEXT_BYTES = 2 * 1024 * 1024;
// UI-owned snapshots only; full durable transcripts remain in Codex.
export const MAX_RETAINED_TEXT_CHARS = 8 * 1024 * 1024;
export const MAX_HISTORY_ITEM_CHARS = 64 * 1024;
export interface Project { path: string; name: string }
export interface Thread { id: string; project: string; title: string; model: string }
export function projectFromPath(raw: string): Project | null {
  const path = raw.trim();
  // Native host enforces canonicalization and trusted roots. This is input validation only.
  if (!path || path.length > 4096 || !/^(?:[A-Za-z]:[\\/]|\/)/.test(path)) return null;
  const name = path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
  return { path, name };
}
export function appendProject(projects: Project[], raw: string): Project[] {
  const next = projectFromPath(raw);
  if (!next) return projects;
  if (projects.some((p) => p.path.toLowerCase() === next.path.toLowerCase())) return projects;
  return [next, ...projects].slice(0, MAX_PROJECTS);
}
export function addThread(threads: Thread[], thread: Thread): Thread[] {
  if (!thread.id || thread.id.length > 256) return threads;
  return [thread, ...threads.filter((item) => item.id !== thread.id)].slice(0, MAX_THREADS);
}
export function cappedMessages(messages: UIMessage[]): UIMessage[] {
  // Work backwards to retain only the newest messages fitting BOTH budgets.
  // Never build a transient many-gigabyte UI array from restored history.
  let begin = messages.length;
  let chars = 0;
  for (let i = messages.length - 1; i >= 0 && messages.length - i <= MAX_RENDER_HISTORY; i--) {
    const size = messages[i].text.length;
    if (size > MAX_RETAINED_TEXT_CHARS - chars) break;
    chars += size;
    begin = i;
  }
  return begin === 0 ? messages : messages.slice(begin);
}
export function appendDelta(current: string, delta: string): string {
  if (current.length + delta.length > MAX_TEXT_BYTES) throw Error('Stream exceeded safe UI text limit');
  return current + delta;
}
export function restoreProjects(read: (key: string) => string | null): Project[] {
  try {
    const raw: unknown = JSON.parse(read(PROJECT_KEY) || '[]');
    if (!Array.isArray(raw)) return [];
    return raw.filter((p): p is Project => p && typeof p.path === 'string' && projectFromPath(p.path)?.name === p.name).slice(0, MAX_PROJECTS);
  } catch { return []; }
}
export function restoreThreads(read: (key: string) => string | null): Thread[] {
  try {
    const raw: unknown = JSON.parse(read(THREAD_KEY) || '[]');
    if (!Array.isArray(raw)) return [];
    return raw.filter((t): t is Thread => t && typeof t.id === 'string' && t.id.length <= 256 &&
      typeof t.project === 'string' && t.project.length <= 4096 && typeof t.title === 'string' &&
      t.title.length <= 96 && typeof t.model === 'string' && t.model.length <= 128).slice(0, MAX_THREADS);
  } catch { return []; }
}
export function saveMetadata(write: (key: string, value: string) => void, projects: Project[], threads: Thread[]) {
  write(PROJECT_KEY, JSON.stringify(projects.slice(0, MAX_PROJECTS)));
  write(THREAD_KEY, JSON.stringify(threads.slice(0, MAX_THREADS)));
}
export function restoreThreadMessages(raw: unknown): UIMessage[] {
  // Server-owned durable turns only. Never invent historical messages on an unreadable payload.
  const thread = raw && typeof raw === 'object' ? (raw as Record<string, unknown>).thread : undefined;
  const turns = thread && typeof thread === 'object' ? (thread as Record<string, unknown>).turns : undefined;
  if (!Array.isArray(turns)) return [];
  const newestFirst: UIMessage[] = [];
  let retainedChars = 0;
  // Walk backwards and stop as soon as the UI budget is filled. Slicing a
  // giant history first can create unbounded transient arrays on resume.
  function take(role: 'user' | 'assistant', text: string, key: string): boolean {
    const preview = text.length > MAX_HISTORY_ITEM_CHARS
      ? text.slice(0, MAX_HISTORY_ITEM_CHARS) + '\\n[Preview limited; full text remains in Codex.]'
      : text;
    if (preview.length > MAX_RETAINED_TEXT_CHARS - retainedChars) return false;
    newestFirst.push({ id: key, role, text: preview, at: 'History' });
    retainedChars += preview.length;
    return newestFirst.length < MAX_RENDER_HISTORY;
  }
  for (let ti = turns.length - 1; ti >= 0 && newestFirst.length < MAX_RENDER_HISTORY; ti--) {
    const turn = turns[ti];
    if (!turn || typeof turn !== 'object') continue;
    const record = turn as Record<string, unknown>;
    const items = Array.isArray(record.items) ? record.items : [];
    for (let j = items.length - 1; j >= 0; j--) {
      const item = items[j];
      if (!item || typeof item !== 'object') continue;
      const value = item as Record<string, unknown>;
      if (value.type !== 'agentMessage' || typeof value.text !== 'string') continue;
      if (!take('assistant', value.text, 'resume-assistant-' + ti + '-' + j)) return newestFirst.reverse();
    }
    const input = Array.isArray(record.input) ? record.input : [];
    for (let j = input.length - 1; j >= 0; j--) {
      const value = input[j];
      if (!value || typeof value !== 'object' || typeof value.text !== 'string') continue;
      if (!take('user', value.text, 'resume-user-' + ti + '-' + j)) return newestFirst.reverse();
    }
  }
  return newestFirst.reverse();
}
