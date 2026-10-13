<script lang="ts">
	import { ipc } from '$lib/ipc';
	import { focusInvestigationConversation } from '$lib/investigation-navigation';
	import { session, type WorkspaceToolTab } from '$lib/session.svelte';
	import type { Investigation } from '$lib/types';
	import Icon from './Icon.svelte';
	import ContextView from './ContextView.svelte';
	import InvestigationBriefView from './InvestigationBriefView.svelte';

	let workspacePath = $derived(session.catalog.workspace);
	let investigations = $state<Investigation[]>([]);
	let investigationsUnavailable = $state(false);
	let investigationBusy = $state(false);
	let investigationsOpen = $state(false);
	let chooserError = $state('');
	let listRequest = 0;
	let selectionRequest = 0;
	let restoringTabKey = '';
	let activeTab = $derived(session.activeWorkspaceToolTab);
	let currentInvestigation = $derived.by(() => {
		const linkedToChat = investigations.find((item) => item.conversation_id === session.activeChat.id);
		if (linkedToChat) return linkedToChat;
		if (activeTab?.kind === 'investigation' && activeTab.conversationId === session.activeChat.id && activeTab.investigationId) {
			return investigations.find((item) => item.id === activeTab.investigationId) ?? null;
		}
		return null;
	});
	let otherInvestigations = $derived(investigations.filter((item) => item.id !== currentInvestigation?.id));

	$effect(() => {
		const path = workspacePath;
		const version = session.investigationsVersion;
		void version;
		if (!path) {
			listRequest += 1;
			investigations = [];
			investigationsUnavailable = false;
			return;
		}
		const request = ++listRequest;
		investigationsUnavailable = false;
		void ipc.investigationsList().then((records) => {
			if (request !== listRequest || path !== workspacePath) return;
			investigations = records.filter((item) => item.workspace_id === path)
				.sort((left, right) => right.updated_at_ms - left.updated_at_ms);
		}).catch(() => {
			if (request !== listRequest) return;
			investigations = [];
			investigationsUnavailable = true;
		});
		return () => {
			if (request === listRequest) listRequest += 1;
		};
	});

	$effect(() => {
		const path = workspacePath;
		const tab = activeTab;
		const paneOpen = session.workspacePaneOpen;
		const records = investigations;
		if (!path || !paneOpen || !tab || tab.kind !== 'investigation' || !tab.investigationId) return;
		const item = records.find((record) => record.id === tab.investigationId);
		if (!item || (session.activeChat.id === tab.conversationId && session.activeInvestigationId === item.id)) return;
		const key = `${path}:${tab.id}`;
		if (restoringTabKey === key) return;
		restoringTabKey = key;
		void focusInvestigationConversation(
			item,
			'preserve',
			undefined,
			() => session.workspacePaneOpen && session.catalog.workspace === path && session.activeWorkspaceToolTab?.id === tab.id
		).then((linked) => {
			if (!linked || !session.workspacePaneOpen || session.catalog.workspace !== path || session.activeWorkspaceToolTab?.id !== tab.id) return;
			session.openWorkspaceToolTab('investigation', linked.id, linked.conversation_id ?? session.activeChat.id);
			session.activateWorkspaceToolTab(tab.id);
		}).catch((cause) => {
			if (session.activeWorkspaceToolTab?.id === tab.id) chooserError = cause instanceof Error ? cause.message : String(cause);
		}).finally(() => {
			if (restoringTabKey === key) restoringTabKey = '';
		});
	});

	function investigationCoverage(item: Investigation): string {
		const active = (item.questions ?? []).filter((question) => question.status === 'active');
		if (!active.length) return item.status === 'complete' ? 'Complete' : 'No Questions yet';
		const answered = active.filter((question) => question.answer_status === 'answered').length;
		const open = active.length - answered;
		if (item.status === 'archived') return `Archived · ${answered}/${active.length} answered`;
		if (item.status === 'complete') return open ? `Closed with gaps · ${open}` : `Complete · ${answered}/${active.length}`;
		return open ? `${answered}/${active.length} answered · ${open} need attention` : `${answered}/${active.length} answered`;
	}

	function nextInvestigationTitle(): string {
		const titles = new Set(investigations.map((item) => item.title.trim().toLocaleLowerCase()));
		if (!titles.has('new investigation')) return 'New investigation';
		let suffix = 2;
		while (titles.has(`new investigation ${suffix}`)) suffix += 1;
		return `New investigation ${suffix}`;
	}

	async function openInvestigation(item: Investigation, tabId?: string): Promise<void> {
		if (investigationBusy || !workspacePath || item.workspace_id !== workspacePath) return;
		investigationsOpen = false;
		const request = ++selectionRequest;
		const path = workspacePath;
		investigationBusy = true;
		chooserError = '';
		try {
			const linked = await focusInvestigationConversation(
				item,
				'preserve',
				undefined,
				() => request === selectionRequest && workspacePath === path && session.catalog.workspace === path
			);
			if (!linked || request !== selectionRequest || workspacePath !== path) return;
			investigations = investigations.map((record) => record.id === linked.id ? linked : record);
			session.openWorkspaceToolTab('investigation', linked.id, linked.conversation_id ?? session.activeChat.id);
			if (tabId) session.activateWorkspaceToolTab(tabId);
			session.setActiveInvestigation(linked.id);
			session.investigationsVersion += 1;
		} catch (cause) {
			if (request === selectionRequest) chooserError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (request === selectionRequest) investigationBusy = false;
		}
	}

	function openCurrentInvestigation(): void {
		investigationsOpen = false;
		const tab = session.openWorkspaceToolTab('investigation', null, session.activeChat.id);
		if (tab) session.activateWorkspaceToolTab(tab.id);
	}

	async function createInvestigation(): Promise<void> {
		const path = workspacePath;
		if (!path || investigationBusy) return;
		investigationsOpen = false;
		const request = ++selectionRequest;
		investigationBusy = true;
		chooserError = '';
		try {
			if (!session.newConversation(path)) throw new Error('Could not start a conversation in this workspace.');
			const conversation = session.activeChat;
			const record = await ipc.investigationCreate(path, nextInvestigationTitle(), undefined, undefined, conversation.id);
			if (request !== selectionRequest || workspacePath !== path) return;
			investigations = [record, ...investigations.filter((item) => item.id !== record.id)];
			session.setActiveInvestigation(record.id);
			session.openWorkspaceToolTab('investigation', record.id, conversation.id);
			session.investigationsVersion += 1;
		} catch (cause) {
			if (request === selectionRequest) chooserError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (request === selectionRequest) investigationBusy = false;
		}
	}

	async function activateTab(tab: WorkspaceToolTab): Promise<void> {
		if (tab.kind === 'context') {
			session.activateWorkspaceToolTab(tab.id);
			return;
		}
		const item = investigations.find((record) => record.id === tab.investigationId);
		if (item) {
			await openInvestigation(item, tab.id);
			return;
		}
		session.activateWorkspaceToolTab(tab.id);
		if (tab.conversationId && session.activeChat.id !== tab.conversationId && workspacePath) {
			session.focusWorkspace(workspacePath, tab.conversationId);
		}
	}

	function tabLabel(tab: WorkspaceToolTab): string {
		if (tab.kind === 'context') return 'Data & context';
		return investigations.find((item) => item.id === tab.investigationId)?.title.trim() || 'Investigation';
	}
</script>

<section class="workspace" aria-label="Workspace tools">
	{#if session.workspaceToolTabs.length > 0}
		<div class="workspace-tabs-bar">
			<div class="workspace-tool-tabs" role="tablist" aria-label="Open workspace tools">
				{#each session.workspaceToolTabs as tab (tab.id)}
						<div class="workspace-tool-tab-group" role="presentation">
						<button
							class="workspace-tool-tab"
							class:active={activeTab?.id === tab.id}
							role="tab"
							aria-selected={activeTab?.id === tab.id}
							aria-controls="workspace-tool-content"
							data-testid={`workspace-tool-tab-${tab.id}`}
							title={tabLabel(tab)}
							onclick={() => void activateTab(tab)}
						>
							<Icon name={tab.kind === 'context' ? 'table' : 'bookmark'} size={14} />
							<span>{tabLabel(tab)}</span>
						</button>
						<button
							class="workspace-tool-tab-close"
							aria-label={`Close ${tabLabel(tab)} tab`}
							title={`Close ${tabLabel(tab)}`}
							onclick={() => session.closeWorkspaceToolTab(tab.id)}
						>
							<Icon name="x" size={12} />
						</button>
					</div>
				{/each}
			</div>
			<button
				class="workspace-tool-add"
				type="button"
				aria-label="Add tool tab"
				title="Add a tool"
				data-testid="workspace-tool-add"
				onclick={() => session.openWorkspaceToolChooser()}
			>
				<Icon name="plus" size={16} />
			</button>
		</div>
	{/if}

	{#if session.workspaceToolChooserOpen}
		<div class="workspace-tool-chooser" data-testid="workspace-tool-chooser">
			<div class="workspace-tool-chooser-inner">
				<header class="workspace-tool-chooser-heading">
					<h2>Open a tool</h2>
					<p>Choose what to keep beside this conversation.</p>
				</header>
			<div class="workspace-tool-list" aria-label="Available workspace tools">
				<button
					class="workspace-tool-option workspace-tool-dropdown-trigger"
					type="button"
					data-testid="workspace-tool-investigations"
					aria-expanded={investigationsOpen}
					aria-controls="workspace-investigation-options"
					aria-label={`Investigations${investigations.length ? `, ${investigations.length} available` : ''}`}
					onclick={() => (investigationsOpen = !investigationsOpen)}
				>
					<span class="workspace-tool-option-icon"><Icon name="bookmark" size={16} /></span>
					<span class="workspace-tool-option-copy">
						<strong>Investigations</strong>
					</span>
					{#if investigations.length}
						<span class="workspace-tool-dropdown-count" aria-hidden="true">{investigations.length}</span>
					{/if}
					<Icon name="chevron-right" size={14} />
				</button>
				{#if investigationsOpen}
					<div class="workspace-investigation-options" id="workspace-investigation-options" role="group" aria-label="Choose an Investigation">
						{#if currentInvestigation}
							<button
								class="workspace-tool-option workspace-tool-option-compact"
								type="button"
								data-testid={`workspace-investigation-option-${currentInvestigation.id}`}
								disabled={investigationBusy}
								onclick={() => void openInvestigation(currentInvestigation)}
							>
								<span class="workspace-tool-option-icon"><Icon name="bookmark" size={14} /></span>
								<span class="workspace-tool-option-copy">
									<strong>{currentInvestigation.title || 'Untitled Investigation'}</strong>
									<small>{investigationCoverage(currentInvestigation)}</small>
								</span>
							</button>
						{:else}
							<button
								class="workspace-tool-option workspace-tool-option-compact"
								type="button"
								data-testid="workspace-tool-current-conversation"
								onclick={openCurrentInvestigation}
							>
								<span class="workspace-tool-option-icon"><Icon name="ask" size={14} /></span>
								<span class="workspace-tool-option-copy">
									<strong>Current conversation</strong>
									<small>Open its Investigation workspace</small>
								</span>
							</button>
						{/if}
						{#each otherInvestigations as item (item.id)}
							<button
								class="workspace-tool-option workspace-tool-option-compact"
								type="button"
								data-testid={`workspace-investigation-option-${item.id}`}
								disabled={investigationBusy}
								onclick={() => void openInvestigation(item)}
							>
								<span class="workspace-tool-option-icon"><Icon name="bookmark" size={14} /></span>
								<span class="workspace-tool-option-copy">
									<strong>{item.title || 'Untitled Investigation'}</strong>
									<small>{investigationCoverage(item)}</small>
								</span>
							</button>
						{/each}
						{#if investigationsUnavailable}
							<p class="workspace-tool-list-note" role="status">Investigations are unavailable right now.</p>
						{:else if !investigations.length}
							<p class="workspace-tool-list-note">No saved Investigations yet.</p>
						{/if}
						<button
							class="workspace-tool-option workspace-tool-option-compact workspace-tool-new"
							type="button"
							data-testid="workspace-tool-new-investigation"
							disabled={investigationBusy}
							onclick={() => void createInvestigation()}
						>
							<span class="workspace-tool-option-icon"><Icon name="plus" size={14} /></span>
							<span class="workspace-tool-option-copy"><strong>New Investigation</strong></span>
						</button>
					</div>
				{/if}
				<div class="workspace-tool-group-label workspace-tool-group-secondary">Workspace</div>
				<button
					class="workspace-tool-option"
					type="button"
					data-testid="workspace-tool-context"
					onclick={() => session.openWorkspaceToolTab('context')}
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
			{#if chooserError}<p class="workspace-tool-error" role="alert">{chooserError}</p>{/if}
		</div>
	</div>
	{:else if activeTab}
		<div class="workspace-content" id="workspace-tool-content" role="tabpanel" aria-label={tabLabel(activeTab)}>
			{#if activeTab.kind === 'context'}
				<ContextView />
			{:else}
				<InvestigationBriefView />
			{/if}
		</div>
	{/if}
</section>

<style>
	.workspace {
		display: flex;
		flex: 1;
		flex-direction: column;
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
		padding: clamp(16px, 4vw, 36px);
		overflow-y: auto;
	}
	.workspace-tool-chooser-inner {
		width: min(100%, 440px);
	}
	.workspace-tool-chooser-heading { margin: 0 0 var(--space-4); }
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
	.workspace-tool-list { display: grid; gap: 3px; }
	.workspace-tool-group-label {
		padding: 8px 10px 4px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 550;
	}
	.workspace-tool-group-secondary { margin-top: var(--space-3); }
	.workspace-tool-option {
		display: flex;
		width: 100%;
		min-width: 0;
		min-height: 52px;
		align-items: center;
		gap: var(--space-3);
		padding: 8px 10px;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--text);
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.workspace-tool-option:hover:not(:disabled) { background: var(--bg-inset); }
	.workspace-tool-option:focus-visible { outline: none; box-shadow: var(--focus-ring); }
	.workspace-tool-option > :global(svg.icon) { margin-left: auto; color: var(--text-faint); }
	.workspace-tool-dropdown-trigger {
		min-height: 34px;
		gap: 8px;
		padding: 0 10px;
		border: 1px solid var(--pane-edge);
		border-radius: 999px;
		background: color-mix(in srgb, var(--text) 3%, var(--sidebar-surface));
		color: var(--text-dim);
		transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.workspace-tool-dropdown-trigger:hover:not(:disabled),
	.workspace-tool-dropdown-trigger[aria-expanded='true'] {
		border-color: var(--border-strong);
		background: var(--sidebar-hover);
		color: var(--text);
	}
	.workspace-tool-dropdown-trigger:focus-visible {
		outline: 2px solid var(--brand);
		outline-offset: 2px;
		box-shadow: none;
	}
	.workspace-tool-dropdown-trigger .workspace-tool-option-icon {
		width: 16px;
		height: 16px;
		border-radius: 0;
		background: transparent;
		color: inherit;
	}
	.workspace-tool-dropdown-trigger .workspace-tool-option-copy {
		display: block;
		flex: 1;
	}
	.workspace-tool-dropdown-trigger > :global(svg.icon:last-child) {
		flex: none;
		margin-left: 0;
		color: var(--text-faint);
		transform: rotate(90deg);
		transition: transform var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.workspace-tool-dropdown-trigger[aria-expanded='true'] > :global(svg.icon:last-child) {
		color: var(--text-dim);
		transform: rotate(-90deg);
	}
	.workspace-tool-dropdown-count {
		flex: none;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-variant-numeric: tabular-nums;
	}
	.workspace-tool-option-icon {
		display: grid;
		width: 30px;
		height: 30px;
		flex: none;
		place-items: center;
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.workspace-tool-option-copy { display: grid; min-width: 0; gap: 2px; }
	.workspace-tool-option-copy strong {
		overflow: hidden;
		font-size: var(--fs-sm);
		font-weight: 550;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.workspace-tool-option-copy small { color: var(--text-faint); font-size: var(--fs-xs); }
	.workspace-tool-option-compact { min-height: 42px; padding-left: 14px; }
	.workspace-tool-option-compact .workspace-tool-option-icon { width: 26px; height: 26px; }
	.workspace-investigation-options {
		display: grid;
		gap: 2px;
		margin: 2px 0 8px 12px;
		padding: 4px 0 4px 8px;
		border-left: 1px solid var(--border);
	}
	.workspace-tool-option-compact .workspace-tool-option-icon { background: transparent; }
	.workspace-tool-new { color: var(--text-dim); }
	.workspace-tool-option:disabled { color: var(--text-faint); cursor: not-allowed; }
	.workspace-tool-list-note,
	.workspace-tool-error { margin: 6px 10px; color: var(--text-faint); font-size: var(--fs-xs); }
	.workspace-tool-error { color: var(--danger, #b42318); }
	.workspace-tool-option-unavailable { min-height: 44px; }
	.workspace-terminal-icon { font-family: var(--mono); font-size: 15px; font-weight: 600; }
	.workspace-tabs-bar {
		display: flex;
		flex: none;
		align-items: center;
		gap: 6px;
		min-width: 0;
		min-height: 46px;
		padding: 6px 10px;
		border-bottom: 1px solid var(--border);
	}
	.workspace-tool-tabs {
		display: flex;
		flex: 1;
		align-items: center;
		gap: 4px;
		min-width: 0;
		overflow-x: auto;
		scrollbar-width: thin;
	}
	.workspace-tool-tab-group {
		display: flex;
		flex: none;
		align-items: center;
		gap: 1px;
		padding: 2px;
		border-radius: 999px;
		background: transparent;
	}
	.workspace-tool-tab-group:has(.workspace-tool-tab.active) { background: var(--bg-inset); }
	.workspace-tool-tab,
	.workspace-tool-tab-close,
	.workspace-tool-add {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border: 0;
		background: transparent;
		color: var(--text-dim);
		cursor: pointer;
	}
	.workspace-tool-tab {
		gap: 7px;
		max-width: min(240px, 32vw);
		min-height: 30px;
		padding: 0 9px;
		border-radius: 999px;
		font: inherit;
		font-size: var(--fs-xs);
	}
	.workspace-tool-tab span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.workspace-tool-tab.active { color: var(--text); font-weight: 550; }
	.workspace-tool-tab :global(svg) { flex: none; color: var(--text-faint); }
	.workspace-tool-tab:hover,
	.workspace-tool-tab-close:hover,
	.workspace-tool-add:hover { background: var(--sidebar-hover); color: var(--text); }
	.workspace-tool-tab-close,
	.workspace-tool-add { width: 28px; height: 28px; flex: none; border-radius: 999px; }
	.workspace-tool-tab-close :global(svg) { color: currentColor; }
	.workspace-tool-tab:focus-visible,
	.workspace-tool-tab-close:focus-visible,
	.workspace-tool-add:focus-visible { outline: none; box-shadow: var(--focus-ring); }
	.workspace-content {
		display: flex;
		flex: 1;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
	}
</style>
