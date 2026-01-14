<script lang="ts">
  import { Button, Sidebar, SidebarGroup, Spinner } from 'flowbite-svelte';
  import { PlusOutline, MessagesOutline } from 'flowbite-svelte-icons';
  import { chatStore } from '$lib/stores/chat.store';
  import { agentStore } from '$lib/stores/agent.store';
  import ChatItem from './ChatItem.svelte';

  async function handleNewChat() {
    const agentState = $agentStore;
    if (agentState.agents.length === 0) return;

    await chatStore.create({
      agent_id: agentState.agents[0].id,
    });
  }
</script>

<div class="w-64 border-r border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 flex flex-col h-full">
  <div class="p-3 border-b border-gray-200 dark:border-gray-700">
    <Button class="w-full" on:click={handleNewChat}>
      <PlusOutline class="w-4 h-4 mr-2" slot="start" />
      New Chat
    </Button>
  </div>

  <Sidebar class="flex-1">
    <SidebarGroup>
      {#if $chatStore.loading}
        <div class="flex justify-center p-4">
          <Spinner size="6" />
        </div>
      {:else if $chatStore.error}
        <div class="text-sm text-red-500 p-3">{$chatStore.error}</div>
      {:else}
        {#each $chatStore.chats as chat (chat.id)}
          <ChatItem
            {chat}
            isActive={$chatStore.currentChatId === chat.id}
            on:select={(e) => chatStore.select(chat.id)}
            on:delete={(e) => chatStore.delete(chat.id)}
          />
        {:else}
          <div class="p-4 text-sm text-gray-500 dark:text-gray-400 text-center">
            <MessagesOutline class="w-8 h-8 mx-auto mb-2 opacity-50" />
            <p>No chats yet</p>
            <p class="text-xs mt-1">Create your first chat to get started</p>
          </div>
        {/each}
      {/if}
    </SidebarGroup>
  </Sidebar>
</div>
