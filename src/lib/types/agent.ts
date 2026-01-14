/**
 * Agent type definitions
 *
 * Matches Rust Agent model
 */

/** AI Provider types */
export type AgentProvider =
  | 'llama_server'
  | 'ollama'
  | 'open_router'
  | 'open_ai'
  | 'anthropic'
  | 'custom';

/** Agent entity */
export interface Agent {
  id: string;
  name: string;
  system_prompt: string;
  provider: AgentProvider;
  model_path?: string;
  model_id?: string;
  mmproj_path?: string;
  api_key?: string;
  api_endpoint?: string;
  context_window: number;
  is_aggregator: boolean;
  created_at: string;
  updated_at: string;
}

/** Request to create an agent */
export interface CreateAgentRequest {
  name: string;
  system_prompt: string;
  provider: AgentProvider;
  model_path?: string;
  model_id?: string;
  mmproj_path?: string;
  api_key?: string;
  api_endpoint?: string;
  context_window?: number;
  is_aggregator?: boolean;
}

/** Request to update an agent */
export interface UpdateAgentRequest {
  name?: string;
  system_prompt?: string;
  provider?: AgentProvider;
  model_path?: string;
  model_id?: string;
  mmproj_path?: string;
  api_key?: string;
  api_endpoint?: string;
  context_window?: number;
  is_aggregator?: boolean;
}

/** Model info from provider API */
export interface ModelInfo {
  id: string;
  name: string;
  provider: AgentProvider;
  context_length?: number;
  is_free?: boolean;
}

/** Provider option for dropdowns */
export interface ProviderOption {
  value: AgentProvider;
  label: string;
  requiresApiKey: boolean;
  requiresModelPath: boolean;
  defaultEndpoint?: string;
  supportsModelList: boolean;
}

/** Provider options list */
export const PROVIDER_OPTIONS: ProviderOption[] = [
  {
    value: 'llama_server',
    label: 'Llama Server',
    requiresApiKey: false,
    requiresModelPath: true,
    supportsModelList: true,
  },
  {
    value: 'ollama',
    label: 'Ollama',
    requiresApiKey: false,
    requiresModelPath: false,
    defaultEndpoint: 'http://localhost:11434',
    supportsModelList: true,
  },
  {
    value: 'open_ai',
    label: 'OpenAI',
    requiresApiKey: true,
    requiresModelPath: false,
    defaultEndpoint: 'https://api.openai.com/v1',
    supportsModelList: true,
  },
  {
    value: 'open_router',
    label: 'OpenRouter',
    requiresApiKey: true,
    requiresModelPath: false,
    defaultEndpoint: 'https://openrouter.ai/api/v1',
    supportsModelList: true,
  },
  {
    value: 'anthropic',
    label: 'Anthropic',
    requiresApiKey: true,
    requiresModelPath: false,
    defaultEndpoint: 'https://api.anthropic.com/v1',
    supportsModelList: true,
  },
  {
    value: 'custom',
    label: 'Custom API',
    requiresApiKey: true,
    requiresModelPath: false,
    supportsModelList: false,
  },
];

/** Get provider option by value */
export function getProviderOption(provider: AgentProvider): ProviderOption {
  return PROVIDER_OPTIONS.find(p => p.value === provider) ?? PROVIDER_OPTIONS[0];
}
