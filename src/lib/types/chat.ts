/**
 * Chat type definitions
 *
 * Matches Rust Chat and ChatMessage models
 */

import type { MessageRole } from './inference';

/** Strongly-typed ID types */
export type ChatId = string;
export type MessageId = string;

/** Chat message with metadata */
export interface StoredChatMessage {
  id: MessageId;
  chat_id: ChatId;
  role: MessageRole;
  content: string;
  images: string[];
  created_at: string;
  usage?: MessageUsage;
}

/** Message token usage */
export interface MessageUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

/** Chat entity */
export interface Chat {
  id: ChatId;
  title: string;
  agent_id: string;
  project_id?: string;
  is_pinned: boolean;
  archived: boolean;
  created_at: string;
  updated_at: string;
}

/** Chat with all its messages */
export interface ChatWithMessages {
  chat: Chat;
  messages: StoredChatMessage[];
}

/** Chat summary for list views */
export interface ChatSummary {
  id: ChatId;
  title: string;
  agent_id: string;
  project_id?: string;
  is_pinned: boolean;
  archived: boolean;
  message_count: number;
  last_message: string;
  updated_at: string;
}

/** Request to create a new chat */
export interface CreateChatRequest {
  agent_id: string;
  project_id?: string;
  title?: string;
}

/** Request to update a chat */
export interface UpdateChatRequest {
  title?: string;
  project_id?: string;
  is_pinned?: boolean;
  archived?: boolean;
}

/** Request to add a message to a chat */
export interface AddMessageRequest {
  role: MessageRole;
  content: string;
  images?: string[];
}
