<script lang="ts">
  import { Button, Input, Modal, Label } from 'flowbite-svelte';
  import { FolderPlusOutline, FolderOutline } from 'flowbite-svelte-icons';
  import { projectStore, rootProjects, selectedProjectId } from '$lib/stores/project.store';
  import ProjectItem from './ProjectItem.svelte';

  let showModal = false;
  let newProjectName = '';

  async function handleCreateProject() {
    if (!newProjectName.trim()) return;

    const result = await projectStore.create({
      name: newProjectName.trim(),
    });

    if (result) {
      newProjectName = '';
      showModal = false;
    }
  }
</script>

<div class="py-3 border-b border-gray-200 dark:border-gray-700">
  <div class="flex items-center justify-between px-3 mb-2">
    <span class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">
      Projects
    </span>
    <Button size="xs" color="alternative" on:click={() => (showModal = true)}>
      <FolderPlusOutline class="w-3 h-3" slot="start" />
    </Button>
  </div>

  <div class="space-y-0.5 px-2">
    <!-- All Chats (default) -->
    <button
      class="w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm hover:bg-gray-100 dark:hover:bg-gray-700 {$selectedProjectId === null
        ? 'bg-gray-100 dark:bg-gray-700'
        : ''}"
      on:click={() => projectStore.select(null)}
    >
      <FolderOutline class="w-4 h-4" />
      <span>All Chats</span>
    </button>

    {#each $rootProjects as project (project.id)}
      <ProjectItem {project} depth={0} />
    {/each}
  </div>
</div>

<!-- Create Project Modal -->
<Modal bind:open={showModal} size="sm" title="New Project">
  <div class="space-y-4">
    <div>
      <Label for="project-name">Project Name</Label>
      <Input
        id="project-name"
        placeholder="e.g., Coding, Creative, Research"
        bind:value={newProjectName}
        on:keydown={(e) => e.key === 'Enter' && handleCreateProject()}
      />
    </div>
  </div>

  <svelte:fragment slot="footer">
    <Button on:click={handleCreateProject} disabled={!newProjectName.trim()}>
      Create Project
    </Button>
    <Button color="alternative" on:click={() => (showModal = false)}>
      Cancel
    </Button>
  </svelte:fragment>
</Modal>
