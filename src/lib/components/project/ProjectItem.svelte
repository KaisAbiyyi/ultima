<script lang="ts">
  import { ChevronDownOutline, ChevronRightOutline, FolderOutline } from 'flowbite-svelte-icons';
  import { projectStore } from '$lib/stores/project.store';
  import type { Project } from '$lib/types';

  export let project: Project;
  export let depth = 0;

  $: children = projectStore.getChildren(project.id);
  $: isSelected = $projectStore.selectedProjectId === project.id;

  let expanded = false;
</script>

<div>
  <button
    class="w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors {isSelected
      ? 'bg-primary-100 dark:bg-primary-900'
      : ''}"
    style="padding-left: {depth * 16 + 8}px"
    on:click={() => projectStore.select(project.id)}
  >
    <!-- Expand/Collapse -->
    {#if children.length > 0}
      <div
        class="w-4 h-4 flex items-center justify-center cursor-pointer"
        on:click|stopPropagation={() => (expanded = !expanded)}
      >
        {#if expanded}
          <ChevronDownOutline class="w-3 h-3" />
        {:else}
          <ChevronRightOutline class="w-3 h-3" />
        {/if}
      </div>
    {:else}
      <span class="w-4"></span>
    {/if}

    <!-- Icon -->
    {#if project.icon}
      <span>{project.icon}</span>
    {:else}
      <FolderOutline
        class="w-4 h-4"
        style={project.color ? `color: ${project.color}` : ''}
      />
    {/if}

    <!-- Name -->
    <span class="truncate flex-1 text-left">{project.name}</span>
  </button>

  <!-- Children (recursive) -->
  {#if expanded && children.length > 0}
    {#each children as child (child.id)}
      <svelte:self project={child} depth={depth + 1} />
    {/each}
  {/if}
</div>
