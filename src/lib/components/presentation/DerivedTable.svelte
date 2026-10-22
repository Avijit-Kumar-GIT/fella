<script lang="ts">
	import type { BoundResult } from '../../presentation/types';
	import { derivedRowsResult, exactValue } from '../../presentation/results';
	import { formatPresentationValue, presentationColumnLabel, type PresentationFormatOptions } from '../../presentation/format';
	import ResultTable from './ResultTable.svelte';
	let { source, rows, fields = [], columnFormats = {} }: { source: BoundResult; rows: unknown[]; fields?: string[]; columnFormats?: Record<string, PresentationFormatOptions> } = $props();
	let result = $derived(derivedRowsResult(source, rows));
	let columns = $derived(fields.length ? fields : result.columns);
	let page = $state(0);
	const pageSize = 50;
	let indices = $derived(result.rows.map((_, index) => index));
	let pages = $derived(Math.max(1, Math.ceil(indices.length / pageSize)));
	let current = $derived(Math.min(page, pages - 1));
	let visible = $derived(indices.slice(current * pageSize, (current + 1) * pageSize));
</script>

<div class="derived-table">
	<p class="note">Display-derived rows · based on saved result {source.id}; not a new verified analysis.</p>
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div class="table-scroll" role="region" aria-label="Display-derived rows" tabindex="0">
		<table><caption class="sr-only">Rows derived locally from saved result {source.id}</caption>
			<thead><tr>{#each columns as field}<th scope="col">{presentationColumnLabel(field, columnFormats[field])}</th>{/each}</tr></thead>
			<tbody>
				{#each visible as rowIndex}
					<tr>
						{#each columns as field}
							{@const exact = exactValue(result, rowIndex, field)}
							{@const column = result.columns.indexOf(field)}
							{@const sourceValue = result.rows[rowIndex][column] === null ? null : exact}
							{@const formatted = formatPresentationValue(sourceValue, columnFormats[field])}
							<td title={formatted.formatted ? `Exact derived value: ${formatted.exact}` : undefined}>
								{formatted.display}
								{#if formatted.formatted && formatted.exact !== formatted.display}<span class="sr-only">Exact derived value: {formatted.exact}</span>{/if}
							</td>
						{/each}
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	<p class="extent">{result.rows.length} display row{result.rows.length === 1 ? '' : 's'}{#if !source.complete} · Source result is incomplete ({source.rows.length} of {source.row_count} saved rows).{/if}</p>
	{#if pages > 1}<nav aria-label="Display rows pages"><button disabled={current === 0} onclick={() => page = current - 1}>Previous</button><span>{current + 1} / {pages}</span><button disabled={current >= pages - 1} onclick={() => page = current + 1}>Next</button></nav>{/if}
	<details><summary>Exact saved source values</summary><ResultTable result={source} /></details>
</div>

<style>
	.derived-table { min-width: 0; }
	.note, .extent { margin: .4rem 0; color: var(--text-faint); font-size: .73rem; line-height: 1.5; }
	details { margin-top: .65rem; color: var(--text-dim); font-size: .76rem; }
	summary { cursor: pointer; }
	details :global(.result-table) { margin-top: .5rem; }
	.table-scroll { overflow: auto; max-height: 28rem; border-radius: var(--radius-sm); outline-offset: 3px; }
	table { width: 100%; border-collapse: collapse; text-align: left; font-size: .8rem; font-variant-numeric: tabular-nums; }
	th, td { padding: .6rem .8rem; vertical-align: top; border-bottom: 1px solid var(--border); white-space: pre-wrap; overflow-wrap: anywhere; max-width: 30rem; }
	th { position: sticky; top: 0; background: var(--bg-raised); color: var(--text-dim); font-weight: 500; }
	nav { display: flex; align-items: center; gap: .8rem; margin-top: .6rem; font-size: .75rem; }
	button { border-radius: 999px; border: 1px solid var(--border); padding: .35rem .75rem; color: var(--text); background: var(--bg-raised); cursor: pointer; }
	button:disabled { opacity: .4; cursor: default; }
	.sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
</style>
