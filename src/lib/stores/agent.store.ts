/**
 * Agent Store
 *
 * Reactive state management for agents
 */

import { writable, derived } from 'svelte/store';
import type { Agent, CreateAgentRequest, UpdateAgentRequest } from '$lib/types';
import { agentService } from '$lib/services';

/** Store state interface */
interface AgentState {
  agents: Agent[];
  selectedId: string | null;
  loading: boolean;
  error: string | null;
}

/** Initial state */
const initialState: AgentState = {
  agents: [],
  selectedId: null,
  loading: false,
  error: null,
};

/** Create the agent store */
function createAgentStore() {
  const { subscribe, set, update } = writable<AgentState>(initialState);

  return {
    subscribe,

    /** Load all agents */
    async load(): Promise<void> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const agents = await agentService.getAll();
        update((s) => ({ ...s, agents, loading: false }));
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to load agents',
        }));
      }
    },

    /** Create a new agent */
    async create(request: CreateAgentRequest): Promise<Agent | null> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const agent = await agentService.create(request);
        update((s) => ({
          ...s,
          agents: [...s.agents, agent],
          loading: false,
        }));
        return agent;
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to create agent',
        }));
        return null;
      }
    },

    /** Update an agent */
    async update(id: string, request: UpdateAgentRequest): Promise<Agent | null> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        const agent = await agentService.update(id, request);
        update((s) => ({
          ...s,
          agents: s.agents.map((a) => (a.id === id ? agent : a)),
          loading: false,
        }));
        return agent;
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to update agent',
        }));
        return null;
      }
    },

    /** Delete an agent */
    async delete(id: string): Promise<boolean> {
      update((s) => ({ ...s, loading: true, error: null }));
      try {
        await agentService.delete(id);
        update((s) => ({
          ...s,
          agents: s.agents.filter((a) => a.id !== id),
          selectedId: s.selectedId === id ? null : s.selectedId,
          loading: false,
        }));
        return true;
      } catch (err) {
        update((s) => ({
          ...s,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to delete agent',
        }));
        return false;
      }
    },

    /** Select an agent */
    select(id: string | null): void {
      update((s) => ({ ...s, selectedId: id }));
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

/** Agent store singleton */
export const agentStore = createAgentStore();

/** Derived: selected agent */
export const selectedAgent = derived(agentStore, ($store) =>
  $store.agents.find((a) => a.id === $store.selectedId) ?? null
);

/** Derived: local agents (llama_server) */
export const localAgents = derived(agentStore, ($store) =>
  $store.agents.filter((a) => a.provider === 'llama_server')
);

/** Derived: external agents */
export const externalAgents = derived(agentStore, ($store) =>
  $store.agents.filter((a) => a.provider !== 'llama_server' && a.provider !== 'ollama')
);

/** Derived: aggregator agents */
export const aggregatorAgents = derived(agentStore, ($store) =>
  $store.agents.filter((a) => a.is_aggregator)
);
