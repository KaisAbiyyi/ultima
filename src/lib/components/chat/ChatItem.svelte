<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { SidebarItem, Dropdown, DropdownItem } from 'flowbite-svelte';
  import {
    MessagesOutline,
    TrashBinOutline,
    EditOutline,
    DotsVerticalOutline,
    MapPinOutline
  } from 'flowbite-svelte-icons';
  import type { Chat } from '$lib/types';

  export let chat: Chat;
  export let isActive = false;

  const dispatch = createEventDispatcher();

  let dropdownOpen = false;

  function formatDate(dateStr: string): string {
    const date = new Date(dateStr);
    const now = new Date();
    const diff = now.getTime() - date.getTime();

    if (diff < 86400000) return 'Today';
    if (diff < 172800000) return 'Yesterday';
    return date.toLocaleDateString();
  }
</script>

<SidebarItem
  label={chat.title}
  active={isActive}
  on:click={() => dispatch('select')}
  class="group"
>
  <svelte:fragment slot="icon">
    {#if chat.is_pinned}
      <MapPinOutline class="w-4 h-4 text-primary-500" />
    {:else}
      <MessagesOutline class="w-4 h-4" />
    {/if}
  </svelte:fragment>

  <svelte:fragment slot="subtext">
    <span class="text-xs text-gray-500">{formatDate(chat.updated_at)}</span>
  </svelte:fragment>

  <button
    slot="arrow"
    class="opacity-0 group-hover:opacity-100 p-1 rounded hover:bg-gray-200 dark:hover:bg-gray-600"
    on:click|stopPropagation={() => (dropdownOpen = !dropdownOpen)}
  >
    <DotsVerticalOutline class="w-4 h-4" />
  </button>
</SidebarItem>

{#if dropdownOpen}
  <div class="relative">
    <Dropdown class="w-36 absolute right-0 z-10" on:click={() => dropdownOpen = false}>
      <DropdownItem on:click={() => dispatch('rename')}>
        <EditOutline class="w-4 h-4 mr-2" slot="start" />
        Rename
      </DropdownItem>
      <DropdownItem on:click={() => dispatch('delete')} class="text-red-500">
        <TrashBinOutline class="w-4 h-4 mr-2" slot="start" />
        Delete
      </DropdownItem>
    </Dropdown>
  </div>
{/if}
