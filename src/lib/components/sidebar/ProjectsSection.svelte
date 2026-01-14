<script lang="ts">
    import { Button, Modal, Input, Label } from "flowbite-svelte";
    import {
        FolderPlusOutline,
        PlusOutline,
        FolderOutline,
        ChevronRightOutline,
        DotsVerticalOutline,
        EditOutline,
        TrashBinOutline,
    } from "flowbite-svelte-icons";
    import { projectStore } from "$lib/stores/project.store";
    import { chatStore } from "$lib/stores/chat.store";
    import { derived } from "svelte/store";
    import { goto } from "$app/navigation";
    import type { Project } from "$lib/types";

    export let collapsed = false;

    // Max 5 projects sorted by most recent chat activity
    const sidebarProjects = derived(
        [projectStore, chatStore],
        ([$projects, $chats]) => {
            const projectScores = new Map<string, number>();

            // Score by most recent chat activity
            $chats.chats.forEach((chat) => {
                if (chat.project_id) {
                    const time = new Date(chat.updated_at).getTime();
                    const current = projectScores.get(chat.project_id) ?? 0;
                    projectScores.set(chat.project_id, Math.max(current, time));
                }
            });

            return $projects.projects
                .sort(
                    (a, b) =>
                        (projectScores.get(b.id) ?? 0) -
                        (projectScores.get(a.id) ?? 0),
                )
                .slice(0, 5);
        },
    );

    const hasMoreProjects = derived(
        projectStore,
        ($store) => $store.projects.length > 5,
    );

    let showCreateModal = false;
    let showAllModal = false;
    let newProjectName = "";
    let searchQuery = "";
    let openMenuId: string | null = null;

    function openCreateModal() {
        console.log("Opening create project modal");
        showCreateModal = true;
    }

    async function handleCreateProject() {
        if (!newProjectName.trim()) return;
        console.log("Creating project:", newProjectName);
        const result = await projectStore.create({
            name: newProjectName.trim(),
        });
        if (result) {
            newProjectName = "";
            showCreateModal = false;
        }
    }

    function selectProject(project: Project) {
        goto(`/project/${project.id}`);
    }

    function toggleMenu(e: MouseEvent, projectId: string) {
        e.stopPropagation();
        openMenuId = openMenuId === projectId ? null : projectId;
    }

    async function handleRenameProject(project: Project) {
        openMenuId = null;
        const newName = prompt("Enter new project name:", project.name);
        if (newName && newName.trim()) {
            await projectStore.update(project.id, { name: newName.trim() });
        }
    }

    async function handleDeleteProject(project: Project) {
        openMenuId = null;
        if (
            confirm(
                `Delete project "${project.name}"? Chats will not be deleted.`,
            )
        ) {
            await projectStore.delete(project.id);
        }
    }

    function handleClickOutside() {
        openMenuId = null;
    }

    $: filteredAllProjects = $projectStore.projects.filter((p) =>
        p.name.toLowerCase().includes(searchQuery.toLowerCase()),
    );
</script>

<svelte:window onclick={handleClickOutside} />

{#if !collapsed}
    <div class="px-3 py-2 border-b border-gray-200 dark:border-gray-700">
        <!-- Header -->
        <div class="flex items-center justify-between mb-2">
            <span
                class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider"
            >
                Projects
            </span>
            <button
                type="button"
                class="text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors p-1"
                onclick={openCreateModal}
                aria-label="Create Project"
            >
                <PlusOutline class="w-4 h-4" />
            </button>
        </div>

        <!-- Project list (max 5) -->
        {#each $sidebarProjects as project (project.id)}
            <div class="relative group">
                <div
                    class="w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm transition-colors cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-700"
                    role="button"
                    tabindex="0"
                    onclick={() => selectProject(project)}
                    onkeydown={(e) =>
                        e.key === "Enter" && selectProject(project)}
                >
                    <FolderOutline class="w-4 h-4 shrink-0" />
                    <span class="truncate flex-1 text-left">{project.name}</span
                    >
                    <button
                        type="button"
                        class="shrink-0 p-0.5 rounded opacity-0 group-hover:opacity-100 hover:bg-gray-200 dark:hover:bg-gray-600"
                        onclick={(e) => toggleMenu(e, project.id)}
                    >
                        <DotsVerticalOutline class="w-3 h-3" />
                    </button>
                </div>

                <!-- Dropdown menu -->
                {#if openMenuId === project.id}
                    <div
                        class="absolute right-0 top-full mt-1 z-50 w-32 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg py-1"
                        role="menu"
                        tabindex="-1"
                        onclick={(e) => e.stopPropagation()}
                        onkeydown={(e) =>
                            e.key === "Escape" && (openMenuId = null)}
                    >
                        <button
                            type="button"
                            class="w-full flex items-center gap-2 px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"
                            onclick={() => handleRenameProject(project)}
                        >
                            <EditOutline class="w-4 h-4" />
                            Rename
                        </button>
                        <button
                            type="button"
                            class="w-full flex items-center gap-2 px-3 py-2 text-sm text-red-600 hover:bg-gray-100 dark:hover:bg-gray-700"
                            onclick={() => handleDeleteProject(project)}
                        >
                            <TrashBinOutline class="w-4 h-4" />
                            Delete
                        </button>
                    </div>
                {/if}
            </div>
        {/each}

        <!-- View All button -->
        {#if $hasMoreProjects}
            <button
                class="w-full flex items-center gap-2 px-2 py-1.5 rounded text-sm text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-700"
                onclick={() => (showAllModal = true)}
            >
                <ChevronRightOutline class="w-4 h-4" />
                <span>View All Projects...</span>
            </button>
        {/if}
    </div>
{/if}

<!-- Create Project Modal -->
<Modal bind:open={showCreateModal} size="sm" title="New Project">
    <div class="space-y-4">
        <div>
            <Label for="new-project-name">Project Name</Label>
            <Input
                id="new-project-name"
                placeholder="e.g., Work, Personal, Research"
                bind:value={newProjectName}
                onkeydown={(e) => e.key === "Enter" && handleCreateProject()}
            />
        </div>
    </div>
    <svelte:fragment slot="footer">
        <Button onclick={handleCreateProject} disabled={!newProjectName.trim()}>
            Create Project
        </Button>
        <Button color="alternative" onclick={() => (showCreateModal = false)}>
            Cancel
        </Button>
    </svelte:fragment>
</Modal>

<!-- View All Projects Modal -->
<Modal bind:open={showAllModal} size="md" title="All Projects">
    <div class="space-y-4">
        <!-- Search -->
        <Input placeholder="Search projects..." bind:value={searchQuery} />

        <!-- Project list -->
        <div class="space-y-1 max-h-96 overflow-y-auto">
            {#each filteredAllProjects as project (project.id)}
                <div class="flex items-center gap-2">
                    <button
                        class="flex-1 flex items-center gap-2 px-3 py-2 rounded text-sm transition-colors hover:bg-gray-100 dark:hover:bg-gray-700"
                        onclick={() => {
                            selectProject(project);
                            showAllModal = false;
                        }}
                    >
                        <FolderOutline class="w-4 h-4 shrink-0" />
                        <span class="truncate flex-1 text-left"
                            >{project.name}</span
                        >
                    </button>
                    <button
                        type="button"
                        class="p-1 text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
                        onclick={() => handleRenameProject(project)}
                    >
                        <EditOutline class="w-4 h-4" />
                    </button>
                    <button
                        type="button"
                        class="p-1 text-red-500 hover:text-red-700"
                        onclick={() => handleDeleteProject(project)}
                    >
                        <TrashBinOutline class="w-4 h-4" />
                    </button>
                </div>
            {:else}
                <p class="text-center text-gray-500 dark:text-gray-400 py-4">
                    {searchQuery ? "No projects found" : "No projects yet"}
                </p>
            {/each}
        </div>
    </div>

    <svelte:fragment slot="footer">
        <Button
            onclick={() => {
                showAllModal = false;
                showCreateModal = true;
            }}
        >
            <FolderPlusOutline class="w-4 h-4 mr-2" />
            Create Project
        </Button>
        <Button color="alternative" onclick={() => (showAllModal = false)}>
            Close
        </Button>
    </svelte:fragment>
</Modal>
