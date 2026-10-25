import { describe, expect, it } from 'vitest';
import { cappedMessages, MAX_RETAINED_TEXT_CHARS, MAX_RENDER_HISTORY, MAX_HISTORY_ITEM_CHARS, restoreThreadMessages } from './desktop-state';
import { createMessage, makeHistory } from './history';

describe('NC-08 history retention and hostile resume payloads', () => {
  it('bounds both row count and retained text for large in-memory views', () => {
    const many = makeHistory(10_000);
    expect(cappedMessages(many)).toHaveLength(MAX_RENDER_HISTORY);
    const huge = Array.from({ length: 400 }, (_, i) => createMessage('assistant', 'a'.repeat(32 * 1024), i));
    const limited = cappedMessages(huge);
    expect(limited.length).toBeLessThan(400);
    expect(limited.reduce((sum, m) => sum + m.text.length, 0)).toBeLessThanOrEqual(MAX_RETAINED_TEXT_CHARS);
    expect(limited.at(-1)?.id).toBe(huge.at(-1)?.id);
  });
  it('restores latest turns in chronological order and never materializes every item', () => {
    const turns = Array.from({ length: 20_000 }, (_, i) => ({
      input: [{ type: 'text', text: 'user-' + i }],
      items: [{ type: 'agentMessage', text: 'reply-' + i }]
    }));
    const result = restoreThreadMessages({ thread: { turns } });
    expect(result).toHaveLength(MAX_RENDER_HISTORY);
    expect(result[0].text).toBe('user-17500');
    expect(result.at(-1)?.text).toBe('reply-19999');
  });
  it('clips massive history item previews but never silently changes the Codex source', () => {
    const result = restoreThreadMessages({ thread: { turns: [{
      input: [{ type: 'text', text: 'z'.repeat(MAX_HISTORY_ITEM_CHARS * 2) }],
      items: [{ type: 'agentMessage', text: 'ok' }]
    }] } });
    expect(result).toHaveLength(2);
    expect(result[0].text.length).toBeLessThan(MAX_HISTORY_ITEM_CHARS + 100);
    expect(result[0].text).toContain('[Preview limited; full text remains in Codex.]');
    expect(result[1].text).toBe('ok');
  });
  it('rejects malformed payloads without inventing history', () => {
    expect(restoreThreadMessages({ thread: { turns: [null, {}, { items: [null, { type: 'commandExecution' }] }] } })).toEqual([]);
  });
});
