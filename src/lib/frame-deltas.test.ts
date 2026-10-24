import { describe, expect, it } from 'vitest';
import { FrameDeltas, MAX_FRAME_DELTA_CHARS } from './frame-deltas';
describe('NC-08 frame-coalesced deltas', () => {
  function harness() {
    const callbacks = new Map<number, () => void>();
    const output: string[] = [];
    let next = 0;
    const batch = new FrameDeltas(
      cb => { callbacks.set(++next, cb); return next; },
      id => { callbacks.delete(id); },
      text => { output.push(text); }
    );
    const paint = () => { const tasks = [...callbacks.values()]; callbacks.clear(); tasks.forEach(cb => cb()); };
    return { batch, callbacks, output, paint };
  }
  it('10k rapid deltas schedule one frame and preserve exact order', () => {
    const { batch, callbacks, output, paint } = harness();
    for (let i = 0; i < 10_000; i++) expect(batch.push('x')).toBe(true);
    expect(callbacks.size).toBe(1);
    expect(output).toEqual([]);
    paint();
    expect(output).toEqual(['x'.repeat(10_000)]);
    expect(batch.queuedChars).toBe(0);
  });
  it('backpressure is explicit, bounded and does not discard accepted data', () => {
    const { batch, output } = harness();
    expect(batch.push('a'.repeat(MAX_FRAME_DELTA_CHARS))).toBe(true);
    expect(batch.push('b')).toBe(false);
    expect(batch.queuedChars).toBe(MAX_FRAME_DELTA_CHARS);
    batch.drain();
    expect(output).toEqual(['a'.repeat(MAX_FRAME_DELTA_CHARS)]);
    expect(batch.push('c')).toBe(true);
  });
  it('teardown cancels scheduled work and drops stale callbacks', () => {
    const { batch, callbacks, output, paint } = harness();
    batch.push('old');
    batch.clear();
    expect(callbacks.size).toBe(0);
    paint();
    expect(output).toEqual([]);
    batch.push('new');
    batch.drain();
    expect(output).toEqual(['new']);
    expect(callbacks.size).toBe(0);
  });
  it('fences restored or changed target by allowing explicit clear', () => {
    const { batch, output, paint } = harness();
    batch.push('turn A');
    batch.clear();
    batch.push('turn B');
    paint();
    expect(output).toEqual(['turn B']);
  });
});
