<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';

	let workspacePath = $derived(session.activeChat.workspaceScope);
	let workspaceName = $derived(
		workspacePath?.replace(/[/\\]+$/, '').split(/[\\/]/).at(-1) ?? ''
	);
	let workspaceAvailable = $derived(workspacePath === session.catalog.workspace);
	let isWorkspaceView = $derived(
		session.workspaceView === 'ask' ||
		session.workspaceView === 'board' ||
		session.workspaceView === 'workspace'
	);
</script>

{#if workspacePath && isWorkspaceView}
	<div class="workspace-context-bar">
		<div class="workspace-identity" role="group" aria-label={`Workspace: ${workspaceName}`}>
			<Icon name="folder" size={16} />
			<span class="workspace-identity-label">Workspace</span>
			<span class="workspace-identity-separator" aria-hidden="true">/</span>
			<strong class="workspace-name" title={workspacePath}>{workspaceName}</strong>
		</div>

		{#if session.workspaceView === 'ask' || session.workspaceView === 'board'}
			<Button
				variant="outline"
				size="sm"
				class="workspace-open-investigation"
				disabled={!workspaceAvailable}
				title={workspaceAvailable ? undefined : 'Reconnect this workspace to open its Investigation'}
				onclick={() => session.setWorkspacePane('investigation')}
			>
				<Icon name="bookmark" size={14} />Open Investigation
			</Button>
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
		align-items: center;
		gap: 8px;
		min-width: 0;
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.workspace-identity :global(svg) {
		flex: none;
	}
	.workspace-identity-label,
	.workspace-identity-separator {
		flex: none;
	}
	.workspace-name {
		min-width: 0;
		overflow: hidden;
		color: var(--text);
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	:global(.workspace-open-investigation) {
		flex: none;
		min-height: 30px;
		gap: 6px;
		border-radius: var(--radius);
		font-size: var(--fs-sm);
	}
	@media (max-width: 620px) {
		.workspace-context-bar {
			gap: var(--space-2);
			padding-inline: var(--space-2);
		}
		.workspace-identity-label,
		.workspace-identity-separator {
			display: none;
		}
	}
</style>
