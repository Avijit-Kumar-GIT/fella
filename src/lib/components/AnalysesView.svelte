<script lang="ts">
	import { dispatch } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { AnalysisArtifact, Message } from '$lib/types';
	import Icon from './Icon.svelte';
	import MessageView from './Message.svelte';

	let query = $state('');
	let evidenceOpen = $state(true);
	let workspace = $derived(session.catalog.workspace);
	let visible = $derived.by(() => {
		const scoped = workspace
			? session.analyses.filter(
					(analysis) => !analysis.answer.workspace?.path || analysis.answer.workspace.path === workspace
				)
			: session.analyses;
		const q = query.trim().toLowerCase();
		return q
			? scoped.filter((analysis) => `${analysis.title} ${analysis.question}`.toLowerCase().includes(q))
			: scoped;
	});
	let selected = $derived.by(() => {
		if (session.selectedAnalysisId) {
			const exact = visible.find((analysis) => analysis.id === session.selectedAnalysisId);
			if (exact) return exact;
		}
		return visible[0] ?? null;
	});
	let detailMessage = $derived.by((): Message | null => {
		if (!selected) return null;
		return {
			id: selected.message_id,
			role: 'assistant',
			text: selected.answer.text,
			answer: selected.answer,
			pending: false,
			ts: selected.created_at_ms
		};
	});

	$effect(() => {
		const id = selected?.id ?? null;
		if (session.selectedAnalysisId !== id) session.selectAnalysis(id);
	});
	$effect(() => {
		selected?.id;
		evidenceOpen = true;
	});

	function select(analysis: AnalysisArtifact): void {
		session.selectAnalysis(analysis.id);
		evidenceOpen = true;
	}

	function askAgain(): void {
		if (!selected) return;
		session.setWorkspaceView('ask');
		void dispatch(selected.question);
	}

	function forkFollowUp(): void {
		if (!selected) return;
		session.startFromAnalysis(selected);
	}

	function removeSelected(): void {
		if (!selected) return;
		session.deleteAnalysis(selected.id);
	}

	function formatDate(ms: number): string {
		return new Intl.DateTimeFormat(undefined, {
			month: 'short',
			day: 'numeric',
			year: new Date(ms).getFullYear() === new Date().getFullYear() ? undefined : 'numeric'
		}).format(ms);
	}

	function evidenceCount(analysis: AnalysisArtifact): string {
		const n = analysis.answer.evidence.length;
		return `${n} step${n === 1 ? '' : 's'}`;
	}
</script>

<section class="analyses-page" aria-labelledby="analyses-title">
	<header class="page-head">
		<div>
			<p class="eyebrow">Workspace library</p>
			<h1 id="analyses-title">Analyses</h1>
			<p class="lede">Answers you chose to keep, with their evidence and workspace snapshot.</p>
		</div>
		<label class="searchbox">
			<Icon name="search" size={14} />
			<span class="sr-only">Search analyses</span>
			<input bind:value={query} placeholder="Search analyses…" spellcheck="false" />
		</label>
	</header>

	{#if !visible.length}
		<div class="empty-state">
			<div class="empty-icon"><Icon name="bookmark" size={21} /></div>
			<h2>{query ? 'No matching analyses' : 'Your saved analyses will live here'}</h2>
			<p>
				{query
					? 'Try a different question or clear the search.'
					: 'When an answer is useful, save it from the conversation. Fella keeps the question, chart, evidence, and source snapshot together.'}
			</p>
			{#if query}
				<button class="pill ghost" type="button" onclick={() => (query = '')}>Clear search</button>
			{:else}
				<button class="pill primary" type="button" onclick={() => session.setWorkspaceView('ask')}>
					<Icon name="compose" size={14} /> Ask a question
				</button>
			{/if}
		</div>
	{:else}
		<div class="analysis-layout">
			<div class="analysis-list" role="listbox" aria-label="Saved analyses">
				{#each visible as analysis (analysis.id)}
					<button
						class="analysis-row"
						class:selected={selected?.id === analysis.id}
						type="button"
						role="option"
						aria-selected={selected?.id === analysis.id}
						onclick={() => select(analysis)}
					>
						<span class="analysis-icon"><Icon name="bookmark" size={14} /></span>
						<span class="analysis-copy">
							<strong>{analysis.title}</strong>
							<small>{formatDate(analysis.created_at_ms)} · {evidenceCount(analysis)}</small>
						</span>
					</button>
				{/each}
			</div>

			<article class="analysis-detail">
				{#if selected && detailMessage}
					<header class="detail-head">
						<div>
							<p class="eyebrow">Saved analysis</p>
							<h2>{selected.title}</h2>
							<p class="question">“{selected.question}”</p>
						</div>
						<div class="detail-actions">
							<button class="pill primary" type="button" onclick={forkFollowUp}>
								<Icon name="plus" size={13} /> Fork follow-up
							</button>
							<button class="pill ghost" type="button" onclick={askAgain}>
								<Icon name="compose" size={13} /> Ask again
							</button>
							<button class="icon-action danger" type="button" aria-label="Delete analysis" title="Delete analysis" onclick={removeSelected}>
								<Icon name="x" size={14} />
							</button>
						</div>
					</header>
					<div class="provenance">
						<span><Icon name="folder" size={12} /> {selected.answer.workspace?.path?.split(/[/\\]/).pop() ?? 'Local workspace'}</span>
						{#if selected.answer.workspace?.revision}<span>snapshot {selected.answer.workspace.revision.slice(0, 8)}</span>{/if}
						<span>{selected.answer.evidence.length} evidence step{selected.answer.evidence.length === 1 ? '' : 's'}</span>
					</div>
					<div class="answer-surface">
						<MessageView
							message={detailMessage}
							expanded={evidenceOpen}
							ontoggle={() => (evidenceOpen = !evidenceOpen)}
						/>
					</div>
				{:else}
					<div class="detail-empty">Select an analysis to read it.</div>
				{/if}
			</article>
		</div>
	{/if}
</section>

<style>
	.analyses-page {
		flex: 1;
		min-height: 0;
		width: 100%;
		max-width: var(--content-max);
		margin: 0 auto;
		padding: var(--space-6) var(--pad) var(--space-6);
		overflow: auto;
	}
	.page-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-5);
		margin-bottom: var(--space-5);
	}
	.eyebrow {
		margin: 0 0 var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	h1,
	h2 {
		margin: 0;
		font-weight: 620;
		letter-spacing: -0.03em;
	}
	h1 {
		font-size: clamp(24px, 3vw, 32px);
		line-height: 1.15;
	}
	h2 {
		font-size: var(--fs-lg);
		line-height: 1.3;
	}
	.lede {
		margin: var(--space-2) 0 0;
		color: var(--text-dim);
	}
	.searchbox {
		width: min(280px, 100%);
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		color: var(--text-faint);
		background: var(--bg-inset);
		border: 1px solid transparent;
		border-radius: var(--radius-sm);
	}
	.searchbox:focus-within {
		border-color: var(--border-strong);
		box-shadow: var(--focus-ring);
	}
	.searchbox input {
		width: 100%;
		border: 0;
		outline: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-sm);
	}
	.searchbox input::placeholder {
		color: var(--text-faint);
	}
	.analysis-layout {
		display: grid;
		grid-template-columns: minmax(220px, 0.5fr) minmax(0, 1.5fr);
		min-height: 480px;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
		overflow: hidden;
	}
	.analysis-list {
		min-width: 0;
		padding: var(--space-2);
		border-right: 1px solid var(--border);
		overflow: auto;
	}
	.analysis-row {
		width: 100%;
		display: grid;
		grid-template-columns: 28px minmax(0, 1fr);
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-3);
		border-radius: var(--radius-sm);
		text-align: left;
	}
	.analysis-row:hover,
	.analysis-row.selected {
		background: var(--bg-inset);
	}
	.analysis-row.selected {
		box-shadow: inset 2px 0 var(--brand);
	}
	.analysis-icon {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--brand) 10%, transparent);
		color: var(--brand);
	}
	.analysis-copy {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.analysis-copy strong {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--fs-sm);
		font-weight: 560;
	}
	.analysis-copy small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.analysis-detail {
		min-width: 0;
		padding: var(--space-5) clamp(var(--space-4), 5vw, var(--space-6));
		overflow: auto;
	}
	.detail-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-4);
		padding-bottom: var(--space-4);
		border-bottom: 1px solid var(--border);
	}
	.detail-head h2 {
		font-size: clamp(18px, 2vw, 24px);
	}
	.question {
		margin: var(--space-2) 0 0;
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.detail-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		flex: none;
	}
	.pill {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}
	.icon-action {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.icon-action:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.icon-action.danger:hover {
		color: var(--err);
	}
	.provenance {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
		padding: var(--space-3) 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.provenance span {
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}
	.answer-surface {
		max-width: 76ch;
	}
	.detail-empty,
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		min-height: 360px;
		padding: var(--space-6);
		text-align: center;
	}
	.empty-icon {
		display: grid;
		place-items: center;
		width: 42px;
		height: 42px;
		margin-bottom: var(--space-3);
		border-radius: 50%;
		background: color-mix(in srgb, var(--brand) 10%, transparent);
		color: var(--brand);
	}
	.empty-state h2 {
		font-size: var(--fs-xl);
	}
	.empty-state p {
		max-width: 48ch;
		margin: var(--space-2) 0 var(--space-4);
		color: var(--text-dim);
	}
	@media (max-width: 760px) {
		.analyses-page {
			padding-inline: var(--space-4);
		}
		.page-head {
			flex-direction: column;
		}
		.searchbox {
			width: 100%;
		}
		.analysis-layout {
			grid-template-columns: 1fr;
		}
		.analysis-list {
			max-height: 220px;
			border-right: 0;
			border-bottom: 1px solid var(--border);
		}
	}
	@media (max-width: 520px) {
		.detail-head {
			flex-direction: column;
		}
	}
</style>
