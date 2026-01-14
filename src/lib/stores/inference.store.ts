/**
 * Inference Store
 *
 * Reactive state for inference and server status
 */

import { writable, derived } from 'svelte/store';
import type { ServerStatus, ChatMessage } from '$lib/types';
import { inferenceService } from '$lib/services';

/** Store state interface */
interface InferenceState {
  serverStatus: ServerStatus;
  streaming: boolean;
  currentRequestId: string | null;
  streamedContent: string;
  error: string | null;
}

/** Initial state */
const initialState: InferenceState = {
  serverStatus: {
    running: false,
    port: 8080,
  },
  streaming: false,
  currentRequestId: null,
  streamedContent: '',
  error: null,
};

/** Create inference store */
function createInferenceStore() {
  const { subscribe, set, update } = writable<InferenceState>(initialState);

  return {
    subscribe,

    /** Start local server */
    async startServer(modelPath: string): Promise<boolean> {
      update((s) => ({ ...s, error: null }));
      try {
        await inferenceService.startServer(modelPath);
        update((s) => ({
          ...s,
          serverStatus: { ...s.serverStatus, running: true, model_path: modelPath },
        }));
        return true;
      } catch (err) {
        update((s) => ({
          ...s,
          error: err instanceof Error ? err.message : 'Failed to start server',
        }));
        return false;
      }
    },

    /** Stop local server */
    async stopServer(): Promise<boolean> {
      update((s) => ({ ...s, error: null }));
      try {
        await inferenceService.stopServer();
        update((s) => ({
          ...s,
          serverStatus: { running: false, port: s.serverStatus.port },
        }));
        return true;
      } catch (err) {
        update((s) => ({
          ...s,
          error: err instanceof Error ? err.message : 'Failed to stop server',
        }));
        return false;
      }
    },

    /** Update server status */
    updateServerStatus(status: Partial<ServerStatus>): void {
      update((s) => ({
        ...s,
        serverStatus: { ...s.serverStatus, ...status },
      }));
    },

    /** Start streaming */
    startStreaming(requestId: string): void {
      update((s) => ({
        ...s,
        streaming: true,
        currentRequestId: requestId,
        streamedContent: '',
        error: null,
      }));
    },

    /** Append token */
    appendToken(token: string): void {
      update((s) => ({
        ...s,
        streamedContent: s.streamedContent + token,
      }));
    },

    /** Complete streaming */
    completeStreaming(): void {
      update((s) => ({
        ...s,
        streaming: false,
        currentRequestId: null,
      }));
    },

    /** Set error */
    setError(error: string): void {
      update((s) => ({
        ...s,
        streaming: false,
        currentRequestId: null,
        error,
      }));
    },

    /** Clear error */
    clearError(): void {
      update((s) => ({ ...s, error: null }));
    },

    /** Reset store */
    reset(): void {
      set(initialState);
    },
  };
}

/** Inference store singleton */
export const inferenceStore = createInferenceStore();

/** Derived: is server running */
export const isServerRunning = derived(
  inferenceStore,
  ($store) => $store.serverStatus.running
);

/** Derived: is streaming */
export const isStreaming = derived(
  inferenceStore,
  ($store) => $store.streaming
);
