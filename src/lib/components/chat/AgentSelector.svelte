<script lang="ts">
    import { createEventDispatcher } from "svelte";
    import { Button, Select, Input } from "flowbite-svelte";
    import { UserCircleOutline, SearchOutline } from "flowbite-svelte-icons";
    import { agentStore } from "$lib/stores/agent.store";
    import AgentSelectCard from "$lib/components/agent/AgentSelectCard.svelte";
    import type { Agent } from "$lib/types";

    const dispatch = createEventDispatcher<{ select: { agentId: string } }>();

    let selecting = false;
    let sortBy = "name_asc";
    let searchQuery = "";

    const sortOptions = [
        { value: "name_asc", name: "Name A-Z" },
        { value: "name_desc", name: "Name Z-A" },
        { value: "created_desc", name: "Newest First" },
        { value: "created_asc", name: "Oldest First" },
    ];

    // Filter agents based on search query
    $: filteredAgents = $agentStore.agents.filter((agent) =>
        searchQuery
            ? agent.name.toLowerCase().includes(searchQuery.toLowerCase())
            : true,
    );

    // Sort agents based on selected option
    $: sortedAgents = [...filteredAgents].sort((a, b) => {
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
    });

    function selectAgent(agent: Agent) {
        if (selecting) return;
        selecting = true;

        console.log("Agent selected:", agent.name);
        dispatch("select", { agentId: agent.id });
        selecting = false;
    }
</script>

<div class="flex-1 flex flex-col items-start p-8 overflow-y-auto">
    <div
        class="w-full max-w-4xl mb-8 flex flex-col md:flex-row md:items-end justify-between gap-4"
    >
        <div class="text-left">
            <h2 class="text-2xl font-bold text-gray-900 dark:text-white mb-2">
                Select an Agent
            </h2>
            <p class="text-gray-500 dark:text-gray-400">
                Choose which AI agent you want to chat with
            </p>
        </div>

        {#if $agentStore.agents.length > 0}
            <div class="flex gap-2">
                <div class="relative w-64">
                    <Input
                        placeholder="Search agents..."
                        bind:value={searchQuery}
                    >
                        <SearchOutline
                            slot="left"
                            class="w-4 h-4 text-gray-500 dark:text-gray-400"
                        />
                    </Input>
                </div>
                <Select items={sortOptions} bind:value={sortBy} class="w-40" />
            </div>
        {/if}
    </div>

    {#if $agentStore.agents.length === 0}
        <div class="text-center w-full">
            <UserCircleOutline
                class="w-16 h-16 mx-auto mb-4 text-gray-300 dark:text-gray-600"
            />
            <p class="text-gray-500 dark:text-gray-400 mb-4">
                No agents available yet
            </p>
            <Button
                color="primary"
                onclick={() => (window as any).__openAgentModal?.()}
            >
                Create Your First Agent
            </Button>
        </div>
    {:else}
        <!-- Grid is now directly here, controls moved up -->
        <div
            class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 w-full max-w-4xl"
        >
            {#each sortedAgents as agent (agent.id)}
                <AgentSelectCard {agent} onclick={() => selectAgent(agent)} />
            {/each}
        </div>
    {/if}
</div>
