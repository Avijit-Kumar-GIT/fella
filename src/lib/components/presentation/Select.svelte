<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { usePresentation } from './context';
	type Option = { value: string; label: string };
	let { props }: BaseComponentProps<{ name: string; options: Option[]; stateKey: string; defaultValue?: string }> = $props();
	const context = usePresentation();
	let selected = $derived(String(context.state[`control:${props.stateKey}`] ?? props.defaultValue ?? props.options[0]?.value ?? ''));
	function change(value: string) { context.update(`control:${props.stateKey}`, value); }
</script>
<label class="select-control presentation-select"><span>{props.name}</span><select value={selected} onchange={(event) => change(event.currentTarget.value)}>
	{#each props.options as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
</select></label>
<style>
	.select-control { display: grid; gap: var(--space-1); min-width: 0; max-width: 100%; color: var(--text-dim); font-size: var(--fs-xs); }
	select { width: 100%; min-width: 0; padding: 7px 11px; font: inherit; color-scheme: light dark; }
</style>
