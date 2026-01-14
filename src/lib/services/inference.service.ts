/**
 * Inference Service
 *
 * Handles inference-related Tauri command invocations
 */

import { safeInvoke, isTauri } from '$lib/utils/tauri';
import type { UnlistenFn } from '@tauri-apps/api/event';
import type {
  ChatMessage,
  ChatCompletionResponse,
  ServerStatus,
  StreamTokenPayload,
  StreamCompletePayload,
  StreamErrorPayload,
  SidecarStatusPayload,
} from '$lib/types';
import { EVENTS } from '$lib/types';

/**
 * Inference service
 */
export const inferenceService = {
  /**
   * Non-streaming chat completion
   */
  async chat(agentId: string, messages: ChatMessage[]): Promise<ChatCompletionResponse> {
    const result = await safeInvoke<ChatCompletionResponse>('chat_completion', {
      agentId,
      messages,
    });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Streaming chat completion
   */
  async chatStream(
    agentId: string,
    messages: ChatMessage[],
    callbacks: {
      onToken?: (payload: StreamTokenPayload) => void;
      onComplete?: (payload: StreamCompletePayload) => void;
      onError?: (payload: StreamErrorPayload) => void;
    }
  ): Promise<{ requestId: string; unsubscribe: () => void }> {
    if (!isTauri()) {
      throw new Error('Streaming requires Tauri environment');
    }
    
    const { listen } = await import('@tauri-apps/api/event');
    const { invoke } = await import('@tauri-apps/api/core');
    
    const unlisteners: UnlistenFn[] = [];

    // Subscribe to events
    if (callbacks.onToken) {
      const unlisten = await listen<StreamTokenPayload>(
        EVENTS.STREAM_TOKEN,
        (event) => callbacks.onToken?.(event.payload)
      );
      unlisteners.push(unlisten);
    }

    if (callbacks.onComplete) {
      const unlisten = await listen<StreamCompletePayload>(
        EVENTS.STREAM_COMPLETE,
        (event) => callbacks.onComplete?.(event.payload)
      );
      unlisteners.push(unlisten);
    }

    if (callbacks.onError) {
      const unlisten = await listen<StreamErrorPayload>(
        EVENTS.STREAM_ERROR,
        (event) => callbacks.onError?.(event.payload)
      );
      unlisteners.push(unlisten);
    }

    // Start streaming
    const requestId = await invoke<string>('chat_completion_stream', {
      agentId,
      messages,
    });

    return {
      requestId,
      unsubscribe: () => unlisteners.forEach((fn) => fn()),
    };
  },

  /**
   * Quick chat helper
   */
  async quickChat(agentId: string, userMessage: string): Promise<string> {
    const result = await safeInvoke<string>('quick_chat', {
      agentId,
      userMessage,
    });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Start local server
   */
  async startServer(modelPath: string): Promise<void> {
    await safeInvoke<void>('start_local_server', { modelPath });
  },

  /**
   * Stop local server
   */
  async stopServer(): Promise<void> {
    await safeInvoke<void>('stop_local_server');
  },

  /**
   * Get server status
   */
  async getServerStatus(): Promise<ServerStatus> {
    const result = await safeInvoke<ServerStatus>('get_server_status');
    return result ?? { running: false, port: 0 };
  },

  /**
   * Listen to sidecar status
   */
  async onSidecarStatus(
    callback: (payload: SidecarStatusPayload) => void
  ): Promise<UnlistenFn | null> {
    if (!isTauri()) return null;
    
    const { listen } = await import('@tauri-apps/api/event');
    return listen<SidecarStatusPayload>(
      EVENTS.SIDECAR_STATUS,
      (event) => callback(event.payload)
    );
  },
};
