/**
 * Agent Service
 *
 * Handles all agent-related Tauri command invocations
 */

import { safeInvoke, safeInvokeWithFallback, isTauri } from '$lib/utils/tauri';
import type {
  Agent,
  CreateAgentRequest,
  UpdateAgentRequest
} from '$lib/types';

/**
 * Agent service for CRUD operations
 */
export const agentService = {
  /**
   * Create a new agent
   */
  async create(request: CreateAgentRequest): Promise<Agent> {
    const result = await safeInvoke<Agent>('create_agent', { request });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Get all agents
   */
  async getAll(): Promise<Agent[]> {
    return safeInvokeWithFallback<Agent[]>('get_agents', undefined, []);
  },

  /**
   * Get agent by ID
   */
  async getById(id: string): Promise<Agent> {
    const result = await safeInvoke<Agent>('get_agent', { id });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Update an agent
   */
  async update(id: string, request: UpdateAgentRequest): Promise<Agent> {
    const result = await safeInvoke<Agent>('update_agent', { id, request });
    if (!result) {
      throw new Error('Tauri not available');
    }
    return result;
  },

  /**
   * Delete an agent
   */
  async delete(id: string): Promise<void> {
    await safeInvoke<void>('delete_agent', { id });
  },

  /**
   * Duplicate an agent
   */
  async duplicate(id: string, newName: string): Promise<Agent> {
    const original = await this.getById(id);
    return this.create({
      name: newName,
      system_prompt: original.system_prompt,
      provider: original.provider,
      model_path: original.model_path,
      model_id: original.model_id,
      api_key: original.api_key,
      api_endpoint: original.api_endpoint,
      context_window: original.context_window,
      is_aggregator: original.is_aggregator,
    });
  },

  /**
   * Validate agent configuration
   */
  validate(request: CreateAgentRequest): string[] {
    const errors: string[] = [];

    if (!request.name?.trim()) {
      errors.push('Name is required');
    }

    if (!request.provider) {
      errors.push('Provider is required');
    }

    // Local providers that don't need API keys
    const localProviders = ['llama_server', 'local', 'ollama'];
    const isLocalProvider = localProviders.includes(request.provider);

    if (request.provider === 'llama_server' && !request.model_path) {
      errors.push('Model path is required for llama_server provider');
    }

    // Only external API providers need API key and model ID
    if (!isLocalProvider) {
      if (!request.api_key) {
        errors.push('API key is required for external providers');
      }
      if (!request.model_id) {
        errors.push('Model ID is required for external providers');
      }
    }

    return errors;
  },
};
