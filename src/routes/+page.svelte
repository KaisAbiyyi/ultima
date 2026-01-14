<script lang="ts">
  import ChatView from "$lib/components/chat/ChatView.svelte";
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { chatStore } from "$lib/stores/chat.store";
  import { goto } from "$app/navigation";
  import { isDraftChat } from "$lib/constants";

  // Reactive URL parameter monitoring
  $: chatId = $page.url.searchParams.get("chatId");

  $: if (chatId) {
    if (chatId !== chatStore.getCurrentChatId()) {
      chatStore.select(chatId);
    }
  } else {
    // If no chatId, default to new chat (Agent Selector) if not already selecting
    // But we need to be careful not to override if user is doing something else.
    // However, on root /, it should be new chat or select chat.
    // Let's enforce New Chat mode if store is idle.
    const currentId = chatStore.getCurrentChatId();
    if (!currentId || isDraftChat(currentId)) {
      // Ensure we are in "Start New Chat" mode cleanly
      chatStore.startNewChat();
    }
  }
</script>

<!-- Main chat view - sidebar is handled by layout, data loading is in layout -->
<div class="flex-1 flex flex-col h-full overflow-hidden">
  <ChatView />
</div>
