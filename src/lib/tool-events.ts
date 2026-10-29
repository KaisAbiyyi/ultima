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
  /** Complete immutable App Server params; never synthesize a descriptor in the UI. */
  request: Record<string, unknown>;
};
export type ToolItem = {
  id: string; threadId: string; turnId: string;
  kind: 'command' | 'fileChange' | 'other'; state: string;
  command: string; commandDescriptor: string | string[] | null; cwd: string;
  output: string; diff: string; paths: string[];
  truncated: boolean;
  /** True only when a complete, untruncated server snapshot was validated. */
  previewComplete: boolean;
};
function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
}
const REQUEST_FIELDS = new Set([
  'threadId', 'turnId', 'itemId', 'startedAtMs', 'command', 'cwd', 'grantRoot',
  'availableDecisions', 'reason'
]);
const ESCALATION_FIELDS = [
  'additionalPermissions', 'networkApprovalContext', 'proposedExecpolicyAmendment',
  'proposedNetworkPolicyAmendments'
];
function commandText(value: unknown): string | null {
  if (typeof value === 'string') return value.trim() && value.length <= MAX_COMMAND_CHARS ? value : null;
  if (!Array.isArray(value) || !value.length || !value.every(v =>
    typeof v === 'string' && v.length > 0 && !/\s/.test(v))) return null;
  const joined = value.join(' ');
  return joined.length <= MAX_COMMAND_CHARS ? joined : null;
}
/** No silent normalization of an ambiguous argv into a displayed shell command. */
function commandMatches(a: unknown, b: unknown): boolean {
  const aText = commandText(a), bText = commandText(b);
  return aText !== null && bText !== null && JSON.stringify(a) === JSON.stringify(b);
}
function safeRelativePath(path: string): boolean {
  return path.length > 0 && path.length <= MAX_PATH_CHARS &&
    !/[\\:%\x00-\x1f]/.test(path) && !path.startsWith('/') && !path.endsWith('/') &&
    path.split('/').every(part => part.length > 0 && part !== '.' && part !== '..');
}
function permissionScope(request: Record<string, unknown>, kind: ApprovalRequest['kind'], project: string): boolean {
  if (!project || Object.keys(request).some(k => !REQUEST_FIELDS.has(k) && !ESCALATION_FIELDS.includes(k))) return false;
  if (ESCALATION_FIELDS.some(key => request[key] !== undefined && request[key] !== null)) return false;
  const decisions = request.availableDecisions;
  if (decisions !== undefined &&
    (!Array.isArray(decisions) || !decisions.length || !decisions.includes('accept') ||
      !decisions.every(v => v === 'accept' || v === 'decline' || v === 'cancel'))) return false;
  if (request.reason !== undefined && (typeof request.reason !== 'string' || request.reason.length > 4096)) return false;
  if (kind === 'command') return request.grantRoot === undefined &&
    typeof request.cwd === 'string' && request.cwd === project && commandText(request.command) !== null;
  return request.command === undefined && request.cwd === undefined &&
    (request.grantRoot === undefined || request.grantRoot === project);
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
  const request = record(v.request);
  if (!request || !Object.keys(request).length ||
    new TextEncoder().encode(JSON.stringify(request)).length > 8192 ||
    request.threadId !== v.threadId || request.turnId !== v.turnId ||
    request.itemId !== v.itemId || request.startedAtMs !== v.startedAtMs) return null;
  return { id: v.id as string | number, kind: v.kind as ApprovalRequest['kind'],
    threadId: v.threadId, turnId: v.turnId, itemId: v.itemId,
    startedAtMs: v.startedAtMs as number, request: structuredClone(request) };
}
export function sameApproval(a: ApprovalRequest, b: ApprovalRequest): boolean {
  return a.id === b.id && a.kind === b.kind && a.threadId === b.threadId &&
    a.turnId === b.turnId && a.itemId === b.itemId && a.startedAtMs === b.startedAtMs &&
    JSON.stringify(a.request) === JSON.stringify(b.request);
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
  // Agent text and internal reasoning items are not tool executions.
  if (item?.type === 'agentMessage' || item?.type === 'reasoning' || item?.type === 'userMessage') return items;
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
    command: '', commandDescriptor: null, cwd: '', output: existing?.output ?? '', diff: '', paths: [],
    truncated: existing?.truncated ?? false, previewComplete: false
  };
  tool.cwd = typeof item?.cwd === 'string' && item.cwd.length <= MAX_PATH_CHARS ? item.cwd : '';
  if (item?.type === 'commandExecution') {
    tool.kind = 'command';
    const command = commandText(item.command);
    tool.commandDescriptor = command === null ? null : structuredClone(item.command) as string | string[];
    tool.command = command ?? '';
    tool.truncated ||= command === null;
    tool.previewComplete = command !== null && tool.cwd.length > 0 && !tool.truncated;
    if (typeof item.aggregatedOutput === 'string') {
      tool.output = bounded(item.aggregatedOutput, MAX_TOOL_OUTPUT_CHARS);
      tool.truncated ||= item.aggregatedOutput.length > MAX_TOOL_OUTPUT_CHARS;
    }
  } else if (item?.type === 'fileChange') {
    tool.kind = 'fileChange';
    const preview = filePreview(item.changes, item);
    tool.paths = preview.paths;
    tool.diff = preview.diff;
    tool.truncated ||= preview.truncated;
    tool.previewComplete = preview.complete && !tool.truncated;
  }
  return [...items.filter(t => toolIdentity(t) !== key), tool].slice(-MAX_TOOL_ITEMS);
}
/** A reviewed tool is never a grant for another selected thread or active turn. */
export function canApproveInActiveTurn(
  request: ApprovalRequest, items: ToolItem[], activeThread: string, activeTurn: string, project: string
): boolean {
  return request.threadId === activeThread && request.turnId === activeTurn && canApprove(request, items, project);
}
export function canApprove(request: ApprovalRequest, items: ToolItem[], project: string): boolean {
  const item = items.find(v => matchesApprovalEvidence(request, v));
  if (!item || !item.previewComplete || item.truncated ||
      !permissionScope(request.request, request.kind, project) ||
      request.request.threadId !== request.threadId || request.request.turnId !== request.turnId ||
      request.request.itemId !== request.itemId || request.request.startedAtMs !== request.startedAtMs) return false;
  if (request.kind === 'command') {
    return item.kind === 'command' && commandMatches(request.request.command, item.commandDescriptor) &&
      item.cwd === request.request.cwd && item.cwd === project && item.command.length > 0;
  }
  return item.kind === 'fileChange' && item.paths.length > 0 && item.diff.length > 0 &&
    (!item.cwd || item.cwd === project) && item.paths.every(safeRelativePath);
}
