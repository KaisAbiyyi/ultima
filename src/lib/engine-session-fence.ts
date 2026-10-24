/** NC-06 transport generation fence. Prevents late IPC from mutating a newer session. */
export class EngineSessionFence<T extends { session: number }> {
  private active: number | null = null;
  private connecting = false;
  private early: T[] = [];
  private readonly maxEarly = 64;

  get session(): number | null { return this.active; }

  beginConnect(): void {
    this.active = null;
    this.connecting = true;
    this.early = [];
  }
  /** Delivers only events from the newly acknowledged generation. */
  connected(session: number): T[] {
    if (!Number.isSafeInteger(session) || session < 1 || !this.connecting) {
      throw new Error('Invalid engine session');
    }
    this.active = session;
    this.connecting = false;
    const accepted = this.early.filter(event => event.session === session);
    this.early = [];
    return accepted;
  }
  beginDisconnect(): void {
    this.active = null;
    this.connecting = false;
    this.early = [];
  }
  accept(event: T): boolean {
    if (this.active !== null) return event.session === this.active;
    if (this.connecting && this.early.length < this.maxEarly) this.early.push(event);
    return false;
  }
}
