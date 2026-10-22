<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { filteredRows, requireResult } from '../../presentation/results';
	import { renderBoundChart, type ChartView } from '../../presentation/vega-runtime';
	import { usePresentation } from './context';
	import ResultTable from './ResultTable.svelte';
	let { props }: ComponentRenderProps<{ resultId: string; spec: Record<string, unknown>; title?: string }> = $props();
	const context = usePresentation();
	let result = $derived(requireResult(context.results, props.resultId));
	let indices = $derived(filteredRows(result, context.state));
	let container: HTMLDivElement;
	let status = $state('Loading visualization…');
	let failed = $state(false);
	$effect(() => {
		const source = result;
		const spec = props.spec;
		const rows = indices;
		const target = container;
		if (!target) return;
		let disposed = false;
		let view: ChartView | undefined;
		status = 'Loading visualization…'; failed = false;
		target.replaceChildren();
		const mount = document.createElement('div');
		const styles = getComputedStyle(target);
		const colors = { text: styles.getPropertyValue('--text-dim').trim() || '#777777', grid: styles.getPropertyValue('--border').trim() || '#888888', accent: styles.getPropertyValue('--accent').trim() || '#648fbe' };
		renderBoundChart(mount, spec, source, rows, colors, target.clientWidth).then((created) => {
			if (disposed) created.finalize();
			else { target.replaceChildren(mount); view = created; status = ''; }
		}).catch((error) => {
			if (!disposed) { failed = true; status = `Visualization unavailable: ${error instanceof Error ? error.message : 'rendering failed'}. Saved values are available below.`; }
		});
		let timer: ReturnType<typeof setTimeout>;
		const observer = new ResizeObserver(() => {
			clearTimeout(timer);
			timer = setTimeout(() => {
				if (!disposed && view && !['facet', 'repeat', 'concat', 'hconcat', 'vconcat'].some((key) => key in spec)) void view.width(Math.max(160, Math.min(target.clientWidth - 70, 1200))).resize().runAsync().catch(() => {});
			}, 80);
		});
		observer.observe(target);
		return () => { disposed = true; clearTimeout(timer); observer.disconnect(); view?.finalize(); };
	});
</script>
<figure>
	{#if props.title}<figcaption>{props.title}</figcaption>{/if}
	<div class="chart-scroll" aria-label={props.title ?? 'Visualization of saved results'} role="img"><div class="chart-target" bind:this={container}></div></div>
	{#if status}<p class="status" role="status">{status}</p>{/if}
	{#if !result.complete}<p class="scope">This visualization uses {result.rows.length} saved rows, not the full dataset ({result.row_count} rows).</p>{/if}
	<p class="scope">Display mappings may summarize saved rows. Exact source values remain available below.</p>
	<details open={failed}><summary>Exact saved values</summary><ResultTable {result} {indices} /></details>
</figure>
<style>
	figure { margin: 0; min-width: 0; } figcaption { font-size: .85rem; font-weight: 550; margin-bottom: .65rem; }
	.chart-scroll { overflow: auto; max-height: 48rem; } .chart-target { min-width: 200px; width: 100%; }
	.chart-target :global(canvas) { display: block; } .status, .scope { color: var(--text-faint); font-size: .75rem; line-height: 1.5; margin: .4rem 0; }
	details { margin-top: .5rem; } summary { cursor: pointer; width: fit-content; color: var(--text-dim); font-size: .75rem; padding: .3rem 0; }
</style>
