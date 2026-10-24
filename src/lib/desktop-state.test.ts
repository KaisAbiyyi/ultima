import { describe, expect, it } from 'vitest';
import { appendDelta, appendProject, addThread, cappedMessages, MAX_PROJECTS, MAX_RENDER_HISTORY, MAX_TEXT_BYTES, restoreProjects, restoreThreads, restoreThreadMessages } from './desktop-state';
import { makeHistory } from './history';
describe('NC-06 offline security and state', () => {
  it('accepts only absolute selected paths and bounds recent projects', () => {
    expect(appendProject([], '../escape')).toEqual([]);
    expect(appendProject([], 'C:\\projects\\ultima')[0].name).toBe('ultima');
    const all = Array.from({ length: 20 }, (_, i) => 'C:\\projects\\p' + i).reduce(appendProject, []);
    expect(all).toHaveLength(MAX_PROJECTS);
    expect(appendProject(all, 'C:\\projects\\p19')).toHaveLength(MAX_PROJECTS);
  });
  it('bounds metadata and tolerates corrupt storage without loading credentials', () => {
    expect(restoreProjects(() => '{')).toEqual([]);
    expect(restoreThreads(() => JSON.stringify([{ id: 'a', project: '/a', title: 't', model: '' }, { id: 'x'.repeat(500), project: '/a', title: 't', model: '' }]))).toHaveLength(1);
    const a = { id: 'safe', project: '/tmp', title: 'test', model: '' };
    expect(addThread([a], a)).toHaveLength(1);
  });
  it('keeps history and streamed text within fixed caps', () => {
    expect(cappedMessages(makeHistory(10_000))).toHaveLength(MAX_RENDER_HISTORY);
    expect(() => appendDelta('x'.repeat(MAX_TEXT_BYTES), 'y')).toThrow();
    expect(appendDelta('a','b')).toBe('ab');
  });
  it('resumes real turns only; never guesses missing history', () => {
    expect(restoreThreadMessages({})).toEqual([]);
    expect(restoreThreadMessages({ thread: { turns: [{ input: [{ type: 'text', text: 'Hi' }], items: [{ type: 'agentMessage', text: 'Hello' }] }] } }).map(m => m.text)).toEqual(['Hi', 'Hello']);
  });
});
