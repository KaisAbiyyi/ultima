/**
 * Inference type definitions
 */

/** Message role */
export type MessageRole = 'system' | 'user' | 'assistant' | 'tool';

/** Chat message */
export interface ChatMessage {
  role: MessageRole;
  content: string;
  name?: string;
}

/** Chat completion request */
export interface ChatCompletionRequest {
  model: string;
  messages: ChatMessage[];
  temperature?: number;
  top_p?: number;
  max_tokens?: number;
  stream?: boolean;
  stop?: string[];
}

/** Token usage */
export interface TokenUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

/** Chat completion choice */
export interface ChatCompletionChoice {
  index: number;
  message: ChatMessage;
  finish_reason?: string;
}

/** Chat completion response */
export interface ChatCompletionResponse {
  id: string;
  object: string;
  created: number;
  model: string;
  choices: ChatCompletionChoice[];
  usage?: TokenUsage;
}

/** Server status */
export interface ServerStatus {
  running: boolean;
  model_path?: string;
  port: number;
}

/** Helper to create messages */
export const createMessage = {
  system: (content: string): ChatMessage => ({
    role: 'system',
    content,
  }),
  user: (content: string): ChatMessage => ({
    role: 'user',
    content,
  }),
  assistant: (content: string): ChatMessage => ({
    role: 'assistant',
    content,
  }),
};
