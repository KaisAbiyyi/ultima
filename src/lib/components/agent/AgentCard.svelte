<!--
  Agent Card Component

  Card display for a single agent with actions
-->
<script lang="ts">
  import { Card, Button } from "flowbite-svelte";
  import { TrashBinOutline, ImageOutline } from "flowbite-svelte-icons";
  import type { Agent } from "$lib/types";
  import { supportsVision } from "$lib/utils/vision";

  type Props = {
    agent: Agent;
    onEdit?: (agent: Agent) => void;
    onDelete?: (agent: Agent) => void;
    onChat?: (agent: Agent) => void;
  };

  let { agent, onEdit, onDelete, onChat }: Props = $props();

  /** Provider badge color */
  const badgeClass = $derived(
    agent.provider === "llama_server" || agent.provider === "ollama"
      ? "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100"
      : "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-100",
  );

  /** Check if agent supports vision */
  const hasVision = $derived(supportsVision(agent));
</script>

<Card class="group hover:shadow-lg transition-shadow">
  <div class="pb-2">
    <div class="flex items-start justify-between">
      <div class="space-y-1">
        <h5
          class="text-lg font-bold tracking-tight text-gray-900 dark:text-white"
        >
          {agent.name}
        </h5>
        <div class="flex flex-wrap gap-2">
          <span class="text-xs px-2 py-0.5 rounded-full {badgeClass}">
            {agent.provider}
          </span>
          {#if hasVision}
            <span
              class="text-xs px-2 py-0.5 rounded-full bg-amber-100 text-amber-800 dark:bg-amber-900 dark:text-amber-100 flex items-center gap-1"
            >
              <ImageOutline class="w-3 h-3" />
              Vision
            </span>
          {/if}
          {#if agent.is_aggregator}
            <span
              class="text-xs px-2 py-0.5 rounded-full bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-100"
            >
              Aggregator
            </span>
          {/if}
        </div>
      </div>
    </div>
  </div>

  <div class="pb-2">
    {#if agent.system_prompt}
      <p class="text-sm text-gray-500 dark:text-gray-400 line-clamp-2">
        {agent.system_prompt}
      </p>
    {:else}
      <p class="text-sm text-gray-500 dark:text-gray-400 italic">
        No system prompt
      </p>
    {/if}

    <div class="mt-2 text-xs text-gray-500 dark:text-gray-400">
      {#if agent.model_path}
        <span title={agent.model_path}>
          Model: {agent.model_path.split(/[/\\]/).pop()}
        </span>
      {:else if agent.model_id}
        <span>Model: {agent.model_id}</span>
      {/if}
    </div>
  </div>

  <div class="pt-2">
    <div class="flex gap-2 w-full">
      <Button size="sm" class="flex-1" onclick={() => onChat?.(agent)}>
        Chat
      </Button>
      <Button color="alternative" size="sm" onclick={() => onEdit?.(agent)}>
        Edit
      </Button>
      <Button color="light" size="sm" onclick={() => onDelete?.(agent)}>
        <TrashBinOutline class="w-4 h-4 text-red-600" />
      </Button>
    </div>
  </div>
</Card>
