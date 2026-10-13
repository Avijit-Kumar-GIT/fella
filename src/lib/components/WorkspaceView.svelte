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
	<div class="workspace-tool-chooser" data-testid="workspace-tool-chooser" hidden={!session.workspaceToolChooserOpen}>
		<div class="workspace-tool-chooser-inner">
			<header class="workspace-tool-chooser-heading">
				<h2>Choose a tool</h2>
				<p>Open a tool for this workspace.</p>
			</header>
			<div class="workspace-tool-list" role="group" aria-label="Available workspace tools">
				<button
					class="workspace-tool-option"
					type="button"
					data-testid="workspace-tool-investigations"
					onclick={() => session.setWorkspacePane('investigation')}
				>
					<span class="workspace-tool-option-icon"><Icon name="bookmark" size={16} /></span>
					<span class="workspace-tool-option-copy">
						<strong>Investigations</strong>
						<small>Questions and results</small>
					</span>
					<Icon name="chevron-right" size={16} />
				</button>
				<button
					class="workspace-tool-option"
					type="button"
					data-testid="workspace-tool-context"
					onclick={() => session.setWorkspacePane('context')}
				>
					<span class="workspace-tool-option-icon"><Icon name="table" size={16} /></span>
					<span class="workspace-tool-option-copy">
						<strong>Data &amp; context</strong>
						<small>Files and workspace definitions</small>
					</span>
					<Icon name="chevron-right" size={16} />
				</button>
				<button
					class="workspace-tool-option workspace-tool-option-unavailable"
					type="button"
					disabled
					aria-label="Terminal. Shell access is not available in this version."
				>
					<span class="workspace-tool-option-icon workspace-terminal-icon" aria-hidden="true">›_</span>
					<span class="workspace-tool-option-copy">
						<strong>Terminal</strong>
						<small>Not available in this version</small>
					</span>
				</button>
			</div>
		</div>
	</div>
	<Tabs.Root value={pane} onValueChange={changePane} class="workspace-tabs-root" hidden={session.workspaceToolChooserOpen}>
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
	.workspace-tool-chooser {
		display: grid;
		flex: 1;
		min-width: 0;
		min-height: 0;
		place-items: center;
		padding: clamp(20px, 5vw, 48px);
	}
	.workspace-tool-chooser[hidden],
	:global(.workspace-tabs-root[hidden]) {
		display: none !important;
	}
	.workspace-tool-chooser-inner {
		width: min(100%, 440px);
	}
	.workspace-tool-chooser-heading {
		margin: 0 0 var(--space-4);
	}
	.workspace-tool-chooser-heading h2 {
		margin: 0;
		color: var(--text);
		font-size: var(--fs-lg);
		font-weight: 600;
		letter-spacing: -0.015em;
	}
	.workspace-tool-chooser-heading p {
		margin: var(--space-1) 0 0;
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.workspace-tool-list {
		display: grid;
		gap: var(--space-2);
	}
	.workspace-tool-option {
		display: flex;
		width: 100%;
		min-width: 0;
		min-height: 58px;
		align-items: center;
		gap: var(--space-3);
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-raised);
		color: var(--text);
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
	}
	.workspace-tool-option:hover:not(:disabled) {
		border-color: var(--border-strong);
		background: var(--bg-inset);
	}
	.workspace-tool-option:focus-visible {
		outline: none;
		box-shadow: var(--focus-ring);
	}
	.workspace-tool-option > :global(svg.icon) {
		margin-left: auto;
		color: var(--text-faint);
	}
	.workspace-tool-option-icon {
		display: grid;
		width: 32px;
		height: 32px;
		flex: none;
		place-items: center;
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.workspace-tool-option-copy {
		display: grid;
		min-width: 0;
		gap: 2px;
	}
	.workspace-tool-option-copy strong {
		font-size: var(--fs-sm);
		font-weight: 550;
	}
	.workspace-tool-option-copy small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.workspace-tool-option:disabled {
		background: transparent;
		color: var(--text-dim);
		cursor: not-allowed;
	}
	.workspace-terminal-icon {
		font-family: var(--mono);
		font-size: 15px;
		font-weight: 600;
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
