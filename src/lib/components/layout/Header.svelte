<script lang="ts">
  import { page } from "$app/stores";
  import { Button, Breadcrumb, BreadcrumbItem } from "flowbite-svelte";
  import { MoonOutline, SunOutline, PlusOutline } from "flowbite-svelte-icons";
  import { browser } from "$app/environment";
  import { breadcrumbStore } from "$lib/stores";

  const pageTitles: Record<string, string> = {
    "/": "Chat",
    "/settings": "Settings",
  };

  let pageTitle = $derived(pageTitles[$page.url.pathname] || "Ultima");

  let darkMode = $state(false);

  $effect(() => {
    if (browser) {
      darkMode = document.documentElement.classList.contains("dark");
    }
  });

  function toggleDarkMode() {
    darkMode = !darkMode;
    if (browser) {
      if (darkMode) {
        document.documentElement.classList.add("dark");
      } else {
        document.documentElement.classList.remove("dark");
      }
      localStorage.setItem("ultima_dark_mode", String(darkMode));
    }
  }

  function handleAddAgent() {
    if (browser && (window as any).__openAgentModal) {
      (window as any).__openAgentModal();
    }
  }
</script>

<header
  class="h-14 border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 flex items-center justify-between px-6"
>
  <div class="flex items-center">
    {#if $breadcrumbStore.length > 0}
      <Breadcrumb aria-label="Default breadcrumb example" navClass="flex">
        {#each $breadcrumbStore as item, i}
          <BreadcrumbItem href={item.href} home={i === 0}>
            {#if item.icon}
              <svelte:component this={item.icon} class="w-4 h-4 mr-2" />
            {/if}
            {item.label}
          </BreadcrumbItem>
        {/each}
      </Breadcrumb>
    {:else}
      <h2 class="text-lg font-semibold text-gray-900 dark:text-white">
        {pageTitle}
      </h2>
    {/if}
  </div>

  <div class="flex items-center gap-2">
    <!-- Add Agent Button (hide on agents page) -->
    {#if !$page.url.pathname.startsWith("/agents")}
      <Button color="primary" size="sm" onclick={handleAddAgent}>
        <PlusOutline class="w-4 h-4 mr-1" />
        Add Agent
      </Button>
    {/if}

    <!-- Dark Mode Toggle -->
    <Button color="light" size="sm" class="p-2" onclick={toggleDarkMode}>
      {#if darkMode}
        <SunOutline class="w-4 h-4" />
      {:else}
        <MoonOutline class="w-4 h-4" />
      {/if}
    </Button>
  </div>
</header>
