/** NC-10 numeric-only observations. Inactive until an explicit local probe start. */
export type TimingKind = 'input' | 'stream';
export class NativePerformance {
  private enabled = false;
  private generation = 0;
  private pending = 0;
  private dropped = 0;
  private values: Record<TimingKind, number[]> = { input: [], stream: [] };
  constructor(
    private now: () => number,
    private afterPaint: (callback: () => void) => void,
    private capacity = 4096
  ) {
    if (!Number.isInteger(capacity) || capacity < 1 || capacity > 4096) throw Error('Invalid timing capacity');
  }
  start() {
    this.generation++;
    this.enabled = true;
    this.pending = 0;
    this.dropped = 0;
    this.values = { input: [], stream: [] };
  }
  stop() {
    this.start();
    this.enabled = false;
  }
  observe(kind: TimingKind) {
    if (!this.enabled) return;
    if (this.pending >= this.capacity || this.values[kind].length + this.pending >= this.capacity) {
      this.dropped++;
      return;
    }
    const begin = this.now();
    const generation = this.generation;
    this.pending++;
    this.afterPaint(() => {
      if (generation !== this.generation || !this.enabled) return;
      this.pending--;
      const elapsed = this.now() - begin;
      if (Number.isFinite(elapsed) && elapsed >= 0) this.values[kind].push(elapsed);
      else this.dropped++;
    });
  }
  snapshot() {
    return { boundary: 'flush-two-raf-proxy' as const, enabled: this.enabled, dropped: this.dropped,
      pending: this.pending, input: [...this.values.input], stream: [...this.values.stream] };
  }
}
