<script lang="ts">
	import { Tabs } from '$lib/components/ui/tabs';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';
	import SourcesView from './SourcesView.svelte';
	import ContextView from './ContextView.svelte';

	let pane = $derived(session.workspacePane);

	function changePane(value: string): void {
		if (value === 'sources' || value === 'context') session.setWorkspacePane(value);
	}
</script>

<section class="workspace" aria-label="Workspace">
	<Tabs.Root value={pane} onValueChange={changePane} class="workspace-tabs-root">
		<Tabs.List class="fella-ui-tabs-list workspace-tabs" aria-label="Workspace sections">
			<Tabs.Trigger value="sources" class="fella-ui-tabs-trigger workspace-tab">
				<Icon name="table" size={16} />
				<span>Sources</span>
			</Tabs.Trigger>
			<Tabs.Trigger value="context" class="fella-ui-tabs-trigger workspace-tab">
				<Icon name="bookmark" size={16} />
				<span>Guide</span>
			</Tabs.Trigger>
		</Tabs.List>

		<Tabs.Content value="sources" class="workspace-content"><SourcesView /></Tabs.Content>
		<Tabs.Content value="context" class="workspace-content"><ContextView /></Tabs.Content>
	</Tabs.Root>
</section>

<style>
	.workspace {
		flex: 1;
		min-width: 0;
		min-height: 0;
		background: var(--bg);
	}
	:global(.workspace-tabs-root) {
		height: 100%;
		min-width: 0;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
	:global(.workspace-tabs) {
		flex: none;
		gap: 3px;
		max-width: var(--content-max);
		width: 100%;
		margin: 0 auto;
		padding: var(--space-3) var(--pad) 0;
		border-bottom: 1px solid var(--border);
	}
	:global(.workspace-tab) {
		min-height: 34px;
		gap: 7px;
		padding: 0 10px;
		border-bottom: 2px solid transparent;
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	:global(.workspace-tab[data-state='active']) {
		border-bottom-color: var(--brand);
	}
	:global(.workspace-tab svg) {
		flex: none;
		color: var(--text-faint);
	}
	:global(.workspace-content) {
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
</style>
