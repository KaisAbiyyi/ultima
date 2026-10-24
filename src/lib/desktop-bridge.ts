/** NC-06 thin Tauri bridge: all permission/process policy lives in Rust. */
export type EngineEvent = { session: number } & (
  | { type: 'status'; state: 'connecting' | 'ready' | 'error' | 'disconnected'; message?: string }
  | { type: 'reply'; id: number; method: string; ok: boolean; result: unknown }
  | { type: 'threadStarted' | 'turnStarted' | 'turnCompleted' | 'itemStarted' | 'itemCompleted' | 'delta' | 'requestResolved'; params: Record<string, unknown> }
  | { type: 'approvalBlocked'; kind: string }
);

export type ManagedRuntimeStatus = {
  mode: 'system' | 'managed';
  version: string | null;
  managedSupported: boolean;
  pinnedVersion: string | null;
  assetUrl: string | null;
};

export interface EngineBridge {
  connect(executable: string, project: string): Promise<number>;
  call(method: 'model/list' | 'thread/start' | 'thread/resume' | 'turn/start' | 'turn/interrupt', params: Record<string, unknown>): Promise<number>;
  disconnect(): Promise<void>;
  runtimeStatus(): Promise<ManagedRuntimeStatus>;
  runtimeStage(source: string): Promise<void>;
  runtimeActivate(project: string): Promise<void>;
  runtimeSystem(): Promise<void>;
  runtimeRollback(): Promise<void>;
  listen(listener: (event: EngineEvent) => void): Promise<() => void>;
}
export function desktopAvailable(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
export async function nativeBridge(): Promise<EngineBridge> {
  if (!desktopAvailable()) throw Error('Start the Ultima Tauri desktop app to connect to Codex.');
  const [{ invoke }, { listen }] = await Promise.all([
    import('@tauri-apps/api/core'), import('@tauri-apps/api/event')
  ]);
  return {
    connect: (executable, project) => invoke<number>('ultima_connect', { executable, project }),
    call: (method, params) => invoke<number>('ultima_call', { method, params }),
    disconnect: () => invoke<void>('ultima_disconnect'),
    runtimeStatus: () => invoke<ManagedRuntimeStatus>('ultima_runtime_status'),
    runtimeStage: (source) => invoke<void>('ultima_runtime_stage', { source }),
    runtimeActivate: (project) => invoke<void>('ultima_runtime_activate', { project }),
    runtimeSystem: () => invoke<void>('ultima_runtime_system'),
    runtimeRollback: () => invoke<void>('ultima_runtime_rollback'),
    listen: async (handler) => {
      const unlisten = await listen<EngineEvent>('ultima:engine', (event) => handler(event.payload));
      return () => unlisten();
    }
  };
}
