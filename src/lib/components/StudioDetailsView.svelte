<script lang="ts">
	import { ipc } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import type { ArtifactRecord, StudioDocument } from '$lib/analytics-artifacts';
	import Icon from './Icon.svelte';

	let { workspace, documentId }: { workspace: string | null; documentId: string | null } = $props();
	let document = $state<StudioDocument | null>(null);
	let artifacts = $state<ArtifactRecord[]>([]);
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
		error = '';
		if (!root || !id) {
			loading = false;
			return;
		}
		loading = true;
		void Promise.all([ipc.studioList(root), ipc.artifactList(root)]).then(([documents, available]) => {
			if (request !== requestId || workspace !== root || documentId !== id) return;
			document = documents.find((item) => item.id === id) ?? null;
			artifacts = available;
		}).catch((cause) => {
			if (request === requestId) error = cause instanceof Error ? cause.message : String(cause);
		}).finally(() => {
			if (request === requestId) loading = false;
		});
		return () => {
			if (request === requestId) requestId += 1;
		};
	});

	let includedArtifacts = $derived.by(() => {
		if (!document) return [];
		return document.placements.map((placement) => ({
			placement,
			artifact: artifacts.find((item) => item.id === placement.artifact_id) ?? null
		}));
	});
	let outline = $derived.by(() => {
		if (!document) return [];
		return document.body.split(/\r?\n/)
			.map((line) => line.match(/^#{1,3}\s+(.+?)\s*#*$/)?.[1]?.trim())
			.filter((heading): heading is string => Boolean(heading));
	});

	function dateLabel(timestamp: number): string {
		return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(timestamp));
	}
</script>

<aside class="report-details" aria-label="Report details" aria-busy={loading}>
	<header class="details-header">
		<div class="details-icon"><Icon name="file" size={16} /></div>
		<div class="details-title"><h2>{document?.title || 'Report details'}</h2><p>Report</p></div>
	</header>
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
					<ul class="outline-list">{#each outline as heading, index (`${heading}:${index}`)}<li>{heading}</li>{/each}</ul>
				{:else}
					<p class="section-empty">No report headings yet.</p>
				{/if}
			</section>

			<section class="details-section">
				<div class="section-heading"><h3>Saved analysis</h3><span>{document.placements.length}</span></div>
				{#if includedArtifacts.length}
					<ul class="analysis-list">
						{#each includedArtifacts as item (item.placement.id)}
							<li><span class="analysis-icon"><Icon name="table" size={14} /></span><span class="analysis-copy"><strong>{item.artifact?.title ?? 'Saved result'}</strong><small>Revision {item.placement.revision}</small></span></li>
						{/each}
					</ul>
				{:else}
					<p class="section-empty">No saved results in this report.</p>
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
			<p class="updated-label">Updated {dateLabel(document.updated_at_ms)}</p>
		</div>
	{/if}
</aside>

<style>
	.report-details { display:flex; flex:1; min-width:0; min-height:0; flex-direction:column; overflow:hidden; color:var(--text); }
	.details-header { display:flex; align-items:center; gap:11px; padding:22px 20px 18px; border-bottom:1px solid var(--border); }
	.details-icon { display:grid; width:34px; height:34px; flex:none; place-items:center; border-radius:11px; background:var(--bg-raised); color:var(--text-dim); }
	.details-title { min-width:0; }
	.details-title h2 { overflow:hidden; margin:0; font-size:var(--fs-md); font-weight:600; text-overflow:ellipsis; white-space:nowrap; }
	.details-title p { margin:3px 0 0; color:var(--text-faint); font-size:var(--fs-xs); }
	.details-scroll { min-height:0; flex:1; overflow:auto; padding:18px 20px 24px; }
	.details-section { padding:0 0 20px; margin:0 0 19px; border-bottom:1px solid var(--border); }
	.details-section h3 { margin:0; color:var(--text-faint); font-size:var(--fs-xs); font-weight:600; }
	.section-heading { display:flex; align-items:center; justify-content:space-between; }
	.section-heading > span { color:var(--text-faint); font-size:var(--fs-xs); }
	.outline-list, .analysis-list, .history-list { display:grid; gap:2px; margin:9px 0 0; padding:0; list-style:none; }
	.outline-list li { min-width:0; padding:6px 8px; border-radius:var(--radius-sm); color:var(--text-dim); font-size:var(--fs-sm); line-height:1.4; overflow-wrap:anywhere; }
	.analysis-list li { display:flex; min-width:0; align-items:center; gap:10px; padding:7px 4px; }
	.analysis-icon { display:grid; width:28px; height:28px; flex:none; place-items:center; border-radius:9px; background:var(--bg-raised); color:var(--text-faint); }
	.analysis-copy { display:grid; min-width:0; gap:2px; }
	.analysis-copy strong, .history-list strong { overflow:hidden; color:var(--text-dim); font-size:var(--fs-sm); font-weight:500; text-overflow:ellipsis; white-space:nowrap; }
	.analysis-copy small, .history-list small { color:var(--text-faint); font-size:11px; }
	.history-list { gap:10px; }
	.history-list li { display:grid; min-width:0; gap:3px; padding-left:10px; border-left:1px solid var(--border-strong); }
	.section-empty, .details-state { margin:10px 0 0; color:var(--text-faint); font-size:var(--fs-sm); line-height:1.5; }
	.details-state { padding:20px; }
	.details-error { color:var(--err); overflow-wrap:anywhere; }
	.updated-label { margin:0; color:var(--text-faint); font-size:11px; }
	@container investigation-pane (max-width: 430px) { .details-header { padding:16px 14px; } .details-scroll { padding:14px 14px 20px; } }
</style>
