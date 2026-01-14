<!--
  Model File Picker Component

  File selector for .gguf model files using Tauri dialog
-->
<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { Button } from 'flowbite-svelte';
  import { CloseOutline, FileOutline } from 'flowbite-svelte-icons';

  type Props = {
    value?: string;
    disabled?: boolean;
    label?: string;
  };

  let { value = $bindable(''), disabled = false, label = 'Model File' }: Props = $props();

  /** Extract filename from path */
  const displayName = $derived(value ? value.split(/[\/\\]/).pop() : '');

  /** Open file picker dialog */
  async function openFilePicker() {
    if (disabled) return;

    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: 'GGUF Models',
            extensions: ['gguf'],
          },
          {
            name: 'All Files',
            extensions: ['*'],
          },
        ],
        title: 'Select Model File',
      });

      if (selected && typeof selected === 'string') {
        value = selected;
      }
    } catch (err) {
      console.error('Failed to open file picker:', err);
    }
  }

  /** Clear selected file */
  function clearSelection() {
    value = '';
  }
</script>

<div class="space-y-2">
  <label for="model-file" class="text-sm font-medium text-gray-900 dark:text-white">
    {label}
  </label>

  <div class="flex gap-2">
    <div class="flex-1 min-w-0">
      {#if value}
        <div
          class="flex items-center gap-2 px-3 py-2 border rounded-lg bg-gray-100 dark:bg-gray-800 border-gray-300 dark:border-gray-600 truncate"
          title={value}
        >
          <FileOutline class="w-4 h-4 shrink-0 text-gray-500 dark:text-gray-400" />
          <span class="truncate text-sm text-gray-900 dark:text-white">{displayName}</span>
        </div>
      {:else}
        <div
          class="flex items-center px-3 py-2 border rounded-lg border-dashed border-gray-300 dark:border-gray-600 text-gray-500 dark:text-gray-400"
        >
          <span class="text-sm">No file selected</span>
        </div>
      {/if}
    </div>

    <div class="flex gap-1 shrink-0">
      <Button
        color="alternative"
        size="sm"
        onclick={openFilePicker}
        {disabled}
      >
        Browse
      </Button>

      {#if value}
        <Button
          color="light"
          size="sm"
          onclick={clearSelection}
          {disabled}
        >
          <CloseOutline class="w-4 h-4" />
        </Button>
      {/if}
    </div>
  </div>

  {#if value}
    <p class="text-xs text-gray-500 dark:text-gray-400 truncate" title={value}>
      {value}
    </p>
  {/if}
</div>
