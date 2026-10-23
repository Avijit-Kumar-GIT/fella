<script lang="ts">
	import { ipc } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import type { AnalysisTurn } from '$lib/types';
	import type { ArtifactRecord, StudioDocument } from '$lib/analytics-artifacts';
	import Icon from './Icon.svelte';

	let { workspace, documentId, onclose }: { workspace: string | null; documentId: string | null; onclose?: () => void } = $props();
	let document = $state<StudioDocument | null>(null);
	let artifacts = $state<ArtifactRecord[]>([]);
	let analysisRuns = $state<AnalysisTurn[]>([]);
	let activeOutline = $state('');
	let loading = $state(false);
	let error = $state('');
	let requestId = 0;

	$effect(() => {
		const root = workspace;
		const id = documentId;
		const version = session.reportsVersion;
		void version;
		const request = ++requestId;
		document = null;
		artifacts = [];
		analysisRuns = [];
		error = '';
		if (!root || !id) {
			loading = false;
			return;
		}
		loading = true;
		void Promise.all([ipc.studioList(root), ipc.artifactList(root)]).then(async ([documents, available]) => {
			if (request !== requestId || workspace !== root || documentId !== id) return;
			const selectedDocument = documents.find((item) => item.id === id) ?? null;
			document = selectedDocument;
			artifacts = available;
			if (selectedDocument) {
				const runIds = [...new Set(selectedDocument.placements.flatMap((placement) => {
					const revision = available.find((item) => item.id === placement.artifact_id)?.revisions.find((item) => item.revision === placement.revision);
					return revision?.run_id ? [revision.run_id] : [];
				}))];
				const runs = await Promise.all(runIds.map((runId) => ipc.analysisTurnLoad(runId).catch(() => null)));
				if (request === requestId && workspace === root && documentId === id) analysisRuns = runs.filter((run): run is AnalysisTurn => run !== null);
			}
		}).catch((cause) => {
			if (request === requestId) error = cause instanceof Error ? cause.message : String(cause);
		}).finally(() => {
			if (request === requestId) loading = false;
		});
		return () => {
			if (request === requestId) requestId += 1;
		};
	});

	let outline = $derived.by(() => {
		if (!document) return [];
		return document.body.split(/\r?\n/)
			.map((line) => line.match(/^#{1,3}\s+(.+?)\s*#*$/)?.[1]?.trim())
			.filter((heading): heading is string => Boolean(heading));
	});
	let citedSources = $derived([...new Set(analysisRuns.flatMap((run) => run.source_scope?.used_sources.map((source) => source.source) ?? []))]);
	let runsNeedingReview = $derived(analysisRuns.filter((run) => run.result.status !== 'verified').length);

	function dateLabel(timestamp: number): string {
		return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(timestamp));
	}

	function jumpToHeading(heading: string): void {
		activeOutline = heading;
		const target = Array.from(window.document.querySelectorAll<HTMLElement>('.studio-document h1, .studio-document h2, .studio-document h3'))
			.find((item) => item.closest<HTMLElement>('[data-studio-document-id]')?.dataset.studioDocumentId === documentId
				&& item.textContent?.trim() === heading);
		target?.scrollIntoView({ behavior: 'smooth', block: 'start' });
	}
</script>

<aside class="report-details" aria-label="Report details" aria-busy={loading}>
	<div class="details-toolbar"><span>Report details</span><button type="button" aria-label="Close report details" title="Close panel" onclick={() => onclose?.()}><Icon name="x" size={14} /></button></div>
	{#if loading}
		<p class="details-state" role="status">Loading report details…</p>
	{:else if error}
		<p class="details-state details-error" role="alert">Couldn’t load report details: {error}</p>
	{:else if !document}
		<p class="details-state">Choose a report or create one to see its contents here.</p>
	{:else}
		<div class="details-scroll">
			<section class="details-section">
				<h3>Outline</h3>
				{#if outline.length}
					<ul class="outline-list">{#each outline as heading, index (`${heading}:${index}`)}<li><button class:active={activeOutline === heading} onclick={() => jumpToHeading(heading)}><i></i>{heading}</button></li>{/each}</ul>
				{:else}
					<p class="section-empty">No report headings yet.</p>
				{/if}
			</section>

			<section class="details-section">
				<div class="section-heading"><h3>Review</h3><span>{analysisRuns.length} {analysisRuns.length === 1 ? 'run' : 'runs'}</span></div>
				<div class="review-card">
					{#if !analysisRuns.length}
						<p>No analysis is linked to this report yet.</p>
					{:else if runsNeedingReview}
						<p>{runsNeedingReview} saved {runsNeedingReview === 1 ? 'result needs' : 'results need'} review.</p>
					{:else}
						<p>All linked results passed their recorded checks.</p>
					{/if}
				</div>
			</section>

			<section class="details-section">
				<div class="section-heading"><h3>Sources cited</h3><span>{citedSources.length}</span></div>
				{#if citedSources.length}
					<ul class="source-list">{#each citedSources as source (source)}<li><Icon name={/\.(md|txt|pdf)$/i.test(source) ? 'file' : 'table'} size={12} /><span title={source}>{source}</span></li>{/each}</ul>
				{:else}
					<p class="section-empty">No source references are available.</p>
				{/if}
			</section>

			{#if document.checkpoints.length}
				<section class="details-section">
					<div class="section-heading"><h3>History</h3><span>{document.checkpoints.length}</span></div>
					<ol class="history-list">
						{#each [...document.checkpoints].reverse().slice(0, 5) as checkpoint (checkpoint.id)}
							<li><strong>{checkpoint.title}</strong><small>{dateLabel(checkpoint.created_at_ms)}</small></li>
						{/each}
					</ol>
				</section>
			{/if}
		</div>
	{/if}
</aside>

<style>
	.report-details { display:flex; flex:1; min-width:0; min-height:0; flex-direction:column; overflow:hidden; color:var(--text); }
	.details-toolbar { display:flex; height:48px; flex:none; align-items:center; justify-content:space-between; padding:0 14px; color:var(--text-dim); font-size:var(--fs-xs); }
	.details-toolbar button { display:grid; width:30px; height:30px; place-items:center; border:0; border-radius:50%; background:var(--bg-raised); color:var(--text-dim); cursor:pointer; }
	.details-toolbar button:hover { background:var(--bg-inset); color:var(--text); }
	.details-scroll { min-height:0; flex:1; overflow:auto; padding:18px 20px 24px; }
	.details-section { padding:0 0 18px; margin:0 0 22px; }
	.details-section h3 { margin:0; color:var(--text-faint); font-size:var(--fs-xs); font-weight:600; }
	.section-heading { display:flex; align-items:center; justify-content:space-between; }
	.section-heading > span { color:var(--text-faint); font-size:var(--fs-xs); }
	.outline-list, .history-list { display:grid; gap:2px; margin:9px 0 0; padding:0; list-style:none; }
	.outline-list button { display:flex; width:100%; align-items:center; gap:9px; padding:6px 8px; border:0; border-radius:9px; background:transparent; color:var(--text-dim); font:inherit; font-size:var(--fs-sm); line-height:1.4; text-align:left; cursor:pointer; overflow-wrap:anywhere; }
	.outline-list button:hover { color:var(--text); }
	.outline-list button.active { color:var(--text); }
	.outline-list button i { width:2px; height:15px; flex:none; border-radius:999px; background:transparent; }
	.outline-list button.active i { background:var(--brand-icon); }
	.review-card { padding:12px 14px; border-radius:16px; background:var(--bg-raised); color:var(--text-dim); font-size:var(--fs-xs); line-height:1.5; }
	.review-card p { margin:0; }
	.source-list { display:grid; gap:4px; margin:9px 0 0; padding:0; list-style:none; }
	.source-list li { display:flex; min-width:0; align-items:center; gap:9px; padding:6px 4px; color:var(--text-dim); font:11px var(--mono); }
	.source-list li > :global(svg) { flex:none; color:var(--text-faint); }
	.source-list li > span:last-child { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
	.history-list strong { overflow:hidden; color:var(--text-dim); font-size:var(--fs-sm); font-weight:500; text-overflow:ellipsis; white-space:nowrap; }
	.history-list small { color:var(--text-faint); font-size:11px; }
	.history-list { gap:10px; }
	.history-list li { display:grid; min-width:0; gap:3px; padding-left:10px; border-left:1px solid var(--border-strong); }
	.section-empty, .details-state { margin:10px 0 0; color:var(--text-faint); font-size:var(--fs-sm); line-height:1.5; }
	.details-state { padding:20px; }
	.details-error { color:var(--err); overflow-wrap:anywhere; }
	@container investigation-pane (max-width: 430px) { .details-toolbar { padding-inline:12px; } .details-scroll { padding:16px 14px 20px; } }
</style>
