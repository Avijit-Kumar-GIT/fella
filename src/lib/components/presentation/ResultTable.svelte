<script lang="ts">
	import type { BoundResult } from '../../presentation/types';
	import { exactValue } from '../../presentation/results';
	import { formatPresentationValue, presentationColumnLabel, type PresentationFormatOptions } from '../../presentation/format';
	let { result, fields = result.columns, indices = result.rows.map((_, row) => row), columnFormats = {}, exactValueLabel = 'Exact saved value', caption = `Exact values from saved result ${result.id}` }: { result: BoundResult; fields?: string[]; indices?: number[]; columnFormats?: Record<string, PresentationFormatOptions>; exactValueLabel?: string; caption?: string } = $props();
	let numericFields = $derived(new Set(result.columns.filter((field, column) => result.rows.some(row => typeof row[column] === 'number') && result.rows.every(row => row[column] === null || typeof row[column] === 'number'))));
	let page = $state(0);
	const pageSize = 50;
	let pages = $derived(Math.max(1, Math.ceil(indices.length / pageSize)));
	let current = $derived(Math.min(page, pages - 1));
	let visible = $derived(indices.slice(current * pageSize, (current + 1) * pageSize));
</script>
<div class="result-table">
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div class="table-scroll presentation-table-scroll" role="region" aria-label="Saved result values" tabindex="0">
		<table class:comparison-table={fields.length > 3 && numericFields.size >= fields.length - 1}><caption class="sr-only">{caption}</caption>
			<thead><tr>{#each fields as field}<th scope="col" class:numeric={numericFields.has(field)}>{presentationColumnLabel(field, columnFormats[field])}</th>{/each}</tr></thead>
			<tbody>
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
</style>
