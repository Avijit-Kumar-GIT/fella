<script lang="ts">
	import { ipc } from '$lib/ipc';
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
		<span class="replay-muted"><span class="spinner" aria-hidden="true"></span>Checking workspace freshness…</span>
	{:else if phase === 'error'}
		<button class="replay-link" type="button" onclick={() => void check()}>
			<Icon name="refresh" size={12} /> Couldn't check freshness
		</button>
		{#if error}<span class="replay-error">{error}</span>{/if}
	{:else if status}
		<div class="replay-row">
			<span class:changed={status.revision_changed} class="replay-muted">
				<span class="replay-mark" aria-hidden="true"></span>{summary(status)}
			</span>
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
					<button class="rerun" type="button" disabled={rerunning} onclick={() => void rerun()}>
						<Icon name="refresh" size={12} /> {rerunning ? 'Rerunning…' : 'Rerun'}
					</button>
				{/if}
			</div>
		</div>
	{:else}
		<button class="replay-link" type="button" onclick={() => void check()}>
			<Icon name="refresh" size={12} /> Check workspace freshness
		</button>
	{/if}
</div>

<style>
	.replay-status {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.replay-row,
	.replay-actions,
	.replay-muted,
	.replay-link,
	.rerun {
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
	.replay-muted.changed {
		color: var(--warn);
	}
	.replay-mark {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ok);
	}
	.changed .replay-mark {
		background: var(--warn);
	}
	.replay-actions {
		gap: var(--space-2);
		flex-wrap: wrap;
	}
	.change-list,
	.replay-error {
		color: var(--text-faint);
	}
	.replay-error {
		display: block;
		margin-top: 4px;
		overflow-wrap: anywhere;
	}
	.replay-link,
	.rerun {
		gap: 5px;
		padding: 0;
		border: 0;
		border-radius: 0;
		background: transparent;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.replay-link:hover,
	.rerun:hover:not(:disabled) {
		color: var(--text);
		text-decoration: underline;
	}
	.rerun {
		padding: 2px 6px;
		border-radius: var(--radius-chip);
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.rerun:hover:not(:disabled) {
		text-decoration: none;
	}
	.rerun:disabled {
		cursor: wait;
		opacity: 0.65;
	}
	.spinner {
		width: 10px;
		height: 10px;
		border: 1px solid var(--border-strong);
		border-top-color: var(--brand);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
