<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import type { Answer, ContextSection, EvidenceItem } from '$lib/types';
	import Icon from './Icon.svelte';
	import PythonCalculationDetails from './PythonCalculationDetails.svelte';
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
			: answer.evidence.some(
					(item) => !item.error && item.verifier_disposition?.state !== 'excluded'
				)
	);
	let providedContext = $derived(
		(answer.provenance?.context_sections ?? []).map((section) => CONTEXT_LABEL[section])
	);

	// Which steps have their raw detail (SQL, table, output) revealed.
	let openDetail = $state<Record<number, boolean>>({});
	let showModelCalls = $state(false);
	function toggleDetail(i: number) {
		openDetail = { ...openDetail, [i]: !openDetail[i] };
	}
	function formatDuration(ms: number): string {
		return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
	}
	function modeLabel(mode: NonNullable<Answer['trace']>['mode']): string {
		switch (mode) {
			case 'model_only':
				return 'Direct answer';
			case 'workspace_ask':
				return 'Workspace analysis';
			case 'workspace_inspect':
				return 'Workspace inspection';
			default:
				return 'Analysis';
		}
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
		read_prior_analysis: 'Retrieved an earlier analysis from this conversation',
		list_files: 'Listed your files',
		make_chart: 'Drew a chart'
	};
	function stepLabel(e: EvidenceItem): string {
		if (e.note?.trim()) return e.note.trim();
		if (FALLBACK[e.tool]) return FALLBACK[e.tool];
		return e.tool;
	}
	function isPythonExecution(e: EvidenceItem): boolean {
		return e.tool === 'run_python';
	}
	function isForecastExecution(e: EvidenceItem): boolean {
		return e.tool === 'forecast_analysis';
	}

	// The model's `note` is already the step's headline drop it from the raw
	// args dump so it isn't shown twice. Python source is shown in its own code
	// block rather than as an ordinary argument.
	function argsForDisplay(e: EvidenceItem): Record<string, unknown> {
		const args = e.args ?? {};
		const { note: _note, ...rest } = args;
		if (e.tool === 'run_python') delete rest.code;
		return rest;
	}
	function sourceLabel(e: EvidenceItem): string {
		return (e.sources ?? []).map((s) => `${s.source} (${s.table})`).join(', ');
	}
	function findingTargetLabel(check: NonNullable<Answer['verification']>[number]): string | undefined {
		const finding = check.finding;
		if (!finding) return undefined;
		const targetId = finding.target_id ? ` ${finding.target_id}` : '';
		return `${finding.target.replace('_', ' ')}${targetId}`;
	}
	function findingEffectLabel(check: NonNullable<Answer['verification']>[number]): string | undefined {
		const effect = check.finding?.effect;
		if (!effect || effect === 'informational') return undefined;
		if (effect === 'repair') return 'Repair requested';
		if (effect === 'exclude_evidence') return 'Evidence excluded';
		if (effect === 'withhold_artifact') return 'Artifact withheld';
		return 'Answer blocked';
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
				{#if answer.trace?.model}
					{@const calls = answer.trace.model_calls ?? []}
					<div class="trace-meta">
						{modeLabel(answer.trace.mode)} · {answer.trace.model}
						{#if calls.length} · {calls.length} model call{calls.length === 1 ? '' : 's'}{/if}
						{#if answer.trace.elapsed_ms != null} · {formatDuration(answer.trace.elapsed_ms)} total{/if}
						{#if answer.usage}
							· {answer.usage.prompt_tokens + answer.usage.completion_tokens} tokens
						{/if}
					</div>
					{#if calls.length}
						<Button
							variant="ghost"
							size="sm"
							class="detailtoggle"
							onclick={() => (showModelCalls = !showModelCalls)}
							aria-expanded={showModelCalls}
						>
							{showModelCalls ? 'hide model timings' : 'show model timings'}
						</Button>
						{#if showModelCalls}
							<ul class="model-calls">
								{#each calls as call, i (`${i}-${call.model}-${call.duration_ms}`)}
									<li>
										{call.model} · {formatDuration(call.duration_ms)}
										{#if !call.success} · failed{/if}
										{#if call.prompt_tokens != null || call.completion_tokens != null}
											· {(call.prompt_tokens ?? 0) + (call.completion_tokens ?? 0)} tokens
										{/if}
									</li>
								{/each}
							</ul>
						{/if}
					{/if}
				{/if}
			</div>
			{#if answer.evidence.length}
			<ol class="steps">
				{#each answer.evidence as e, i (e.id ?? `evidence-${i}`)}
					{@const shownArgs = argsForDisplay(e)}
					{@const sqlIsAlreadyInInputTrace = e.python_input_trace?.queries.some((query) => query.sql === e.sql) ?? false}
					{@const hasDetail =
						!!e.sql ||
						!!e.sources?.length ||
						!!e.python_input_trace ||
						isPythonExecution(e) ||
						isForecastExecution(e) ||
						Object.keys(shownArgs).length > 0 ||
						!!e.output ||
						!!(e.columns && e.rows)}
					<li class="step" class:failed={!!e.error}>
						<span class="line">{stepLabel(e)}</span>
						{#if isPythonExecution(e)}<span class="language-badge">Python</span>{:else if isForecastExecution(e)}<span class="language-badge">Forecast</span>{/if}
						{#if e.tool === 'read_prior_analysis' && e.result_summary.startsWith('retrieved prior analysis from the current workspace revision')}
							<div class="source-line">Earlier execution · same workspace revision</div>
						{/if}
						{#if e.sources?.length}
							<div class="source-line">from {sourceLabel(e)}</div>
						{/if}

						{#if e.error}
							<div class="steperr">didn't work: {e.error}</div>
						{/if}
						{#if e.verifier_disposition}
							<div class="verifier-note">
								{e.verifier_disposition.state === 'excluded' ? 'Not used to support this answer' : `${e.verifier_disposition.artifact} withheld`}: {e.verifier_disposition.reason}
							</div>
						{/if}

						{#if hasDetail}
							<Button
								variant="ghost"
								size="sm"
								class="detailtoggle"
								onclick={() => toggleDetail(i)}
								aria-expanded={!!openDetail[i]}
							>
								{openDetail[i] ? 'hide' : isPythonExecution(e) ? 'show calculation' : isForecastExecution(e) ? 'show forecast method' : e.sql ? 'show the query' : 'show details'}
							</Button>
						{/if}

						{#if openDetail[i] && hasDetail}
							{@const visibleRows =
								e.columns && e.rows && e.row_count != null && e.row_count <= COMPLETE_TABLE_ROWS
									? e.rows
									: e.rows?.slice(0, 20)}
							<div class="detail rich">
								{#if isPythonExecution(e) || isForecastExecution(e)}
									<PythonCalculationDetails evidence={e} />
								{/if}
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
					{#each answer.verification as v, i (`${v.label}-${i}`)}
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
							{#if findingEffectLabel(v)}
								<span class="finding-note">{findingEffectLabel(v)}{#if findingTargetLabel(v)} · {findingTargetLabel(v)}{/if}</span>
							{/if}
						</div>
					{/each}
				</div>
			{/if}

			{#if answer.turn_id && answer.evidence.length}
				<ReplayStatus turnId={answer.turn_id} workspaceId={answer.workspace?.path ?? null} {onrerun} />
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
	.trace-meta {
		margin-top: 3px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.model-calls {
		margin: 4px 0 2px;
		padding-left: 16px;
		color: var(--text-faint);
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
	.language-badge {
		display: inline-block;
		margin-left: 6px;
		padding: 1px 5px;
		border: 1px solid var(--border);
		border-radius: 4px;
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10px;
		line-height: 1.35;
		vertical-align: 1px;
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
	.verifier-note {
		color: var(--warn);
		margin-top: 2px;
		font-size: var(--fs-xs);
	}
	:global(.detailtoggle) {
		display: block;
		height: auto;
		min-height: 0;
		justify-content: flex-start;
		margin-top: 3px;
		padding: 0;
		border-radius: 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 500;
		text-decoration: none;
	}
	:global(.detailtoggle:hover) {
		background: transparent;
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
	.finding-note {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
</style>
