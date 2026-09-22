<script lang="ts">
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';
	import SourcesView from './SourcesView.svelte';
	import ContextView from './ContextView.svelte';

	let pane = $derived(session.workspacePane);
</script>

<section class="workspace" aria-label="Workspace">
	<nav class="workspace-tabs" aria-label="Workspace sections">
		<button
			class:active={pane === 'sources'}
			type="button"
			aria-current={pane === 'sources' ? 'page' : undefined}
			onclick={() => session.setWorkspacePane('sources')}
		>
			<Icon name="table" size={16} />
			<span>Sources</span>
			<small>{session.catalog.sources.length}</small>
		</button>
		<button
			class:active={pane === 'context'}
			type="button"
			aria-current={pane === 'context' ? 'page' : undefined}
			onclick={() => session.setWorkspacePane('context')}
		>
			<Icon name="file" size={16} />
			<span>Context</span>
		</button>
	</nav>

	{#if pane === 'sources'}
		<SourcesView />
	{:else}
		<ContextView />
	{/if}
</section>

<style>
	.workspace {
		flex: 1;
		min-width: 0;
		min-height: 0;
		display: flex;
		flex-direction: column;
		background: var(--bg);
	}
	.workspace-tabs {
		flex: none;
		display: flex;
		align-items: center;
		gap: 3px;
		max-width: var(--content-max);
		width: 100%;
		margin: 0 auto;
		padding: var(--space-3) var(--pad) 0;
		border-bottom: 1px solid var(--border);
	}
	.workspace-tabs button {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		min-height: 34px;
		padding: 0 10px;
		border-bottom: 2px solid transparent;
		color: var(--text-faint);
		font-size: var(--fs-sm);
		font-weight: 560;
		white-space: nowrap;
	}
	.workspace-tabs button:hover {
		color: var(--text);
	}
	.workspace-tabs button.active {
		border-bottom-color: var(--brand);
		color: var(--text);
	}
	.workspace-tabs button :global(svg) {
		color: var(--brand);
		flex: none;
	}
	.workspace-tabs small {
		color: var(--text-faint);
		font-size: 10px;
		font-variant-numeric: tabular-nums;
	}
</style>
