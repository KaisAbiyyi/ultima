import { describe, expect, it } from 'vitest';
import { NativePerformance } from './native-performance';

describe('NC-10 opt-in numeric timing probe', () => {
  it('does no scheduling while disabled and never accepts negative/nonfinite timings', () => {
    const scheduled: (() => void)[] = [];
    let now = 1;
    const probe = new NativePerformance(() => now, callback => scheduled.push(callback));
    probe.observe('input');
    expect(scheduled).toHaveLength(0);
    probe.start();
    probe.observe('input');
    now = -1;
    scheduled.shift()!();
    expect(probe.snapshot().input).toEqual([]);
  });
  it('caps both outstanding work and retained samples, exposes drops and clears on stop', () => {
    const scheduled: (() => void)[] = [];
    let now = 10;
    const probe = new NativePerformance(() => now, callback => scheduled.push(callback), 3);
    probe.start();
    for (let i = 0; i < 8; i++) probe.observe('stream');
    expect(scheduled).toHaveLength(3);
    now = 30;
    scheduled.splice(0).forEach(callback => callback());
    expect(probe.snapshot().stream).toEqual([20, 20, 20]);
    expect(probe.snapshot().dropped).toBe(5);
    probe.observe('stream');
    expect(scheduled).toHaveLength(0);
    expect(probe.snapshot().dropped).toBe(6);
    probe.stop();
    expect(probe.snapshot().stream).toEqual([]);
  });
  it('ignores stale callbacks after a new measurement and snapshots cannot mutate stored data', () => {
    const scheduled: (() => void)[] = [];
    let now = 10;
    const probe = new NativePerformance(() => now, callback => scheduled.push(callback));
    probe.start(); probe.observe('input');
    probe.start(); now = 20;
    scheduled.shift()!();
    expect(probe.snapshot().input).toEqual([]);
    probe.observe('input'); now = 30; scheduled.shift()!();
    const snapshot = probe.snapshot(); snapshot.input.push(999);
    expect(probe.snapshot().input).toEqual([10]);
  });
});
