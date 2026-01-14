/**
 * Application Constants
 * 
 * Centralized constants to eliminate magic strings/numbers and improve maintainability.
 */

// === Chat Constants ===

/** Special ID used for draft chats that haven't been persisted yet */
export const DRAFT_CHAT_ID = 'draft' as const;

/** Type for the draft chat ID */
export type DraftChatId = typeof DRAFT_CHAT_ID;

/** Check if a chat ID represents a draft */
export function isDraftChat(chatId: string | null): chatId is DraftChatId {
  return chatId === DRAFT_CHAT_ID;
}

// === Server Constants ===

/** Default port for the local llama-server */
export const DEFAULT_LLAMA_SERVER_PORT = 6661;

/** Default context window size in tokens */
export const DEFAULT_CONTEXT_WINDOW = 4096;

/** Minimum context window size */
export const MIN_CONTEXT_WINDOW = 128;

/** Server health check timeout in milliseconds */
export const SERVER_HEALTH_TIMEOUT_MS = 60000;

/** Server health check interval in milliseconds */
export const SERVER_HEALTH_INTERVAL_MS = 1000;

// === API Endpoints ===

/** Local llama-server base URL */
export const LOCAL_SERVER_BASE_URL = `http://127.0.0.1:${DEFAULT_LLAMA_SERVER_PORT}`;

/** Local llama-server chat completions endpoint */
export const LOCAL_CHAT_COMPLETIONS_URL = `${LOCAL_SERVER_BASE_URL}/v1/chat/completions`;

/** Local llama-server health endpoint */
export const LOCAL_HEALTH_URL = `${LOCAL_SERVER_BASE_URL}/health`;

// === Provider Identifiers ===

/** Provider type identifiers matching backend */
export const PROVIDERS = {
  LLAMA_SERVER: 'llama_server',
  OLLAMA: 'ollama',
  OPEN_ROUTER: 'open_router',
  OPEN_AI: 'open_ai',
  ANTHROPIC: 'anthropic',
  CUSTOM: 'custom',
} as const;

/** List of local providers that don't need API keys */
export const LOCAL_PROVIDERS = [
  PROVIDERS.LLAMA_SERVER,
  PROVIDERS.OLLAMA,
] as const;

/** Check if a provider is local (doesn't need API key) */
export function isLocalProvider(provider: string): boolean {
  return LOCAL_PROVIDERS.includes(provider as any);
}

// === UI Constants ===

/** Maximum length for chat title before truncation */
export const MAX_CHAT_TITLE_LENGTH = 50;

/** Number of server ready check attempts */
export const MAX_SERVER_READY_ATTEMPTS = 60;

// === Validation ===

/** Agent name max length */
export const MAX_AGENT_NAME_LENGTH = 100;

/** Minimum context window for agents */
export const MIN_AGENT_CONTEXT_WINDOW = 128;
