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
		? 'Add a tool tab'
		: session.workspacePaneOpen ? 'Close the panel' : 'Open the panel');
</script>

{#if isWorkspaceView}
	<div class="workspace-context-bar">
		<div class="layout-switch" role="group" aria-label="Workspace view">
			<Button
				variant="ghost"
				size="icon"
				class="workspace-layout-button"
				aria-label="Chat view"
				title="Chat view"
				aria-pressed={session.contentLayout === 'chat'}
				onclick={() => { session.setContentLayout('chat'); ontoggleexpand && expanded && ontoggleexpand(); }}
			><Icon name="ask" size={16} /></Button>
			<Button
				variant="ghost"
				size="icon"
				class="workspace-layout-button"
				aria-label="Notebook view"
				title="Notebook view"
				aria-pressed={session.contentLayout === 'notebook'}
				onclick={() => session.setContentLayout('notebook')}
			><Icon name="layout-grid" size={16} /></Button>
		</div>
		<div class="workspace-context-actions">
			{#if session.workspacePaneOpen && !compact}
				<Button variant="ghost" size="icon" class="workspace-pane-toggle" aria-label={expanded ? 'Restore split view' : 'Expand panel'} title={expanded ? 'Restore split view' : 'Expand panel'} onclick={ontoggleexpand}><Icon name={expanded ? 'panel' : 'arrow-up-right'} size={14} /></Button>
			{/if}
			{#if workspaceAvailable}
				<Button
					variant="ghost"
					size="icon"
					class="workspace-pane-toggle panel-action"
					type="button"
					aria-expanded={session.contentLayout === 'chat' ? session.workspacePaneOpen : undefined}
					aria-controls="workspace-tools-pane"
					aria-label={panelActionLabel}
					title={panelActionLabel}
					onclick={() => session.contentLayout === 'notebook' ? session.openWorkspaceToolChooser() : session.toggleWorkspacePane()}
				>
					<Icon name="panel" size={16} />
				</Button>
			{:else if historyOnly}
				<Button
					variant="ghost"
					size="icon"
					class="workspace-pane-toggle reconnect-action"
					title="Reconnect folder"
					aria-label="Reconnect folder"
					onclick={() => { if (workspacePath) void openRepository(workspacePath, { reportFailure: true }); }}
				>
					<Icon name="refresh" size={14} />
				</Button>
			{/if}
		</div>
	</div>
{/if}

<style>
	.workspace-context-bar { display:flex; flex:none; align-items:center; gap:5px; min-width:0; }
	.layout-switch { display:flex; flex:none; align-items:center; gap:1px; }
	.workspace-context-actions { display:flex; flex:none; align-items:center; gap:2px; }
	:global(.workspace-layout-button), :global(.workspace-pane-toggle) { display:grid; place-items:center; flex:none; width:30px; min-width:30px; height:30px; min-height:30px; padding:0; border-radius:var(--radius-chip); color:var(--text-faint); }
	:global(.workspace-layout-button:hover), :global(.workspace-pane-toggle:hover), :global(.workspace-layout-button[aria-pressed='true']), :global(.workspace-pane-toggle[aria-expanded='true']) { background:var(--bg-inset); color:var(--text); }
</style>
