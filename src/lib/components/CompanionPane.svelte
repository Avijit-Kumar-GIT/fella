<script lang="ts">
	import { baseName } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { EvidenceItem, SourceInfo } from '$lib/types';
	import { isRenderableChartEvidence, paneFreshness } from '$lib/workbench';
	import Chart from './Chart.svelte';
	import Icon from './Icon.svelte';
	import SourcePreview from './SourcePreview.svelte';

	let pane = $derived(session.activeChat.companionPane);
	let source = $derived.by(() =>
		pane?.kind === 'source'
			? session.catalog.sources.find((item) => item.path === pane.path) ?? null
			: null
	);
	let chartEvidence = $derived.by((): EvidenceItem | null => {
		if (pane?.kind !== 'chart') return null;
		const message = session.activeChat.messages.find((item) => item.id === pane.messageId);
		const item = message?.answer?.evidence[pane.evidenceIndex];
		if (
			!item ||
			!message?.answer ||
			(pane.evidenceId && item.id !== pane.evidenceId) ||
			!isRenderableChartEvidence(item, message.answer.verification ?? [])
		) return null;
		return item;
	});
	let chartSource = $derived.by(() => {
		const names = [...new Set((chartEvidence?.sources ?? [])
			.map((item) => item.source.trim())
			.filter(Boolean))];
		if (names.length === 1) return `Based on ${names[0]}`;
		if (names.length > 1) return `Based on ${names.length} sources`;
		return 'Based on this analysis';
	});
	let freshness = $derived(
		pane
			? paneFreshness(pane, session.catalog.workspace, session.catalog.revision)
			: 'unknown'
	);
	let title = $derived(
		pane?.kind === 'chart'
			? chartEvidence?.chart?.title?.trim() || 'Chart'
			: pane?.kind === 'source'
				? pane.name
				: 'Companion'
	);
	let scopeNotice = $derived.by(() => {
		if (pane?.kind !== 'source') return '';
		const conversationWorkspace = session.activeChat.workspaceScope;
		if (conversationWorkspace && conversationWorkspace !== pane.workspacePath) {
			return 'This conversation is pinned to another folder; opening a preview does not change its scope.';
		}
		return 'Preview only · this file is not added to the conversation context.';
	});

	function relativePath(source: SourceInfo): string {
		const root = session.catalog.workspace?.replace(/[/\\]+$/, '');
		if (!root) return source.path;
		if (source.path === root) return baseName(source.path);
		if (source.path.startsWith(root + '/') || source.path.startsWith(root + '\\')) {
			return source.path.slice(root.length + 1).replace(/\\/g, '/');
		}
		return source.path;
	}

	function reopenCurrentSource(): void {
		if (source) session.openSourcePane(source);
	}
</script>

<aside class="companion" aria-label="Companion pane">
	<header class="companion-head">
		<div class="heading-copy">
			<span class="eyebrow">{pane?.kind === 'chart' ? 'Chart' : 'Source preview'}</span>
			<h2 title={title}>{title}</h2>
		</div>
		<button class="close" type="button" aria-label="Close companion pane and return to conversation" onclick={() => session.closeCompanionPane()}>
			<Icon name="x" size={16} />
			<span class="back-label">Conversation</span>
		</button>
	</header>

	<div class="companion-body">
		{#if pane?.kind === 'chart'}
			{#if freshness === 'different-workspace' || freshness === 'different-revision'}
				<p class="snapshot-note">This chart is from an earlier workspace snapshot. It remains the result produced for that analysis.</p>
			{/if}
			{#if chartEvidence?.chart}
				<Chart spec={chartEvidence.chart} source={chartSource} />
			{:else}
				<p class="empty">This chart is no longer available in the saved conversation.</p>
			{/if}
		{:else if pane?.kind === 'source'}
				{#if freshness === 'different-workspace' || !session.catalog.workspace}
					<p class="snapshot-note">This source belongs to a different mounted folder. Mount that folder to preview it.</p>
				{:else if freshness === 'different-revision'}
					<p class="snapshot-note">The workspace changed after this preview was opened. Reopen the current source to inspect the new snapshot.</p>
					{#if source}
						<button class="reopen" type="button" onclick={reopenCurrentSource}>Open current version</button>
					{/if}
				{:else if freshness === 'unknown'}
					<p class="snapshot-note">The saved snapshot cannot be confirmed. Open the current source to inspect the mounted version.</p>
					{#if source}
						<button class="reopen" type="button" onclick={reopenCurrentSource}>Open current version</button>
					{/if}
				{:else if !source}
				<p class="snapshot-note">This source is no longer in the mounted folder.</p>
			{:else}
				<div class="source-meta">
					<code>{relativePath(source)}</code>
					<p class="source-scope">{scopeNotice}</p>
					{#if source.synopsis}<p>{source.synopsis}</p>{/if}
					{#if source.note}<p class="source-note">{source.note}</p>{/if}
				</div>
				{#if source.view}
					<SourcePreview
						{source}
						workspacePath={session.catalog.workspace}
						revision={session.catalog.revision ?? null}
					/>
				{:else}
					<p class="empty">This file is cataloged as {source.kind.toUpperCase()}; a row preview is not available for this format.</p>
				{/if}
			{/if}
		{:else}
			<p class="empty">Choose a chart or source to open it beside the conversation.</p>
		{/if}
	</div>
</aside>

<style>
	.companion {
		display: flex;
		flex: 0 1 clamp(330px, 38%, 560px);
		flex-direction: column;
		min-width: 280px;
		min-height: 0;
		border-left: 1px solid var(--border);
		background: var(--bg);
	}
	.companion-head {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		min-height: 58px;
		padding: 8px var(--space-4);
		border-bottom: 1px solid var(--border);
	}
	.heading-copy { min-width: 0; }
	.eyebrow {
		display: block;
		margin-bottom: 2px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	h2 {
		margin: 0;
		overflow: hidden;
		color: var(--text);
		font-size: var(--fs-sm);
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.close {
		flex: none;
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.close:hover { background: var(--bg-inset); color: var(--text); }
	.companion-body {
		flex: 1;
		min-width: 0;
		min-height: 0;
		overflow: auto;
		padding: var(--space-3) var(--space-4) var(--space-5);
	}
	.companion-body :global(.chart-card) { margin-top: 0; }
	.source-meta { color: var(--text-dim); font-size: var(--fs-sm); }
	.source-meta code { color: var(--text); overflow-wrap: anywhere; }
	.source-meta p { margin: var(--space-2) 0; }
	.source-note { color: var(--text-faint); }
	.source-scope { color: var(--text-faint); font-size: var(--fs-xs); }
	.snapshot-note {
		margin: 0 0 var(--space-3);
		padding: var(--space-3);
		border-left: 2px solid var(--chart-gold);
		background: var(--bg-inset);
		color: var(--text-dim);
		font-size: var(--fs-sm);
		line-height: 1.5;
	}
	.reopen {
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text);
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.reopen:hover { background: var(--bg-inset); }
	.empty { color: var(--text-faint); font-size: var(--fs-sm); line-height: 1.5; }
	.back-label { display: none; }
	@media (max-width: 860px) {
		.companion {
			position: absolute;
			inset: 0;
			z-index: 3;
			min-width: 0;
			border-left: 0;
			background: var(--bg);
		}
		.back-label { display: inline; }
	}
</style>
