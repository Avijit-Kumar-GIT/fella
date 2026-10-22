<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { openRepository } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';
	let { expanded = false, compact = false, ontoggleexpand }: { expanded?: boolean; compact?: boolean; ontoggleexpand?: () => void } = $props();

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

{#if isWorkspaceView}
	<div class="workspace-context-bar">
		<div class="workspace-identity" role="group" aria-label={workspaceName ? `Workspace: ${workspaceName}` : 'General conversation'}>
			{#if workspacePath}<Icon name="folder" size={16} />{/if}
			<strong class="workspace-name" title={workspacePath ?? undefined}>{workspaceName || 'General'}</strong>
		</div>
		<div class="layout-switch" role="group" aria-label="Window layout">
			<button class:active={session.contentLayout === 'chat'} aria-pressed={session.contentLayout === 'chat'} onclick={() => { session.setContentLayout('chat'); ontoggleexpand && expanded && ontoggleexpand(); }}>Chat</button>
			<button class:active={session.contentLayout === 'notebook'} aria-pressed={session.contentLayout === 'notebook'} onclick={() => session.setContentLayout('notebook')}>Notebook</button>
		</div>
		{#if workspaceAvailable}
			<div class="workspace-context-actions">
				{#if session.contentLayout === 'chat' && session.workspacePaneOpen && (compact || expanded)}
					<Button variant="ghost" size="sm" class="workspace-pane-toggle" title="Return to the current conversation" onclick={() => session.setWorkspacePaneOpen(false)}><Icon name="ask" size={14} />Conversation{#if session.activeChat.busy}<span class="conversation-activity" aria-label="Analysis running"> · Running</span>{/if}</Button>
				{/if}
				{#if session.workspacePaneOpen && !compact}
					<Button variant="ghost" size="sm" class="workspace-pane-toggle" aria-label={expanded ? 'Restore split view' : 'Expand workspace tools'} title={expanded ? 'Restore split view' : 'Expand workspace tools'} onclick={ontoggleexpand}><Icon name={expanded ? 'panel' : 'arrow-up-right'} size={14} /><span>{expanded ? 'Split view' : 'Expand'}</span></Button>
				{/if}
				<Button
					variant="ghost"
					size="sm"
					class="workspace-pane-toggle"
					type="button"
					aria-expanded={session.workspacePaneOpen}
					aria-controls="workspace-tools-pane"
					aria-label={session.contentLayout === 'notebook' ? 'Open a document' : session.workspacePaneOpen ? 'Close workspace tools' : 'Open workspace tools'}
					title={session.contentLayout === 'notebook' ? 'Open a document' : session.workspacePaneOpen ? 'Close workspace tools' : 'Open workspace tools'}
					onclick={() => session.contentLayout === 'notebook' ? session.openWorkspaceToolChooser() : session.toggleWorkspacePane()}
				>
					<Icon name="panel" size={16} />
					<span>{session.contentLayout === 'notebook' ? 'Documents' : 'Tools'}</span>
				</Button>
			</div>
		{:else if session.workspaceView === 'ask' && historyOnly}
			<div class="workspace-history-action">
				<span class="workspace-history-label" role="status">History only</span>
				<Button
					variant="outline"
					size="sm"
					class="workspace-open-investigation"
					onclick={() => { if (workspacePath) void openRepository(workspacePath, { reportFailure: true }); }}
				>
					<Icon name="refresh" size={14} />Reconnect folder
				</Button>
			</div>
		{/if}
	</div>
{/if}

<style>
	.layout-switch { display: flex; flex: none; gap: 2px; padding: 3px; border: 1px solid var(--border); border-radius: 999px; background: var(--bg-inset); }
	.layout-switch button { min-height: 28px; padding: 4px 12px; border: 0; border-radius: 999px; background: transparent; color: var(--text-dim); font: inherit; font-size: var(--fs-sm); cursor: pointer; }
	.layout-switch button.active { background: var(--bg-raised); color: var(--text); box-shadow: var(--shadow-sm); }
	.layout-switch button:focus-visible { outline: 2px solid var(--link); outline-offset: 2px; }
	@media (max-width: 650px) { .workspace-context-bar { gap: 8px; padding-inline: 12px; flex-wrap: wrap; padding-block: 6px; } .workspace-context-actions { gap: 4px; } }
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
