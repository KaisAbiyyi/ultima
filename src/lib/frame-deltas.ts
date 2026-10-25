/**
 * Coalesce transport deltas into at most one UI update per animation frame.
 * A hard cap turns transport overload into an explicit failure, never data loss.
 * The scheduler is injected so lifecycle and pressure can be tested headlessly.
 */
export const MAX_FRAME_DELTA_CHARS = 128 * 1024;
export class FrameDeltas {
  private chunks: string[] = [];
  private chars = 0;
  private frame: number | null = null;
  constructor(
    private readonly schedule: (cb: () => void) => number,
    private readonly cancel: (id: number) => void,
    private readonly deliver: (text: string) => void
  ) {}
  get queuedChars(): number { return this.chars; }
  push(delta: string): boolean {
    if (!delta) return true;
    if (delta.length > MAX_FRAME_DELTA_CHARS - this.chars) return false;
    this.chunks.push(delta);
    this.chars += delta.length;
    if (this.frame === null) {
      this.frame = this.schedule(() => {
        this.frame = null;
        this.drain();
      });
    }
    return true;
  }
  drain(): void {
    if (this.frame !== null) {
      this.cancel(this.frame);
      this.frame = null;
    }
    if (!this.chars) return;
    const chunks = this.chunks;
    this.chunks = [];
    this.chars = 0;
    this.deliver(chunks.join(''));
  }
  clear(): void {
    if (this.frame !== null) this.cancel(this.frame);
    this.frame = null;
    this.chunks = [];
    this.chars = 0;
  }
}
