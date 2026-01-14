<!--
  Shared Agent Card Component
  
  Used in: AgentSelector (new chat), Project Detail, Agents page
  Actions are shown via 3-dot dropdown when showActions is true
-->
<script lang="ts">
    import { Dropdown, DropdownItem } from "flowbite-svelte";
    import {
        UserCircleOutline,
        ImageOutline,
        DotsVerticalOutline,
        EditOutline,
        TrashBinOutline,
    } from "flowbite-svelte-icons";
    import { supportsVision } from "$lib/utils/vision";
    import type { Agent } from "$lib/types";

    type Props = {
        agent: Agent;
        showActions?: boolean;
        onclick?: () => void;
        onEdit?: (agent: Agent) => void;
        onDelete?: (agent: Agent) => void;
    };

    let {
        agent,
        showActions = false,
        onclick,
        onEdit,
        onDelete,
    }: Props = $props();

    /** Provider badge color */
    const badgeClass = $derived(
        agent.provider === "llama_server" || agent.provider === "ollama"
            ? "bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100"
            : "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-100",
    );

    const hasVision = $derived(supportsVision(agent));

    // Unique dropdown ID for this card
    const dropdownId = $derived(`agent-dropdown-${agent.id}`);

    function handleEdit(e: Event) {
        e.stopPropagation();
        onEdit?.(agent);
    }

    function handleDelete(e: Event) {
        e.stopPropagation();
        onDelete?.(agent);
    }

    function handleDropdownClick(e: Event) {
        e.stopPropagation();
    }
</script>

<button
    type="button"
    class="flex flex-col justify-start text-left p-4 rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 hover:border-primary-500 dark:hover:border-primary-500 hover:shadow-md transition-all cursor-pointer w-full h-full relative"
    {onclick}
>
    <div class="flex items-start gap-3">
        <div class="p-2 rounded-lg bg-primary-100 dark:bg-primary-900 shrink-0">
            <UserCircleOutline
                class="w-5 h-5 text-primary-600 dark:text-primary-400"
            />
        </div>
        <div class="flex-1 min-w-0">
            <h3 class="font-semibold text-gray-900 dark:text-white truncate">
                {agent.name}
            </h3>
            <div class="flex flex-wrap items-center gap-1.5 mt-0.5">
                <span class="text-xs px-2 py-0.5 rounded-full {badgeClass}">
                    {agent.provider}
                </span>
                {#if hasVision}
                    <span
                        class="text-xs px-1.5 py-0.5 rounded bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300 flex items-center gap-0.5"
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
            {#if agent.system_prompt}
                <p
                    class="text-xs text-gray-400 dark:text-gray-500 mt-1 line-clamp-2"
                >
                    {agent.system_prompt}
                </p>
            {/if}
            <div
                class="mt-2 text-xs text-gray-500 dark:text-gray-400 space-y-0.5"
            >
                {#if agent.model_path}
                    <div title={agent.model_path}>
                        Model: {agent.model_path.split(/[/\\]/).pop()}
                    </div>
                {:else if agent.model_id}
                    <div>Model: {agent.model_id}</div>
                {/if}
                {#if agent.mmproj_path}
                    <div
                        class="text-amber-600 dark:text-amber-400"
                        title={agent.mmproj_path}
                    >
                        Vision: {agent.mmproj_path.split(/[/\\]/).pop()}
                    </div>
                {/if}
            </div>
        </div>

        {#if showActions}
            <!-- Actions Dropdown -->
            <div class="shrink-0 relative" onclick={handleDropdownClick}>
                <button
                    type="button"
                    id={dropdownId}
                    class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-500 dark:text-gray-400"
                >
                    <DotsVerticalOutline class="w-4 h-4" />
                </button>
            </div>
        {/if}
    </div>
</button>

{#if showActions}
    <Dropdown triggeredBy="#{dropdownId}" placement="bottom-end" class="z-50">
        <DropdownItem
            onclick={handleEdit}
            class="flex items-center text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"
        >
            <EditOutline class="w-4 h-4 mr-2 inline" />
            Edit
        </DropdownItem>
        <DropdownItem
            onclick={handleDelete}
            class="flex items-center text-red-600 dark:text-red-500 hover:bg-gray-100 dark:hover:bg-gray-700"
        >
            <TrashBinOutline class="w-4 h-4 mr-2 inline" />
            Delete
        </DropdownItem>
    </Dropdown>
{/if}
