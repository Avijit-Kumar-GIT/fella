<script lang="ts">
	import type { BoundResult } from '../../presentation/types';
	import { exactValue } from '../../presentation/results';
	let { result, fields = result.columns, indices = result.rows.map((_, row) => row) }: { result: BoundResult; fields?: string[]; indices?: number[] } = $props();
	let page = $state(0);
	const pageSize = 50;
	let pages = $derived(Math.max(1, Math.ceil(indices.length / pageSize)));
	let current = $derived(Math.min(page, pages - 1));
	let visible = $derived(indices.slice(current * pageSize, (current + 1) * pageSize));
</script>
<div class="result-table">
	<!-- Keyboard users must be able to scroll a wide saved-result table. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div class="table-scroll" role="region" aria-label="Saved result values" tabindex="0">
		<table><caption class="sr-only">Exact values from saved result {result.id}</caption>
			<thead><tr>{#each fields as field}<th scope="col">{field}</th>{/each}</tr></thead>
			<tbody>{#each visible as row}<tr>{#each fields as field}<td>{exactValue(result, row, field)}</td>{/each}</tr>{/each}</tbody>
		</table>
	</div>
	<p class="extent">{indices.length} saved {indices.length === 1 ? 'row' : 'rows'}{#if !result.complete} · Incomplete result ({result.row_count} total rows); this is not the full dataset.{/if}</p>
	{#if pages > 1}<nav aria-label="Result pages"><button disabled={current === 0} onclick={() => page = current - 1}>Previous</button><span>{current + 1} / {pages}</span><button disabled={current >= pages - 1} onclick={() => page = current + 1}>Next</button></nav>{/if}
</div>
<style>
	.result-table { min-width: 0; } .table-scroll { overflow: auto; max-height: 28rem; border-radius: var(--radius-sm); outline-offset: 3px; }
	table { width: 100%; border-collapse: collapse; text-align: left; font-size: .8rem; font-variant-numeric: tabular-nums; }
	th, td { padding: .6rem .8rem; vertical-align: top; border-bottom: 1px solid var(--border); white-space: pre-wrap; overflow-wrap: anywhere; max-width: 30rem; }
	th { position: sticky; top: 0; background: var(--bg-raised); color: var(--text-dim); font-weight: 500; }
	.extent { margin: .5rem 0 0; font-size: .75rem; color: var(--text-faint); } nav { display: flex; align-items: center; gap: .8rem; margin-top: .6rem; font-size: .75rem; }
	button { border-radius: 999px; border: 1px solid var(--border); padding: .35rem .75rem; color: var(--text); background: var(--bg-raised); cursor: pointer; } button:disabled { opacity: .4; cursor: default; }
	.sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
</style>
