import { describe, it, expect } from 'vitest';
import { parseApproval, canApprove, sameApproval, trackTool, MAX_TOOL_ITEMS, MAX_TOOL_DIFF_CHARS, MAX_TOOL_OUTPUT_CHARS } from './tool-events';
const approval = { id: 'a', kind: 'command' as const, threadId: 't', turnId: 'z', itemId: 'i', startedAtMs: 1 };
describe('NC-07 bounded tool evidence and explicit approvals', () => {
  it('rejects malformed or forged identity', () => {
    expect(parseApproval(approval)).toEqual(approval);
    expect(parseApproval({ ...approval, itemId: '' })).toBeNull();
    expect(parseApproval({ ...approval, id: -1 })).toBeNull();
    expect(parseApproval({ ...approval, startedAtMs: Number.NaN })).toBeNull();
    expect(sameApproval(approval, { ...approval, startedAtMs: 2 })).toBe(false);
  });
  it('refuses approval without the exact command or diff preview', () => {
    expect(canApprove(approval, [])).toBe(false);
    let tools = trackTool([], 'itemStarted', { item: { id: 'i', type: 'commandExecution', command: 'cargo test' } });
    expect(canApprove(approval, tools)).toBe(true);
    expect(canApprove({ ...approval, kind: 'fileChange' }, tools)).toBe(false);
    const change = { ...approval, kind: 'fileChange' as const };
    tools = trackTool(tools, 'itemStarted', { item: { id: 'i', type: 'fileChange', changes: [{ path: 'src/app.rs', diff: '-a\n+b' }] } });
    expect(canApprove(change, tools)).toBe(true);
    tools = trackTool(tools, 'itemCompleted', { item: { id: 'i', type: 'fileChange', changes: [{ path: 'x', diff: 'a'.repeat(MAX_TOOL_DIFF_CHARS + 500) }] } });
    expect(canApprove(change, tools)).toBe(false);
  });
  it('truncates huge streams, caps item count and shows data as inert text', () => {
    let tools = trackTool([], 'itemStarted', { item: { id: 'i', type: 'commandExecution', command: '<script>alert(1)</script>' } });
    tools = trackTool(tools, 'commandOutput', { itemId: 'i', delta: 'a'.repeat(MAX_TOOL_OUTPUT_CHARS + 100) });
    expect(tools[0].output).toHaveLength(MAX_TOOL_OUTPUT_CHARS);
    expect(tools[0].truncated).toBe(true);
    for (let i = 0; i < 30; i++) tools = trackTool(tools, 'itemStarted', { item: { id: 'next' + i, type: 'commandExecution', command: 'ls' } });
    expect(tools).toHaveLength(MAX_TOOL_ITEMS);
    expect(tools.some(t => t.id === 'i')).toBe(false);
  });
});