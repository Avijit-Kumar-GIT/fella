<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { openRepository } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';
	let { expanded = false, compact = false, ontoggleexpand }: { expanded?: boolean; compact?: boolean; ontoggleexpand?: () => void } = $props();

	let workspacePath = $derived(session.activeChat.workspaceScope);
	let workspaceAvailable = $derived(workspacePath === session.catalog.workspace);
	let historyOnly = $derived(Boolean(workspacePath && (
		session.workspaceAt(workspacePath)?.historyOnly || session.historyOnlyRepositoryPaths.includes(workspacePath)
	)));
	let isWorkspaceView = $derived(
		session.workspaceView === 'ask' ||
		session.workspaceView === 'workspace'
	);
	let panelActionLabel = $derived(session.contentLayout === 'notebook'
		? 'Add a panel tab'
		: session.workspacePaneOpen ? 'Close the panel' : 'Open the panel');
</script>

{#if isWorkspaceView}
	<div class="workspace-context-bar">
		<div class="layout-switch fella-pill-nav" role="group" aria-label="Window layout">
			<button class="fella-pill-nav-item" aria-pressed={session.contentLayout === 'chat'} onclick={() => { session.setContentLayout('chat'); ontoggleexpand && expanded && ontoggleexpand(); }}>Chat</button>
			<button class="fella-pill-nav-item" aria-pressed={session.contentLayout === 'notebook'} onclick={() => session.setContentLayout('notebook')}>Notebook</button>
		</div>
		<div class="workspace-context-actions">
			{#if session.contentLayout === 'chat' && session.workspacePaneOpen && (compact || expanded)}
				<Button variant="ghost" size="sm" class="workspace-pane-toggle" aria-label="Show chat" title="Show chat" onclick={() => session.setWorkspacePaneOpen(false)}><Icon name="ask" size={14} /><span>Chat{#if session.activeChat.busy}<span class="conversation-activity" aria-label="Analysis running"> · Running</span>{/if}</span></Button>
			{/if}
			{#if session.workspacePaneOpen && !compact}
				<Button variant="ghost" size="sm" class="workspace-pane-toggle" aria-label={expanded ? 'Restore split view' : 'Expand panel'} title={expanded ? 'Restore split view' : 'Expand panel'} onclick={ontoggleexpand}><Icon name={expanded ? 'panel' : 'arrow-up-right'} size={14} /><span>{expanded ? 'Split' : 'Expand'}</span></Button>
			{/if}
			{#if workspaceAvailable}
				<Button
					variant="ghost"
					size="sm"
					class="workspace-pane-toggle panel-action"
					type="button"
					aria-expanded={session.contentLayout === 'chat' ? session.workspacePaneOpen : undefined}
					aria-controls="workspace-tools-pane"
					aria-label={panelActionLabel}
					title={panelActionLabel}
					onclick={() => session.contentLayout === 'notebook' ? session.openWorkspaceToolChooser() : session.toggleWorkspacePane()}
				>
					<Icon name="panel" size={16} />
					<span>Panel</span>
				</Button>
			{:else if historyOnly}
				<Button
					variant="ghost"
					size="sm"
					class="workspace-pane-toggle reconnect-action"
					title="Reconnect folder"
					aria-label="Reconnect folder"
					onclick={() => { if (workspacePath) void openRepository(workspacePath, { reportFailure: true }); }}
				>
					<Icon name="refresh" size={14} /><span>Reconnect</span>
				</Button>
			{/if}
		</div>
	</div>
{/if}

<style>
	.workspace-context-bar { display:flex; flex:none; align-items:center; gap:6px; min-width:0; }
	.layout-switch { display:flex; flex:none; gap:2px; padding:2px; }
	.layout-switch :global(.fella-pill-nav-item) { min-height:26px; padding-inline:10px; }
	.workspace-context-actions { display:flex; flex:none; align-items:center; gap:2px; }
	:global(.workspace-pane-toggle) { flex:none; min-height:30px; gap:6px; padding-inline:8px; border-radius:var(--radius-chip); color:var(--text-dim); font-size:var(--fs-xs); }
	:global(.workspace-pane-toggle:hover), :global(.workspace-pane-toggle[aria-expanded='true']) { background:var(--bg-inset); color:var(--text); }
	@media (max-width: 900px) {
		.layout-switch :global(.fella-pill-nav-item) { padding-inline:7px; }
		:global(.workspace-pane-toggle) { width:30px; min-width:30px; padding-inline:0; justify-content:center; }
		:global(.workspace-pane-toggle > span) { display:none; }
	}
</style>
