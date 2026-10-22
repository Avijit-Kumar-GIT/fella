<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { isReactiveAssign } from '@openuidev/lang-core';
	import { usePresentation } from './context';
	let { props }: ComponentRenderProps<{ label: string; checked: unknown }> = $props();
	const context = usePresentation();
	let binding = $derived(isReactiveAssign(props.checked) ? props.checked : null);
	let checked = $derived(Boolean(binding ? context.variables[binding.target] : props.checked));
	function change(value: boolean) { if (binding) context.updateVariable(binding.target, value); }
</script>
<label><input type="checkbox" checked={checked} onchange={(event) => change(event.currentTarget.checked)} /><span>{props.label}</span></label>
<style>
	label { display: inline-flex; align-items: center; gap: .5rem; width: fit-content; max-width: 100%; color: var(--text-dim); font-size: .82rem; cursor: pointer; } input { width: 1rem; height: 1rem; margin: 0; accent-color: var(--accent); } span { overflow-wrap: anywhere; }
</style>
