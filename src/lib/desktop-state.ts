import type { UIMessage } from './history';
const PROJECT_KEY = 'ultima-projects-v1';
const THREAD_KEY = 'ultima-threads-v1';
export const MAX_PROJECTS = 12;
export const MAX_THREADS = 50;
export const MAX_RENDER_HISTORY = 5000;
export const MAX_TEXT_BYTES = 2 * 1024 * 1024;
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
  return messages.length > MAX_RENDER_HISTORY ? messages.slice(-MAX_RENDER_HISTORY) : messages;
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
  const messages: UIMessage[] = [];
  for (const [index, turn] of turns.slice(-MAX_RENDER_HISTORY).entries()) {
    if (!turn || typeof turn !== 'object') continue;
    const record = turn as Record<string, unknown>;
    const input = Array.isArray(record.input) ? record.input : [];
    for (const value of input) {
      if (!value || typeof value !== 'object' || typeof value.text !== 'string') continue;
      messages.push({ id: 'resume-user-' + index + '-' + messages.length, role: 'user', text: value.text.slice(0, MAX_TEXT_BYTES), at: 'History' });
    }
    const items = Array.isArray(record.items) ? record.items : [];
    for (const item of items) {
      if (!item || typeof item !== 'object') continue;
      const value = item as Record<string, unknown>;
      if (value.type !== 'agentMessage' || typeof value.text !== 'string') continue;
      messages.push({ id: 'resume-assistant-' + index + '-' + messages.length, role: 'assistant', text: value.text.slice(0, MAX_TEXT_BYTES), at: 'History' });
    }
  }
  return cappedMessages(messages);
}
