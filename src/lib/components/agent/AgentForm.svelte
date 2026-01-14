<!--
  Agent Form Component

  Form for creating/editing agents with validation
-->
<script lang="ts">
  import {
    Button,
    Input,
    Label,
    Textarea,
    Toggle,
    Range,
    Helper,
  } from "flowbite-svelte";
  import { FolderOpenOutline, EyeOutline, CloseOutline } from "flowbite-svelte-icons";
  import ProviderSelector from "./ProviderSelector.svelte";
  import ModelSelector from "./ModelSelector.svelte";
  import type { Agent, CreateAgentRequest, AgentProvider } from "$lib/types";
  import { getProviderOption } from "$lib/types";
  import { agentService } from "$lib/services";
  import { open } from "@tauri-apps/plugin-dialog";
  import { browser } from "$app/environment";

  type Props = {
    agent?: Agent | null;
    loading?: boolean;
    onSubmit?: (request: CreateAgentRequest) => void;
    onCancel?: () => void;
  };

  let { agent = null, loading = false, onSubmit, onCancel }: Props = $props();

  // Form state
  let name = $state("");
  let systemPrompt = $state("");
  let provider = $state<AgentProvider>("llama_server");
  let modelPath = $state("");
  let mmprojPath = $state("");
  let modelId = $state("");
  let apiKey = $state("");
  let apiEndpoint = $state("");
  let contextWindow = $state(4096);
  let isAggregator = $state(false);
  let modelsDirectory = $state("");

  // Load modelsDirectory from settings
  $effect(() => {
    if (browser) {
      const stored = localStorage.getItem("ultima_models_directory");
      if (stored) {
        modelsDirectory = stored;
      }
    }
  });

  // Validation
  let errors = $state<string[]>([]);

  // Provider config
  const providerConfig = $derived(getProviderOption(provider));
  const showModelPath = $derived(provider === "llama_server");
  const showApiKey = $derived(providerConfig.requiresApiKey);
  const showModelSelector = $derived(
    providerConfig.supportsModelList && provider !== "custom",
  );

  // Context window display
  const contextWindowLabel = $derived(() => {
    if (contextWindow >= 1000) {
      return `${Math.round(contextWindow / 1000)}k tokens`;
    }
    return `${contextWindow} tokens`;
  });

  // Sync form state when agent prop changes
  $effect(() => {
    if (agent) {
      name = agent.name ?? "";
      systemPrompt = agent.system_prompt ?? "";
      // Handle legacy 'local' provider
      provider =
        agent.provider === ("local" as any)
          ? "llama_server"
          : (agent.provider ?? "llama_server");
      modelPath = agent.model_path ?? "";
      mmprojPath = (agent as any).mmproj_path ?? "";
      modelId = agent.model_id ?? "";
      apiKey = agent.api_key ?? "";
      apiEndpoint = agent.api_endpoint ?? "";
      contextWindow = agent.context_window ?? 4096;
      isAggregator = agent.is_aggregator ?? false;
    }
  });

  // Set default endpoint when provider changes
  $effect(() => {
    if (providerConfig.defaultEndpoint && !apiEndpoint) {
      apiEndpoint = providerConfig.defaultEndpoint;
    }
  });

  /** Select models directory */
  async function selectModelsDirectory() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Models Directory",
      });

      if (selected && typeof selected === "string") {
        modelsDirectory = selected;
      }
    } catch (err) {
      console.error("Failed to select directory:", err);
    }
  }

  /** Select mmproj file for vision models */
  async function selectMmprojFile() {
    try {
      const result = await open({
        title: 'Select Vision Projector File',
        filters: [{
          name: 'CLIP Projector',
          extensions: ['gguf', 'mmproj']
        }],
        multiple: false,
      });

      if (result && typeof result === "string") {
        mmprojPath = result;
      }
    } catch (err) {
      console.error("Failed to select mmproj file:", err);
    }
  }

  /** Build request from form */
  function buildRequest(): CreateAgentRequest {
    return {
      name: name.trim(),
      system_prompt: systemPrompt,
      provider,
      model_path: showModelPath ? modelPath : undefined,
      mmproj_path: showModelPath && mmprojPath ? mmprojPath : undefined,
      model_id: showModelSelector || showApiKey ? modelId : undefined,
      api_key: showApiKey ? apiKey : undefined,
      api_endpoint: showApiKey ? apiEndpoint : undefined,
      context_window: contextWindow,
      is_aggregator: isAggregator,
    };
  }

  /** Validate and submit */
  function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    const request = buildRequest();
    errors = agentService.validate(request);

    if (errors.length === 0) {
      onSubmit?.(request);
    }
  }

  /** Handle cancel */
  function handleCancel() {
    onCancel?.();
  }

  /** Handle provider change */
  function handleProviderChange(newProvider: AgentProvider) {
    provider = newProvider;
    // Reset provider-specific fields
    if (provider === "llama_server") {
      apiKey = "";
      apiEndpoint = "";
      modelId = "";
    } else if (provider === "ollama") {
      apiKey = "";
      apiEndpoint = getProviderOption(provider).defaultEndpoint ?? "";
      modelPath = "";
    } else {
      modelPath = "";
      apiEndpoint = getProviderOption(provider).defaultEndpoint ?? "";
    }
    // Reset model selection
    modelId = "";
  }
</script>

<form onsubmit={handleSubmit} class="space-y-6">
  <!-- Errors -->
  {#if errors.length > 0}
    <div
      class="p-3 rounded-lg bg-red-100 border border-red-300 dark:bg-red-900/30 dark:border-red-800"
    >
      <ul class="list-disc list-inside text-sm text-red-600 dark:text-red-400">
        {#each errors as error}
          <li>{error}</li>
        {/each}
      </ul>
    </div>
  {/if}

  <!-- Name -->
  <div class="space-y-2">
    <Label for="name" class="mb-2">Agent Name</Label>
    <Input
      id="name"
      bind:value={name}
      placeholder="My AI Agent"
      required
      disabled={loading}
    />
  </div>

  <!-- Provider -->
  <div class="space-y-2">
    <Label class="mb-2">Provider</Label>
    <ProviderSelector
      bind:value={provider}
      onValueChange={handleProviderChange}
      disabled={loading}
    />
  </div>

  <!-- Model Selection for llama_server -->
  {#if showModelPath}
    <div
      class="space-y-3 p-4 rounded-lg bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-700"
    >
      <div class="flex items-center justify-between">
        <Label class="mb-0">Models Folder</Label>
        <Button
          color="alternative"
          size="xs"
          onclick={selectModelsDirectory}
          disabled={loading}
        >
          <FolderOpenOutline class="w-4 h-4 mr-1" />
          {modelsDirectory ? "Change" : "Select"}
        </Button>
      </div>

      {#if modelsDirectory}
        <p
          class="text-xs text-gray-500 dark:text-gray-400 truncate"
          title={modelsDirectory}
        >
          {modelsDirectory}
        </p>
      {/if}

      <div class="space-y-2">
        <Label class="mb-2">Model</Label>
        <ModelSelector
          {provider}
          bind:value={modelPath}
          {modelsDirectory}
          disabled={loading}
        />
      </div>

      <!-- Vision Projector (mmproj) -->
      <div class="space-y-2">
        <Label for="mmprojPath" class="flex items-center gap-1">
          <EyeOutline class="w-4 h-4" />
          Vision Projector
          <span class="text-gray-400 font-normal text-xs">(optional)</span>
        </Label>
        <Helper class="text-xs">
          Select a .mmproj file for multimodal/vision models (e.g., LLaVA, BakLLaVA)
        </Helper>

        <div class="flex gap-2">
          <Input
            id="mmprojPath"
            class="flex-1"
            placeholder="No vision projector selected"
            value={mmprojPath || ""}
            readonly
          />
          <Button color="alternative" onclick={selectMmprojFile} disabled={loading}>
            <FolderOpenOutline class="w-4 h-4 mr-1" slot="start" />
            Browse
          </Button>
          {#if mmprojPath}
            <Button
              color="red"
              outline
              onclick={() => (mmprojPath = "")}
              disabled={loading}
            >
              <CloseOutline class="w-4 h-4" slot="start" />
            </Button>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  <!-- API Config (External providers) -->
  {#if showApiKey}
    <div
      class="space-y-4 p-4 rounded-lg bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-700"
    >
      <div class="space-y-2">
        <Label for="apiKey" class="mb-2">API Key</Label>
        <Input
          id="apiKey"
          type="password"
          bind:value={apiKey}
          placeholder="sk-..."
          disabled={loading}
        />
      </div>

      <div class="space-y-2">
        <Label for="apiEndpoint" class="mb-2">API Endpoint</Label>
        <Input
          id="apiEndpoint"
          bind:value={apiEndpoint}
          placeholder="https://api.openai.com/v1"
          disabled={loading}
        />
      </div>

      <!-- Model Selector for API providers -->
      {#if showModelSelector}
        <div class="space-y-2">
          <Label class="mb-2">Model</Label>
          <ModelSelector
            {provider}
            bind:value={modelId}
            {apiKey}
            disabled={loading}
          />
        </div>
      {:else}
        <div class="space-y-2">
          <Label for="modelId" class="mb-2">Model ID</Label>
          <Input
            id="modelId"
            bind:value={modelId}
            placeholder="gpt-4, claude-3-opus, etc."
            disabled={loading}
          />
        </div>
      {/if}
    </div>
  {/if}

  <!-- Ollama (no API key needed) -->
  {#if provider === "ollama"}
    <div
      class="space-y-4 p-4 rounded-lg bg-gray-50 dark:bg-gray-800/50 border border-gray-200 dark:border-gray-700"
    >
      <div class="space-y-2">
        <Label for="apiEndpoint" class="mb-2">Ollama Endpoint</Label>
        <Input
          id="apiEndpoint"
          bind:value={apiEndpoint}
          placeholder="http://localhost:11434"
          disabled={loading}
        />
      </div>

      <div class="space-y-2">
        <Label class="mb-2">Model</Label>
        <ModelSelector {provider} bind:value={modelId} disabled={loading} />
      </div>
    </div>
  {/if}

  <!-- System Prompt (Optional) -->
  <div class="space-y-2">
    <Label for="systemPrompt" class="mb-2">
      System Prompt
      <span class="text-gray-400 font-normal text-xs ml-1">(optional)</span>
    </Label>
    <textarea
      id="systemPrompt"
      bind:value={systemPrompt}
      placeholder="You are a helpful assistant..."
      disabled={loading}
      oninput={(e) => {
        const target = e.currentTarget;
        target.style.height = "auto";
        target.style.height = Math.min(target.scrollHeight, 200) + "px";
      }}
      class="w-full min-h-[80px] max-h-[200px] resize-none rounded-lg border border-gray-300 bg-gray-50 p-2.5 text-sm text-gray-900 focus:border-primary-500 focus:ring-primary-500 dark:border-gray-600 dark:bg-gray-700 dark:text-white dark:placeholder-gray-400 dark:focus:border-primary-500 dark:focus:ring-primary-500 disabled:opacity-50"
    ></textarea>
  </div>

  <!-- Advanced Settings -->
  <div
    class="space-y-4 p-4 rounded-lg border border-gray-200 dark:border-gray-700"
  >
    <h4 class="text-sm font-medium text-gray-900 dark:text-white">
      Advanced Settings
    </h4>

    <!-- Context Window Slider -->
    <div class="space-y-3">
      <div class="flex items-center justify-between">
        <Label for="contextWindow" class="mb-0">Context Window</Label>
        <span
          class="text-sm font-medium text-primary-600 dark:text-primary-400"
        >
          {contextWindowLabel()}
        </span>
      </div>
      <Range
        id="contextWindow"
        bind:value={contextWindow}
        min={2048}
        max={131072}
        step={2048}
        disabled={loading}
        class="w-full"
      />
      <!-- Labels positioned at 0%, 33%, 66%, 100% of track -->
      <div
        class="flex justify-between text-xs text-gray-500 dark:text-gray-400 px-1"
      >
        <span>2k</span>
        <span>45k</span>
        <span>88k</span>
        <span>128k</span>
      </div>
    </div>

    <div class="flex items-center justify-between pt-2">
      <div>
        <span class="text-sm font-medium text-gray-900 dark:text-white"
          >Aggregator Agent</span
        >
        <p class="text-xs text-gray-500 dark:text-gray-400">
          Can orchestrate other agents
        </p>
      </div>
      <Toggle bind:checked={isAggregator} disabled={loading} />
    </div>
  </div>

  <!-- Actions -->
  <div class="flex gap-3 justify-end pt-2">
    <Button
      type="button"
      color="alternative"
      onclick={handleCancel}
      disabled={loading}
    >
      Cancel
    </Button>
    <Button type="submit" disabled={loading}>
      {#if loading}
        Saving...
      {:else}
        {agent ? "Update" : "Create"} Agent
      {/if}
    </Button>
  </div>
</form>
