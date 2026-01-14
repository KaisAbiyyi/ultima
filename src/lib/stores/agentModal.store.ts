/**
 * Agent Modal Store
 * 
 * Global state for agent creation/edit modal
 */

import { writable } from 'svelte/store';
import type { Agent } from '$lib/types';

interface AgentModalState {
  open: boolean;
  editAgent: Agent | null;
  loading: boolean;
}

const initialState: AgentModalState = {
  open: false,
  editAgent: null,
  loading: false,
};

function createAgentModalStore() {
  const { subscribe, set, update } = writable<AgentModalState>(initialState);

  return {
    subscribe,
    
    /** Open modal for creating new agent */
    openCreate: () => update(s => ({ ...s, open: true, editAgent: null })),
    
    /** Open modal for editing existing agent */
    openEdit: (agent: Agent) => update(s => ({ ...s, open: true, editAgent: agent })),
    
    /** Close modal */
    close: () => update(s => ({ ...s, open: false, editAgent: null, loading: false })),
    
    /** Set loading state */
    setLoading: (loading: boolean) => update(s => ({ ...s, loading })),
    
    /** Reset store */
    reset: () => set(initialState),
  };
}

export const agentModalStore = createAgentModalStore();
