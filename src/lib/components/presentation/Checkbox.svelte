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
<label class="presentation-checkbox"><input type="checkbox" checked={checked} onchange={(event) => change(event.currentTarget.checked)} /><span>{props.label}</span></label>
<style>
	label { display: inline-flex; min-width: 0; align-items: flex-start; gap: var(--space-2); width: fit-content; max-width: 100%; color: var(--text-dim); font-size: var(--fs-sm); cursor: pointer; }
	input { width: 16px; height: 16px; flex: none; margin: 2px 0 0; accent-color: var(--brand); }
	span { overflow-wrap: anywhere; }
</style>
