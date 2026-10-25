<script lang="ts">
	import { ipc } from '$lib/ipc';
	import { Alert } from '$lib/components/ui/alert';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import { Spinner } from '$lib/components/ui/spinner';
	import type { AnalysisTurnReplayStatus } from '$lib/types';
	import Icon from './Icon.svelte';

	let {
		turnId,
		workspaceId = null,
		onrerun
	}: { turnId: string; workspaceId?: string | null; onrerun?: () => Promise<void> } = $props();

	let phase = $state<'idle' | 'loading' | 'ready' | 'error'>('idle');
	let status = $state<AnalysisTurnReplayStatus | null>(null);
	let error = $state('');
	let rerunning = $state(false);

	async function check(): Promise<void> {
		if (phase === 'loading') return;
		phase = 'loading';
		error = '';
		try {
			status = await ipc.analysisTurnReplayStatus(turnId, workspaceId);
			phase = 'ready';
		} catch (cause) {
			status = null;
			error = cause instanceof Error ? cause.message : String(cause);
			phase = 'error';
		}
	}

	async function rerun(): Promise<void> {
		if (!onrerun || rerunning || !status?.can_rerun) return;
		rerunning = true;
		try {
			await onrerun();
		} finally {
			rerunning = false;
		}
	}

	function summary(value: AnalysisTurnReplayStatus): string {
		if (!value.same_workspace) return 'Open the original workspace to rerun';
		if (!value.snapshot_available) return 'Older turn · freshness unavailable';
		if (!value.revision_changed) return 'Workspace unchanged since this answer';
		const count = value.source_changes.length;
		return `${count} source${count === 1 ? '' : 's'} changed since this answer`;
	}

	$effect(() => {
		workspaceId;
		if (phase === 'idle') void check();
	});
</script>

<div class="replay-status" aria-live="polite">
	{#if phase === 'loading'}
		<span class="replay-muted"><Spinner size={12} />Checking workspace freshness…</span>
	{:else if phase === 'error'}
		<Alert variant="destructive" class="replay-alert">
			<Icon name="alert" size={14} />
			<div class="alert-copy">
				<span>Couldn't check freshness.</span>
				{#if error}<span class="replay-error">{error}</span>{/if}
				<Button variant="link" size="sm" class="retry" onclick={() => void check()}>Try again</Button>
			</div>
		</Alert>
	{:else if status}
		<div class="replay-row">
			<Badge
				variant={status.revision_changed ? 'warning' : status.same_workspace && status.snapshot_available ? 'success' : 'default'}
				class="freshness-badge"
			>
				<span class="replay-mark" aria-hidden="true"></span>{summary(status)}
			</Badge>
			<div class="replay-actions">
				{#if status.source_changes.length}
					<span class="change-list">
						{#each status.source_changes.slice(0, 3) as change (change.name)}
							{change.kind} {change.name}{#if change !== status.source_changes.at(-1)}, {/if}
						{/each}
						{#if status.source_changes.length > 3} · +{status.source_changes.length - 3} more{/if}
					</span>
				{/if}
				{#if status.can_rerun && onrerun}
					<Button variant="secondary" size="sm" class="rerun" disabled={rerunning} onclick={() => void rerun()}>
						<Icon name="refresh" size={12} /> {rerunning ? 'Rerunning…' : 'Rerun'}
					</Button>
				{/if}
			</div>
		</div>
		{#if status.can_rerun && onrerun}
			<p class="rerun-note">Rerun repeats this saved request and its recorded context. To apply changed Investigation settings, run the Question from the Investigation.</p>
		{/if}
		{#if status.source_scope}
			<details class="scope-details">
				<summary>
					Run data scope · {status.source_scope.used_sources.length} source{status.source_scope.used_sources.length === 1 ? '' : 's'} used
				</summary>
				{#if status.source_scope.used_sources.length}
					<ul>
						{#each status.source_scope.used_sources as source (`${source.source}:${source.table ?? ''}`)}
							<li>{source.source}{#if source.table} <span>({source.table})</span>{/if}</li>
						{/each}
					</ul>
				{:else}
					<p>No source-backed evidence was recorded for this run.</p>
				{/if}
				{#if status.source_scope.skipped_sources.length}
					<p class="scope-caption">Skipped by the workspace catalog at run time</p>
					<ul>
						{#each status.source_scope.skipped_sources as skipped (skipped)}
							<li>{skipped}</li>
						{/each}
					</ul>
				{/if}
			</details>
		{/if}
	{:else}
		<Button variant="ghost" size="sm" class="replay-link" onclick={() => void check()}>
			<Icon name="refresh" size={12} /> Check workspace freshness
		</Button>
	{/if}
</div>

<style>
	.replay-status {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.replay-row,
	.replay-actions,
	.replay-muted {
		display: inline-flex;
		align-items: center;
	}
	.replay-row {
		justify-content: space-between;
		gap: var(--space-2);
		flex-wrap: wrap;
	}
	.replay-muted {
		gap: 6px;
		color: var(--text-faint);
	}
	.replay-mark {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: currentColor;
	}
	.replay-actions {
		gap: var(--space-2);
		flex-wrap: wrap;
	}
	.change-list {
		color: var(--text-faint);
	}
	.rerun-note { margin: var(--space-1) 0 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.scope-details {
		margin-top: var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.scope-details summary {
		width: fit-content;
		cursor: pointer;
		color: var(--text-dim);
	}
	.scope-details ul {
		margin: 4px 0 0;
		padding-left: 18px;
	}
	.scope-details li {
		overflow-wrap: anywhere;
	}
	.scope-details p {
		margin: 5px 0 0;
	}
	.scope-caption {
		color: var(--text-dim);
	}
	:global(.replay-alert) { align-items: center; padding: var(--space-2) var(--space-3); }
	.alert-copy { display: flex; align-items: center; flex-wrap: wrap; gap: var(--space-2); min-width: 0; }
	.replay-error {
		color: var(--text-faint);
		overflow-wrap: anywhere;
	}
	.replay-status :global(.replay-link) {
		height: auto;
		min-height: 0;
		padding: 0;
		border-radius: 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 400;
	}
	.replay-status :global(.replay-link:hover) {
		color: var(--text);
		text-decoration: underline;
	}
	.replay-status :global(.rerun) {
		height: auto;
		min-height: 0;
		padding: 2px 6px;
		border-radius: var(--radius-chip);
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.replay-status :global(.rerun:hover:not(:disabled)) {
		text-decoration: none;
	}
	.replay-status :global(.rerun:disabled) {
		cursor: wait;
		opacity: 0.7;
	}
</style>
