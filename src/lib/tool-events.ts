/** NC-07 ephemeral, bounded tool presentation. Never persisted or interpreted as HTML. */
export const MAX_TOOL_ITEMS = 16;
export const MAX_COMMAND_CHARS = 4096;
export const MAX_TOOL_OUTPUT_CHARS = 32 * 1024;
export const MAX_TOOL_DIFF_CHARS = 64 * 1024;
export type ApprovalRequest = {
  id: string | number; kind: 'command' | 'fileChange';
  threadId: string; turnId: string; itemId: string; startedAtMs: number;
};
export type ToolItem = {
  id: string; kind: 'command' | 'fileChange' | 'other'; state: string;
  command: string; output: string; diff: string; paths: string[]; truncated: boolean;
};
function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
}
function bounded(value: unknown, max: number): string {
  return typeof value === 'string' ? value.slice(0, max) : '';
}
export function parseApproval(raw: unknown): ApprovalRequest | null {
  const v = record(raw);
  if (!v || (v.kind !== 'command' && v.kind !== 'fileChange')) return null;
  if (!(typeof v.id === 'string' && v.id.length > 0 && v.id.length <= 128 ||
    typeof v.id === 'number' && Number.isSafeInteger(v.id) && v.id >= 0)) return null;
  for (const key of ['threadId', 'turnId', 'itemId']) {
    if (typeof v[key] !== 'string' || !(v[key] as string).length || (v[key] as string).length > 256) return null;
  }
  if (!Number.isSafeInteger(v.startedAtMs) || (v.startedAtMs as number) < 0) return null;
  return v as ApprovalRequest;
}
export function sameApproval(a: ApprovalRequest, b: ApprovalRequest): boolean {
  return a.id === b.id && a.kind === b.kind && a.threadId === b.threadId &&
    a.turnId === b.turnId && a.itemId === b.itemId && a.startedAtMs === b.startedAtMs;
}
function clampAppend(current: string, delta: unknown, limit: number): [string, boolean] {
  if (typeof delta !== 'string') return [current, false];
  const room = Math.max(0, limit - current.length);
  return [current + delta.slice(0, room), delta.length > room];
}
export function trackTool(items: ToolItem[], type: 'itemStarted' | 'itemCompleted' | 'commandOutput', params: unknown): ToolItem[] {
  const p = record(params);
  if (!p) return items;
  const item = record(p.item);
  const id = typeof (item?.id ?? p.itemId) === 'string' ? (item?.id ?? p.itemId) as string : '';
  if (!id || id.length > 256) return items;
  const idx = items.findIndex(v => v.id === id);
  const before = idx < 0 ? undefined : items[idx];
  const tool: ToolItem = before ? { ...before } : { id, kind: 'other', state: 'running', command: '', output: '', diff: '', paths: [], truncated: false };
  if (item) {
    if (item.type === 'commandExecution') {
      tool.kind = 'command';
      const command = typeof item.command === 'string' ? item.command : Array.isArray(item.command) ? item.command.filter(x => typeof x === 'string').join(' ') : '';
      if (command) { tool.command = bounded(command, MAX_COMMAND_CHARS); tool.truncated ||= command.length > MAX_COMMAND_CHARS; }
      // Some versions send output only on completed snapshots, not outputDelta.
      if (typeof item.aggregatedOutput === 'string' && item.aggregatedOutput.length >= tool.output.length) {
        tool.output = bounded(item.aggregatedOutput, MAX_TOOL_OUTPUT_CHARS);
        tool.truncated ||= item.aggregatedOutput.length > MAX_TOOL_OUTPUT_CHARS;
      }
    }
    if (item.type === 'fileChange') {
      tool.kind = 'fileChange';
      if (Array.isArray(item.changes)) {
        const changes = item.changes.slice(0, 32).map(record).filter((v): v is Record<string, unknown> => !!v);
        tool.paths = changes.map(change => bounded(change.path, 512)).filter(Boolean);
        const rawDiff = changes.map(change => {
          const path = bounded(change.path, 512);
          const diff = bounded(change.diff, MAX_TOOL_DIFF_CHARS);
          return diff ? path + '\n' + diff : '';
        }).filter(Boolean).join('\n\n');
        tool.diff = bounded(rawDiff, MAX_TOOL_DIFF_CHARS);
        tool.truncated ||= changes.length < item.changes.length || rawDiff.length > MAX_TOOL_DIFF_CHARS;
      }
    }
  }
  if (type === 'commandOutput' && tool.kind !== 'fileChange') {
    const [output, truncated] = clampAppend(tool.output, p.delta, MAX_TOOL_OUTPUT_CHARS);
    tool.output = output;
    tool.truncated ||= truncated;
  }
  if (type === 'itemCompleted') tool.state = 'complete';
  const updated = items.filter(v => v.id !== id);
  return [...updated, tool].slice(-MAX_TOOL_ITEMS);
}
export function canApprove(request: ApprovalRequest, items: ToolItem[]): boolean {
  const item = items.find(v => v.id === request.itemId);
  return !!item && ((request.kind === 'command' && item.kind === 'command' && !!item.command && !item.truncated) ||
    (request.kind === 'fileChange' && item.kind === 'fileChange' && !!item.diff && !item.truncated));
}
