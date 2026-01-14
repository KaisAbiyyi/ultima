<script lang="ts">
    import {
        StarSolid,
        TrashBinOutline,
        EditOutline,
        DotsVerticalOutline,
        MessagesOutline,
    } from "flowbite-svelte-icons";
    import { Badge } from "flowbite-svelte";
    import type { Chat, Project } from "$lib/types";

    export let chat: Chat;
    export let project: Project | null = null;
    export let isActive = false;
    export let onselect: () => void = () => {};
    export let ondelete: () => void = () => {};
    export let onpin: () => void = () => {};
    export let onrename: () => void = () => {};

    let menuOpen = false;

    function formatTime(dateStr: string): string {
        const date = new Date(dateStr);
        const now = new Date();
        const diffMs = now.getTime() - date.getTime();
        const diffMins = Math.floor(diffMs / 60000);
        const diffHours = Math.floor(diffMs / 3600000);
        const diffDays = Math.floor(diffMs / 86400000);

        if (diffMins < 1) return "Just now";
        if (diffMins < 60) return `${diffMins}m ago`;
        if (diffHours < 24) return `${diffHours}h ago`;
        if (diffDays === 1) return "Yesterday";
        if (diffDays < 7) return `${diffDays}d ago`;
        return date.toLocaleDateString();
    }

    function handleSelect() {
        onselect();
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onselect();
        }
    }

    function toggleMenu(e: MouseEvent) {
        e.stopPropagation();
        menuOpen = !menuOpen;
    }

    function handlePin(e: MouseEvent) {
        e.stopPropagation();
        menuOpen = false;
        onpin();
    }

    function handleRename(e: MouseEvent) {
        e.stopPropagation();
        menuOpen = false;
        onrename();
    }

    function handleDelete(e: MouseEvent) {
        e.stopPropagation();
        menuOpen = false;
        ondelete();
    }

    // Close menu on outside click
    function handleClickOutside() {
        menuOpen = false;
    }
</script>

<svelte:window onclick={handleClickOutside} />

<div
    class="relative w-full flex items-center gap-2 px-2 py-2 rounded-lg text-sm transition-colors group cursor-pointer
    {isActive
        ? 'bg-primary-100 dark:bg-primary-900 text-primary-700 dark:text-primary-300'
        : 'hover:bg-gray-100 dark:hover:bg-gray-700'}"
    role="button"
    tabindex="0"
    onclick={handleSelect}
    onkeydown={handleKeydown}
>
    <!-- Icon -->
    <MessagesOutline class="w-4 h-4 shrink-0 opacity-60" />

    <!-- Content -->
    <div class="flex-1 min-w-0 text-left">
        <div class="flex items-center gap-2">
            {#if chat.is_pinned}
                <StarSolid class="w-3 h-3 shrink-0 text-primary-500" />
            {/if}
            <span class="truncate font-medium">{chat.title}</span>
            {#if project}
                <Badge size="xs" color="primary" class="shrink-0"
                    >{project.name}</Badge
                >
            {/if}
        </div>
        <div class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
            {formatTime(chat.updated_at)}
        </div>
    </div>

    <!-- Actions button -->
    <button
        type="button"
        class="shrink-0 p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-gray-200 dark:hover:bg-gray-600 transition-opacity"
        onclick={toggleMenu}
    >
        <DotsVerticalOutline class="w-4 h-4" />
    </button>

    <!-- Dropdown menu -->
    {#if menuOpen}
        <div
            class="absolute right-0 top-full mt-1 z-50 w-36 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg py-1"
            role="menu"
            tabindex="-1"
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => e.key === "Escape" && (menuOpen = false)}
        >
            <button
                type="button"
                class="w-full flex items-center gap-2 px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"
                onclick={handlePin}
            >
                <StarSolid class="w-4 h-4" />
                {chat.is_pinned ? "Unpin" : "Pin"}
            </button>
            <button
                type="button"
                class="w-full flex items-center gap-2 px-3 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"
                onclick={handleRename}
            >
                <EditOutline class="w-4 h-4" />
                Rename
            </button>
            <button
                type="button"
                class="w-full flex items-center gap-2 px-3 py-2 text-sm text-red-600 hover:bg-gray-100 dark:hover:bg-gray-700"
                onclick={handleDelete}
            >
                <TrashBinOutline class="w-4 h-4" />
                Delete
            </button>
        </div>
    {/if}
</div>
