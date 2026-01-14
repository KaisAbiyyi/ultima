<!--
  Provider Selector Component

  Dropdown for selecting AI provider with visual indicators
-->
<script lang="ts">
	import { Select } from 'flowbite-svelte';
	import {
		type AgentProvider,
		PROVIDER_OPTIONS,
		getProviderOption
	} from '$lib/types';

	type Props = {
		value?: AgentProvider;
		onValueChange?: (value: AgentProvider) => void;
		disabled?: boolean;
	};

	let { value = $bindable('local' as AgentProvider), onValueChange, disabled = false }: Props = $props();

	/** Create items for the select */
	const selectItems = $derived(
		PROVIDER_OPTIONS.map(option => ({
			value: option.value,
			name: option.label
		}))
	);

	/** Handle selection change */
	function handleChange(event: Event) {
		const target = event.target as HTMLSelectElement;
		const newValue = target.value as AgentProvider;
		if (newValue && newValue !== value) {
			value = newValue;
			onValueChange?.(value);
		}
	}
</script>

<Select
	items={selectItems}
	bind:value
	onchange={handleChange}
	{disabled}
	class="w-full"
/>
