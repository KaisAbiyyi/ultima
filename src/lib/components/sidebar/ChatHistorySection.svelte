<script lang="ts">
    import { Button, Spinner, Modal, Input, Label } from "flowbite-svelte";
    import { MessagesOutline, PlusOutline } from "flowbite-svelte-icons";
    import { chatStore, currentChatId } from "$lib/stores/chat.store";
    import { projectStore } from "$lib/stores/project.store";
    import { inferenceService } from "$lib/services";
    import { derived } from "svelte/store";
    import ChatHistoryItem from "./ChatHistoryItem.svelte";

    export let collapsed = false;

    // Modal states
    let showRenameModal = false;
    let showDeleteModal = false;
    let renameChatId: string | null = null;
    let renameChatTitle = "";
    let deleteChatId: string | null = null;

    // Show all non-archived chats (no project filtering)
    const filteredChats = derived(chatStore, ($store) =>
        $store.chats.filter((c) => !c.archived),
    );

    // Project lookup map for badges
    const projectsById = derived(
        projectStore,
        ($store) => new Map($store.projects.map((p) => [p.id, p])),
    );

    import { goto } from "$app/navigation"; // Add import

    // Start new chat flow - show agent selection and stop server
    async function handleNewChat() {
        console.log("Starting new chat flow");
        try {
            inferenceService
                .stopServer()
                .catch(() => console.log("No server to stop or failed"));
            chatStore.startNewChat();
        } finally {
            // Ensure we clear the query param for new chat
            await goto("/", { replaceState: false });
        }
    }

    // Select a chat - server will be started by ChatInput on mount
    async function handleSelectChat(chatId: string, agentId: string) {
        console.log("Selecting chat:", chatId, "agent:", agentId);
        try {
            await chatStore.select(chatId);
        } finally {
            await goto(`/?chatId=${chatId}`);
        }
    }

    async function handlePinChat(chatId: string, isPinned: boolean) {
        await chatStore.updateChat(chatId, { is_pinned: !isPinned });
    }

    function openRenameModal(chatId: string, currentTitle: string) {
        renameChatId = chatId;
        renameChatTitle = currentTitle;
        showRenameModal = true;
    }

    async function confirmRename() {
        if (renameChatId && renameChatTitle.trim()) {
            await chatStore.updateChat(renameChatId, {
                title: renameChatTitle.trim(),
            });
        }
        showRenameModal = false;
        renameChatId = null;
        renameChatTitle = "";
    }

    function openDeleteModal(chatId: string) {
        deleteChatId = chatId;
        showDeleteModal = true;
    }

    async function confirmDelete() {
        if (deleteChatId) {
            await chatStore.delete(deleteChatId);
        }
        showDeleteModal = false;
        deleteChatId = null;
    }
</script>

{#if !collapsed}
    <div class="flex-1 flex flex-col min-h-0">
        <!-- Header -->
        <div class="px-3 py-2">
            <span
                class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider"
            >
                Chats
            </span>
        </div>

        <!-- Chat list -->
        <div class="flex-1 overflow-y-auto px-2 space-y-0.5">
            {#if $chatStore.loading}
                <div class="flex justify-center py-4">
                    <Spinner size="6" />
                </div>
            {:else if $chatStore.error}
                <div class="text-sm text-red-500 p-3">{$chatStore.error}</div>
            {:else if $filteredChats.length === 0}
                <div class="text-center text-gray-500 dark:text-gray-400 py-8">
                    <MessagesOutline class="w-8 h-8 mx-auto mb-2 opacity-50" />
                    <p class="text-sm">No chats yet</p>
                    <p class="text-xs mt-1">Create your first chat!</p>
                </div>
            {:else}
                {#each $filteredChats as chat (chat.id)}
                    {@const project = chat.project_id
                        ? $projectsById.get(chat.project_id)
                        : null}
                    <ChatHistoryItem
                        {chat}
                        {project}
                        isActive={$currentChatId === chat.id}
                        onselect={() =>
                            handleSelectChat(chat.id, chat.agent_id)}
                        onpin={() => handlePinChat(chat.id, chat.is_pinned)}
                        onrename={() => openRenameModal(chat.id, chat.title)}
                        ondelete={() => openDeleteModal(chat.id)}
                    />
                {/each}
            {/if}
        </div>
    </div>
{/if}

<!-- Rename Modal -->
<Modal
    bind:open={showRenameModal}
    size="xs"
    title="Rename Chat"
    autoclose={false}
>
    <div class="space-y-4">
        <div>
            <Label for="chat-name" class="mb-2">Chat Name</Label>
            <Input
                id="chat-name"
                bind:value={renameChatTitle}
                placeholder="Enter new name"
            />
        </div>
        <div class="flex justify-end gap-2">
            <Button
                color="alternative"
                onclick={() => (showRenameModal = false)}>Cancel</Button
            >
            <Button color="primary" onclick={confirmRename}>Save</Button>
        </div>
    </div>
</Modal>

<!-- Delete Confirmation Modal -->
<Modal
    bind:open={showDeleteModal}
    size="xs"
    title="Delete Chat"
    autoclose={false}
>
    <div class="space-y-4">
        <p class="text-gray-500 dark:text-gray-400">
            Are you sure you want to delete this chat? This action cannot be
            undone.
        </p>
        <div class="flex justify-end gap-2">
            <Button
                color="alternative"
                onclick={() => (showDeleteModal = false)}>Cancel</Button
            >
            <Button color="red" onclick={confirmDelete}>Delete</Button>
        </div>
    </div>
</Modal>
