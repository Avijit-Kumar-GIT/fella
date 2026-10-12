<script lang="ts">
	import { Tabs } from '$lib/components/ui/tabs';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';
	import ContextView from './ContextView.svelte';
	import InvestigationBriefView from './InvestigationBriefView.svelte';

	let pane = $derived(session.workspacePane);

	function changePane(value: string): void {
		if (value === 'context' || value === 'investigation') session.setWorkspacePane(value);
	}
</script>

<section class="workspace" aria-label="Workspace">
	<Tabs.Root value={pane} onValueChange={changePane} class="workspace-tabs-root">
		<Tabs.List class="fella-ui-tabs-list fella-pill-nav workspace-tabs" aria-label="Workspace sections">
			<Tabs.Trigger value="investigation" class="fella-ui-tabs-trigger fella-pill-nav-item workspace-tab">
				<Icon name="bookmark" size={16} />
				<span>Investigation</span>
			</Tabs.Trigger>
			<Tabs.Trigger value="context" class="fella-ui-tabs-trigger fella-pill-nav-item workspace-tab">
				<Icon name="table" size={16} />
				<span>Data &amp; context</span>
			</Tabs.Trigger>
		</Tabs.List>

		<Tabs.Content value="investigation" class="workspace-content"><InvestigationBriefView /></Tabs.Content>
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
		gap: 2px;
		margin: var(--space-3) auto var(--space-1);
	}
	:global(.workspace-tab) {
		min-height: 30px;
		gap: 7px;
		padding-inline: 12px;
	}
	:global(.workspace-tab svg) {
		flex: none;
		color: var(--text-faint);
	}
	:global(.workspace-content) {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	:global(.workspace-content[hidden]) {
		display: none;
	}
</style>
