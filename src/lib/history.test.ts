import { describe, expect, it } from 'vitest';
import { createMessage, makeHistory } from './history';

describe('synthetic message history', () => {
  it('creates exactly ten thousand deterministic safe messages', () => {
    const messages = makeHistory();
    expect(messages).toHaveLength(10_000);
    expect(messages[0]).toEqual(makeHistory(1)[0]);
    expect(messages.at(-1)?.id).toBe('fixture-9999');
    expect(messages.filter((item) => item.role === 'user')).toHaveLength(5_000);
  });
  it('rejects invalid fixture counts', () => {
    expect(() => makeHistory(-1)).toThrow(RangeError);
    expect(() => makeHistory(100_001)).toThrow(RangeError);
    expect(() => makeHistory(0.5)).toThrow(RangeError);
  });
  it('labels mock streamed replies separately from fixtures', () => {
    expect(createMessage('assistant', 'Demo', 10).id).toBe('live-10-assistant');
  });
});
