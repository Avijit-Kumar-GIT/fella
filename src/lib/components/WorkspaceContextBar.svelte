<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { openRepository } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';

	let workspacePath = $derived(session.activeChat.workspaceScope);
	let workspaceName = $derived(
		workspacePath?.replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) ?? ''
	);
	let workspaceAvailable = $derived(workspacePath === session.catalog.workspace);
	let historyOnly = $derived(Boolean(workspacePath && (
		session.workspaceAt(workspacePath)?.historyOnly || session.historyOnlyRepositoryPaths.includes(workspacePath)
	)));
	let isWorkspaceView = $derived(
		session.workspaceView === 'ask' ||
		session.workspaceView === 'workspace'
	);
</script>

{#if workspacePath && isWorkspaceView}
	<div class="workspace-context-bar">
		<div class="workspace-identity" role="group" aria-label={`Workspace: ${workspaceName}`}>
			<Icon name="folder" size={16} />
			<span class="workspace-identity-label">Workspace</span>
			<strong class="workspace-name" title={workspacePath}>{workspaceName}</strong>
		</div>

		{#if workspaceAvailable}
			<div class="workspace-context-actions">
				<Button
					variant="ghost"
					size="sm"
					class="workspace-pane-toggle"
					type="button"
					aria-expanded={session.workspacePaneOpen}
					aria-controls="workspace-tools-pane"
					aria-label={session.workspacePaneOpen ? 'Close workspace tools' : 'Open workspace tools'}
					title={session.workspacePaneOpen ? 'Close workspace tools' : 'Open workspace tools'}
					onclick={() => session.toggleWorkspacePane()}
				>
					<Icon name="panel" size={16} />
					<span>Tools</span>
				</Button>
			</div>
		{:else if session.workspaceView === 'ask' && historyOnly}
			<div class="workspace-history-action">
				<span class="workspace-history-label" role="status">History only</span>
				<Button
					variant="outline"
					size="sm"
					class="workspace-open-investigation"
					onclick={() => void openRepository(workspacePath, { reportFailure: true })}
				>
					<Icon name="refresh" size={14} />Reconnect folder
				</Button>
			</div>
		{/if}
	</div>
{/if}

<style>
	.workspace-context-bar {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		min-height: 48px;
		padding: 0 var(--pad);
		border-bottom: 1px solid var(--border);
		background: var(--workspace-canvas);
	}
	.workspace-identity {
		display: flex;
		flex: 1;
		align-items: center;
		gap: 8px;
		min-width: 0;
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.workspace-identity :global(svg) { flex: none; }
	.workspace-identity-label {
		flex: none;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.workspace-name {
		min-width: 0;
		overflow: hidden;
		color: var(--text);
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.workspace-context-actions,
	.workspace-history-action {
		display: flex;
		flex: none;
		align-items: center;
		gap: var(--space-3);
	}
	.workspace-history-label {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	:global(.workspace-pane-toggle) {
		flex: none;
		min-height: 34px;
		gap: 7px;
		padding-inline: 10px;
		border-radius: var(--radius);
		color: var(--text-dim);
	}
	:global(.workspace-pane-toggle:hover),
	:global(.workspace-pane-toggle[aria-expanded='true']) {
		background: var(--sidebar-hover);
		color: var(--text);
	}
	:global(.workspace-open-investigation) {
		flex: none;
		min-height: 30px;
		gap: 6px;
		border-radius: var(--radius);
		font-size: var(--fs-sm);
	}
</style>
