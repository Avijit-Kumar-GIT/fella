<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { exactRowFilterValues, exactRowFilterValuesFromRow, filteredRows, incompatibleInteractionFilters, requireResult } from '../../presentation/results';
	import { renderBoundChart, type ChartView } from '../../presentation/vega-runtime';
	import { usePresentation } from './context';
	import ResultTable from './ResultTable.svelte';
	let { props }: BaseComponentProps<{ resultId: string; spec: Record<string, unknown>; title?: string; interactionGroup?: string; linkBy?: string[] }> = $props();
	const context = usePresentation();
	let result = $derived(requireResult(context.results, props.resultId));
	let replayed = $derived(context.replayedResultIds.has(result.id));
	let chartData = $derived(result);
	let linkBy = $derived(props.linkBy ?? []);
	let indices = $derived(filteredRows(result, context.state, props.interactionGroup, linkBy, context.interactionScope));
	let incompatibleFilters = $derived(incompatibleInteractionFilters(result, context.state, props.interactionGroup, linkBy, context.interactionScope));
	let container: HTMLDivElement;
	let status = $state('Loading visualization…');
	let failed = $state(false);
	$effect(() => {
		const source = chartData;
		const spec = props.spec;
		const group = props.interactionGroup;
		const linkedFields = linkBy;
		const rows = indices;
		const target = container;
		if (!target) return;
		if (!rows.length) { target.replaceChildren(); failed = false; status = 'No saved rows match the current filters.'; return; }
		let disposed = false;
		let view: ChartView | undefined;
		status = 'Loading visualization…'; failed = false;
		target.replaceChildren();
		const mount = document.createElement('div');
		const styles = getComputedStyle(target);
		const series = ['--chart-violet', '--chart-cyan', '--chart-slate', '--chart-plum', '--chart-orange', '--chart-green', '--chart-gold']
			.map((token) => styles.getPropertyValue(token).trim()).filter(Boolean);
		const colors = { text: styles.getPropertyValue('--text-dim').trim() || '#777777', grid: styles.getPropertyValue('--border').trim() || '#888888', accent: styles.getPropertyValue('--link').trim() || '#648fbe', series };
		let markClick: ((event: unknown, item?: { datum?: unknown } | null) => void) | undefined;
		renderBoundChart(mount, spec, source, rows, colors, target.clientWidth).then((created) => {
			if (disposed) created.finalize();
			else {
				target.replaceChildren(mount); view = created; status = '';
				if (group && linkedFields.length) {
					markClick = (_event, item) => {
						const selected = exactRowFilterValues(source, item?.datum, linkedFields);
						if (selected.length) context.selectRows(source.id, group, selected);
					};
					created.addEventListener('click', markClick);
				}
			}
		}).catch((error) => {
			if (!disposed) { failed = true; status = `Visualization unavailable: ${error instanceof Error ? error.message : 'rendering failed'}. Saved values are available below.`; }
		});
		let timer: ReturnType<typeof setTimeout>;
		const observer = new ResizeObserver(() => {
			clearTimeout(timer);
			timer = setTimeout(() => {
				if (!disposed && view && typeof spec.width !== 'number' && !['facet', 'repeat', 'concat', 'hconcat', 'vconcat'].some((key) => key in spec)) void view.width(Math.max(160, Math.min(target.clientWidth - 70, 1200))).resize().runAsync().catch(() => {});
			}, 80);
		});
		observer.observe(target);
		return () => { disposed = true; clearTimeout(timer); observer.disconnect(); if (view && markClick) view.removeEventListener('click', markClick); view?.finalize(); };
	});
</script>
<figure class="presentation-chart">
	{#if props.title}<figcaption>{props.title}</figcaption>{/if}
	<div class="chart-scroll" aria-label={props.title ?? 'Visualization of saved results'} role="img"><div class="chart-target presentation-chart-target" bind:this={container}></div></div>
	{#if status}<p class="status" role="status">{status}</p>{/if}
	{#if incompatibleFilters.length}<p class="filter-limitation" role="status">This output is unchanged by {incompatibleFilters.map(filter => filter.field).join(', ')} because those fields are not available here.</p>{/if}
	{#if !result.complete}<p class="scope">{#if replayed}The filtered query matched {result.row_count} rows; {result.rows.length} are shown.{:else}This visualization uses {result.rows.length} saved rows, not the full saved result ({result.row_count} rows).{/if}</p>{/if}
	<p class="scope">{#if replayed}Replayed the saved read-only query against its recorded source revision. Exact values remain available below.{:else}The visualization uses saved result rows. Exact source values remain available below.{/if}</p>
	{#if props.interactionGroup && linkBy.length}<p class="scope keyboard-hint">To filter by keyboard, open exact saved values and choose a row.</p>{/if}
	<details open={failed}><summary>{replayed ? 'Exact filtered values' : 'Exact saved values'}</summary><ResultTable {result} {indices} filterFields={linkBy} rowSourceLabel={replayed ? 'replayed' : 'saved'} exactValueLabel={replayed ? 'Exact replayed value' : 'Exact saved value'} caption={`${replayed ? 'Exact values from replayed source query' : 'Exact values from saved result'} ${result.id}`} onrowfilter={props.interactionGroup && linkBy.length ? row => context.selectRows(result.id, props.interactionGroup, exactRowFilterValuesFromRow(result, row, linkBy)) : undefined} /></details>
</figure>
<style>
	figure { margin: 0; }
	figcaption { margin-bottom: var(--space-2); font-size: var(--fs-lg); font-weight: 600; }
	.chart-scroll { width: 100%; max-width: 100%; max-height: 48rem; overflow-x: hidden; overflow-y: auto; }
	.chart-target { width: 100%; max-width: 100%; min-width: 0; }
	.status, .scope, .filter-limitation { margin: var(--space-2) 0; color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.5; }
	details { margin-top: var(--space-2); }
	summary { width: fit-content; max-width: 100%; padding: 5px 0; color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; cursor: pointer; }
</style>
