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
<label class="select-control"><span>{props.name}</span><select value={selected} onchange={(event) => change(event.currentTarget.value)}>
	{#each props.options as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
</select></label>
<style>
	.select-control { display: grid; gap: .38rem; min-width: 10rem; max-width: 100%; color: var(--text-dim); font-size: .77rem; }
	select { width: 100%; max-width: 100%; min-height: 2.35rem; padding: .45rem .75rem; border: 1px solid var(--border); border-radius: var(--radius, 10px); color: var(--text); background: var(--bg-raised); font: inherit; font-size: .84rem; color-scheme: light dark; }
	select:focus-visible { outline: 2px solid color-mix(in srgb, var(--accent) 55%, transparent); outline-offset: 1px; }
</style>
