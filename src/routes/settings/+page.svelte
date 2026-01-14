<script lang="ts">
    import { Button, Label, Toggle, Card } from "flowbite-svelte";
    import {
        FolderOpenOutline,
        RefreshOutline,
        MoonOutline,
        SunOutline,
        CheckOutline,
    } from "flowbite-svelte-icons";
    import { isTauri, safeInvoke } from "$lib/utils/tauri";
    import { browser } from "$app/environment";
    import { themeStore, THEME_OPTIONS, type ThemeColor } from "$lib/stores";

    // Models directory state
    let modelsDirectory = $state("");
    let isScanning = $state(false);
    let modelCount = $state(0);

    // Dark mode state
    let darkMode = $state(false);

    // Theme state
    let currentTheme = $state<ThemeColor>("rose");

    // Subscribe to theme store
    themeStore.subscribe((theme) => {
        currentTheme = theme;
    });

    // Load settings on mount
    $effect(() => {
        if (browser) {
            // Initialize theme
            themeStore.init();

            // Load dark mode preference
            const stored = localStorage.getItem("ultima_dark_mode");
            darkMode =
                stored === "true" ||
                (!stored &&
                    window.matchMedia("(prefers-color-scheme: dark)").matches);
            applyDarkMode(darkMode);

            // Load models directory
            const storedDir = localStorage.getItem("ultima_models_directory");
            if (storedDir) {
                modelsDirectory = storedDir;
                scanModels(storedDir);
            }
        }
    });

    function applyDarkMode(enabled: boolean) {
        if (browser) {
            if (enabled) {
                document.documentElement.classList.add("dark");
            } else {
                document.documentElement.classList.remove("dark");
            }
            localStorage.setItem("ultima_dark_mode", String(enabled));
        }
    }

    function toggleDarkMode() {
        darkMode = !darkMode;
        applyDarkMode(darkMode);
    }

    function selectTheme(theme: ThemeColor) {
        themeStore.setTheme(theme);
    }

    async function selectModelsDirectory() {
        if (!isTauri()) return;

        try {
            const { open } = await import("@tauri-apps/plugin-dialog");
            const selected = await open({
                directory: true,
                multiple: false,
                title: "Select Models Directory",
            });

            if (selected && typeof selected === "string") {
                modelsDirectory = selected;
                localStorage.setItem("ultima_models_directory", selected);
                await scanModels(selected);
            }
        } catch (error) {
            console.error("Failed to select directory:", error);
        }
    }

    async function scanModels(directory: string) {
        if (!isTauri()) return;

        isScanning = true;
        try {
            const models = await safeInvoke<any[]>("list_local_models", {
                directory,
            });
            modelCount = models?.length ?? 0;
        } catch (error) {
            console.error("Failed to scan models:", error);
            modelCount = 0;
        } finally {
            isScanning = false;
        }
    }

    async function refreshModels() {
        if (modelsDirectory) {
            await scanModels(modelsDirectory);
        }
    }
</script>

<div class="w-full h-full p-8 space-y-8 overflow-y-auto">
    <div>
        <h1 class="text-2xl font-bold text-gray-900 dark:text-white">
            Settings
        </h1>
        <p class="text-gray-500 dark:text-gray-400 mt-1">
            Configure your Ultima experience
        </p>
    </div>

    <!-- Appearance -->
    <Card class="w-full !max-w-none p-6">
        <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-4">
            Appearance
        </h2>

        <!-- Dark Mode Toggle -->
        <div class="flex items-center justify-between mb-6">
            <div class="flex items-center gap-3">
                {#if darkMode}
                    <MoonOutline
                        class="w-5 h-5 text-gray-500 dark:text-gray-400"
                    />
                {:else}
                    <SunOutline class="w-5 h-5 text-yellow-500" />
                {/if}
                <div>
                    <p class="font-medium text-gray-900 dark:text-white">
                        Dark Mode
                    </p>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                        Switch between light and dark themes
                    </p>
                </div>
            </div>
            <Toggle bind:checked={darkMode} onchange={toggleDarkMode} />
        </div>

        <!-- Theme Color -->
        <div class="pt-4 border-t border-gray-200 dark:border-gray-700">
            <div class="mb-3">
                <p class="font-medium text-gray-900 dark:text-white">
                    Theme Color
                </p>
                <p class="text-sm text-gray-500 dark:text-gray-400">
                    Choose your accent color
                </p>
            </div>
            <div class="flex flex-wrap gap-3">
                {#each THEME_OPTIONS as option (option.id)}
                    <button
                        type="button"
                        class="flex flex-col items-center gap-1.5 p-2 rounded-lg transition-all hover:bg-gray-100 dark:hover:bg-gray-700 {currentTheme ===
                        option.id
                            ? 'ring-2 ring-offset-2 ring-gray-400 dark:ring-gray-500 dark:ring-offset-gray-800'
                            : ''}"
                        onclick={() => selectTheme(option.id)}
                    >
                        <div
                            class="w-10 h-10 rounded-full flex items-center justify-center shadow-md"
                            style="background-color: {option.color}"
                        >
                            {#if currentTheme === option.id}
                                <CheckOutline class="w-5 h-5 text-white" />
                            {/if}
                        </div>
                        <span class="text-xs text-gray-600 dark:text-gray-400"
                            >{option.name}</span
                        >
                    </button>
                {/each}
            </div>
        </div>
    </Card>

    <!-- Models Directory -->
    <Card class="w-full !max-w-none p-6">
        <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-4">
            Models Directory
        </h2>
        <p class="text-sm text-gray-500 dark:text-gray-400 mb-4">
            Set the folder where your GGUF model files are stored. This will be
            used for llama-server agents.
        </p>

        <div class="space-y-4">
            <div class="flex items-center gap-3">
                <Button
                    color="alternative"
                    onclick={selectModelsDirectory}
                    disabled={isScanning}
                >
                    <FolderOpenOutline class="w-4 h-4 mr-2" />
                    {modelsDirectory ? "Change Folder" : "Select Folder"}
                </Button>

                {#if modelsDirectory}
                    <Button
                        color="light"
                        size="sm"
                        onclick={refreshModels}
                        disabled={isScanning}
                    >
                        <RefreshOutline
                            class="w-4 h-4 {isScanning ? 'animate-spin' : ''}"
                        />
                    </Button>
                {/if}
            </div>

            {#if modelsDirectory}
                <div class="p-3 bg-gray-50 dark:bg-gray-800 rounded-lg">
                    <Label class="text-xs text-gray-500 dark:text-gray-400 mb-1"
                        >Current Directory</Label
                    >
                    <p
                        class="text-sm text-gray-900 dark:text-white font-mono truncate"
                        title={modelsDirectory}
                    >
                        {modelsDirectory}
                    </p>
                    <p class="text-xs text-gray-500 dark:text-gray-400 mt-2">
                        {modelCount} GGUF model{modelCount !== 1 ? "s" : ""} found
                    </p>
                </div>
            {:else}
                <p class="text-sm text-yellow-600 dark:text-yellow-400">
                    No models directory configured. Select a folder to enable
                    local model selection.
                </p>
            {/if}
        </div>
    </Card>

    <!-- About -->
    <Card class="w-full !max-w-none p-6">
        <h2 class="text-lg font-semibold text-gray-900 dark:text-white mb-4">
            About
        </h2>
        <div class="space-y-2 text-sm text-gray-500 dark:text-gray-400">
            <p>
                <span class="font-medium text-gray-900 dark:text-white"
                    >Ultima</span
                > — Local AI Platform
            </p>
            <p>Version 0.1.0</p>
        </div>
    </Card>
</div>
