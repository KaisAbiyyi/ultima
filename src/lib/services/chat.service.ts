/**
 * Chat Service
 *
 * Handles all chat-related Tauri command invocations
 */

import { safeInvoke, safeInvokeWithFallback } from '$lib/utils/tauri';
import type {
  Chat,
  ChatWithMessages,
  StoredChatMessage,
  ChatId,
  CreateChatRequest,
  UpdateChatRequest,
  AddMessageRequest,
} from '$lib/types';

/**
 * Chat service for CRUD operations
 */
export const chatService = {
  /**
   * Create a new chat
   */
  async create(request: CreateChatRequest): Promise<Chat> {
    // Ensure title is always provided - backend requires it
    const fullRequest = {
      agent_id: request.agent_id,
      title: request.title || 'New Chat',
      project_id: request.project_id,
    };
    // Backend expects { request: CreateChatRequest }
    const result = await safeInvoke<Chat>('create_chat', { request: fullRequest });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Get chat by ID
   */
  async getById(id: ChatId): Promise<Chat | null> {
    return safeInvoke<Chat>('get_chat', { id });
  },

  /**
   * Get chat with all messages
   */
  async getWithMessages(id: ChatId): Promise<ChatWithMessages | null> {
    return safeInvoke<ChatWithMessages>('get_chat_with_messages', { id });
  },

  /**
   * Get all chats
   */
  async getAll(): Promise<Chat[]> {
    return safeInvokeWithFallback<Chat[]>('get_all_chats', undefined, []);
  },

  /**
   * Get chats by agent ID
   */
  async getByAgent(agentId: string): Promise<Chat[]> {
    return safeInvokeWithFallback<Chat[]>('get_chats_by_agent', { agent_id: agentId }, []);
  },

  /**
   * Get chats by project ID
   */
  async getByProject(projectId: string): Promise<Chat[]> {
    return safeInvokeWithFallback<Chat[]>('get_chats_by_project', { project_id: projectId }, []);
  },

  /**
   * Update a chat
   */
  async update(id: ChatId, request: UpdateChatRequest): Promise<Chat> {
    const result = await safeInvoke<Chat>('update_chat', { id, request });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Delete a chat
   */
  async delete(id: ChatId): Promise<boolean> {
    return safeInvokeWithFallback<boolean>('delete_chat', { id }, false);
  },

  /**
   * Add a message to a chat
   */
  async addMessage(chatId: ChatId, request: AddMessageRequest): Promise<StoredChatMessage> {
    // Backend CreateMessageRequest expects: chat_id, role, content, images
    const fullRequest = {
      chat_id: chatId,
      role: request.role,
      content: request.content,
      images: request.images || [],
    };
    // Tauri expects camelCase keys matching Rust parameter names
    const result = await safeInvoke<StoredChatMessage>('add_message', { chatId, request: fullRequest });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Get all messages for a chat
   */
  async getMessages(chatId: ChatId): Promise<StoredChatMessage[]> {
    return safeInvokeWithFallback<StoredChatMessage[]>('get_chat_messages', { chatId }, []);
  },

  /**
   * Validate chat request
   */
  validate(request: CreateChatRequest): string[] {
    const errors: string[] = [];

    if (!request.agent_id?.trim()) {
      errors.push('Agent ID is required');
    }

    return errors;
  },
};
