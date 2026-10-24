import { describe, expect, it } from 'vitest';
import { EngineSessionFence } from './engine-session-fence';
type Event = { session: number; type: string };
describe('NC-06 IPC generation fencing', () => {
  it('buffers pre-ack events and only replays matching session', () => {
    const fence = new EngineSessionFence<Event>();
    fence.beginConnect();
    expect(fence.accept({ session: 2, type: 'ready' })).toBe(false);
    expect(fence.accept({ session: 1, type: 'old-error' })).toBe(false);
    expect(fence.connected(2)).toEqual([{ session: 2, type: 'ready' }]);
    expect(fence.accept({ session: 1, type: 'old-disconnected' })).toBe(false);
    expect(fence.accept({ session: 2, type: 'delta' })).toBe(true);
  });
  it('ignores late old deltas, replies and disconnect after quick reconnect', () => {
    const fence = new EngineSessionFence<Event>();
    fence.beginConnect();
    fence.connected(1);
    fence.beginDisconnect();
    for (const type of ['delta', 'reply', 'disconnected']) {
      expect(fence.accept({ session: 1, type })).toBe(false);
    }
    fence.beginConnect();
    expect(fence.accept({ session: 1, type: 'error' })).toBe(false);
    expect(fence.accept({ session: 2, type: 'ready' })).toBe(false);
    expect(fence.connected(2)).toEqual([{ session: 2, type: 'ready' }]);
    for (const type of ['delta', 'reply', 'disconnected']) {
      expect(fence.accept({ session: 1, type })).toBe(false);
    }
    expect(fence.accept({ session: 2, type: 'reply' })).toBe(true);
  });
  it('bounds early buffering and flushes on cancellation', () => {
    const fence = new EngineSessionFence<Event>();
    fence.beginConnect();
    for (let i = 0; i < 500; i++) fence.accept({ session: 3, type: String(i) });
    expect(fence.connected(3)).toHaveLength(64);
    fence.beginDisconnect();
    fence.beginConnect();
    expect(fence.connected(4)).toHaveLength(0);
  });
  it('rejects invalid or unsolicited connection acknowledgement', () => {
    const fence = new EngineSessionFence<Event>();
    expect(() => fence.connected(1)).toThrow();
    fence.beginConnect();
    expect(() => fence.connected(0)).toThrow();
  });
});
