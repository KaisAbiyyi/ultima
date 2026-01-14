<!--
  Agent Modal Component

  Modal wrapper for agent form
-->
<script lang="ts">
  import { Modal } from "flowbite-svelte";
  import AgentForm from "./AgentForm.svelte";
  import type { Agent, CreateAgentRequest } from "$lib/types";

  type Props = {
    open?: boolean;
    agent?: Agent | null;
    loading?: boolean;
    onSubmit?: (request: CreateAgentRequest) => void;
    onCancel?: () => void;
  };

  let {
    open = false,
    agent = null,
    loading = false,
    onSubmit,
    onCancel,
  }: Props = $props();

  /** Modal title */
  const title = $derived(agent ? "Edit Agent" : "Create Agent");
  const description = $derived(
    agent
      ? "Update the agent configuration below."
      : "Configure a new AI agent with your preferred settings.",
  );

  /** Handle form submit */
  function handleSubmit(request: CreateAgentRequest) {
    onSubmit?.(request);
  }

  /** Handle close */
  function handleClose() {
    if (!loading) {
      onCancel?.();
    }
  }
</script>

<Modal
  {open}
  {title}
  size="lg"
  outsideclose={!loading}
  dismissable={!loading}
  onclose={handleClose}
>
  <p class="text-sm text-gray-500 dark:text-gray-400 mb-4">{description}</p>

  <div class="py-4">
    <AgentForm
      {agent}
      {loading}
      onSubmit={handleSubmit}
      onCancel={handleClose}
    />
  </div>
</Modal>
