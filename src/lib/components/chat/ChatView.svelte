<script lang="ts">
  import { Spinner } from "flowbite-svelte";
  import { MessagesOutline } from "flowbite-svelte-icons";
  import { marked } from "marked";
  import {
    chatStore,
    currentChat,
    currentChatMessages,
    messagesLoading,
    selectingAgent,
  } from "$lib/stores/chat.store";
  import { selectedProjectId, projectStore } from "$lib/stores/project.store";
  import { agentStore } from "$lib/stores/agent.store";
  import { breadcrumbStore } from "$lib/stores";
  import { supportsVision } from "$lib/utils/vision";
  import ChatInput from "./ChatInput.svelte";
  import AgentSelector from "./AgentSelector.svelte";
  import TypingIndicator from "./TypingIndicator.svelte";
  import { inferenceService } from "$lib/services";
  import type { Action } from "svelte/action";
  import { onDestroy } from "svelte";
  import { derived } from "svelte/store";

  import hljs from "highlight.js";
  import "highlight.js/styles/github-dark.css";
  import DOMPurify from "dompurify";
  import { markedHighlight } from "marked-highlight";

  // Configure DOMPurify to allow safe HTML elements for markdown
  const ALLOWED_TAGS = [
    "p",
    "br",
    "strong",
    "b",
    "em",
    "i",
    "code",
    "pre",
    "ul",
    "ol",
    "li",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "blockquote",
    "a",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
    "hr",
    "div",
    "span",
    "del",
    "s",
    "sup",
    "sub",
  ];

  const ALLOWED_ATTR = ["href", "target", "rel", "class", "id"];

  // Configure marked with highlight.js extension
  marked.use(
    markedHighlight({
      langPrefix: "hljs language-",
      highlight(code: string, lang: string) {
        if (lang && hljs.getLanguage(lang)) {
          return hljs.highlight(code, { language: lang }).value;
        }
        return hljs.highlightAuto(code).value;
      },
    }),
  );

  marked.setOptions({
    breaks: true,
    gfm: true,
  });

  /**
   * Render markdown to sanitized HTML
   * Uses DOMPurify to prevent XSS attacks from malicious content
   */
  function renderMarkdown(content: string): string {
    const rawHtml = marked.parse(content) as string;
    return DOMPurify.sanitize(rawHtml, {
      ALLOWED_TAGS,
      ALLOWED_ATTR,
      ALLOW_DATA_ATTR: false,
    });
  }

  // Derive current agent and vision support
  const currentAgent = derived(
    [currentChat, agentStore],
    ([$chat, $agents]) => {
      if (!$chat) return null;
      return $agents.agents.find((a) => a.id === $chat.agent_id) ?? null;
    },
  );

  // Update breadcrumbs based on current chat
  $: if ($currentChat) {
    const breadcrumbs = [{ label: "Home", href: "/" }];

    if ($currentChat.project_id) {
      const project = $projectStore.projects.find(
        (p) => p.id === $currentChat?.project_id,
      );
      if (project) {
        breadcrumbs.push({
          label: project.name,
          href: `/project/${project.id}`,
        });
      }
    }

    // Add current chat
    const chatLabel = $currentChat.title || $currentChat.id.slice(0, 8);
    breadcrumbs.push({
      label: chatLabel,
      href: "/", // Staying on home, but indicates current context
    });

    breadcrumbStore.set(breadcrumbs);
  } else {
    breadcrumbStore.set([]);
  }

  // Check if current agent supports vision
  $: hasVision = $currentAgent ? supportsVision($currentAgent) : false;

  // Auto-shutdown server when leaving the view
  onDestroy(() => {
    inferenceService
      .stopServer()
      .catch((err) => console.log("Failed to auto-shutdown:", err));

    // Breadcrumbs are now managed by page navigation, not cleared here
    // to allow persistence when navigating to related pages
  });

  // Auto-scroll to bottom when messages change
  const scrollToBottom: Action<HTMLElement, unknown> = (node) => {
    const scroll = () => {
      node.scrollTop = node.scrollHeight;
    };
    scroll();
    return {
      update: scroll,
    };
  };

  // Handle agent selection - create chat with selected agent
  async function handleAgentSelect(event: CustomEvent<{ agentId: string }>) {
    const { agentId } = event.detail;
    console.log("Agent selected:", agentId, "for project:", $selectedProjectId);

    // Prepare draft chat - server starts by ChatInput, but chat not created in DB yet
    chatStore.prepareNewChat(agentId, $selectedProjectId);
  }
</script>

<div class="flex-1 flex flex-col h-full bg-white dark:bg-gray-900">
  {#if $selectingAgent}
    <!-- Agent Selection Mode -->
    <AgentSelector on:select={handleAgentSelect} />
  {:else if $currentChat}
    <!-- Messages -->
    <div
      class="flex-1 overflow-y-auto p-6 space-y-4"
      use:scrollToBottom={$currentChatMessages}
    >
      {#if $messagesLoading}
        <div class="flex justify-center py-8">
          <Spinner size="8" />
        </div>
      {:else if $currentChatMessages.length === 0}
        <div class="text-center text-gray-500 dark:text-gray-400 py-8">
          <MessagesOutline class="w-8 h-8 mx-auto mb-2 opacity-50" />
          <p>No messages yet. Start the conversation!</p>
        </div>
      {:else}
        {#each $currentChatMessages as message (message.id)}
          <div class="flex" class:justify-end={message.role === "user"}>
            <div
              class="max-w-[80%] rounded-lg p-3 {message.role === 'user'
                ? 'bg-primary-600 text-white'
                : 'bg-gray-100 dark:bg-gray-700 text-gray-900 dark:text-white'}"
            >
              {#if message.role === "user"}
                <div class="whitespace-pre-wrap">{message.content}</div>
              {:else if !message.content || !message.content.trim()}
                <!-- Show typing indicator for empty assistant message (streaming) -->
                <TypingIndicator />
              {:else}
                <!-- Render markdown for assistant messages -->
                <div class="markdown-content">
                  <!-- Use a reactive statement or key to force re-render if needed, though simple html replacement works -->
                  {@html renderMarkdown(message.content)}
                </div>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </div>

    <!-- Input -->
    <ChatInput
      chatId={$currentChat.id}
      agentId={$currentChat.agent_id}
      projectId={$currentChat.project_id}
      supportsVision={hasVision}
    />
  {:else}
    <div
      class="flex-1 flex flex-col items-center justify-center text-gray-500 dark:text-gray-400"
    >
      <MessagesOutline class="w-12 h-12 mb-4" />
      <p>Select a chat or create a new one</p>
    </div>
  {/if}
</div>

<style>
  /* Markdown styles for assistant messages */
  .markdown-content :global(p) {
    margin-bottom: 0.5rem;
  }
  .markdown-content :global(p:last-child) {
    margin-bottom: 0;
  }
  .markdown-content :global(ul),
  .markdown-content :global(ol) {
    margin-left: 1.5rem;
    margin-bottom: 0.5rem;
  }
  .markdown-content :global(li) {
    margin-bottom: 0.25rem;
  }
  .markdown-content :global(code) {
    background-color: rgba(0, 0, 0, 0.2);
    padding: 0.125rem 0.25rem;
    border-radius: 0.25rem;
    font-size: 0.875em;
  }
  .markdown-content :global(pre) {
    background-color: rgba(0, 0, 0, 0.3);
    padding: 0.75rem;
    border-radius: 0.5rem;
    overflow-x: auto;
    margin-bottom: 0.5rem;
  }
  .markdown-content :global(pre code) {
    background: none;
    padding: 0;
  }
  .markdown-content :global(blockquote) {
    border-left: 3px solid rgba(255, 255, 255, 0.3);
    padding-left: 1rem;
    margin-left: 0;
    margin-bottom: 0.5rem;
    opacity: 0.9;
  }
  .markdown-content :global(h1),
  .markdown-content :global(h2),
  .markdown-content :global(h3) {
    font-weight: 600;
    margin-bottom: 0.5rem;
  }
  .markdown-content :global(a) {
    color: #60a5fa;
    text-decoration: underline;
  }

  /* Additional Styles for Tables and general README-like formatting */
  .markdown-content :global(table) {
    border-collapse: collapse;
    width: 100%;
    margin-bottom: 1rem;
    border: 1px solid #374151; /* gray-700 */
  }

  .markdown-content :global(th),
  .markdown-content :global(td) {
    border: 1px solid #374151; /* gray-700 */
    padding: 0.5rem;
    text-align: left;
  }

  .markdown-content :global(th) {
    background-color: rgba(0, 0, 0, 0.2);
    font-weight: 600;
  }

  .markdown-content :global(hr) {
    border-color: #374151; /* gray-700 */
    margin: 1.5rem 0;
  }

  .markdown-content :global(ul) {
    list-style-type: disc;
  }
  .markdown-content :global(ol) {
    list-style-type: decimal;
  }
</style>
