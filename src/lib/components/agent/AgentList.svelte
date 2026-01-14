<!--
  Agent List Component

  Grid list of agent cards
-->
<script lang="ts">
  import { Button } from 'flowbite-svelte';
  import { DesktopPcOutline, PlusOutline } from 'flowbite-svelte-icons';
  import AgentCard from './AgentCard.svelte';
  import type { Agent } from '$lib/types';

  type Props = {
    agents?: Agent[];
    loading?: boolean;
    emptyMessage?: string;
    onEdit?: (agent: Agent) => void;
    onDelete?: (agent: Agent) => void;
    onChat?: (agent: Agent) => void;
    onCreate?: () => void;
  };

  let { 
    agents = [], 
    loading = false, 
    emptyMessage = 'No agents found',
    onEdit,
    onDelete,
    onChat,
    onCreate
  }: Props = $props();
</script>

<div class="space-y-4">
  {#if loading}
    <!-- Loading skeleton -->
    <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {#each Array(3) as _}
        <div class="h-48 rounded-lg bg-gray-200 dark:bg-gray-700 animate-pulse"></div>
      {/each}
    </div>
  {:else if agents.length === 0}
    <!-- Empty state -->
    <div class="flex flex-col items-center justify-center py-12 text-center">
      <DesktopPcOutline class="w-12 h-12 text-gray-400 dark:text-gray-500 mb-4" />
      <h3 class="text-lg font-medium text-gray-900 dark:text-white">{emptyMessage}</h3>
      <p class="text-sm text-gray-500 dark:text-gray-400 mt-1 mb-4">
        Create your first AI agent to get started
      </p>
      <Button onclick={() => onCreate?.()}>
        <PlusOutline class="w-4 h-4 mr-2" />
        Create Agent
      </Button>
    </div>
  {:else}
    <!-- Agent grid -->
    <div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {#each agents as agent (agent.id)}
        <AgentCard
          {agent}
          onEdit={() => onEdit?.(agent)}
          onDelete={() => onDelete?.(agent)}
          onChat={() => onChat?.(agent)}
        />
      {/each}
    </div>
  {/if}
</div>
