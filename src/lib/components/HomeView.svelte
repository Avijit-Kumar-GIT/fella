<script lang="ts">
	import { baseName, dispatch, openContext, openFolder } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { AnalysisArtifact, SourceInfo } from '$lib/types';
	import Icon from './Icon.svelte';

	let workspace = $derived(session.catalog.workspace);
	let folderName = $derived(workspace ? baseName(workspace) : 'Your workspace');
	let sources = $derived(session.catalog.sources);
	let skipped = $derived(session.catalog.skipped ?? []);
	let analyses = $derived.by(() => {
		if (!workspace) return session.analyses;
		return session.analyses.filter(
			(analysis) => !analysis.answer.workspace?.path || analysis.answer.workspace.path === workspace
		);
	});
	let rowCount = $derived(
		sources.reduce((total, source) => total + (source.row_count ?? 0), 0)
	);
	const SUGGESTIONS = [
		'What stands out in these files?',
		'How has this changed over time?',
		'Give me a useful summary of this folder.'
	];

	function ask(question: string): void {
		session.setWorkspaceView('ask');
		void dispatch(question);
	}

	function openAnalysis(analysis: AnalysisArtifact): void {
		session.selectAnalysis(analysis.id);
		session.setWorkspaceView('analyses');
	}

	function formatCount(value: number): string {
		return new Intl.NumberFormat().format(value);
	}

	function formatDate(ms: number): string {
		return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(ms);
	}

	function kindLabel(source: SourceInfo): string {
		return source.kind === 'ndjson' ? 'NDJSON' : source.kind.toUpperCase();
	}
</script>

<section class="home-page" aria-labelledby="home-title">
	<header class="home-head">
		<div>
			<p class="eyebrow">Workspace overview</p>
			<h1 id="home-title">{folderName}</h1>
			<p class="lede">A private view of the files you chose to explore.</p>
		</div>
		<button class="pill ghost" type="button" onclick={() => void openFolder()}>
			<Icon name="folder" size={14} /> Change folder
		</button>
	</header>

	<div class="stats" aria-label="Workspace summary">
		<div class="stat"><strong>{formatCount(sources.length)}</strong><span>readable files</span></div>
		<div class="stat"><strong>{formatCount(rowCount)}</strong><span>known rows</span></div>
		<div class="stat"><strong>{formatCount(analyses.length)}</strong><span>saved analyses</span></div>
		{#if skipped.length}<div class="stat warn"><strong>{formatCount(skipped.length)}</strong><span>files to review</span></div>{/if}
	</div>

	<div class="home-grid">
		<div class="main-column">
			<section class="ask-card" aria-labelledby="ask-title">
				<div class="ask-mark"><Icon name="compose" size={18} /></div>
				<div>
					<p class="eyebrow">Start with a question</p>
					<h2 id="ask-title">What would you like to understand?</h2>
					<p>Fella will show the files, calculations, and checks behind its answer.</p>
				</div>
				<button class="ask-button" type="button" onclick={() => session.setWorkspaceView('ask')}>
					<span>Ask a question</span><Icon name="chevron-right" size={14} />
				</button>
			</section>

			<section class="suggestions" aria-labelledby="suggestions-title">
				<div class="section-head"><h2 id="suggestions-title">Good places to begin</h2><span>From this workspace</span></div>
				<div class="suggestion-list">
					{#each SUGGESTIONS as suggestion (suggestion)}
						<button type="button" onclick={() => ask(suggestion)}>
							<span>{suggestion}</span><Icon name="arrow-up-right" size={13} />
						</button>
					{/each}
				</div>
			</section>

			<section class="source-summary" aria-labelledby="source-summary-title">
				<div class="section-head"><h2 id="source-summary-title">What Fella found</h2><button type="button" onclick={() => session.setWorkspaceView('sources')}>See all</button></div>
				<div class="source-chips">
					{#each sources.slice(0, 8) as source (source.path)}
						<span class="source-chip"><Icon name={source.view ? 'table' : 'file'} size={13} /><span>{source.name}</span><small>{kindLabel(source)}</small></span>
					{:else}
						<p class="quiet">No readable files yet. Open Sources to see what was skipped.</p>
					{/each}
				</div>
			</section>
		</div>

		<aside class="home-side">
			<section class="recent" aria-labelledby="recent-title">
				<div class="section-head"><h2 id="recent-title">Recent analyses</h2><button type="button" onclick={() => session.setWorkspaceView('analyses')}>View all</button></div>
				<div class="analysis-list">
					{#each analyses.slice(0, 4) as analysis (analysis.id)}
						<button class="analysis-row" type="button" onclick={() => openAnalysis(analysis)}>
							<span class="analysis-icon"><Icon name="bookmark" size={13} /></span>
							<span><strong>{analysis.title}</strong><small>{formatDate(analysis.created_at_ms)}</small></span>
						</button>
					{:else}
						<div class="quiet-block"><Icon name="bookmark" size={15} /><span>Save a useful answer and it will live here.</span></div>
					{/each}
				</div>
			</section>

			<section class="context-note">
				<div class="section-head"><h2>Make it yours</h2></div>
				<p>Tell Fella what your columns, dates, and categories mean in <code>fella.md</code>.</p>
				<button class="text-button" type="button" onclick={() => void openContext()}>Open workspace context <Icon name="arrow-up-right" size={13} /></button>
			</section>
		</aside>
	</div>
</section>

<style>
	.home-page {
		flex: 1;
		min-height: 0;
		width: min(1080px, 100%);
		margin: 0 auto;
		padding: var(--space-6) var(--pad) var(--space-6);
		overflow: auto;
	}
	.home-head {
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
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}
	h1,
	h2 {
		margin: 0;
		font-weight: 620;
		letter-spacing: -0.03em;
	}
	h1 {
		font-size: clamp(24px, 3vw, 34px);
		line-height: 1.12;
	}
	h2 {
		font-size: var(--fs-lg);
		line-height: 1.3;
	}
	.lede {
		margin: var(--space-2) 0 0;
		color: var(--text-dim);
	}
	.stats {
		display: flex;
		align-items: stretch;
		gap: 1px;
		margin-bottom: var(--space-5);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--border);
		overflow: hidden;
	}
	.stat {
		min-width: 120px;
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: var(--space-3) var(--space-4);
		background: var(--bg-raised);
	}
	.stat strong {
		font-size: var(--fs-lg);
		font-weight: 620;
	}
	.stat span {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.stat.warn strong {
		color: var(--warn);
	}
	.home-grid {
		display: grid;
		grid-template-columns: minmax(0, 1.35fr) minmax(250px, 0.65fr);
		gap: var(--space-5);
		align-items: start;
	}
	.main-column,
	.home-side {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		min-width: 0;
	}
	.ask-card {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--space-4);
		padding: var(--space-5);
		border: 1px solid color-mix(in srgb, var(--brand) 24%, var(--border));
		border-radius: var(--radius);
		background: color-mix(in srgb, var(--brand) 5%, var(--bg-raised));
	}
	.ask-mark {
		display: grid;
		place-items: center;
		width: 40px;
		height: 40px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--brand) 15%, transparent);
		color: var(--brand);
	}
	.ask-card p:not(.eyebrow) {
		margin: var(--space-1) 0 0;
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.ask-button {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-sm);
		background: var(--brand);
		color: #fff;
		font-size: var(--fs-sm);
		font-weight: 560;
		white-space: nowrap;
	}
	.ask-button:hover {
		background: color-mix(in srgb, var(--brand) 88%, var(--bg));
	}
	.suggestions,
	.source-summary,
	.recent,
	.context-note {
		padding-top: var(--space-4);
		border-top: 1px solid var(--border);
	}
	.section-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
		margin-bottom: var(--space-3);
	}
	.section-head > span {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.section-head button,
	.text-button {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		color: var(--link);
		font-size: var(--fs-xs);
	}
	.section-head button:hover,
	.text-button:hover {
		text-decoration: underline;
	}
	.suggestion-list {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-2);
	}
	.suggestion-list button {
		min-height: 74px;
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-2);
		padding: var(--space-3);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-raised);
		color: var(--text-dim);
		text-align: left;
		font-size: var(--fs-sm);
		transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.suggestion-list button:hover {
		border-color: var(--border-strong);
		background: var(--bg-inset);
		color: var(--text);
	}
	.source-chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.source-chip {
		max-width: 100%;
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 5px 8px;
		border: 1px solid var(--border);
		border-radius: var(--radius-chip);
		background: var(--bg-raised);
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.source-chip > span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.source-chip :global(svg) {
		color: var(--brand);
	}
	.source-chip small {
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10px;
	}
	.analysis-list {
		display: grid;
		gap: 2px;
	}
	.analysis-row {
		width: 100%;
		display: grid;
		grid-template-columns: 26px minmax(0, 1fr);
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2);
		border-radius: var(--radius-sm);
		text-align: left;
	}
	.analysis-row:hover {
		background: var(--bg-inset);
	}
	.analysis-icon {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--brand) 10%, transparent);
		color: var(--brand);
	}
	.analysis-row span:last-child {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.analysis-row strong {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--fs-sm);
		font-weight: 540;
	}
	.analysis-row small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.quiet,
	.quiet-block,
	.context-note p {
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.quiet {
		margin: 0;
	}
	.quiet-block {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2);
	}
	.quiet-block :global(svg) {
		flex: none;
		color: var(--text-faint);
	}
	.context-note p {
		margin: 0 0 var(--space-3);
	}
	.context-note code {
		font-family: var(--mono);
		color: var(--text-dim);
	}
	@media (max-width: 820px) {
		.home-grid {
			grid-template-columns: 1fr;
		}
		.home-side {
			display: grid;
			grid-template-columns: repeat(2, minmax(0, 1fr));
			gap: var(--space-5);
		}
	}
	@media (max-width: 620px) {
		.home-page {
			padding-inline: var(--space-4);
		}
		.home-head {
			flex-direction: column;
		}
		.stats {
			flex-wrap: wrap;
		}
		.stat {
			flex: 1 0 40%;
		}
		.ask-card {
			grid-template-columns: auto minmax(0, 1fr);
		}
		.ask-button {
			grid-column: 1 / -1;
			justify-self: start;
		}
		.suggestion-list,
		.home-side {
			grid-template-columns: 1fr;
		}
	}
</style>
