/** NC-07 ephemeral, bounded tool evidence. Unverified previews never authorize actions. */
export const MAX_TOOL_ITEMS = 16;
export const MAX_COMMAND_CHARS = 4096;
export const MAX_TOOL_OUTPUT_CHARS = 32 * 1024;
export const MAX_TOOL_DIFF_CHARS = 64 * 1024;
export const MAX_FILE_CHANGES = 32;
export const MAX_PATH_CHARS = 512;

export type ApprovalRequest = {
  id: string | number; kind: 'command' | 'fileChange';
  threadId: string; turnId: string; itemId: string; startedAtMs: number;
};
export type ToolItem = {
  id: string; threadId: string; turnId: string;
  kind: 'command' | 'fileChange' | 'other'; state: string;
  command: string; output: string; diff: string; paths: string[];
  truncated: boolean;
  /** True only when a complete, untruncated server snapshot was validated. */
  previewComplete: boolean;
};
function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
}
function bounded(value: unknown, max: number): string {
  return typeof value === 'string' ? value.slice(0, max) : '';
}
function identity(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= 256;
}
export function parseApproval(raw: unknown): ApprovalRequest | null {
  const v = record(raw);
  if (!v || (v.kind !== 'command' && v.kind !== 'fileChange')) return null;
  if (!(typeof v.id === 'string' && v.id.length > 0 && v.id.length <= 128 ||
    typeof v.id === 'number' && Number.isSafeInteger(v.id) && v.id >= 0)) return null;
  if (!identity(v.threadId) || !identity(v.turnId) || !identity(v.itemId)) return null;
  if (!Number.isSafeInteger(v.startedAtMs) || (v.startedAtMs as number) < 0) return null;
  return v as ApprovalRequest;
}
export function sameApproval(a: ApprovalRequest, b: ApprovalRequest): boolean {
  return a.id === b.id && a.kind === b.kind && a.threadId === b.threadId &&
    a.turnId === b.turnId && a.itemId === b.itemId && a.startedAtMs === b.startedAtMs;
}
/** JSON array is unambiguous even if a provider ID contains a separator. */
export function toolIdentity(item: Pick<ToolItem, 'threadId' | 'turnId' | 'id'>): string {
  return JSON.stringify([item.threadId, item.turnId, item.id]);
}
export function approvalToolIdentity(approval: ApprovalRequest): string {
  return JSON.stringify([approval.threadId, approval.turnId, approval.itemId]);
}
export function matchesApprovalEvidence(request: ApprovalRequest, item: ToolItem): boolean {
  return request.threadId === item.threadId && request.turnId === item.turnId && request.itemId === item.id;
}
type FilePreview = { diff: string; paths: string[]; complete: boolean; truncated: boolean };

/**
 * The preview is atomic: EVERY declared change must have a valid path and full
 * diff, and the full aggregate must fit. Do not silently filter/slice entries.
 * The server may omit diffs entirely; in that case approvals remain decline-only.
 */
function filePreview(raw: unknown, snapshot: Record<string, unknown>): FilePreview {
  const invalid = (diff = '', paths: string[] = []): FilePreview =>
    ({ diff, paths, complete: false, truncated: true });
  if (!Array.isArray(raw) || raw.length === 0 || raw.length > MAX_FILE_CHANGES) return invalid();
  // Reject any explicit signal of partial evidence or contradictory change counts.
  if (snapshot.truncated === true || snapshot.isTruncated === true ||
      snapshot.hasMore === true || snapshot.partial === true) return invalid();
  for (const key of ['totalChanges', 'changeCount']) {
    if (key in snapshot &&
        (!Number.isSafeInteger(snapshot[key]) || snapshot[key] !== raw.length)) return invalid();
  }
  const paths: string[] = [];
  const seen = new Set<string>();
  let full = '';
  for (const change of raw) {
    const entry = record(change);
    const path = entry?.path;
    const diff = entry?.diff;
    if (typeof path !== 'string' || !path.trim() || path.length > MAX_PATH_CHARS ||
        seen.has(path) || typeof diff !== 'string' || !diff.trim() ||
        entry?.truncated === true || entry?.isTruncated === true ||
        entry?.partial === true || entry?.hasMore === true) return invalid(full, paths);
    paths.push(path);
    seen.add(path);
    const segment = (full ? '\n\n' : '') + path + '\n' + diff;
    if (segment.length > MAX_TOOL_DIFF_CHARS - full.length) {
      return invalid((full + segment).slice(0, MAX_TOOL_DIFF_CHARS), paths);
    }
    full += segment;
  }
  return { diff: full, paths, complete: true, truncated: false };
}
function clampAppend(current: string, delta: unknown, limit: number): [string, boolean] {
  if (typeof delta !== 'string') return [current, false];
  const room = Math.max(0, limit - current.length);
  return [current + delta.slice(0, room), delta.length > room];
}
export function trackTool(items: ToolItem[], type: 'itemStarted' | 'itemCompleted' | 'commandOutput', params: unknown): ToolItem[] {
  const p = record(params);
  if (!p || !identity(p.threadId) || !identity(p.turnId)) return items;
  const item = record(p.item);
  const id = item?.id ?? p.itemId;
  if (!identity(id)) return items;
  const key = JSON.stringify([p.threadId, p.turnId, id]);
  const existing = items.find(t => toolIdentity(t) === key);
  // Never merge an item with a reused ID belonging to a different thread/turn.
  if (type === 'commandOutput') {
    if (!existing || existing.kind !== 'command') return items;
    const [output, truncated] = clampAppend(existing.output, p.delta, MAX_TOOL_OUTPUT_CHARS);
    const updated = { ...existing, output, truncated: existing.truncated || truncated };
    return [...items.filter(t => toolIdentity(t) !== key), updated].slice(-MAX_TOOL_ITEMS);
  }
  const tool: ToolItem = {
    id, threadId: p.threadId, turnId: p.turnId,
    kind: 'other', state: type === 'itemCompleted' ? 'complete' : 'running',
    command: '', output: existing?.output ?? '', diff: '', paths: [],
    truncated: false, previewComplete: false
  };
  if (item?.type === 'commandExecution') {
    tool.kind = 'command';
    const command = typeof item.command === 'string' ? item.command :
      Array.isArray(item.command) && item.command.every(x => typeof x === 'string') ? item.command.join(' ') : '';
    tool.command = bounded(command, MAX_COMMAND_CHARS);
    tool.truncated = command.length > MAX_COMMAND_CHARS;
    tool.previewComplete = command.trim().length > 0 && !tool.truncated;
    if (typeof item.aggregatedOutput === 'string') {
      tool.output = bounded(item.aggregatedOutput, MAX_TOOL_OUTPUT_CHARS);
      tool.truncated ||= item.aggregatedOutput.length > MAX_TOOL_OUTPUT_CHARS;
    }
  } else if (item?.type === 'fileChange') {
    tool.kind = 'fileChange';
    const preview = filePreview(item.changes, item);
    tool.paths = preview.paths;
    tool.diff = preview.diff;
    tool.truncated = preview.truncated;
    tool.previewComplete = preview.complete;
  }
  return [...items.filter(t => toolIdentity(t) !== key), tool].slice(-MAX_TOOL_ITEMS);
}
export function canApprove(request: ApprovalRequest, items: ToolItem[]): boolean {
  const item = items.find(v => matchesApprovalEvidence(request, v));
  return !!item && item.previewComplete && !item.truncated && (
    request.kind === 'command' && item.kind === 'command' && !!item.command ||
    request.kind === 'fileChange' && item.kind === 'fileChange' &&
      item.paths.length > 0 && !!item.diff
  );
}
