<script lang="ts">
	import type { AnalyticalDiffConsumer, AnalyticalDiffDimension, AnalyticalDiffRevision, ArtifactComparison, NumericDelta } from '$lib/analytics-artifacts';
	let { comparison, busy = false, error = '', onadopt, onkeep, onopenrevision, onopenconsumer }: {
		comparison: ArtifactComparison;
		busy?: boolean;
		error?: string;
		onadopt?: (revision: number) => void;
		onkeep?: () => void;
		onopenrevision?: (revision: number) => void;
		onopenconsumer?: (consumer: AnalyticalDiffConsumer) => void;
	} = $props();
	let metadata = $derived(comparison.analytical_diff);
	let resultPages = $state<Record<string, number>>({});
	let sections = $derived([
		{ name: 'Meaning', dimension: metadata?.meaning },
		{ name: 'Inputs', dimension: metadata?.inputs },
		{ name: 'Method', dimension: metadata?.method },
		{ name: 'Results', dimension: metadata?.results }
	]);
	function revisionLabel(revision: AnalyticalDiffRevision | undefined, fallback: number): string {
		const role = revision?.role;
		if (!role) return `Revision ${fallback}`;
		return `${role === 'current' ? 'Current' : role === 'proposed' ? 'Proposed' : 'Historical'} · revision ${revision?.revision ?? fallback}`;
	}
	let beforeLabel = $derived(revisionLabel(metadata?.before, comparison.before_revision));
	let afterLabel = $derived(revisionLabel(metadata?.after, comparison.after_revision));
	function status(dimension: AnalyticalDiffDimension) { return dimension.status === 'changed' ? 'Changed' : dimension.status === 'unchanged' ? 'No recorded change' : 'Not available'; }
	function deltaLabel(delta: NumericDelta): string {
		if (delta.kind === 'unavailable') return `Unavailable: ${delta.reason}`;
		return `${delta.kind === 'finite_float' ? 'Approx. ' : ''}${delta.delta}`;
	}
	const pageSize = 50;
</script>

<section class="analytical-diff" aria-label="Analytical comparison" aria-busy={busy}>
	<header class="diff-header">
		<div><h3>Review analytical changes</h3><p>Saved records only. Nothing is rerun or adopted by opening this comparison.</p></div>
		<div class="revision-pair">
			{#if onopenrevision}<button type="button" class="revision" disabled={busy} onclick={() => onopenrevision?.(comparison.before_revision)}>{beforeLabel}</button>{:else}<span class="revision">{beforeLabel}</span>{/if}
			<span aria-hidden="true">→</span>
			{#if onopenrevision}<button type="button" class="revision" disabled={busy} onclick={() => onopenrevision?.(comparison.after_revision)}>{afterLabel}</button>{:else}<span class="revision">{afterLabel}</span>{/if}
		</div>
		{#if metadata?.after.change_note}<p class="change-note">{metadata.after.change_note}</p>{/if}
	</header>
	{#if error}<p class="error" role="alert">{error}</p>{/if}
	{#each sections as section}
		<section class="dimension" aria-label={section.name}>
			<div class="dimension-heading"><h4>{section.name}</h4>{#if section.dimension}<span class="dimension-status" data-status={section.dimension.status}>{status(section.dimension)}</span>{/if}</div>
			{#if section.dimension}
				<p class="summary">{section.dimension.summary}</p>
				{@const changed = section.dimension.items.filter(item => item.changed)}
				{#if changed.length && section.name !== 'Method'}
					<dl class="changes">{#each changed.slice(0, 4) as item}<div class="change"><dt>{item.label}</dt><dd class="before"><span class="value-label">Before</span>{item.before ?? 'Not recorded'}</dd><dd class="after"><span class="value-label">After</span>{item.after ?? 'Not recorded'}</dd></div>{/each}</dl>
				{/if}
				{#if section.dimension.items.length}
					<details class="record-details"><summary>{section.name === 'Method' ? 'Inspect saved methods and presentation' : 'All recorded details'}</summary>
						<dl class="changes">{#each section.dimension.items as item}<div class="change" data-changed={item.changed}><dt>{item.label}{#if item.changed}<span class="item-status">Changed</span>{/if}</dt><dd class="before"><span class="value-label">Before</span>{item.before || (item.before === '' ? 'None recorded' : 'Not recorded')}</dd><dd class="after"><span class="value-label">After</span>{item.after || (item.after === '' ? 'None recorded' : 'Not recorded')}</dd></div>{/each}</dl>
					</details>
				{/if}
				{#if section.dimension.limitations.length}<ul class="limitations">{#each section.dimension.limitations as limitation}<li>{limitation}</li>{/each}</ul>{/if}
			{:else}<p class="summary">Detailed metadata was not recorded for this comparison.</p>{/if}
			{#if section.name === 'Results'}
				{#if !comparison.compatible}<p class="incompatible">These results cannot be compared numerically.</p><ul class="limitations">{#each comparison.reasons as reason}<li>{reason}</li>{/each}</ul>
				{:else}
					{#each comparison.tables as table, index}
						{@const key = `${comparison.before_revision}:${comparison.after_revision}:${index}`}
						{@const pages = Math.max(1, Math.ceil(table.values.length / pageSize))}
						{@const page = Math.min(resultPages[key] ?? 0, pages - 1)}
						<div class="values">
							<h5>{table.after_result_id}</h5>
							<!-- A focusable scroll region is intentional for keyboard users. -->
							<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
							<div class="table-scroll" role="region" aria-label={`Exact saved differences for ${table.after_result_id}`} tabindex="0"><table>
								<thead><tr><th scope="col">{table.key_fields.join(' · ') || 'Result'}</th><th scope="col">Measure</th><th scope="col">Before</th><th scope="col">After</th><th scope="col">Change</th></tr></thead>
								<tbody>{#each table.values.slice(page * pageSize, (page + 1) * pageSize) as value}<tr><th scope="row">{value.key.join(' · ') || 'All rows'}</th><td>{value.field}{value.unit ? ` (${value.unit})` : ''}</td><td>{value.before}</td><td>{value.after}</td><td>{deltaLabel(value.delta)}</td></tr>{/each}</tbody>
							</table></div>
							{#if pages > 1}<nav aria-label={`Difference pages for ${table.after_result_id}`}><button type="button" disabled={busy || page === 0} onclick={() => resultPages = { ...resultPages, [key]: page - 1 }}>Previous</button><span>{page + 1} / {pages}</span><button type="button" disabled={busy || page >= pages - 1} onclick={() => resultPages = { ...resultPages, [key]: page + 1 }}>Next</button></nav>{/if}
						</div>
					{/each}
				{/if}
			{/if}
		</section>
	{/each}
	<section class="dimension" aria-label="Impact"><div class="dimension-heading"><h4>Impact</h4><span class="dimension-status">{metadata?.impact.scanned ? 'Explicit references' : 'Not scanned'}</span></div>
		<p class="summary">{metadata?.impact.summary ?? 'Dependencies have not been scanned.'}</p>
		{#if metadata?.impact.consumers.length}<ul class="consumers">{#each metadata.impact.consumers as consumer}<li>{#if onopenconsumer}<button type="button" class="consumer" disabled={busy} onclick={() => onopenconsumer?.(consumer)}>{consumer.document_title}{#if consumer.checkpoint_title} · checkpoint: {consumer.checkpoint_title}{/if}<span>Revision {consumer.pinned_revision} remains pinned</span></button>{:else}<span>{consumer.document_title}{#if consumer.checkpoint_title} · checkpoint: {consumer.checkpoint_title}{/if} · revision {consumer.pinned_revision} remains pinned</span>{/if}</li>{/each}</ul>{/if}
		{#if metadata?.impact.limitations.length}<ul class="limitations">{#each metadata.impact.limitations as limitation}<li>{limitation}</li>{/each}</ul>{/if}
	</section>
	{#if comparison.warnings.length}<details class="warnings"><summary>Comparison limitations</summary><ul class="limitations">{#each comparison.warnings as warning}<li>{warning}</li>{/each}</ul></details>{/if}
	{#if onkeep || onadopt && metadata?.after.role === 'proposed'}<footer>
		<p>Previous revisions and pinned outputs stay available.</p><div class="actions">
			{#if onkeep}<button type="button" disabled={busy} onclick={() => onkeep?.()}>Keep current revision</button>{/if}
			{#if onadopt && metadata?.after.role === 'proposed'}<button type="button" class="primary" disabled={busy} onclick={() => onadopt?.(comparison.after_revision)}>{busy ? 'Saving…' : 'Use proposed revision'}</button>{/if}
		</div>
	</footer>{/if}
</section>

<style>
	.analytical-diff { container-type: inline-size; min-width: 0; color: var(--text); font-size: .82rem; line-height: 1.55; }
	h3, h4, h5, p, dl, dd { margin: 0; } h3 { font-size: 1rem; font-weight: 600; letter-spacing: -.015em; } h4 { font-size: .9rem; font-weight: 580; } h5 { font-weight: 500; margin-bottom: .5rem; }
	.diff-header { padding-bottom: 1.3rem; } .diff-header p, .change-note { color: var(--text-dim); margin-top: .35rem; } .revision-pair { display: flex; align-items: center; flex-wrap: wrap; gap: .5rem; margin-top: .9rem; color: var(--text-faint); }
	.revision { display: inline-block; border-radius: 999px; padding: .3rem .7rem; background: var(--bg-inset); color: var(--text); font-size: .75rem; }
	.dimension { padding: .9rem 0 1.2rem; } .dimension-heading { display: flex; align-items: baseline; flex-wrap: wrap; gap: .65rem; margin-bottom: .35rem; } .dimension-status { color: var(--text-faint); font-size: .73rem; } .dimension-status[data-status='changed'] { color: var(--text); }
	.summary, .limitations, footer p { color: var(--text-dim); } .summary { max-width: 75ch; } .record-details { margin-top: .65rem; } summary { width: fit-content; cursor: pointer; font-size: .76rem; color: var(--text-dim); padding: .2rem 0; }
	.changes { margin-top: .65rem; display: grid; gap: .9rem; } .change { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); column-gap: 1rem; row-gap: .25rem; } dt { grid-column: 1 / -1; font-size: .76rem; font-weight: 550; } dd { white-space: pre-wrap; overflow-wrap: anywhere; max-height: 18rem; overflow: auto; }
	.before { color: var(--text-dim); } .value-label { display: block; color: var(--text-faint); font-size: .68rem; margin-bottom: .2rem; } .item-status { font-weight: 400; font-size: .68rem; color: var(--text-faint); margin-left: .5rem; }
	.limitations { margin: .6rem 0 0; padding-left: 1.1rem; font-size: .73rem; } .limitations li + li { margin-top: .2rem; } .values { margin-top: .9rem; min-width: 0; } .table-scroll { overflow: auto; max-height: 27rem; border-radius: var(--radius-sm); outline-offset: 3px; }
	table { border-collapse: collapse; width: 100%; font-size: .76rem; font-variant-numeric: tabular-nums; } th, td { padding: .55rem .7rem; border-bottom: 1px solid var(--border); text-align: left; vertical-align: top; } th { font-weight: 500; } thead th { position: sticky; top: 0; background: var(--bg-raised); color: var(--text-dim); } td { overflow-wrap: anywhere; min-width: 6rem; max-width: 24rem; }
	nav { display: flex; gap: .6rem; align-items: center; margin-top: .6rem; font-size: .73rem; } .incompatible { margin-top: .65rem; font-weight: 550; } .consumers { list-style: none; margin: .6rem 0 0; padding: 0; display: grid; gap: .4rem; }
	.consumer { display: block; text-align: left; padding: .4rem 0; border: 0; background: transparent; color: var(--text); } .consumer span { display: block; font-size: .7rem; color: var(--text-faint); }
	button { cursor: pointer; border: 1px solid var(--border); border-radius: 999px; padding: .4rem .8rem; color: var(--text); background: var(--bg-raised); font: inherit; font-size: .76rem; } button:disabled { opacity: .5; cursor: default; } button:hover:not(:disabled) { background: var(--bg-inset); } .primary { background: var(--accent); border-color: transparent; color: var(--bg-raised); } .primary:hover:not(:disabled) { background: var(--text-dim); }
	footer { margin-top: 1.2rem; display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: .8rem; } footer p { font-size: .73rem; } .actions { display: flex; flex-wrap: wrap; gap: .5rem; } .warnings { margin-top: .4rem; } .error { margin: .6rem 0; color: var(--danger); }
	@container (max-width: 520px) { .change { grid-template-columns: minmax(0, 1fr); } dt { grid-column: 1; } dd + dd { margin-top: .4rem; } footer { align-items: stretch; } .actions { width: 100%; } }
</style>
