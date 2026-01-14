<script lang="ts">
  import { Spinner } from "flowbite-svelte";
  import {
    PaperPlaneOutline,
    ImageOutline,
    StopSolid,
  } from "flowbite-svelte-icons";
  import { chatStore, currentChatMessages } from "$lib/stores/chat.store";
  import { agentStore } from "$lib/stores/agent.store";
  import { inferenceService } from "$lib/services";
  import type { StoredChatMessage, ChatMessage } from "$lib/types";
  import { get } from "svelte/store";
  import { onMount, onDestroy } from "svelte";
  import {
    DRAFT_CHAT_ID,
    isDraftChat,
    LOCAL_CHAT_COMPLETIONS_URL,
    MAX_CHAT_TITLE_LENGTH,
    MAX_SERVER_READY_ATTEMPTS,
    SERVER_HEALTH_INTERVAL_MS,
    PROVIDERS,
  } from "$lib/constants";

  export let chatId: string;
  export let agentId: string;
  export let projectId: string | null = null;
  export let supportsVision: boolean = false;

  let content = "";
  let sending = false;
  let streaming = false;
  let streamedContent = "";
  let statusMessage = "";
  let serverReady = false;
  let aborted = false;
  let destroyed = false; // Track if component is destroyed

  // Track current stream unsubscribe function for cleanup
  let currentUnsubscribe: (() => void) | null = null;

  // Test if chat completions actually work
  async function testChatEndpoint(): Promise<boolean> {
    try {
      const response = await fetch(LOCAL_CHAT_COMPLETIONS_URL, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          messages: [{ role: "user", content: "hi" }],
          max_tokens: 1,
          stream: false,
        }),
      });

      if (response.ok) {
        console.log("Chat endpoint test passed");
        return true;
      }

      const errorText = await response.text();
      console.log(
        "Chat endpoint test returned:",
        response.status,
        errorText.substring(0, 100),
      );
      return false;
    } catch (err) {
      if (!destroyed) {
        console.log("Chat endpoint test failed:", err);
      }
      return false;
    }
  }

  // Wait until chat endpoint actually works
  async function waitForServerReady(
    maxAttempts = MAX_SERVER_READY_ATTEMPTS,
  ): Promise<boolean> {
    for (let i = 0; i < maxAttempts; i++) {
      // Exit early if component is destroyed
      if (destroyed) {
        console.log("Component destroyed, stopping server polling");
        return false;
      }

      statusMessage = `Loading model... (${i + 1}/${maxAttempts})`;

      // Direct test of chat endpoint
      const ready = await testChatEndpoint();
      if (ready) {
        console.log("Server fully ready after", i + 1, "attempts");
        statusMessage = "";
        return true;
      }

      await new Promise((resolve) =>
        setTimeout(resolve, SERVER_HEALTH_INTERVAL_MS),
      );
    }
    statusMessage = "";
    return false;
  }

  // Initialize server on mount if needed
  async function initializeServer() {
    const agents = get(agentStore).agents;
    const agent = agents.find((a) => a.id === agentId);

    if (!agent) {
      console.error("Agent not found:", agentId);
      serverReady = false;
      return;
    }

    console.log("Agent provider:", agent.provider);

    if (agent.provider === PROVIDERS.LLAMA_SERVER) {
      if (!agent.model_path) {
        console.error("Agent has no model_path configured");
        serverReady = false;
        return;
      }

      statusMessage = "Checking server...";

      // Check server status
      const status = await inferenceService.getServerStatus();
      console.log("Server status:", status);

      if (!status.running) {
        console.log("Starting llama-server with model:", agent.model_path);
        statusMessage = "Starting server...";
        try {
          await inferenceService.startServer(agent.model_path);
        } catch (err) {
          console.error("Failed to start server:", err);
          statusMessage = "Failed to start server";
          serverReady = false;
          return;
        }
      }

      // Wait until chat endpoint actually works
      serverReady = await waitForServerReady();
      if (!serverReady) {
        statusMessage = "Server failed to load model";
      }
    } else {
      // External API, always ready
      serverReady = true;
    }
  }

  // Initialize on mount
  onMount(() => {
    initializeServer();
  });

  // Cleanup on destroy
  onDestroy(() => {
    destroyed = true;
    if (currentUnsubscribe) {
      currentUnsubscribe();
      currentUnsubscribe = null;
    }
  });

  async function handleSubmit() {
    if (!content.trim() || sending || streaming || !serverReady) return;

    // Clean up any previous stream listeners
    if (currentUnsubscribe) {
      currentUnsubscribe();
      currentUnsubscribe = null;
    }

    const userContent = content.trim();
    content = "";
    sending = true;

    console.log("Sending message:", userContent);

    try {
      // 0. Handle draft chat creation
      let effectiveChatId = chatId;
      if (isDraftChat(chatId)) {
        const title =
          userContent.length > MAX_CHAT_TITLE_LENGTH
            ? userContent.slice(0, MAX_CHAT_TITLE_LENGTH) + "..."
            : userContent;
        const newChat = await chatStore.create({
          agent_id: agentId,
          project_id: projectId ?? undefined,
          title: title,
        });

        if (!newChat) {
          console.error("Failed to create chat from draft");
          sending = false;
          chatStore.updateLastMessage("Error: Failed to create chat");
          return;
        }
        effectiveChatId = newChat.id;
        // The store update in create() will trigger ChatView to re-render ChatInput with new ID
        // allowing this function to complete but future interactions use real ID.
        // NOTE: We continue execution here.
      }

      // 1. Add user message to database
      const userMessage = await chatStore.addMessage(effectiveChatId, {
        role: "user",
        content: userContent,
      });

      if (!userMessage) {
        console.error("Failed to add user message");
        sending = false;
        return;
      }

      // 2. Get conversation history form store (store has been updated with user message)
      // Filter out empty messages and ensure role alternation
      let lastRole = "";
      const messages: ChatMessage[] = $currentChatMessages
        .filter((m) => m.content && m.content.trim()) // Remove empty messages
        .filter((m) => {
          // Remove consecutive same-role messages (keep only the last one)
          if (m.role === lastRole) {
            return false;
          }
          lastRole = m.role;
          return true;
        })
        .map((m) => ({
          role: m.role,
          content: m.content,
        }));

      // 3. Start streaming
      streaming = true;
      streamedContent = "";

      // Create placeholder assistant message for streaming
      chatStore.addMessageLocal({
        id: crypto.randomUUID(),
        chat_id: effectiveChatId,
        role: "assistant",
        content: "",
        images: [],
        created_at: new Date().toISOString(),
      });

      // 4. Stream response
      console.log("Starting stream with agent:", agentId);

      let myRequestId: string | null = null;
      let completed = false;

      const { requestId, unsubscribe } = await inferenceService.chatStream(
        agentId,
        messages,
        {
          onToken: (payload) => {
            if (myRequestId && payload.request_id !== myRequestId) return;
            if (completed) return;
            streamedContent += payload.token;
            chatStore.updateLastMessage(streamedContent);
          },
          onComplete: async (payload) => {
            if (myRequestId && payload.request_id !== myRequestId) return;
            if (completed) return;
            completed = true;

            console.log("Stream complete:", payload);
            streaming = false;

            if (currentUnsubscribe) {
              currentUnsubscribe();
              currentUnsubscribe = null;
            }

            await chatStore.addMessage(effectiveChatId, {
              role: "assistant",
              content: payload.content,
            });

            await chatStore.loadMessages(effectiveChatId);
          },
          onError: (payload) => {
            if (myRequestId && payload.request_id !== myRequestId) return;
            if (completed) return;
            completed = true;

            console.error("Inference error:", payload);
            streaming = false;

            if (currentUnsubscribe) {
              currentUnsubscribe();
              currentUnsubscribe = null;
            }

            chatStore.updateLastMessage(`Error: ${payload.message}`);
          },
        },
      );

      myRequestId = requestId;
      currentUnsubscribe = unsubscribe;
      console.log("Stream started with requestId:", requestId);
    } catch (err: any) {
      console.error("Failed to send message:", err);
      streaming = false;
      chatStore.updateLastMessage(`Error: ${err?.message || "Unknown error"}`);
    } finally {
      sending = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  }

  // Abort current generation and save partial response
  async function handleAbort() {
    if (!streaming) return;

    aborted = true;

    // Unsubscribe from stream events
    if (currentUnsubscribe) {
      currentUnsubscribe();
      currentUnsubscribe = null;
    }

    streaming = false;

    // Save the partial response to database if there's content
    if (streamedContent.trim()) {
      await chatStore.addMessage(chatId, {
        role: "assistant",
        content: streamedContent + " [stopped]",
      });
      await chatStore.loadMessages(chatId);
    }

    aborted = false;
    streamedContent = "";
  }

  $: placeholder =
    statusMessage ||
    (!serverReady ? "Waiting for server..." : "Type a message...");
  $: isDisabled = sending || streaming || !serverReady;
</script>

<div class="border-t border-gray-700 bg-gray-900 p-2">
  <div class="flex items-center gap-2">
    {#if supportsVision}
      <button
        type="button"
        class="p-2 text-gray-400 hover:text-white hover:bg-gray-700 rounded transition-colors"
        title="Attach image"
      >
        <ImageOutline class="w-5 h-5" />
      </button>
    {/if}

    <input
      type="text"
      class="flex-1 bg-gray-800 border-0 text-white placeholder-gray-500 rounded-lg px-4 py-2.5 focus:ring-1 focus:ring-primary-500 focus:outline-none"
      {placeholder}
      bind:value={content}
      onkeydown={handleKeydown}
      disabled={isDisabled}
    />

    {#if streaming}
      <!-- Abort button when streaming -->
      <button
        type="button"
        class="p-2 bg-red-600 hover:bg-red-700 text-white rounded-lg transition-colors"
        onclick={handleAbort}
        title="Stop generating"
      >
        <StopSolid class="w-5 h-5" />
      </button>
    {:else}
      <!-- Send button when not streaming -->
      <button
        type="button"
        class="p-2 bg-primary-600 hover:bg-primary-700 text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        onclick={handleSubmit}
        disabled={!content.trim() || isDisabled}
      >
        {#if sending || !serverReady}
          <Spinner size="5" color="gray" />
        {:else}
          <PaperPlaneOutline class="w-5 h-5" />
        {/if}
      </button>
    {/if}
  </div>
</div>
