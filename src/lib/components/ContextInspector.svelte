<script lang="ts">
	import { dispatch } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { AnalysisArtifact, EvidenceItem, SourceInfo } from '$lib/types';
	import Icon from './Icon.svelte';
	import SourcePreview from './SourcePreview.svelte';

	let selection = $derived(session.inspectorSelection);
	let workspace = $derived(session.catalog.workspace);
	let source = $derived.by((): SourceInfo | null => {
		if (selection?.kind !== 'source') return null;
		return session.catalog.sources.find((item) => item.path === selection.path) ?? null;
	});
	let analysis = $derived.by((): AnalysisArtifact | null => {
		if (selection?.kind !== 'analysis') return null;
		return session.analyses.find((item) => item.id === selection.id) ?? null;
	});
	let answerMessage = $derived.by(() => {
		if (selection?.kind !== 'answer') return null;
		return session.activeChat?.messages.find((message) => message.id === selection.messageId) ?? null;
	});
	let answerStep = $derived.by(() => {
		if (selection?.kind !== 'answer' || selection.stepIndex == null) return null;
		return session.activeChat?.runSteps[selection.stepIndex] ?? null;
	});
	let evidence = $derived.by((): EvidenceItem | null => {
		if (answerStep?.evidence) return answerStep.evidence;
		if (selection?.kind !== 'answer' || !answerMessage?.answer) return null;
		return answerMessage.answer.evidence[selection.stepIndex ?? -1] ?? null;
	});

	function relativePath(path: string): string {
		if (!workspace) return path;
		const root = workspace.replace(/[/\\]+$/, '');
		if (path.startsWith(root + '/') || path.startsWith(root + '\\')) {
			return path.slice(root.length + 1).replace(/\\/g, '/');
		}
		return path;
	}

	function baseName(path: string): string {
		return path.replace(/[/\\]+$/, '').split(/[/\\]/).pop() ?? path;
	}

	function kindLabel(item: SourceInfo): string {
		const kind = item.kind.toLowerCase();
		return kind.charAt(0).toUpperCase() + kind.slice(1);
	}

	function formatBytes(bytes: number): string {
		if (!Number.isFinite(bytes) || bytes < 1024) return `${Math.max(0, bytes || 0)} B`;
		const units = ['KB', 'MB', 'GB'];
		let value = bytes / 1024;
		let unit = units[0];
		for (let i = 1; value >= 1024 && i < units.length; i++) {
			value /= 1024;
			unit = units[i];
		}
		return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${unit}`;
	}

	function formatDate(ms: number): string {
		return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric', year: 'numeric' }).format(ms);
	}

	function useSource(): void {
		if (!source) return;
		session.addContextReference({
			kind: 'source',
			key: source.path,
			label: source.name,
			detail: relativePath(source.path)
		});
		session.setWorkspaceView('ask');
		session.closeInspector();
	}

	function useAnalysis(): void {
		if (!analysis) return;
		session.addContextReference({
			kind: 'analysis',
			key: analysis.id,
			label: analysis.title,
			detail: analysis.question
		});
		session.setWorkspaceView('ask');
		session.closeInspector();
	}

	function forkAnalysis(): void {
		if (!analysis) return;
		session.startFromAnalysis(analysis);
		session.closeInspector();
	}

	function askAgain(): void {
		if (!analysis) return;
		session.setWorkspaceView('ask');
		session.closeInspector();
		void dispatch(analysis.question);
	}

	function openSourcePage(): void {
		session.closeInspector();
		session.setWorkspaceView('sources');
	}

	function openAnalysisPage(): void {
		if (!analysis) return;
		session.selectAnalysis(analysis.id);
		session.closeInspector();
		session.setWorkspaceView('analyses');
	}

	function answerStatus(): string {
		if (!answerMessage?.answer) return 'Still working';
		if (answerMessage.answer.status === 'verified') return 'Verified';
		if (answerMessage.answer.status === 'needs_review') return 'Needs review';
		if (answerMessage.answer.status === 'insufficient_data') return 'Insufficient data';
		if (answerMessage.answer.status === 'failed') return 'Could not verify';
		return 'Answer received';
	}
</script>

<aside class="inspector" aria-label="Context inspector">
	<header class="inspector-head">
		<div>
			<p class="eyebrow">Inspector</p>
			<h2>{source?.name ?? analysis?.title ?? (evidence ? 'Run step' : 'Answer details')}</h2>
		</div>
		<button class="close" type="button" aria-label="Close inspector" title="Close inspector" onclick={() => session.closeInspector()}>
			<Icon name="x" size={15} />
		</button>
	</header>

	<div class="inspector-body">
		{#if source}
			<div class="object-mark"><Icon name={source.view ? 'table' : 'file'} size={18} /></div>
			<p class="type-label">{kindLabel(source)}</p>
			<p class="path" title={source.path}>{relativePath(source.path)}</p>
			<div class="facts">
				<div><span>Size</span><strong>{formatBytes(source.size_bytes)}</strong></div>
				<div><span>Rows</span><strong>{source.row_count == null ? '—' : source.row_count.toLocaleString()}</strong></div>
				<div><span>Fields</span><strong>{source.columns?.length ?? '—'}</strong></div>
			</div>
			{#if source.synopsis}<p class="description">{source.synopsis}</p>{/if}
			{#if source.note}<div class="note"><span>Ingest note</span>{source.note}</div>{/if}
			<SourcePreview source={source} />
			{#if source.columns?.length}
				<div class="section">
					<div class="section-title"><span>Fields</span><span>{source.columns.length}</span></div>
					{#each source.columns.slice(0, 12) as column (column.name)}
						<div class="field-row"><code>{column.name}</code><small>{column.type}</small></div>
					{/each}
					{#if source.columns.length > 12}<p class="muted">+ {source.columns.length - 12} more fields</p>{/if}
				</div>
			{/if}
			<div class="actions">
				<button class="pill primary" type="button" onclick={useSource}><Icon name="plus" size={13} /> Use in Ask</button>
				<button class="pill ghost" type="button" onclick={openSourcePage}>Open Sources</button>
			</div>
		{:else if analysis}
			<div class="object-mark"><Icon name="bookmark" size={18} /></div>
			<p class="type-label">Saved analysis · {formatDate(analysis.created_at_ms)}</p>
			<p class="question">“{analysis.question}”</p>
			<div class="facts two">
				<div><span>Evidence</span><strong>{analysis.answer.evidence.length} steps</strong></div>
				<div><span>Checks</span><strong>{analysis.answer.verification.filter((check) => check.ok).length}/{analysis.answer.verification.length || 0}</strong></div>
			</div>
			{#if analysis.answer.workspace?.path}
				<p class="path" title={analysis.answer.workspace.path}><Icon name="folder" size={12} /> {baseName(analysis.answer.workspace.path)}</p>
			{/if}
			<p class="description">Use this saved result as a starting point for a follow-up question, or rerun the original question against the current workspace.</p>
			<div class="actions">
				<button class="pill primary" type="button" onclick={forkAnalysis}><Icon name="plus" size={13} /> Fork follow-up</button>
				<button class="pill ghost" type="button" onclick={useAnalysis}><Icon name="bookmark" size={13} /> Use in Ask</button>
				<button class="pill ghost" type="button" onclick={askAgain}><Icon name="compose" size={13} /> Ask again</button>
				<button class="text-action" type="button" onclick={openAnalysisPage}>Open saved analysis <Icon name="arrow-up-right" size={12} /></button>
			</div>
		{:else if answerMessage?.answer}
			<div class="answer-state"><span class="status-dot"></span><strong>{answerStatus()}</strong></div>
			{#if answerMessage.answer.workspace?.path}
				<p class="path" title={answerMessage.answer.workspace.path}><Icon name="folder" size={12} /> {baseName(answerMessage.answer.workspace.path)}</p>
			{/if}
			{#if evidence}
				<div class="evidence-card">
					<div class="evidence-heading"><span class="type-label">Evidence step</span><code>{evidence.tool}</code></div>
					{#if evidence.note}<p>{evidence.note}</p>{/if}
					<strong>{evidence.result_summary}</strong>
					{#if evidence.row_count != null}<small>{evidence.row_count.toLocaleString()} rows · {evidence.ms}ms</small>{/if}
					{#if evidence.sql}
						<details><summary>View query</summary><pre>{evidence.sql}</pre></details>
					{/if}
					{#if evidence.error}<div class="error">{evidence.error}</div>{/if}
				</div>
			{:else}
				<p class="description">{answerMessage.answer.evidence.length} evidence step{answerMessage.answer.evidence.length === 1 ? '' : 's'} support this answer.</p>
			{/if}
			<div class="section">
				<div class="section-title"><span>Verification</span><span>{answerMessage.answer.verification.length}</span></div>
				{#each answerMessage.answer.verification as check (check.label)}
					<div class="check-row"><Icon name={check.ok ? 'check' : 'alert'} size={12} /><span>{check.label}</span></div>
				{/each}
			</div>
		{:else}
			<div class="empty-inspector">
				<div class="object-mark"><Icon name="info" size={18} /></div>
				<strong>Nothing selected yet</strong>
				<p>Choose a source, saved analysis, or run step to see its shape and provenance here.</p>
			</div>
		{/if}
	</div>
</aside>

<style>
	.inspector {
		width: min(330px, 35vw);
		min-width: 280px;
		flex: none;
		display: flex;
		flex-direction: column;
		min-height: 0;
		border-left: 1px solid var(--border);
		background: var(--bg-raised);
		animation: inspector-in var(--dur) var(--ease);
	}
	.inspector-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-4);
		border-bottom: 1px solid var(--border);
	}
	.inspector-head h2 {
		max-width: 22ch;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--fs-lg);
		font-weight: 600;
	}
	.eyebrow,
	.type-label {
		margin: 0 0 var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.close {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.close:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.inspector-body {
		min-height: 0;
		overflow: auto;
		padding: var(--space-5) var(--space-4);
	}
	.object-mark {
		display: grid;
		place-items: center;
		width: 42px;
		height: 42px;
		margin-bottom: var(--space-3);
		border-radius: 12px;
		background: color-mix(in srgb, var(--brand) 11%, transparent);
		color: var(--brand);
	}
	.type-label {
		font-size: 10px;
	}
	.path {
		display: flex;
		align-items: center;
		gap: 5px;
		margin: 0 0 var(--space-4);
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10.5px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.facts {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: var(--space-2);
		margin-bottom: var(--space-4);
	}
	.facts.two {
		grid-template-columns: repeat(2, 1fr);
	}
	.facts div {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding-top: var(--space-2);
		border-top: 1px solid var(--border);
	}
	.facts span {
		color: var(--text-faint);
		font-size: 10.5px;
	}
	.facts strong {
		font-size: var(--fs-sm);
		font-weight: 580;
	}
	.description,
	.question {
		margin: 0 0 var(--space-4);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.question {
		color: var(--text);
		line-height: 1.45;
	}
	.note {
		margin-bottom: var(--space-4);
		padding: var(--space-2) var(--space-3);
		border-left: 2px solid var(--warn);
		background: color-mix(in srgb, var(--warn) 7%, transparent);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.note span {
		display: block;
		margin-bottom: 2px;
		color: var(--warn);
		font-size: 10px;
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.section {
		margin-top: var(--space-4);
		border-top: 1px solid var(--border);
	}
	.section-title {
		display: flex;
		justify-content: space-between;
		padding: var(--space-3) 0 var(--space-2);
		color: var(--text-dim);
		font-size: 10px;
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.section-title span:last-child {
		color: var(--text-faint);
		font-weight: 500;
	}
	.field-row,
	.check-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		padding: 4px 0;
		font-size: var(--fs-xs);
	}
	.field-row code {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.field-row small {
		flex: none;
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10px;
	}
	.check-row {
		justify-content: flex-start;
		color: var(--text-dim);
	}
	.check-row :global(svg) {
		color: var(--ok);
	}
	.muted {
		margin: var(--space-2) 0 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin-top: var(--space-5);
	}
	.actions .pill {
		font-size: var(--fs-xs);
		padding: 6px 10px;
	}
	.text-action {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		width: 100%;
		margin-top: var(--space-1);
		color: var(--link);
		font-size: var(--fs-xs);
		text-align: left;
		white-space: nowrap;
	}
	.text-action:hover {
		text-decoration: underline;
	}
	.answer-state {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: var(--space-3);
		font-size: var(--fs-sm);
	}
	.status-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ok);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 15%, transparent);
	}
	.evidence-card {
		padding: var(--space-3);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
	}
	.evidence-heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
	}
	.evidence-heading .type-label {
		margin: 0;
	}
	.evidence-heading code,
	.evidence-card pre {
		font-family: var(--mono);
		font-size: 10px;
	}
	.evidence-card p {
		margin: 0 0 var(--space-2);
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.evidence-card > strong {
		display: block;
		font-size: var(--fs-xs);
		font-weight: 560;
	}
	.evidence-card > small {
		display: block;
		margin-top: 4px;
		color: var(--text-faint);
		font-size: 10px;
	}
	details {
		margin-top: var(--space-2);
		border-top: 1px solid var(--border);
	}
	details summary {
		padding-top: var(--space-2);
		color: var(--link);
		font-size: 10.5px;
		cursor: pointer;
	}
	.evidence-card pre {
		max-height: 180px;
		margin: var(--space-2) 0 0;
		padding: var(--space-2);
		overflow: auto;
		white-space: pre-wrap;
		word-break: break-word;
		background: var(--bg-raised);
		border-radius: var(--radius-chip);
	}
	.error {
		margin-top: var(--space-2);
		color: var(--err);
		font-size: var(--fs-xs);
	}
	.empty-inspector {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		padding-top: var(--space-6);
		color: var(--text-dim);
	}
	.empty-inspector .object-mark {
		margin-bottom: var(--space-4);
	}
	.empty-inspector strong {
		color: var(--text);
		font-size: var(--fs-lg);
	}
	.empty-inspector p {
		margin: var(--space-2) 0;
		font-size: var(--fs-sm);
	}
	@keyframes inspector-in {
		from { opacity: 0; transform: translateX(10px); }
		to { opacity: 1; transform: translateX(0); }
	}
	@media (max-width: 800px) {
		.inspector {
			position: absolute;
			top: 0;
			right: 0;
			bottom: 0;
			z-index: 5;
			width: min(330px, calc(100vw - 48px));
			box-shadow: var(--shadow-pop);
		}
	}
</style>
