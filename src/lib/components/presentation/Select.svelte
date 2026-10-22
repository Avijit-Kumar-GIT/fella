<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { isReactiveAssign } from '@openuidev/lang-core';
	import { usePresentation } from './context';
	type Option = { value: string; label: string };
	let { props }: ComponentRenderProps<{ name: string; options: Option[]; value: unknown }> = $props();
	const context = usePresentation();
	let binding = $derived(isReactiveAssign(props.value) ? props.value : null);
	let selected = $derived(String(binding ? context.variables[binding.target] ?? '' : props.value ?? ''));
	function change(value: string) { if (binding) context.updateVariable(binding.target, value); }
</script>
<label class="select-control presentation-select"><span>{props.name}</span><select value={selected} onchange={(event) => change(event.currentTarget.value)}>
	{#each props.options as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
</select></label>
<style>
	.select-control { display: grid; gap: var(--space-1); min-width: 0; max-width: 100%; color: var(--text-dim); font-size: var(--fs-xs); }
	select { width: 100%; min-width: 0; padding: 7px 11px; font: inherit; color-scheme: light dark; }
</style>
