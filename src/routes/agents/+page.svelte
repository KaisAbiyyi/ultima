<script lang="ts">
    import { Button, Card } from "flowbite-svelte";
    import { PlusOutline, UserCircleOutline } from "flowbite-svelte-icons";
    import { agentStore } from "$lib/stores/agent.store";
    import AgentSelectCard from "$lib/components/agent/AgentSelectCard.svelte";
    import type { Agent } from "$lib/types";
    import { browser } from "$app/environment";

    // Sort state
    let sortBy = $state("name_asc");
    const sortOptions = [
        { value: "name_asc", name: "Name A-Z" },
        { value: "name_desc", name: "Name Z-A" },
        { value: "created_desc", name: "Newest First" },
        { value: "created_asc", name: "Oldest First" },
    ];

    // Sort agents
    const sortedAgents = $derived(
        [...$agentStore.agents].sort((a, b) => {
            switch (sortBy) {
                case "name_asc":
                    return a.name.localeCompare(b.name);
                case "name_desc":
                    return b.name.localeCompare(a.name);
                case "created_desc":
                    return (
                        new Date(b.created_at).getTime() -
                        new Date(a.created_at).getTime()
                    );
                case "created_asc":
                    return (
                        new Date(a.created_at).getTime() -
                        new Date(b.created_at).getTime()
                    );
                default:
                    return 0;
            }
        }),
    );

    function handleCreate() {
        if (browser && (window as any).__openAgentModal) {
            (window as any).__openAgentModal();
        }
    }

    function handleEdit(agent: Agent) {
        if (browser && (window as any).__openAgentModal) {
            (window as any).__openAgentModal(agent);
        }
    }

    async function handleDelete(agent: Agent) {
        if (confirm(`Delete agent "${agent.name}"? This cannot be undone.`)) {
            await agentStore.delete(agent.id);
        }
    }
</script>

<div class="w-full h-full p-8 space-y-6 overflow-y-auto">
    <!-- Header -->
    <div class="flex items-center justify-between">
        <div>
            <h1 class="text-2xl font-bold text-gray-900 dark:text-white">
                Agents
            </h1>
            <p class="text-gray-500 dark:text-gray-400 mt-1">
                Manage your AI agents
            </p>
        </div>
        <div class="flex items-center gap-3">
            <select
                bind:value={sortBy}
                class="text-sm bg-white dark:bg-gray-700 border border-gray-300 dark:border-gray-600 rounded-lg px-3 py-2 focus:ring-primary-500 focus:border-primary-500"
            >
                {#each sortOptions as option}
                    <option value={option.value}>{option.name}</option>
                {/each}
            </select>
            <Button color="primary" onclick={handleCreate}>
                <PlusOutline class="w-4 h-4 mr-2" />
                Create Agent
            </Button>
        </div>
    </div>

    <!-- Agent List -->
    {#if $agentStore.agents.length === 0}
        <Card class="w-full !max-w-none p-8 text-center">
            <UserCircleOutline
                class="w-16 h-16 mx-auto mb-4 text-gray-300 dark:text-gray-600"
            />
            <h3
                class="text-lg font-semibold text-gray-900 dark:text-white mb-2"
            >
                No Agents Yet
            </h3>
            <p class="text-gray-500 dark:text-gray-400 mb-4">
                Create your first AI agent to start chatting
            </p>
            <Button color="primary" onclick={handleCreate}>
                <PlusOutline class="w-4 h-4 mr-2" />
                Create Your First Agent
            </Button>
        </Card>
    {:else}
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {#each sortedAgents as agent (agent.id)}
                <AgentSelectCard
                    {agent}
                    showActions={true}
                    onEdit={handleEdit}
                    onDelete={handleDelete}
                />
            {/each}
        </div>
    {/if}
</div>
