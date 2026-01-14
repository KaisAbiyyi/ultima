<!--
  Delete Confirmation Dialog

  Confirms agent deletion with warning
-->
<script lang="ts">
	import { Modal, Button } from 'flowbite-svelte';
	import { ExclamationCircleOutline } from 'flowbite-svelte-icons';
	import type { Agent } from '$lib/types';

	type Props = {
		open?: boolean;
		agent?: Agent | null;
		loading?: boolean;
		onConfirm?: () => void;
		onCancel?: () => void;
	};

	let { open = $bindable(false), agent = null, loading = false, onConfirm, onCancel }: Props = $props();

	/** Handle confirm */
	function handleConfirm() {
		if (!loading) {
			onConfirm?.();
			open = false;
		}
	}

	/** Handle cancel */
	function handleCancel() {
		if (!loading) {
			onCancel?.();
			open = false;
		}
	}
</script>

<Modal bind:open size="xs" outsideclose={!loading} dismissable={!loading}>
	<div class="text-center">
		<ExclamationCircleOutline class="mx-auto mb-4 h-12 w-12 text-gray-400 dark:text-gray-200" />
		<h3 class="mb-2 text-lg font-medium text-gray-900 dark:text-white">Delete Agent</h3>
		<p class="mb-5 text-sm text-gray-500 dark:text-gray-400">
			{#if agent}
				Are you sure you want to delete <strong>{agent.name}</strong>?
				This action cannot be undone.
			{:else}
				Are you sure you want to delete this agent?
			{/if}
		</p>
		<div class="flex justify-center gap-4">
			<Button color="alternative" disabled={loading} onclick={handleCancel}>
				Cancel
			</Button>
			<Button color="red" disabled={loading} onclick={handleConfirm}>
				{#if loading}
					Deleting...
				{:else}
					Delete
				{/if}
			</Button>
		</div>
	</div>
</Modal>
