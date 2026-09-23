<script lang="ts">
	import type { Answer, EvidenceItem, VerificationStatus } from '$lib/types';
	import Icon from './Icon.svelte';
	import { answerStatus } from '$lib/verify';
	import { session } from '$lib/session.svelte';

	let {
		answer,
		scope = '',
		expanded = false,
		ontoggle,
		messageId = ''
	}: { answer: Answer; scope?: string; expanded?: boolean; ontoggle?: () => void; messageId?: string } = $props();

	// Stable id so the toggle can point at the panel it controls.
	const bodyId = 'evidence-' + Math.random().toString(36).slice(2, 9);

	let stepCount = $derived(answer.evidence.length);
	let ms = $derived(answer.evidence.reduce((n, e) => n + e.ms, 0));
	let warns = $derived(answer.verification.filter((v) => !v.ok).length);
	let status = $derived(answerStatus(answer));
	let hasBackground = $derived(/^\s*Background:/m.test(answer.text));
	const STATUS_LABEL: Record<VerificationStatus, string> = {
		verified: 'checked against your data',
		needs_review: 'needs review',
		insufficient_data: 'not enough data',
		failed: 'could not fully check'
	};
	const COMPLETE_TABLE_ROWS = 100;

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

	function openDetails(): void {
		if (messageId) session.openInspector({ kind: 'answer', messageId });
	}
</script>

<div class="evidence">
	<div class="evidence-head">
		<button
			class="summary"
			onclick={ontoggle}
			aria-expanded={expanded}
			aria-controls={bodyId}
		>
			<span class="caret" class:open={expanded} aria-hidden="true">
				<Icon name="chevron-right" size={12} />
			</span>
			{#if stepCount === 0}
				Answered from general knowledge
			{:else}
				Checked workspace · {stepCount} step{stepCount === 1 ? '' : 's'} · {(ms / 1000).toFixed(1)}s
				{#if hasBackground}<span class="bg">· background</span>{/if}
				{#if answer.workspace}<span class="snapshot">· folder snapshot</span>{/if}
			{/if}
			{#if scope}<span class="scope">· {scope}</span>{/if}
			<span class="status {status}">· {STATUS_LABEL[status]}</span>
			{#if warns > 0}<span class="warn">· {warns} to check</span>{/if}
		</button>
		{#if messageId}
			<button class="details-link" type="button" aria-label="Open answer details" onclick={openDetails}>
				Details <Icon name="info" size={12} />
			</button>
		{/if}
	</div>

	{#if expanded}
		<div class="body" id={bodyId}>
			<ol class="steps">
				{#each answer.evidence as e, i (e.id ?? `evidence-${i}`)}
					{@const shownArgs = argsWithoutNote(e.args)}
					{@const hasDetail =
						!!e.sql ||
						!!e.sources?.length ||
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
									<pre class="sql">{e.sql}</pre>
								{:else if Object.keys(shownArgs).length > 0}
									<dl class="args">
										{#each Object.entries(shownArgs) as [k, v] (k)}
											<dt>{k}</dt>
											<dd>{typeof v === 'string' ? v : JSON.stringify(v)}</dd>
										{/each}
									</dl>
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

			{#if answer.verification.length}
				<div class="verify">
					{#each answer.verification as v (v.label)}
						<div class="check">
							<span class="mark" class:ok={v.ok} class:bad={!v.ok} aria-hidden="true">
								<Icon name={v.ok ? 'check' : 'alert'} size={12} />
							</span>
							<span>{v.label}</span>
							{#if v.detail}<span class="detail-note">— {v.detail}</span>{/if}
						</div>
					{/each}
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.evidence {
		margin-top: var(--space-4);
		padding-top: var(--space-3);
		border-top: 1px solid var(--border);
		font-size: var(--fs-sm);
	}
	.evidence-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
	}
	.summary {
		min-width: 0;
		flex: 1;
		border: none;
		padding: 1px 0;
		color: var(--text-dim);
		background: transparent;
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		white-space: normal;
		letter-spacing: 0.01em;
		font-size: var(--fs-xs);
	}
	.details-link {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		flex: none;
		padding: 1px 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.details-link:hover {
		color: var(--text);
	}
	.summary:hover {
		background: transparent;
		color: var(--text);
	}
	.caret {
		display: inline-flex;
		color: var(--text-faint);
		transition: transform var(--dur-fast) var(--ease);
	}
	.caret.open {
		transform: rotate(90deg);
	}
	.warn {
		color: var(--warn);
	}
	.bg {
		color: var(--text-faint);
	}
	.snapshot {
		color: var(--text-faint);
	}
	.scope {
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-faint);
	}
	.status.verified {
		color: var(--ok);
	}
	.status.needs_review {
		color: var(--warn);
	}
	.status.insufficient_data {
		color: var(--text-faint);
	}
	.status.failed {
		color: var(--err);
	}
	.body {
		margin: 6px 0 2px;
		padding-left: 10px;
		border-left: 2px solid var(--border);
		display: flex;
		flex-direction: column;
		gap: 10px;
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
