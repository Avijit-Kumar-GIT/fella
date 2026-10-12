<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { ipc } from '$lib/ipc';
	import type { AnalysisTurn, Answer, ContextSection, EvidenceItem } from '$lib/types';
	import PythonCalculationDetails from './PythonCalculationDetails.svelte';
	import ReplayStatus from './ReplayStatus.svelte';
	import VerificationChecks from './VerificationChecks.svelte';

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
	let savedRunOpen = $state(false);
	let savedRunLoading = $state(false);
	let savedRunError = $state('');
	let savedRun = $state<AnalysisTurn | null>(null);
	let savedEvidence = $derived(
		(savedRun?.result.evidence ?? []).filter(isPersistedEvidenceItem)
	);
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
	function isPersistedEvidenceItem(value: unknown): value is EvidenceItem {
		if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
		const item = value as Record<string, unknown>;
		if (typeof item.tool !== 'string' || typeof item.result_summary !== 'string') return false;
		for (const key of ['id', 'note', 'sql', 'output', 'error']) {
			if (item[key] !== undefined && typeof item[key] !== 'string') return false;
		}
		if (
			item.sources !== undefined &&
			(!Array.isArray(item.sources) ||
				!item.sources.every(
					(source) =>
						source &&
						typeof source === 'object' &&
						typeof source.source === 'string' &&
						typeof source.table === 'string'
				))
		) {
			return false;
		}
		if (item.columns !== undefined && (!Array.isArray(item.columns) || !item.columns.every((column) => typeof column === 'string'))) return false;
		if (item.rows !== undefined && (!Array.isArray(item.rows) || !item.rows.every(Array.isArray))) return false;
		if (item.row_count !== undefined && typeof item.row_count !== 'number') return false;
		if (item.args !== undefined && (!item.args || typeof item.args !== 'object' || Array.isArray(item.args))) return false;
		if (item.python_input_trace !== undefined) {
			if (!item.python_input_trace || typeof item.python_input_trace !== 'object') return false;
			const trace = item.python_input_trace as Record<string, unknown>;
			if (
				typeof trace.complete !== 'boolean' ||
				!Array.isArray(trace.queries) ||
				!trace.queries.every(
					(query) =>
						query &&
						typeof query === 'object' &&
						typeof query.sql === 'string' &&
						Array.isArray(query.columns) &&
						query.columns.every((column: unknown) => typeof column === 'string') &&
						typeof query.row_count === 'number' &&
						typeof query.truncated === 'boolean'
				)
			) {
				return false;
			}
		}
		return true;
	}
	function evidenceCell(value: unknown): string {
		if (value == null) return '';
		if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
			return String(value);
		}
		return JSON.stringify(value);
	}
	function checkAppliesToEvidence(check: Answer['verification'][number], evidence: EvidenceItem): boolean {
		if (!evidence.id || !check.finding) return false;
		const finding = check.finding;
		const linkedEvidence = finding.evidence_ids ?? [];
		if (finding.target === 'artifact') return finding.target_id === evidence.id;
		if (finding.target === 'evidence') {
			return finding.target_id === evidence.id || linkedEvidence.includes(evidence.id);
		}
		return finding.target === 'claim' && linkedEvidence.includes(evidence.id);
	}
	function checksForEvidence(evidence: EvidenceItem): Answer['verification'] {
		return answer.verification.filter((check) => checkAppliesToEvidence(check, evidence));
	}
	let runWideChecks = $derived(
		answer.verification.filter((check) => !answer.evidence.some((evidence) => checkAppliesToEvidence(check, evidence)))
	);
	async function toggleSavedRun(): Promise<void> {
		if (savedRunOpen) {
			savedRunOpen = false;
			return;
		}
		savedRunOpen = true;
		if (!answer.turn_id || savedRun || savedRunLoading) return;
		savedRunLoading = true;
		savedRunError = '';
		try {
			savedRun = await ipc.analysisTurnLoad(answer.turn_id);
		} catch (cause) {
			savedRunError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			savedRunLoading = false;
		}
	}
	function runStateLabel(state: AnalysisTurn['state']): string {
		return state.replaceAll('_', ' ');
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
			<VerificationChecks
				status={answer.status}
				checks={runWideChecks}
				heading={runWideChecks.length ? 'Run-wide checks' : undefined}
			/>
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
					<li
						id={`${bodyId}-step-${i}`}
						class="step"
						class:failed={!!e.error}
						data-evidence-id={e.id}
						tabindex="-1"
					>
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
						<VerificationChecks
							checks={checksForEvidence(e)}
							heading="Checks on this output"
							compact={true}
						/>

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

			{#if answer.turn_id}
				<div class="saved-run">
					<Button
						variant="ghost"
						size="sm"
						class="detailtoggle"
						aria-expanded={savedRunOpen}
						aria-controls={`saved-run-${bodyId}`}
						onclick={() => void toggleSavedRun()}
					>
						{savedRunOpen ? 'Hide saved run record' : 'Open saved run record'}
					</Button>
					{#if savedRunOpen}
						<section class="saved-run-panel" id={`saved-run-${bodyId}`} aria-label="Saved analysis run">
							{#if savedRunLoading}
								<p role="status">Loading the saved run…</p>
							{:else if savedRunError}
								<p class="saved-run-error" role="alert">Couldn't load the saved run: {savedRunError}</p>
							{:else if savedRun}
								<div class="saved-run-heading">
									<strong>Canonical analysis run</strong>
									<code>{savedRun.id}</code>
								</div>
								<dl class="saved-run-facts">
									<div><dt>State</dt><dd>{runStateLabel(savedRun.state)}</dd></div>
									<div><dt>Method</dt><dd>{savedRun.plan.strategy.replaceAll('_', ' ')}</dd></div>
									<div><dt>Execution steps</dt><dd>{savedRun.trace.steps.length}</dd></div>
									{#if savedRun.workspace_revision}<div><dt>Workspace revision</dt><dd><code>{savedRun.workspace_revision}</code></dd></div>{/if}
									{#if savedRun.source_scope}<div><dt>Source scope</dt><dd>{savedRun.source_scope.used_sources.length} used · {savedRun.source_scope.skipped_sources.length} skipped</dd></div>{/if}
								</dl>
								{#if savedRun.investigation_context}
									<div class="saved-run-section saved-run-definitions">
										<h4>Investigation context used</h4>
										<div class="saved-run-context-scope">
											<strong>Requested source scope</strong>
											<p>{savedRun.investigation_context.source_selection.mode === 'all_sources'
												? 'All sources in the Investigation scope'
												: `${savedRun.investigation_context.source_selection.included_paths.length} selected source${savedRun.investigation_context.source_selection.included_paths.length === 1 ? '' : 's'}`}</p>
											{#if savedRun.investigation_context.source_selection.mode === 'selected_sources' && savedRun.investigation_context.source_selection.included_paths.length}
												<ul>
													{#each savedRun.investigation_context.source_selection.included_paths as path (path)}<li>{path}</li>{/each}
												</ul>
											{/if}
										</div>
										{#if savedRun.investigation_context.definitions.length}
											<ul>
												{#each savedRun.investigation_context.definitions as definition (definition.id)}
													<li><strong>{definition.kind === 'definition' ? 'Definition' : 'Assumption'} · revision {definition.revision}</strong><span>{definition.text}</span></li>
												{/each}
											</ul>
										{:else}
											<p>No saved definitions or assumptions were active for this run.</p>
										{/if}
									</div>
								{/if}
								{#if savedRun.contract?.assumptions.length}
									<div class="saved-run-section saved-run-assumptions">
										<h4>Assumptions recorded for this run</h4>
										<p>These are interpretive choices, not observed facts. To correct one, state the replacement in a follow-up; this saved run remains unchanged.</p>
										<ul>{#each savedRun.contract.assumptions as assumption, i (`${savedRun.id}-assumption-${i}`)}<li>{assumption}</li>{/each}</ul>
									</div>
								{/if}
								{#if savedRun.plan.steps.length}
									<div class="saved-run-section">
										<h4>Planned steps</h4>
										<ol>{#each savedRun.plan.steps as step, i (`${savedRun.id}-plan-${i}`)}<li>{step.replaceAll('_', ' ')}</li>{/each}</ol>
									</div>
								{/if}
								{#if savedRun.trace.steps.length}
									<div class="saved-run-section">
										<h4>Recorded execution</h4>
										<ol class="saved-run-trace">
											{#each savedRun.trace.steps as step (step.id)}
												<li>
													<div class="saved-run-step-heading">
														<strong>{step.operation.replaceAll('_', ' ')}</strong>
														<span>{step.success ? 'Succeeded' : 'Failed'} · {formatDuration(step.duration_ms)}</span>
													</div>
													{#if step.summary}<p>{step.summary}</p>{/if}
													{#if step.sources.length}<p class="saved-run-sources">Sources: {step.sources.join(', ')}</p>{/if}
												</li>
											{/each}
										</ol>
									</div>
								{/if}
								<div class="saved-run-section">
									<h4>Recorded evidence</h4>
									{#if savedEvidence.length}
										<ol class="saved-run-evidence">
											{#each savedEvidence as evidence, i (`${savedRun.id}-evidence-${evidence.id ?? i}`)}
												{@const columns = evidence.columns ?? evidence.result_table?.columns}
												{@const rows = evidence.rows ?? evidence.result_table?.rows}
												<li>
													<details>
														<summary>
															<strong>{stepLabel(evidence)}</strong>
															<span>{sourceLabel(evidence) || 'No source reference recorded'}</span>
														</summary>
														<p>{evidence.result_summary}</p>
														{#if evidence.error}<p class="saved-run-evidence-error">Execution error: {evidence.error}</p>{/if}
														{#if evidence.verifier_disposition}
															<p class="saved-run-evidence-error">
																{evidence.verifier_disposition.state === 'excluded' ? 'Excluded from support' : `${evidence.verifier_disposition.artifact} withheld`}: {evidence.verifier_disposition.reason}
															</p>
														{/if}
														{#if evidence.tool === 'run_python' || evidence.tool === 'forecast_analysis'}
															<PythonCalculationDetails {evidence} />
														{/if}
														{#if evidence.sql}<pre class="sql">{evidence.sql}</pre>{/if}
														{#if evidence.python_input_trace?.queries.length}
															<div class="saved-run-python-inputs">
																<strong>Data read by this calculation</strong>
																{#each evidence.python_input_trace.queries as query, queryIndex (`${i}-input-${queryIndex}`)}
																	<pre class="sql">{query.sql}</pre>
																	<span>{query.row_count} rows · {query.columns.join(', ') || 'no columns'}{query.truncated ? ' · result capped' : ''}</span>
																{/each}
															</div>
														{/if}
														{#if evidence.output}<pre class="output">{evidence.output}</pre>{/if}
														{#if columns && rows}
															<div class="saved-run-table-sample">
																<strong>Recorded result sample</strong>
																<pre>{JSON.stringify({ columns, rows: rows.slice(0, 20) }, null, 2)}</pre>
																{#if evidence.row_count != null && rows.length > 20}
																	<p>Showing 20 of {evidence.row_count} rows.</p>
																{/if}
															</div>
														{/if}
													</details>
												</li>
											{/each}
										</ol>
									{:else}
										<p>No inspectable evidence was stored for this run.</p>
									{/if}
								</div>
							{/if}
						</section>
					{/if}
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
	.saved-run { display: flex; flex-direction: column; align-items: flex-start; gap: var(--space-2); }
	.saved-run-panel { width: min(100%, 620px); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); color: var(--text-dim); }
	.saved-run-panel p { margin: 0; }
	.saved-run-heading { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-2); color: var(--text); font-size: var(--fs-xs); }
	.saved-run-heading code, .saved-run-facts code { overflow-wrap: anywhere; font-family: var(--mono); }
	.saved-run-facts { display: flex; flex-wrap: wrap; gap: var(--space-3) var(--space-5); margin: var(--space-2) 0 0; font-size: var(--fs-xs); }
	.saved-run-facts div { min-width: 100px; }
	.saved-run-facts dt { color: var(--text-faint); }
	.saved-run-facts dd { margin: 2px 0 0; color: var(--text); }
	.saved-run-section { margin-top: var(--space-3); }
	.saved-run-section h4 { margin: 0 0 var(--space-1); color: var(--text-faint); font-size: var(--fs-xs); font-weight: 600; }
	.saved-run-section ol { margin: 0; padding-left: 20px; color: var(--text); font-size: var(--fs-xs); }
	.saved-run-definitions ul { display: grid; gap: 8px; margin: 0; padding: 0; list-style: none; }
	.saved-run-definitions li { display: grid; gap: 2px; }
	.saved-run-definitions li span, .saved-run-definitions p { color: var(--text-dim); white-space: pre-wrap; overflow-wrap: anywhere; }
	.saved-run-definitions p { margin: 0; }
	.saved-run-assumptions p { margin: 0 0 var(--space-2); color: var(--text-dim); font-size: var(--fs-xs); }
	.saved-run-assumptions ul { display: grid; gap: var(--space-1); margin: 0; padding-left: 20px; color: var(--text); font-size: var(--fs-xs); }
	.saved-run-assumptions li { overflow-wrap: anywhere; }
	.saved-run-trace { display: flex; flex-direction: column; gap: var(--space-2); }
	.saved-run-trace li { padding-left: 2px; }
	.saved-run-step-heading { display: flex; flex-wrap: wrap; gap: var(--space-1) var(--space-2); }
	.saved-run-step-heading span, .saved-run-sources { color: var(--text-faint); }
	.saved-run-evidence { display: flex; flex-direction: column; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
	.saved-run-evidence summary { display: flex; flex-wrap: wrap; gap: var(--space-1) var(--space-2); cursor: pointer; }
	.saved-run-evidence summary span, .saved-run-python-inputs span { color: var(--text-faint); }
	.saved-run-evidence details > p { margin: var(--space-1) 0; color: var(--text-dim); }
	.saved-run-evidence-error { color: var(--warn) !important; }
	.saved-run-python-inputs { display: grid; gap: var(--space-1); margin: var(--space-2) 0; }
	.saved-run-table-sample { margin-top: var(--space-2); }
	.saved-run-table-sample strong { color: var(--text-faint); font-size: var(--fs-xs); }
	.saved-run-table-sample pre { max-height: 260px; margin: var(--space-1) 0 0; overflow: auto; font-family: var(--mono); font-size: var(--fs-xs); }
	.saved-run-section p { margin-top: 2px; }
	.saved-run-context-scope strong { color: var(--text-faint); font-size: var(--fs-xs); }
	.saved-run-context-scope p { margin: 2px 0 0; color: var(--text-dim); }
	.saved-run-context-scope ul { margin: var(--space-1) 0 0; padding-left: var(--space-5); color: var(--text-dim); }
	.saved-run-context-scope li { overflow-wrap: anywhere; }
	.saved-run-error { color: var(--danger, #b42318); overflow-wrap: anywhere; }
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
</style>
