<!--
  Model Selector Component
  
  Dynamic model dropdown based on provider
  - llama_server: Load models from local folder
  - ollama: Load models from Ollama API
  - open_router: Load from API with free filter
  - open_ai/anthropic: Load from API
-->
<script lang="ts">
    import { Select, Toggle, Spinner } from "flowbite-svelte";
    import { RefreshOutline } from "flowbite-svelte-icons";
    import type { AgentProvider, ModelInfo } from "$lib/types";
    import { safeInvoke } from "$lib/utils/tauri";

    type Props = {
        provider: AgentProvider;
        value?: string;
        apiKey?: string;
        modelsDirectory?: string;
        disabled?: boolean;
    };

    let {
        provider,
        value = $bindable(""),
        apiKey = "",
        modelsDirectory = "",
        disabled = false,
    }: Props = $props();

    let models = $state<ModelInfo[]>([]);
    let loading = $state(false);
    let error = $state<string | null>(null);
    let freeOnly = $state(true);

    // Select items for dropdown
    const selectItems = $derived(
        models.map((m) => ({
            value: m.id,
            name:
                m.name +
                (m.context_length
                    ? ` (${Math.round(m.context_length / 1000)}k)`
                    : "") +
                (m.is_free ? " ★" : ""),
        })),
    );

    // Load models when provider or dependencies change
    $effect(() => {
        loadModels();
    });

    async function loadModels() {
        if (disabled) return;

        loading = true;
        error = null;
        models = [];

        try {
            switch (provider) {
                case "llama_server":
                    if (modelsDirectory) {
                        const result = await safeInvoke<ModelInfo[]>(
                            "list_local_models",
                            { directory: modelsDirectory },
                        );
                        models = result ?? [];
                    }
                    break;

                case "ollama":
                    const ollamaModels = await safeInvoke<ModelInfo[]>(
                        "list_ollama_models",
                        {},
                    );
                    models = ollamaModels ?? [];
                    break;

                case "open_router":
                    if (apiKey) {
                        const orModels = await safeInvoke<ModelInfo[]>(
                            "list_openrouter_models",
                            {
                                apiKey,
                                freeOnly,
                            },
                        );
                        models = orModels ?? [];
                    }
                    break;

                case "open_ai":
                    if (apiKey) {
                        const oaiModels = await safeInvoke<ModelInfo[]>(
                            "list_openai_models",
                            { apiKey },
                        );
                        models = oaiModels ?? [];
                    }
                    break;

                case "anthropic":
                    if (apiKey) {
                        const claudeModels = await safeInvoke<ModelInfo[]>(
                            "list_anthropic_models",
                            { apiKey },
                        );
                        models = claudeModels ?? [];
                    }
                    break;

                default:
                    models = [];
            }

            // Auto-select first model if none selected
            if (models.length > 0 && !value) {
                value = models[0].id;
            }
        } catch (err) {
            error =
                err instanceof Error ? err.message : "Failed to load models";
            console.error("Failed to load models:", err);
        } finally {
            loading = false;
        }
    }

    function handleChange(event: Event) {
        const target = event.target as HTMLSelectElement;
        value = target.value;
    }

    function handleFreeOnlyChange() {
        if (provider === "open_router") {
            loadModels();
        }
    }
</script>

<div class="space-y-3">
    <div class="flex items-center gap-2">
        <div class="flex-1">
            {#if loading}
                <div
                    class="flex items-center gap-2 px-3 py-2 bg-gray-100 dark:bg-gray-800 rounded-lg"
                >
                    <Spinner size="4" />
                    <span class="text-sm text-gray-500 dark:text-gray-400"
                        >Loading models...</span
                    >
                </div>
            {:else if error}
                <div
                    class="px-3 py-2 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg"
                >
                    <span class="text-sm text-red-600 dark:text-red-400"
                        >{error}</span
                    >
                </div>
            {:else if models.length === 0}
                <div class="px-3 py-2 bg-gray-100 dark:bg-gray-800 rounded-lg">
                    <span class="text-sm text-gray-500 dark:text-gray-400">
                        {#if provider === "llama_server" && !modelsDirectory}
                            Select a models folder first
                        {:else if (provider === "open_ai" || provider === "anthropic" || provider === "open_router") && !apiKey}
                            Enter API key to load models
                        {:else}
                            No models available
                        {/if}
                    </span>
                </div>
            {:else}
                <Select
                    items={selectItems}
                    bind:value
                    onchange={handleChange}
                    {disabled}
                    class="w-full"
                />
            {/if}
        </div>

        <button
            type="button"
            class="p-2 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors disabled:opacity-50"
            onclick={loadModels}
            disabled={loading || disabled}
            title="Refresh models"
        >
            <RefreshOutline class="w-5 h-5 {loading ? 'animate-spin' : ''}" />
        </button>
    </div>

    <!-- Free only toggle for OpenRouter -->
    {#if provider === "open_router"}
        <div class="flex items-center gap-2">
            <Toggle
                bind:checked={freeOnly}
                size="small"
                onchange={handleFreeOnlyChange}
                disabled={disabled || !apiKey}
            />
            <span class="text-sm text-gray-600 dark:text-gray-400"
                >Free models only</span
            >
        </div>
    {/if}

    <!-- Model count -->
    {#if models.length > 0 && !loading}
        <p class="text-xs text-gray-500 dark:text-gray-400">
            {models.length} model{models.length !== 1 ? "s" : ""} available
        </p>
    {/if}
</div>
