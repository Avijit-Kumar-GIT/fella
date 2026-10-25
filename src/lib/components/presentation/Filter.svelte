<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { exactValue, requireResult, type FilterSelection } from '../../presentation/results';
	import { usePresentation } from './context';
	let { props }: BaseComponentProps<{ resultId: string; field: string; name: string; title?: string }> = $props();
	const context = usePresentation();
	let menu: HTMLDetailsElement;
	let result = $derived(requireResult(context.results, props.resultId));
	let options = $derived([...new Set(result.rows.map((_, row) => exactValue(result, row, props.field)))]);
	let selected = $derived((context.state[`filter:${props.name}`] as FilterSelection | undefined)?.value ?? null);
	function select(value: string | null) { context.update(`filter:${props.name}`, { resultId: props.resultId, field: props.field, value }); if (menu) menu.open = false; }
</script>
<div class="filter presentation-filter">
	<span class="label">{props.title ?? props.field}</span>
	<details bind:this={menu}><summary>{selected ?? 'All saved values'}<span aria-hidden="true">⌄</span></summary>
		<div class="options presentation-filter-options" role="group" aria-label={props.title ?? props.field}>
			<button aria-pressed={selected === null} onclick={() => select(null)}>All saved values</button>
			{#each options as value}<button aria-pressed={selected === value} onclick={() => select(value)}>{value}</button>{/each}
		</div>
	</details>
	{#if !result.complete}<span class="scope">Filters saved rows only, not the full dataset.</span>{/if}
</div>
<style>
	.filter { display: flex; min-width: 0; max-width: 100%; align-items: center; flex-wrap: wrap; gap: var(--space-2); font-size: var(--fs-sm); }
	.label, .scope { color: var(--text-dim); overflow-wrap: anywhere; }
	.scope { font-size: var(--fs-xs); }
	details { position: relative; min-width: 0; max-width: 100%; }
	summary { display: flex; min-height: 36px; max-width: 100%; align-items: center; justify-content: space-between; gap: var(--space-3); padding: 6px 12px; list-style: none; overflow-wrap: anywhere; cursor: pointer; }
	summary::-webkit-details-marker { display: none; }
	.options { position: absolute; z-index: 5; top: calc(100% + var(--space-1)); left: 0; max-height: 18rem; overflow-x: hidden; overflow-y: auto; padding: var(--space-1); }
	button { display: block; width: 100%; padding: 7px 9px; border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--text); font-size: var(--fs-sm); text-align: left; overflow-wrap: anywhere; cursor: pointer; }
	button:hover, button[aria-pressed='true'] { background: var(--bg-inset); }
</style>
