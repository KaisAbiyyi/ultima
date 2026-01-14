<script lang="ts">
  import "../app.css";
  import Sidebar from "$lib/components/layout/Sidebar.svelte";
  import Header from "$lib/components/layout/Header.svelte";
  import { Modal, Button } from "flowbite-svelte";
  import { AgentForm } from "$lib/components/agent";
  import { agentStore, themeStore } from "$lib/stores";
  import { chatStore } from "$lib/stores/chat.store";
  import { projectStore } from "$lib/stores/project.store";
  import { onMount } from "svelte";
  import type { CreateAgentRequest, Agent } from "$lib/types";

  type Props = {
    children: import("svelte").Snippet;
  };

  let { children }: Props = $props();

  // Modal state - must be $state for Flowbite Modal bind:open
  let modalOpen = $state(false);
  let editAgent = $state<Agent | null>(null);
  let formLoading = $state(false);
  let dataLoaded = $state(false);

  // Expose modal controls globally and load data on mount
  onMount(async () => {
    // Initialize theme first (synchronous)
    themeStore.init();

    // Setup modal controls
    (window as any).__openAgentModal = (agent?: Agent) => {
      editAgent = agent ?? null;
      modalOpen = true;
    };
    (window as any).__closeAgentModal = () => {
      modalOpen = false;
      editAgent = null;
      formLoading = false;
    };

    // Load all stores on app start
    try {
      await Promise.all([
        agentStore.load(),
        chatStore.load(),
        projectStore.load(),
      ]);
      dataLoaded = true;
    } catch (err) {
      console.error("Failed to load data:", err);
      dataLoaded = true; // Still show UI even if load fails
    }
  });

  const modalTitle = $derived(editAgent ? "Edit Agent" : "Create Agent");

  async function handleSubmit(request: CreateAgentRequest) {
    formLoading = true;
    try {
      let result;
      if (editAgent) {
        result = await agentStore.update(editAgent.id, request);
      } else {
        result = await agentStore.create(request);
      }

      // Only close modal if creation was successful
      if (result) {
        modalOpen = false;
        editAgent = null;
      } else {
        console.error(
          "Agent operation returned null - check Tauri console for errors",
        );
      }
    } catch (error) {
      console.error("Failed to save agent:", error);
      // Don't close modal on error so user can try again
    } finally {
      formLoading = false;
    }
  }

  function handleCancel() {
    if (!formLoading) {
      modalOpen = false;
      editAgent = null;
    }
  }
</script>

<div class="flex h-screen bg-gray-50 dark:bg-gray-900">
  <!-- Sidebar -->
  <Sidebar />

  <!-- Main Content -->
  <div class="flex-1 flex flex-col overflow-hidden">
    <Header />
    <main class="flex-1 overflow-auto">
      {@render children()}
    </main>
  </div>
</div>

<!-- Global Agent Modal -->
<Modal
  bind:open={modalOpen}
  title={modalTitle}
  size="lg"
  outsideclose={!formLoading}
  dismissable={!formLoading}
  class="!overflow-hidden"
>
  <div class="max-h-[70vh] overflow-y-auto overflow-x-hidden pr-2 -mr-2">
    <p class="text-sm text-gray-500 dark:text-gray-400 mb-4">
      {editAgent
        ? "Update the agent configuration below."
        : "Configure a new AI agent with your preferred settings."}
    </p>

    <AgentForm
      agent={editAgent}
      loading={formLoading}
      onSubmit={handleSubmit}
      onCancel={handleCancel}
    />
  </div>
</Modal>
