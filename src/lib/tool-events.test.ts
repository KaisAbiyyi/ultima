import { describe, it, expect } from 'vitest';
import {
  approvalToolIdentity, parseApproval, canApprove, canApproveInActiveTurn, matchesApprovalEvidence, sameApproval,
  toolIdentity, trackTool, MAX_FILE_CHANGES, MAX_TOOL_ITEMS, MAX_TOOL_DIFF_CHARS,
  MAX_TOOL_OUTPUT_CHARS, type ApprovalRequest
} from './tool-events';

const approval: ApprovalRequest =
  { id: 'a', kind: 'command', threadId: 't', turnId: 'z', itemId: 'i', startedAtMs: 1 };
const fileApproval: ApprovalRequest = { ...approval, kind: 'fileChange' };
const change = (path: string, diff: unknown): Record<string, unknown> => ({ path, diff });
const itemEvent = (threadId: string, turnId: string, id: string, type: string, data: Record<string, unknown> = {}) =>
  ({ threadId, turnId, item: { id, type, ...data } });
const fileEvent = (changes: unknown, data: Record<string, unknown> = {}, threadId = 't', turnId = 'z', id = 'i') =>
  itemEvent(threadId, turnId, id, 'fileChange', { changes, ...data });

describe('NC-07 bounded tool evidence and explicit approvals', () => {
  it('rejects malformed or forged approval identity', () => {
    expect(parseApproval(approval)).toEqual(approval);
    expect(parseApproval({ ...approval, itemId: '' })).toBeNull();
    expect(parseApproval({ ...approval, id: -1 })).toBeNull();
    expect(parseApproval({ ...approval, startedAtMs: Number.NaN })).toBeNull();
    expect(sameApproval(approval, { ...approval, startedAtMs: 2 })).toBe(false);
  });

  it('allows a complete two-file preview only when ALL diffs are represented', () => {
    const changes = [change('src/a.ts', '-oldA\n+newA'), change('src/b.ts', '-oldB\n+newB')];
    const evidence = trackTool([], 'itemStarted', fileEvent(changes, { totalChanges: 2 }));
    expect(evidence[0].paths).toEqual(['src/a.ts', 'src/b.ts']);
    expect(evidence[0].diff).toContain('-oldA\n+newA');
    expect(evidence[0].diff).toContain('-oldB\n+newB');
    expect(evidence[0].previewComplete).toBe(true);
    expect(evidence[0].truncated).toBe(false);
    expect(canApprove(fileApproval, evidence)).toBe(true);
  });

  it.each([
    ['missing second diff', [change('src/a.ts', '+a'), { path: 'src/b.ts' }]],
    ['null second diff', [change('src/a.ts', '+a'), change('src/b.ts', null)]],
    ['empty second diff', [change('src/a.ts', '+a'), change('src/b.ts', '')]],
    ['whitespace second diff', [change('src/a.ts', '+a'), change('src/b.ts', '   ')]],
    ['missing path', [change('src/a.ts', '+a'), { diff: '+b' }]],
    ['malformed second entry', [change('src/a.ts', '+a'), 'not an object']],
    ['duplicate path', [change('src/a.ts', '+a'), change('src/a.ts', '+b')]],
    ['entry flagged partial', [change('src/a.ts', '+a'), { path: 'src/b.ts', diff: '+b', partial: true }]]
  ])('fails closed on multi-file %s', (_name, changes) => {
    const items = trackTool([], 'itemStarted', fileEvent(changes));
    expect(items[0].previewComplete).toBe(false);
    expect(items[0].truncated).toBe(true);
    expect(canApprove(fileApproval, items)).toBe(false);
  });

  it('never interprets one nonempty diff as proof of all declared files', () => {
    const first = change('src/a.ts', '+a');
    for (const data of [{ totalChanges: 2 }, { changeCount: 2 }, { hasMore: true }, { partial: true }, { truncated: true }]) {
      const tools = trackTool([], 'itemStarted', fileEvent([first], data));
      expect(canApprove(fileApproval, tools)).toBe(false);
    }
    for (const bad of [undefined, null, [], {}, 'patch text']) {
      expect(canApprove(fileApproval, trackTool([], 'itemStarted', fileEvent(bad)))).toBe(false);
    }
    const good = trackTool([], 'itemStarted', fileEvent([first, change('src/b.ts', '+b')]));
    const nowIncomplete = trackTool(good, 'itemCompleted', fileEvent([first, { path: 'src/b.ts' }]));
    expect(canApprove(fileApproval, good)).toBe(true);
    expect(canApprove(fileApproval, nowIncomplete)).toBe(false);
  });

  it('rejects per-item overflow and aggregate overflow across otherwise-valid files', () => {
    const first = change('src/a.ts', 'x'.repeat(Math.floor(MAX_TOOL_DIFF_CHARS / 2)));
    const second = change('src/b.ts', 'y'.repeat(Math.floor(MAX_TOOL_DIFF_CHARS / 2)));
    const twoFiles = trackTool([], 'itemStarted', fileEvent([first, second]));
    expect(twoFiles[0].diff.length).toBeLessThanOrEqual(MAX_TOOL_DIFF_CHARS);
    expect(twoFiles[0].truncated).toBe(true);
    expect(canApprove(fileApproval, twoFiles)).toBe(false);
    const tooMany = Array.from({ length: MAX_FILE_CHANGES + 1 }, (_, n) => change(`file-${n}`, '+a'));
    expect(canApprove(fileApproval, trackTool([], 'itemStarted', fileEvent(tooMany)))).toBe(false);
  });

  it('binds preview evidence to exact thread, turn and item, not reused item ID', () => {
    const command = itemEvent('t', 'turn-one', 'i', 'commandExecution', { command: 'cargo test' });
    const first = trackTool([], 'itemStarted', command);
    expect(canApprove({ ...approval, turnId: 'turn-one' }, first)).toBe(true);
    expect(canApprove({ ...approval, turnId: 'turn-two' }, first)).toBe(false);
    expect(canApprove({ ...approval, threadId: 'other', turnId: 'turn-one' }, first)).toBe(false);
    expect(canApprove({ ...approval, itemId: 'other', turnId: 'turn-one' }, first)).toBe(false);
    expect(matchesApprovalEvidence({ ...approval, turnId: 'turn-one' }, first[0])).toBe(true);
    expect(toolIdentity(first[0])).toBe(approvalToolIdentity({ ...approval, turnId: 'turn-one' }));
    const second = trackTool(first, 'itemStarted', itemEvent('t', 'turn-two', 'i', 'commandExecution', {}));
    expect(second).toHaveLength(2);
    expect(canApprove({ ...approval, turnId: 'turn-one' }, second)).toBe(true);
    expect(canApprove({ ...approval, turnId: 'turn-two' }, second)).toBe(false);
    expect(trackTool(first, 'commandOutput', { threadId: 't', turnId: 'turn-two', itemId: 'i', delta: 'fake' })).toEqual(first);
  });

  it('revokes Allow once when the selected active turn changes', () => {
    const tools = trackTool([], 'itemStarted', fileEvent([
      change('src/a.ts', '+a'), change('src/b.ts', '+b')
    ]));
    expect(canApproveInActiveTurn(fileApproval, tools, 't', 'z')).toBe(true);
    expect(canApproveInActiveTurn(fileApproval, tools, 't', 'next-turn')).toBe(false);
    expect(canApproveInActiveTurn(fileApproval, tools, 'other-thread', 'z')).toBe(false);
    expect(canApproveInActiveTurn(fileApproval, [], 't', 'z')).toBe(false);
  });

  it('never consumes unscoped or forged turn output as an approval preview', () => {
    const base = trackTool([], 'itemStarted', itemEvent('t', 'z', 'i', 'commandExecution', { command: 'cargo test' }));
    expect(trackTool(base, 'itemCompleted', { threadId: 't', item: { id: 'i', type: 'fileChange', changes: [change('x', '+b')] } })).toEqual(base);
    expect(trackTool(base, 'itemCompleted', fileEvent([change('x', '+b')], {}, 't', 'other', 'i'))[0]).toEqual(base[0]);
    expect(canApprove(fileApproval, base)).toBe(false);
    expect(canApprove(approval, base)).toBe(true);
  });

  it('refuses blind/truncated command approvals and bounds output and item count', () => {
    expect(canApprove(approval, [])).toBe(false);
    expect(canApprove(approval, trackTool([], 'itemStarted',
      itemEvent('t', 'z', 'i', 'commandExecution', {})))).toBe(false);
    let tools = trackTool([], 'itemStarted',
      itemEvent('t', 'z', 'i', 'commandExecution', { command: '<script>alert(1)</script>' }));
    expect(canApprove(approval, tools)).toBe(true);
    tools = trackTool(tools, 'commandOutput',
      { threadId: 't', turnId: 'z', itemId: 'i', delta: 'a'.repeat(MAX_TOOL_OUTPUT_CHARS + 100) });
    expect(tools[0].output).toHaveLength(MAX_TOOL_OUTPUT_CHARS);
    expect(tools[0].truncated).toBe(true);
    expect(canApprove(approval, tools)).toBe(false);
    for (let n = 0; n < 30; n++) {
      tools = trackTool(tools, 'itemStarted',
        itemEvent('t', 'z', 'item' + n, 'commandExecution', { command: 'ls' }));
    }
    expect(tools).toHaveLength(MAX_TOOL_ITEMS);
    expect(tools.some(t => t.id === 'i')).toBe(false);
  });
});
