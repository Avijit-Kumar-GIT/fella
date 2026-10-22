<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { isReactiveAssign } from '@openuidev/lang-core';
	import { usePresentation } from './context';
	type Option = { value: string; label: string };
	let { props }: ComponentRenderProps<{ name: string; options: Option[]; value: unknown }> = $props();
	const context = usePresentation();
	let binding = $derived(isReactiveAssign(props.value) ? props.value : null);
	let selected = $derived(String(binding ? context.variables[binding.target] ?? '' : props.value ?? ''));
	function choose(value: string) { if (binding) context.updateVariable(binding.target, value); }
</script>
<fieldset><legend>{props.name}</legend><div class="segments presentation-segments" role="group" aria-label={props.name}>{#each props.options as option (option.value)}<button type="button" aria-pressed={selected === option.value} onclick={() => choose(option.value)}>{option.label}</button>{/each}</div></fieldset>
<style>
	fieldset { min-width: 0; max-width: 100%; padding: 0; margin: 0; border: 0; }
	legend { max-width: 100%; margin-bottom: var(--space-1); color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.segments { width: fit-content; }
	button { min-width: 0; }
</style>
