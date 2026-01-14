/**
 * Chat Store
 *
 * Reactive state management for chats with SOLID principles
 */

import { writable, derived, get } from 'svelte/store';
import type {
  Chat,
  ChatId,
  CreateChatRequest,
  UpdateChatRequest,
  AddMessageRequest,
  StoredChatMessage,
} from '$lib/types';
import { chatService } from '$lib/services';
import { DRAFT_CHAT_ID, isDraftChat } from '$lib/constants';

/** Store state interface */
interface ChatState {
  chats: Chat[];
  currentChatId: ChatId | null;
  messages: StoredChatMessage[];
  loading: boolean;
  messagesLoading: boolean;
  error: string | null;
  selectingAgent: boolean; // True when user clicked "New Chat" and needs to pick agent
  draftAgentId: string | null; // Agent ID for the draft chat
  draftProjectId: string | null; // Project ID for the draft chat
}

/** Initial state */
const initialState: ChatState = {
  chats: [],
  currentChatId: null,
  messages: [],
  loading: false,
  messagesLoading: false,
  error: null,
  selectingAgent: false,
  draftAgentId: null,
  draftProjectId: null,
};

/** Create the chat store */
function createChatStore() {
  const { subscribe, set, update } = writable<ChatState>(initialState);

  return {
    subscribe,

    // ... existing load methods ...

    /** Load all chats */
    async load(): Promise<void> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const chats = await chatService.getAll();
        update((s) => ({ ...s, chats, loading: false }));
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to load chats',
        }));
      }
    },

    // ... other load methods unchanged ...

    /** Load chats by agent */
    async loadByAgent(agentId: string): Promise<void> {
        // ... unchanged ...
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const chats = await chatService.getByAgent(agentId);
        update((s) => ({ ...s, chats, loading: false }));
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to load chats',
        }));
      }
    },

    /** Load chats by project */
    async loadByProject(projectId: string): Promise<void> {
        // ... unchanged ...
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const chats = await chatService.getByProject(projectId);
        update((s) => ({ ...s, chats, loading: false }));
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to load chats',
        }));
      }
    },


    /** Load messages for a chat */
    async loadMessages(chatId: ChatId): Promise<void> {
      if (isDraftChat(chatId)) {
        update((s) => ({ ...s, messages: [], messagesLoading: false }));
        return;
      }
      update((s) => ({ ...s, messagesLoading: true }));
      try {
        const messages = await chatService.getMessages(chatId);
        update((s) => ({ ...s, messages, messagesLoading: false }));
      } catch (err) {
        console.error('Failed to load messages:', err);
        update((s) => ({ ...s, messages: [], messagesLoading: false }));
      }
    },

    /** Create a new chat */
    async create(request: CreateChatRequest): Promise<Chat | null> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const chat = await chatService.create(request);
        update((s) => ({
          ...s,
          chats: [chat, ...s.chats],
          currentChatId: chat.id,
          messages: [], // New chat has no messages
          loading: false,
          draftAgentId: null, // Clear draft
        }));
        return chat;
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to create chat',
        }));
        return null;
      }
    },

    // ... update, delete, addMessage unchanged ...
    /** Update a chat */
    async update(id: ChatId, request: UpdateChatRequest): Promise<Chat | null> {
        // ... unchanged ...
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const chat = await chatService.update(id, request);
        update((s) => ({
          ...s,
          chats: s.chats.map((c) => (c.id === id ? chat : c)),
          loading: false,
        }));
        return chat;
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to update chat',
        }));
        return null;
      }
    },

    /** Delete a chat */
    async delete(id: ChatId): Promise<boolean> {
        // ... unchanged ...
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const deleted = await chatService.delete(id);
        if (deleted) {
          update((s) => ({
            ...s,
            chats: s.chats.filter((c) => c.id !== id),
            currentChatId: s.currentChatId === id ? null : s.currentChatId,
            messages: s.currentChatId === id ? [] : s.messages,
            loading: false,
          }));
          return true;
        }
        update((s) => ({ ...s, loading: false }));
        return false;
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to delete chat',
        }));
        return false;
      }
    },

    /** Add a message to a chat */
    async addMessage(chatId: ChatId, request: AddMessageRequest): Promise<StoredChatMessage | null> {
        // ... unchanged ...
      try {
        const message = await chatService.addMessage(chatId, request);
        update((s) => {
          // Add message to current messages if this is current chat
          const newMessages = s.currentChatId === chatId 
            ? [...s.messages, message]
            : s.messages;
          
          // Move chat to top of list
          const chats = [...s.chats];
          const idx = chats.findIndex((c) => c.id === chatId);
          if (idx !== -1) {
            const [chat] = chats.splice(idx, 1);
            chats.unshift(chat);
          }
          
          return { ...s, chats, messages: newMessages };
        });
        return message;
      } catch (err) {
        update((s) => ({
          ...s,
          error: err instanceof Error ? err.message : 'Failed to add message',
        }));
        return null;
      }
    },

    // ... addMessageLocal, updateLastMessage ...
    /** Add a message locally (for streaming) without API call */
    addMessageLocal(message: StoredChatMessage): void {
      update((s) => ({
        ...s,
        messages: [...s.messages, message],
      }));
    },

    /** Update the last message (for streaming content) */
    updateLastMessage(content: string): void {
      update((s) => {
        if (s.messages.length === 0) return s;
        const messages = [...s.messages];
        const last = messages[messages.length - 1];
        messages[messages.length - 1] = { ...last, content };
        return { ...s, messages };
      });
    },

    /** Select a chat and load its messages */
    async select(id: ChatId | null): Promise<void> {
      update((s) => ({ ...s, currentChatId: id, messages: [], selectingAgent: false, draftAgentId: null }));
      if (id && !isDraftChat(id)) {
        await this.loadMessages(id);
      }
    },
    
    // ... getCurrentChatId, clearError ...
    /** Get current chat ID */
    getCurrentChatId(): ChatId | null {
      return get({ subscribe }).currentChatId;
    },

    /** Clear error */
    clearError(): void {
      update((s) => ({ ...s, error: null }));
    },

    /** Start new chat flow - show agent selection */
    startNewChat(): void {
      update((s) => ({ ...s, selectingAgent: true, currentChatId: null, messages: [], draftAgentId: null }));
    },

    /** Prepare a draft chat (server starts, but not DB record) */
    prepareNewChat(agentId: string, projectId: string | null = null): void {
      update((s) => ({ 
        ...s, 
        selectingAgent: false, 
        currentChatId: DRAFT_CHAT_ID, 
        draftAgentId: agentId,
        draftProjectId: projectId,
        messages: [] 
      }));
    },

    /** Cancel new chat - exit agent selection mode */
    cancelNewChat(): void {
      update((s) => ({ ...s, selectingAgent: false, draftAgentId: null, draftProjectId: null, currentChatId: isDraftChat(s.currentChatId) ? null : s.currentChatId }));
    },

    // ... aliases ...
    /** Update a chat (alias for update) */
    async updateChat(id: ChatId, request: UpdateChatRequest): Promise<Chat | null> {
      return this.update(id, request);
    },

    /** Reset store */
    reset(): void {
      set(initialState);
    },
  };
}

/** Chat store singleton */
export const chatStore = createChatStore();

/** Derived: current chat */
export const currentChat = derived(chatStore, ($store) => {
  if (isDraftChat($store.currentChatId) && $store.draftAgentId) {
    // Return a mock chat object for the draft
    return {
      id: DRAFT_CHAT_ID,
      agent_id: $store.draftAgentId,
      project_id: $store.draftProjectId,
      title: 'New Chat',
      is_pinned: false,
      archived: false,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    } as Chat;
  }
  return $store.chats.find((c) => c.id === $store.currentChatId) ?? null;
});

/** Derived: current chat ID */
export const currentChatId = derived(chatStore, ($store) => $store.currentChatId);

/** Derived: current chat messages */
export const currentChatMessages = derived(chatStore, ($store) => $store.messages);

/** Derived: messages loading state */
export const messagesLoading = derived(chatStore, ($store) => $store.messagesLoading);

/** Derived: selecting agent mode */
export const selectingAgent = derived(chatStore, ($store) => $store.selectingAgent);

/** Derived: active chats (not archived) */
export const activeChats = derived(chatStore, ($store) =>
  $store.chats.filter((c) => !c.archived)
);

/** Derived: pinned chats */
export const pinnedChats = derived(chatStore, ($store) =>
  $store.chats.filter((c) => c.is_pinned && !c.archived)
);

/** Derived: archived chats */
export const archivedChats = derived(chatStore, ($store) =>
  $store.chats.filter((c) => c.archived)
);

/** Derived: chats by project */
export function chatsByProject(projectId: string) {
  return derived(chatStore, ($store) =>
    $store.chats.filter((c) => c.project_id === projectId && !c.archived)
  );
}

/** Derived: chats by agent */
export function chatsByAgent(agentId: string) {
  return derived(chatStore, ($store) =>
    $store.chats.filter((c) => c.agent_id === agentId && !c.archived)
  );
}
