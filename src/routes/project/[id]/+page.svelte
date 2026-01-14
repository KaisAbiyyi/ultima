<script lang="ts">
    import { page } from "$app/stores";
    import { goto } from "$app/navigation";
    import { Button, Card, Spinner, Select, Input } from "flowbite-svelte";
    import {
        ArrowLeftOutline,
        FolderOutline,
        UserCircleOutline,
        SearchOutline,
        HomeOutline,
    } from "flowbite-svelte-icons";
    import { projectStore } from "$lib/stores/project.store";
    import { chatStore } from "$lib/stores/chat.store";
    import { agentStore } from "$lib/stores/agent.store";
    import { inferenceService } from "$lib/services";
    import AgentSelectCard from "$lib/components/agent/AgentSelectCard.svelte";

    import type { Agent } from "$lib/types";

    // Sort state
    let sortBy = "name_asc";
    let agentSearchQuery = "";

    const sortOptions = [
        { value: "name_asc", name: "Name A-Z" },
        { value: "name_desc", name: "Name Z-A" },
        { value: "created_desc", name: "Newest First" },
        { value: "created_asc", name: "Oldest First" },
    ];

    // Filter agents based on search query
    $: filteredAgents = $agentStore.agents.filter((agent) =>
        agentSearchQuery
            ? agent.name.toLowerCase().includes(agentSearchQuery.toLowerCase())
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

    // Get project ID from route params - must be reactive!
    $: projectId = $page.params.id;

    // Find project - derives from both projectStore and page params
    $: project = $projectStore.projects.find((p) => p.id === projectId) ?? null;

    // Chats for this project - derives from both chatStore and page params
    $: projectChats = $chatStore.chats.filter(
        (c) => c.project_id === projectId && !c.archived,
    );

    // Chat sorting and search state
    let chatSortBy = "newest";
    let chatSearchQuery = "";

    const chatSortOptions = [
        { value: "newest", name: "Newest" },
        { value: "oldest", name: "Oldest" },
        { value: "az", name: "Name A-Z" },
        { value: "za", name: "Name Z-A" },
    ];

    // Derived sorted/filtered chats
    $: sortedProjectChats = projectChats
        .filter((c) =>
            chatSearchQuery
                ? c.title.toLowerCase().includes(chatSearchQuery.toLowerCase())
                : true,
        )
        .sort((a, b) => {
            switch (chatSortBy) {
                case "newest":
                    return (
                        new Date(b.updated_at).getTime() -
                        new Date(a.updated_at).getTime()
                    );
                case "oldest":
                    return (
                        new Date(a.updated_at).getTime() -
                        new Date(b.updated_at).getTime()
                    );
                case "az":
                    return a.title.localeCompare(b.title);
                case "za":
                    return b.title.localeCompare(a.title);
                default:
                    return 0;
            }
        });

    // Select project when component mounts or projectId changes
    $: if (projectId) projectStore.select(projectId);

    // Update breadcrumbs when project is loaded
    $: if (project) {
        breadcrumbStore.set([
            { label: "Home", href: "/" },
            {
                label: project.name,
                href: `/project/${project.id}`,
            },
        ]);
    }

    import { onDestroy } from "svelte";
    import { breadcrumbStore } from "$lib/stores";

    // onDestroy(() => {
    //    breadcrumbStore.set([]);
    // });

    // Handle agent selection to start new chat
    async function handleAgentSelect(agent: Agent) {
        console.log(
            "Starting new chat in project",
            projectId,
            "with agent",
            agent.name,
        );

        // Stop any running server
        try {
            await inferenceService.stopServer();
        } catch {
            // Ignore
        }

        // Prepare draft chat with this project and agent
        chatStore.prepareNewChat(agent.id, projectId);

        // Navigate to home which will show the chat view with draft
        // For new chats, we don't have an ID yet, so we stay on root.
        // The store state 'draft' will handle the UI.
        await goto("/");
    }

    // Handle selecting an existing chat
    async function handleSelectChat(chatId: string) {
        await chatStore.select(chatId);
        await goto(`/?chatId=${chatId}`);
    }

    function handleBack() {
        goto("/");
    }
</script>

<div
    class="flex-1 flex flex-col h-full bg-white dark:bg-gray-900 overflow-hidden"
>
    {#if project}
        <div class="flex-1 overflow-y-auto p-8">
            <!-- Agent Selection Section -->
            <section class="mb-8">
                <div class="flex items-center justify-between mb-4">
                    <div>
                        <h2
                            class="text-lg font-semibold text-gray-900 dark:text-white"
                        >
                            Start New Chat
                        </h2>
                        <p class="text-sm text-gray-500 dark:text-gray-400">
                            Select an agent to start a new chat in this project
                        </p>
                    </div>
                    <div class="flex items-center gap-2">
                        <div class="relative w-64">
                            <Input
                                placeholder="Search agents..."
                                bind:value={agentSearchQuery}
                            >
                                <SearchOutline
                                    slot="left"
                                    class="w-4 h-4 text-gray-500 dark:text-gray-400"
                                />
                            </Input>
                        </div>
                        <Select
                            items={sortOptions}
                            bind:value={sortBy}
                            class="w-40"
                        />
                    </div>
                </div>

                {#if $agentStore.agents.length === 0}
                    <div
                        class="text-center py-8 text-gray-500 dark:text-gray-400"
                    >
                        <UserCircleOutline
                            class="w-12 h-12 mx-auto mb-2 opacity-50"
                        />
                        <p>No agents available. Create one first.</p>
                    </div>
                {:else}
                    <div
                        class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
                    >
                        {#each sortedAgents as agent (agent.id)}
                            <AgentSelectCard
                                {agent}
                                onclick={() => handleAgentSelect(agent)}
                            />
                        {/each}
                    </div>
                {/if}
            </section>

            <!-- Existing Chats Section -->
            <section>
                <div class="flex items-center justify-between gap-4 mb-4">
                    <h2
                        class="text-lg font-semibold text-gray-900 dark:text-white shrink-0"
                    >
                        Project Chats
                    </h2>
                    <div
                        class="flex items-center gap-2 flex-1 max-w-md ml-auto"
                    >
                        <div class="relative flex-1">
                            <Input
                                placeholder="Search chats..."
                                bind:value={chatSearchQuery}
                            >
                                <SearchOutline
                                    slot="left"
                                    class="w-4 h-4 text-gray-500 dark:text-gray-400"
                                />
                            </Input>
                        </div>
                        <Select
                            items={chatSortOptions}
                            bind:value={chatSortBy}
                            class="w-32"
                        />
                    </div>
                </div>

                {#if projectChats.length === 0}
                    <div
                        class="text-center py-12 bg-white dark:bg-gray-800 border-2 border-dashed border-gray-200 dark:border-gray-700 rounded-xl"
                    >
                        <FolderOutline
                            class="w-12 h-12 mx-auto mb-3 text-gray-300 dark:text-gray-600"
                        />
                        <h3
                            class="text-lg font-medium text-gray-900 dark:text-white"
                        >
                            No chats yet
                        </h3>
                        <p class="text-gray-500 dark:text-gray-400 mt-1">
                            Select an agent above to start the first
                            conversation in this project.
                        </p>
                    </div>
                {:else}
                    <div class="grid gap-3">
                        {#each sortedProjectChats as chat (chat.id)}
                            {@const agent = $agentStore.agents.find(
                                (a) => a.id === chat.agent_id,
                            )}
                            <button
                                class="group w-full text-left p-5 rounded-xl bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 hover:border-primary-500 dark:hover:border-primary-500 hover:shadow-md transition-all duration-200 relative overflow-hidden"
                                onclick={() => handleSelectChat(chat.id)}
                            >
                                <div
                                    class="flex items-start justify-between gap-4"
                                >
                                    <div class="flex-1 min-w-0">
                                        <h3
                                            class="text-base font-semibold text-gray-900 dark:text-white truncate group-hover:text-primary-600 dark:group-hover:text-primary-400 transition-colors"
                                        >
                                            {chat.title}
                                        </h3>
                                        <div
                                            class="flex items-center gap-2 mt-1.5 text-xs text-gray-500 dark:text-gray-400"
                                        >
                                            <span
                                                class="flex items-center gap-1"
                                            >
                                                <UserCircleOutline
                                                    class="w-3.5 h-3.5"
                                                />
                                                {agent?.name ?? "Unknown Agent"}
                                            </span>
                                            <span>•</span>
                                            <span>
                                                {new Date(
                                                    chat.updated_at,
                                                ).toLocaleDateString()}
                                            </span>
                                        </div>
                                    </div>
                                    {#if chat.is_pinned}
                                        <span
                                            class="shrink-0 text-xs font-medium bg-primary-50 dark:bg-primary-900/40 text-primary-600 dark:text-primary-400 px-2 py-0.5 rounded border border-primary-100 dark:border-primary-800"
                                        >
                                            Pinned
                                        </span>
                                    {/if}
                                </div>
                            </button>
                        {/each}
                    </div>
                {/if}
            </section>
        </div>
    {:else}
        <div class="flex-1 flex items-center justify-center">
            <Spinner size="8" />
        </div>
    {/if}
</div>
