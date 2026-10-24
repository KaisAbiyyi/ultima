import { describe, expect, it } from 'vitest';
import { visibleRange } from './virtual';

describe('bounded virtual message window', () => {
  it('does not render all 10k rows at startup', () => {
    const range = visibleRange(10_000, 0, 800, 88, 6);
    expect(range).toEqual({ start: 0, end: 16 });
  });
  it('clamps the bottom of a large conversation', () => {
    const range = visibleRange(10_000, 879_300, 700, 88, 6);
    expect(range.end).toBe(10_000);
    expect(range.end - range.start).toBeLessThan(30);
  });
  it('handles empty lists and invalid heights', () => {
    expect(visibleRange(0, 0, 500, 88)).toEqual({ start: 0, end: 0 });
    expect(() => visibleRange(12, 0, 500, 0)).toThrow(RangeError);
  });
});
