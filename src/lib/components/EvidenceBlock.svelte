<script lang="ts">
	import type { Answer, ContextSection, EvidenceItem } from '$lib/types';
	import Icon from './Icon.svelte';
	import ReplayStatus from './ReplayStatus.svelte';

	let {
		answer,
		expanded = false,
		bodyId,
		onrerun
	}: { answer: Answer; expanded?: boolean; bodyId: string; onrerun?: () => Promise<void> } = $props();

	const COMPLETE_TABLE_ROWS = 100;
	const CONTEXT_LABEL: Record<ContextSection, string> = {
		user_context: 'user-provided context',
		workspace_schema: 'workspace schema',
		workspace_model: 'workspace definitions',
		conversation: 'conversation history',
		folder_memory: 'workspace memory'
	};
	let hasToolEvidence = $derived(
		answer.provenance
			? answer.provenance.evidence_ids.length > 0
			: answer.evidence.some((item) => !item.error)
	);
	let providedContext = $derived(
		(answer.provenance?.context_sections ?? []).map((section) => CONTEXT_LABEL[section])
	);

	// Which steps have their raw detail (SQL, table, output) revealed.
	let openDetail = $state<Record<number, boolean>>({});
	function toggleDetail(i: number) {
		openDetail = { ...openDetail, [i]: !openDetail[i] };
	}

	// Plain-language fallback when the model didn't write a note for a step.
	const FALLBACK: Record<string, string> = {
		run_sql: 'Ran a query',
		describe_schema: 'Looked at the columns',
		sample_rows: 'Looked at some rows',
		grep_files: 'Searched your documents for a word or phrase',
		read_file: 'Read one of your files',
		run_python: 'Ran a calculation',
		forecast_analysis: 'Forecasted and backtested a time series',
		list_files: 'Listed your files',
		make_chart: 'Drew a chart'
	};
	function stepLabel(e: EvidenceItem): string {
		if (e.note?.trim()) return e.note.trim();
		if (FALLBACK[e.tool]) return FALLBACK[e.tool];
		return e.tool;
	}

	// The model's `note` is already the step's headline drop it from the raw
	// args dump so it isn't shown twice.
	function argsWithoutNote(args: Record<string, unknown>): Record<string, unknown> {
		const { note: _note, ...rest } = args;
		return rest;
	}
	function sourceLabel(e: EvidenceItem): string {
		return (e.sources ?? []).map((s) => `${s.source} (${s.table})`).join(', ');
	}

</script>

	{#if expanded}
		<div class="evidence-body" id={bodyId}>
			<div class="basis">
				<div class="checks-heading">Basis</div>
				{#if hasToolEvidence}
					<div class="basis-copy">Workspace tool results are linked to the steps below.</div>
				{:else if answer.provenance}
					<div class="basis-copy">No workspace tool result is attached; this is a model response informed by general knowledge and any context listed here.</div>
				{:else}
					<div class="basis-copy">Source details were not recorded for this archived answer.</div>
				{/if}
				{#if providedContext.length}
					<div class="basis-copy">Context supplied to the model: {providedContext.join(', ')}.</div>
				{/if}
				{#if answer.provenance?.clarification_of}
					<div class="basis-copy">This analysis continues from your clarification.</div>
				{/if}
			</div>
			{#if answer.evidence.length}
			<ol class="steps">
				{#each answer.evidence as e, i (e.id ?? `evidence-${i}`)}
					{@const shownArgs = argsWithoutNote(e.args)}
					{@const sqlIsAlreadyInInputTrace = e.python_input_trace?.queries.some((query) => query.sql === e.sql) ?? false}
					{@const hasDetail =
						!!e.sql ||
						!!e.sources?.length ||
						!!e.python_input_trace ||
						Object.keys(shownArgs).length > 0 ||
						!!e.output ||
						!!(e.columns && e.rows)}
					<li class="step" class:failed={!!e.error}>
						<span class="line">{stepLabel(e)}</span>
						{#if e.sources?.length}
							<div class="source-line">from {sourceLabel(e)}</div>
						{/if}

						{#if e.error}
							<div class="steperr">didn't work: {e.error}</div>
						{/if}

						{#if hasDetail}
							<button
								class="detailtoggle"
								onclick={() => toggleDetail(i)}
								aria-expanded={!!openDetail[i]}
							>
								{openDetail[i] ? 'hide' : e.sql ? 'show the query' : 'show details'}
							</button>
						{/if}

						{#if openDetail[i] && hasDetail}
							{@const visibleRows =
								e.columns && e.rows && e.row_count != null && e.row_count <= COMPLETE_TABLE_ROWS
									? e.rows
									: e.rows?.slice(0, 20)}
							<div class="detail rich">
								{#if e.sql}
									{#if !sqlIsAlreadyInInputTrace}<pre class="sql">{e.sql}</pre>{/if}
								{:else if Object.keys(shownArgs).length > 0}
									<dl class="args">
										{#each Object.entries(shownArgs) as [k, v] (k)}
											<dt>{k}</dt>
											<dd>{typeof v === 'string' ? v : JSON.stringify(v)}</dd>
										{/each}
									</dl>
								{/if}
								{#if e.python_input_trace}
									<div class="python-inputs">
										<div class="input-heading">Data read by this calculation</div>
						{#if !e.python_input_trace.complete}
							<div class="input-warning">Input trace is incomplete; not all reads may be inspectable or replayable.</div>
						{/if}
						{#if !e.python_input_trace.queries.length}
							<div class="input-warning">No SQL input was recorded for this calculation.</div>
						{/if}
										{#each e.python_input_trace.queries as query, qi (qi)}
											<pre class="sql">{query.sql}</pre>
											<div class="input-meta">
												{query.row_count} rows · {query.columns.join(', ') || 'no columns'}{query.truncated ? ' · result capped' : ''}
											</div>
										{/each}
									</div>
								{/if}
								{#if !e.error}
									<div class="result">{e.result_summary}</div>
									{#if e.output}
										<pre class="output">{e.output}</pre>
									{/if}
									{#if e.columns && e.rows}
										<div class="tablewrap">
											<table>
												<thead>
													<tr>{#each e.columns as c (c)}<th>{c}</th>{/each}</tr>
												</thead>
												<tbody>
													{#each visibleRows ?? [] as row, ri (ri)}
														<tr>{#each row as cell, ci (ci)}<td>{cell}</td>{/each}</tr>
													{/each}
												</tbody>
											</table>
										</div>
										{#if e.row_count != null && visibleRows && visibleRows.length < e.row_count}
											<p class="table-note">Showing {visibleRows.length} of {e.row_count} rows.</p>
										{/if}
									{/if}
								{/if}
							</div>
						{/if}
					</li>
				{/each}
			</ol>
			{/if}

			{#if answer.verification.length}
				<div class="checks-heading">Checks</div>
				<div class="verify">
					{#each answer.verification as v (v.label)}
						<div class="check">
							<span
								class="mark"
								class:ok={v.ok}
								class:bad={!v.ok}
								role="img"
								aria-label={v.ok ? 'Passed' : 'Issue'}
							>
								<Icon name={v.ok ? 'check' : 'alert'} size={12} />
							</span>
							<span>{v.label}</span>
							{#if v.detail}<span class="detail-note">— {v.detail}</span>{/if}
						</div>
					{/each}
				</div>
			{/if}

			{#if answer.turn_id && answer.evidence.length}
				<ReplayStatus turnId={answer.turn_id} {onrerun} />
			{/if}
		</div>
	{/if}

<style>
	.evidence-body {
		margin: var(--space-3) 0 2px;
		padding: var(--space-3) 0 var(--space-2) 10px;
		border-top: 1px solid var(--border);
		border-left: 2px solid var(--border);
		display: flex;
		flex-direction: column;
		font-size: var(--fs-sm);
		gap: 10px;
	}
	.basis {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.basis-copy {
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.steps {
		margin: 0;
		padding: 0 0 0 1.6em;
		display: flex;
		flex-direction: column;
		gap: 9px;
	}
	.step {
		color: var(--text-dim);
	}
	.step::marker {
		color: var(--text-faint);
	}
	.line {
		color: var(--text);
	}
	.source-line {
		margin-top: 2px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.step.failed .line {
		color: var(--warn);
	}
	.steperr {
		color: var(--err);
		margin-top: 2px;
	}
	.detailtoggle {
		display: block;
		margin-top: 3px;
		padding: 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		background: transparent;
	}
	.detailtoggle:hover {
		color: var(--text-dim);
	}
	.detail {
		margin-top: 4px;
	}
	.python-inputs {
		margin: 8px 0;
		padding-top: 7px;
		border-top: 1px solid var(--border);
	}
	.input-heading {
		margin-bottom: 4px;
		color: var(--text-dim);
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	.input-meta,
	.input-warning {
		margin: 3px 0 8px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.input-warning {
		color: var(--warn);
	}
	.args {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 1px var(--space-3);
		margin: var(--space-1) 0;
	}
	.args dt {
		font-family: var(--mono);
		font-size: var(--fs-xs);
		color: var(--text-faint);
	}
	.args dd {
		margin: 0;
		font-family: var(--mono);
		font-size: var(--fs-xs);
		color: var(--text-dim);
		overflow-wrap: anywhere;
	}
	/* pre / table / th / td come from the shared `.rich` rules in app.css;
	   only these overrides are local. */
	.detail :global(pre.sql) {
		color: var(--text);
	}
	.detail :global(pre.output) {
		color: var(--text-dim);
		max-height: 260px;
		overflow: auto;
	}
	.result {
		color: var(--text-dim);
		margin: 2px 0;
	}
	.tablewrap {
		overflow-x: auto;
		margin-top: 4px;
	}
	.table-note {
		margin: 3px 0 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.detail :global(td) {
		white-space: nowrap;
	}
	.checks-heading {
		margin-top: var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	.verify {
		margin-top: 4px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.check {
		display: flex;
		gap: var(--space-2);
		align-items: baseline;
	}
	.check .mark {
		display: inline-flex;
		align-self: center;
	}
	.mark.ok {
		color: var(--ok);
	}
	.mark.bad {
		color: var(--warn);
	}
	.detail-note {
		color: var(--text-faint);
	}
</style>
