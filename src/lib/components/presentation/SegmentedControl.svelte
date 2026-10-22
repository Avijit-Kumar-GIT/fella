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
<fieldset><legend>{props.name}</legend><div class="segments" role="group" aria-label={props.name}>{#each props.options as option (option.value)}<button type="button" aria-pressed={selected === option.value} onclick={() => choose(option.value)}>{option.label}</button>{/each}</div></fieldset>
<style>
	fieldset { min-width: 0; padding: 0; margin: 0; border: 0; } legend { margin-bottom: .35rem; color: var(--text-dim); font-size: .76rem; }
	.segments { display: flex; flex-wrap: wrap; width: fit-content; max-width: 100%; gap: .18rem; padding: .2rem; border-radius: 999px; background: var(--bg-inset); }
	button { min-height: 2rem; padding: .35rem .75rem; border: 0; border-radius: 999px; color: var(--text-dim); background: transparent; font: inherit; font-size: .78rem; cursor: pointer; }
	button[aria-pressed='true'] { color: var(--text); background: var(--bg-raised); box-shadow: 0 1px 2px #0002; }
</style>
