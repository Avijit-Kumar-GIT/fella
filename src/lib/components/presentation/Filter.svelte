<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { exactValue, requireResult, type FilterSelection } from '../../presentation/results';
	import { usePresentation } from './context';
	let { props }: ComponentRenderProps<{ resultId: string; field: string; name: string; title?: string }> = $props();
	const context = usePresentation();
	let menu: HTMLDetailsElement;
	let result = $derived(requireResult(context.results, props.resultId));
	let options = $derived([...new Set(result.rows.map((_, row) => exactValue(result, row, props.field)))]);
	let selected = $derived((context.state[`filter:${props.name}`] as FilterSelection | undefined)?.value ?? null);
	function select(value: string | null) { context.update(`filter:${props.name}`, { resultId: props.resultId, field: props.field, value }); if (menu) menu.open = false; }
</script>
<div class="filter">
	<span class="label">{props.title ?? props.field}</span>
	<details bind:this={menu}><summary>{selected ?? 'All saved values'}<span aria-hidden="true">⌄</span></summary>
		<div class="options" role="group" aria-label={props.title ?? props.field}>
			<button aria-pressed={selected === null} onclick={() => select(null)}>All saved values</button>
			{#each options as value}<button aria-pressed={selected === value} onclick={() => select(value)}>{value}</button>{/each}
		</div>
	</details>
	{#if !result.complete}<span class="scope">Filters saved rows only, not the full dataset.</span>{/if}
</div>
<style>
	.filter { display: flex; align-items: center; gap: .65rem; flex-wrap: wrap; font-size: .8rem; }
	.label, .scope { color: var(--text-dim); } .scope { font-size: .75rem; }
	details { position: relative; max-width: 100%; } summary { display: flex; align-items: center; gap: 1rem; cursor: pointer; padding: .4rem .8rem; border: 1px solid var(--border); background: var(--bg-raised); border-radius: 999px; list-style: none; overflow-wrap: anywhere; } summary::-webkit-details-marker { display: none; }
	.options { position: absolute; z-index: 5; top: calc(100% + .3rem); min-width: 13rem; max-width: min(24rem, 80cqw); max-height: 18rem; overflow: auto; padding: .3rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-raised); box-shadow: 0 10px 30px #0002; }
	button { display: block; text-align: left; width: 100%; padding: .5rem .65rem; border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--text); overflow-wrap: anywhere; cursor: pointer; }
	button:hover, button[aria-pressed='true'] { background: var(--bg-inset); }
</style>
