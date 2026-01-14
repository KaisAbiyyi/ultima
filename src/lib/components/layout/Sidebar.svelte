<script lang="ts">
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { cn } from "$lib/utils/cn";
  import {
    CogOutline,
    ChevronLeftOutline,
    ChevronRightOutline,
    UsersOutline,
    PlusOutline,
    SearchOutline,
    CloseOutline,
  } from "flowbite-svelte-icons";
  import { Button, Tooltip, Modal, Input } from "flowbite-svelte";
  import { sidebarStore } from "$lib/stores";
  import { inferenceService } from "$lib/services";
  import { chatStore } from "$lib/stores/chat.store";
  import { onMount } from "svelte";
  import { browser } from "$app/environment";
  import { ProjectsSection, ChatHistorySection } from "$lib/components/sidebar";

  function toggleSidebar() {
    sidebarStore.toggle();
  }

  // New chat handler
  function handleNewChat() {
    chatStore.cancelNewChat();
    chatStore.select(null);
    chatStore.startNewChat();
    goto("/");
  }

  // Search modal state
  let showSearchModal = $state(false);
  let searchQuery = $state("");

  // Filter chats based on search query
  const filteredChats = $derived(
    searchQuery.trim()
      ? $chatStore.chats.filter(
          (chat) =>
            chat.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
            (chat.project_id &&
              chat.project_id
                .toLowerCase()
                .includes(searchQuery.toLowerCase())),
        )
      : [],
  );

  async function handleSelectSearchResult(chatId: string) {
    showSearchModal = false;
    searchQuery = "";
    await chatStore.select(chatId);
    goto("/");
  }

  // Navigate to settings and stop server
  async function handleSettingsClick(e: MouseEvent) {
    e.preventDefault();
    try {
      // Fire and forget server stop
      inferenceService
        .stopServer()
        .catch((err) => console.error("Failed to stop server:", err));

      // Clear current chat
      chatStore.cancelNewChat();
      chatStore.select(null);
    } catch (err) {
      console.error("Error during settings navigation cleanup:", err);
    } finally {
      // Always navigate
      await goto("/settings");
    }
  }

  // Use simple state initialized to false
  let collapsed = $state(false);

  // Subscribe to store on mount for proper client-side reactivity
  onMount(() => {
    const unsubscribe = sidebarStore.subscribe((value) => {
      collapsed = value;
    });
    return unsubscribe;
  });
</script>

<aside
  class={cn(
    "border-r border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 flex flex-col h-full transition-all duration-300",
    collapsed ? "w-16" : "w-72",
  )}
>
  <!-- Logo -->
  <div
    class="p-4 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between shrink-0"
  >
    {#if !collapsed}
      <div>
        <h1 class="text-xl font-bold text-gray-900 dark:text-white">Ultima</h1>
        <p class="text-xs text-gray-500 dark:text-gray-400">
          Local AI Platform
        </p>
      </div>
    {:else}
      <span class="text-xl font-bold text-gray-900 dark:text-white mx-auto"
        >U</span
      >
    {/if}
  </div>

  <!-- New Chat & Search Buttons -->
  <div class="p-2 space-y-1 shrink-0">
    <button
      type="button"
      onclick={handleNewChat}
      class={cn(
        "flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors w-full",
        collapsed ? "justify-center" : "",
        "hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white",
      )}
    >
      <PlusOutline class="w-5 h-5 shrink-0" />
      {#if !collapsed}
        <span>New Chat</span>
      {/if}
    </button>
    {#if collapsed}
      <Tooltip placement="right">New Chat</Tooltip>
    {/if}

    <button
      type="button"
      onclick={() => (showSearchModal = true)}
      class={cn(
        "flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors w-full",
        collapsed ? "justify-center" : "",
        "hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white",
      )}
    >
      <SearchOutline class="w-5 h-5 shrink-0" />
      {#if !collapsed}
        <span>Search Chats</span>
      {/if}
    </button>
    {#if collapsed}
      <Tooltip placement="right">Search Chats</Tooltip>
    {/if}
  </div>

  <!-- Projects Section -->
  <ProjectsSection {collapsed} />

  <!-- Chat History Section -->
  <ChatHistorySection {collapsed} />

  <!-- Bottom Section -->
  <div class="shrink-0 mt-auto border-t border-gray-200 dark:border-gray-700">
    <!-- Settings Link -->
    <div class="p-2">
      <a
        href="/settings"
        onclick={handleSettingsClick}
        class={cn(
          "flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors w-full",
          collapsed ? "justify-center" : "",
          $page.url.pathname.startsWith("/settings")
            ? "bg-primary-600 text-white"
            : "hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white",
        )}
      >
        <CogOutline class="w-5 h-5 shrink-0" />
        {#if !collapsed}
          <span>Settings</span>
        {/if}
      </a>
      {#if collapsed}
        <Tooltip placement="right">Settings</Tooltip>
      {/if}
    </div>

    <!-- Agents Link -->
    <div class="p-2 pt-0">
      <a
        href="/agents"
        class={cn(
          "flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors w-full",
          collapsed ? "justify-center" : "",
          $page.url.pathname.startsWith("/agents")
            ? "bg-primary-600 text-white"
            : "hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white",
        )}
      >
        <UsersOutline class="w-5 h-5 shrink-0" />
        {#if !collapsed}
          <span>Agents</span>
        {/if}
      </a>
      {#if collapsed}
        <Tooltip placement="right">Agents</Tooltip>
      {/if}
    </div>

    <!-- Footer with Toggle -->
    <div
      class="p-2 border-t border-gray-200 dark:border-gray-700 flex items-center justify-between"
    >
      {#if !collapsed}
        <span class="text-xs text-gray-500 dark:text-gray-400">v0.1.0</span>
      {/if}
      <Button
        color="light"
        size="xs"
        class={cn("!p-1.5", collapsed ? "mx-auto" : "")}
        onclick={toggleSidebar}
      >
        {#if collapsed}
          <ChevronRightOutline class="w-4 h-4" />
        {:else}
          <ChevronLeftOutline class="w-4 h-4" />
        {/if}
      </Button>
    </div>
  </div>
</aside>

<!-- Search Modal -->
<Modal bind:open={showSearchModal} size="md" title="Search Chats" outsideclose>
  <div class="space-y-4">
    <Input
      type="search"
      placeholder="Type to search chats..."
      bind:value={searchQuery}
      class="w-full"
    />

    {#if searchQuery.trim()}
      <div class="max-h-80 overflow-y-auto space-y-1">
        {#if filteredChats.length === 0}
          <p class="text-center text-gray-500 dark:text-gray-400 py-4">
            No chats found
          </p>
        {:else}
          {#each filteredChats as chat (chat.id)}
            <button
              type="button"
              class="w-full text-left px-3 py-2 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
              onclick={() => handleSelectSearchResult(chat.id)}
            >
              <p class="font-medium text-gray-900 dark:text-white truncate">
                {chat.title}
              </p>
              <p class="text-xs text-gray-500 dark:text-gray-400">
                {new Date(chat.updated_at).toLocaleDateString()}
              </p>
            </button>
          {/each}
        {/if}
      </div>
    {:else}
      <p class="text-center text-gray-500 dark:text-gray-400 py-4">
        Start typing to search your chats
      </p>
    {/if}
  </div>
</Modal>
