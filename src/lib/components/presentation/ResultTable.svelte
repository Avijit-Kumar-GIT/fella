<script lang="ts">
	import type { BoundResult } from '../../presentation/types';
	import { exactValue } from '../../presentation/results';
	import { formatPresentationValue, presentationColumnLabel, type PresentationFormatOptions } from '../../presentation/format';
	import Icon from '../Icon.svelte';
	let { result, fields = result.columns, indices = result.rows.map((_, row) => row), columnFormats = {}, exactValueLabel = 'Exact saved value', caption = `Exact values from saved result ${result.id}`, filterFields = fields, onrowfilter }: { result: BoundResult; fields?: string[]; indices?: number[]; columnFormats?: Record<string, PresentationFormatOptions>; exactValueLabel?: string; caption?: string; filterFields?: string[]; onrowfilter?: (row: number) => void } = $props();
	let numericFields = $derived(new Set(result.columns.filter((field, column) => result.rows.some(row => typeof row[column] === 'number') && result.rows.every(row => row[column] === null || typeof row[column] === 'number'))));
	let page = $state(0);
	const pageSize = 50;
	let pages = $derived(Math.max(1, Math.ceil(indices.length / pageSize)));
	let current = $derived(Math.min(page, pages - 1));
	let visible = $derived(indices.slice(current * pageSize, (current + 1) * pageSize));
</script>
<div class="result-table">
	{#if !visible.length}<p class="sr-only" role="status">No saved rows match the current filters.</p>{/if}
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div class="table-scroll presentation-table-scroll" role="region" aria-label="Saved result values" tabindex="0">
		<table class:comparison-table={fields.length > 3 && numericFields.size >= fields.length - 1}><caption class="sr-only">{caption}</caption>
			<thead><tr>{#each fields as field}<th scope="col" class:numeric={numericFields.has(field)}>{presentationColumnLabel(field, columnFormats[field])}</th>{/each}{#if onrowfilter}<th scope="col"><span class="sr-only">Related views</span></th>{/if}</tr></thead>
			<tbody>
				{#if !visible.length}<tr><td class="empty-row" colspan={fields.length + (onrowfilter ? 1 : 0)}>No saved rows match the current filters.</td></tr>{/if}
				{#each visible as row}
					<tr>
						{#each fields as field}
							{@const exact = exactValue(result, row, field)}
							{@const column = result.columns.indexOf(field)}
							{@const sourceValue = result.rows[row][column] === null ? null : exact}
							{@const formatted = formatPresentationValue(sourceValue, columnFormats[field])}
							<td class:numeric={numericFields.has(field)} data-label={presentationColumnLabel(field, columnFormats[field])} title={formatted.formatted ? `${exactValueLabel}: ${formatted.exact}` : undefined}>
								{formatted.display}
								{#if formatted.formatted && formatted.exact !== formatted.display}<span class="sr-only">{exactValueLabel}: {formatted.exact}</span>{/if}
							</td>
						{/each}
						{#if onrowfilter}<td class="row-filter-cell"><button class="row-filter" aria-label={`Filter related views by ${filterFields.map(field => `${presentationColumnLabel(field, columnFormats[field])}: ${exactValue(result, row, field)}`).join(', ')}`} title="Filter related views" onclick={() => onrowfilter?.(row)}><Icon name="sliders" size={13} /><span>Filter</span></button></td>{/if}
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	{#if !result.complete || pages > 1}<p class="extent">{indices.length} saved {indices.length === 1 ? 'row' : 'rows'}{#if !result.complete} · Incomplete result ({result.row_count} total rows); this is not the full dataset.{/if}</p>{/if}
	{#if pages > 1}<nav class="presentation-result-nav" aria-label="Result pages"><button disabled={current === 0} onclick={() => page = current - 1}>Previous</button><span>{current + 1} / {pages}</span><button disabled={current >= pages - 1} onclick={() => page = current + 1}>Next</button></nav>{/if}
</div>
<style>
	:global(.presentation-table-scroll table.comparison-table th:first-child) { width:28%; }
	:global(.presentation-table-scroll .numeric) { text-align:right; }
	.result-table { min-width: 0; max-width: 100%; }
	.extent { margin: var(--space-2) 0 0; color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.5; }
	.sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
	.row-filter-cell { width:1%; white-space:nowrap; }
	.row-filter { display:inline-flex; align-items:center; gap:4px; padding:4px 6px; border:0; border-radius:var(--radius-sm); background:transparent; color:var(--text-faint); font:inherit; font-size:var(--fs-xs); cursor:pointer; }
	.row-filter:hover { background:var(--bg-inset); color:var(--text); }
	.row-filter:focus-visible { outline:2px solid var(--brand); outline-offset:2px; }
	.empty-row { padding:18px 12px; color:var(--text-faint); text-align:center; }
	@container (max-width:420px) { .row-filter span { position:absolute; width:1px; height:1px; overflow:hidden; clip-path:inset(50%); white-space:nowrap; } }
</style>
