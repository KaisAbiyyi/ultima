<script lang="ts">
    import { Modal, Button, Label, Select, Input } from "flowbite-svelte";
    import { agentStore } from "$lib/stores/agent.store";
    import { projectStore, selectedProjectId } from "$lib/stores/project.store";
    import { chatStore } from "$lib/stores/chat.store";

    export let open = false;

    let selectedAgentId = "";
    let chatTitle = "";
    let creating = false;

    // Reset form when modal opens
    $: if (open) {
        chatTitle = "";
        // Pre-select first agent if available
        if ($agentStore.agents.length > 0 && !selectedAgentId) {
            selectedAgentId = $agentStore.agents[0].id;
        }
    }

    async function handleCreate() {
        if (!selectedAgentId || creating) return;

        creating = true;
        console.log("Creating chat with agent:", selectedAgentId);
        try {
            const chat = await chatStore.create({
                agent_id: selectedAgentId,
                project_id: $selectedProjectId ?? undefined,
                title: chatTitle.trim() || undefined,
            });

            if (chat) {
                console.log("Chat created:", chat);
                open = false;
            }
        } finally {
            creating = false;
        }
    }

    function handleCancel() {
        open = false;
    }
</script>

<Modal bind:open size="sm" title="New Chat">
    <div class="space-y-4">
        <!-- Agent Selection -->
        <div>
            <Label for="agent-select" class="mb-2">Select Agent</Label>
            {#if $agentStore.agents.length === 0}
                <p class="text-sm text-red-500">
                    No agents available. Please create an agent first.
                </p>
            {:else}
                <Select id="agent-select" bind:value={selectedAgentId}>
                    {#each $agentStore.agents as agent (agent.id)}
                        <option value={agent.id}>{agent.name}</option>
                    {/each}
                </Select>
            {/if}
        </div>

        <!-- Optional Title -->
        <div>
            <Label for="chat-title" class="mb-2">Chat Title (optional)</Label>
            <Input
                id="chat-title"
                placeholder="New Chat"
                bind:value={chatTitle}
                onkeydown={(e) => e.key === "Enter" && handleCreate()}
            />
        </div>

        <!-- Current Project Info -->
        {#if $selectedProjectId}
            {@const project = $projectStore.projects.find(
                (p) => p.id === $selectedProjectId,
            )}
            {#if project}
                <div class="text-sm text-gray-500 dark:text-gray-400">
                    This chat will be added to project: <strong
                        >{project.name}</strong
                    >
                </div>
            {/if}
        {/if}
    </div>

    <svelte:fragment slot="footer">
        <Button
            onclick={handleCreate}
            disabled={!selectedAgentId ||
                creating ||
                $agentStore.agents.length === 0}
        >
            {creating ? "Creating..." : "Create Chat"}
        </Button>
        <Button color="alternative" onclick={handleCancel}>Cancel</Button>
    </svelte:fragment>
</Modal>
