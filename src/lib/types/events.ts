/**
 * Event type definitions
 *
 * Matches Rust event payloads
 */

/** Stream token event payload */
export interface StreamTokenPayload {
  request_id: string;
  agent_id: string;
  token: string;
  index: number;
}

/** Stream complete event payload */
export interface StreamCompletePayload {
  request_id: string;
  agent_id: string;
  content: string;
  finish_reason?: string;
  usage?: {
    prompt_tokens: number;
    completion_tokens: number;
    total_tokens: number;
  };
}

/** Stream error event payload */
export interface StreamErrorPayload {
  request_id: string;
  agent_id: string;
  code: string;
  message: string;
  retryable: boolean;
}

/** Sidecar status */
export type SidecarStatus = 'starting' | 'running' | 'stopped' | 'error';

/** Sidecar status event payload */
export interface SidecarStatusPayload {
  status: SidecarStatus;
  model_path?: string;
  pid?: number;
  error?: string;
  port: number;
}

/** Event names */
export const EVENTS = {
  STREAM_TOKEN: 'inference:stream-token',
  STREAM_COMPLETE: 'inference:stream-complete',
  STREAM_ERROR: 'inference:stream-error',
  SIDECAR_STATUS: 'inference:sidecar-status',
} as const;
