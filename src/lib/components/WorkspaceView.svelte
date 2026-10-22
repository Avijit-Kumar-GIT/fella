<script lang="ts">
	import { tick, untrack } from 'svelte';
	import { ipc } from '$lib/ipc';
	import { requestArtifactChange } from '$lib/commands';
	import { focusInvestigationConversation } from '$lib/investigation-navigation';
	import { session, type WorkspaceToolTab } from '$lib/session.svelte';
	import type { Investigation } from '$lib/types';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import { Dialog } from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import Icon from './Icon.svelte';
	import ContextView from './ContextView.svelte';
	import NotebookInvestigation from './NotebookInvestigation.svelte';
	import ResultDocument from './ResultDocument.svelte';
	import TerminalToolView from './TerminalToolView.svelte';
	import WorkspaceSourceView from './WorkspaceSourceView.svelte';
	import CompanionPane from './CompanionPane.svelte';
	let { onexpand, notebook = false }: { onexpand?: () => void; notebook?: boolean } = $props();

	let workspacePath = $derived(session.catalog.workspace);
	let investigations = $state<Investigation[]>([]);
	let investigationsUnavailable = $state(false);
	let investigationBusy = $state(false);
	let investigationsOpen = $state(false);
	let chooserError = $state('');
	let createOpen = $state(false);
	let newName = $state('');
	let listRequest = 0;
	let selectionRequest = 0;
	let tabsElement = $state<HTMLDivElement>();
	const restoringTabKeys = new Set<string>();
	let activeTab = $derived(session.activeWorkspaceToolTab);
	let terminalTabs = $derived(session.workspaceToolTabs.filter((tab) => tab.kind === 'terminal'));
	$effect(() => {
		const records = investigations, path = workspacePath;
		const tabIdentity = session.workspaceToolTabs.map(tab => [tab.id, tab.conversationId, tab.investigationId].join(':')).join('|');
		void tabIdentity;
		if (!path || records.some(record => record.workspace_id !== path)) return;
		// A layout can open an unbound tab after the record list has loaded.
		// Reconcile on tab identity too, not only after an IPC list response.
		untrack(() => session.reconcileWorkspaceToolTabs(records));
	});
	$effect(() => {
		const id = activeTab?.id;
		const count = session.workspaceToolTabs.length;
		void count;
		if (!id || !tabsElement || !session.workspacePaneOpen) return;
		void tick().then(() => {
			const selected = tabsElement?.querySelector<HTMLElement>('[aria-selected="true"]');
			if (!selected || !tabsElement) return;
			const rect = selected.getBoundingClientRect(), viewport = tabsElement.getBoundingClientRect();
			if (rect.left < viewport.left) tabsElement.scrollLeft += rect.left - viewport.left - 4;
			else if (rect.right > viewport.right) tabsElement.scrollLeft += rect.right - viewport.right + 32;
		});
	});
	let currentInvestigation = $derived.by(() => {
		if (activeTab?.kind === 'investigation' && activeTab.investigationId) {
			return investigations.find((item) => item.id === activeTab.investigationId) ?? null;
		}
		const linkedToChat = investigations.find((item) => item.conversation_id === session.activeChat.id);
		if (linkedToChat) return linkedToChat;
		if (activeTab?.kind === 'investigation' && activeTab.conversationId === session.activeChat.id && activeTab.investigationId) {
			return investigations.find((item) => item.id === activeTab.investigationId) ?? null;
		}
		return null;
	});
	let activeSource = $derived.by(() => {
		if (activeTab?.kind !== 'source' || !activeTab.sourcePath || !activeTab.sourceName) return null;
		return session.catalog.sources.find((item) => item.path === activeTab.sourcePath && item.name === activeTab.sourceName) ?? null;
	});

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
			const workspaceInvestigations = records.filter((item) => item.workspace_id === path);
			session.rememberInvestigationNames(workspaceInvestigations);
			session.reconcileWorkspaceToolTabs(workspaceInvestigations);
			investigations = workspaceInvestigations
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
		// Explicit picker navigation owns the transition. Restoring the outgoing
		// tab while its replacement chat loads would steal focus back and prevent
		// the requested Investigation from acquiring its own tab.
		if (investigationBusy || !path || !paneOpen || !tab || tab.kind !== 'investigation' || !tab.investigationId) return;
		const item = records.find((record) => record.id === tab.investigationId);
		if (!item || (session.activeChat.id === tab.conversationId && session.activeInvestigationId === item.id)) return;
		const key = `${path}:${tab.id}`;
		if (restoringTabKeys.has(key)) return;
		restoringTabKeys.add(key);
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
			restoringTabKeys.delete(key);
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

	function setInvestigationsOpen(open: boolean): void {
		investigationsOpen = open;
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
			const record = await ipc.investigationCreate(path, newName.trim(), undefined, undefined, conversation.id);
			if (request !== selectionRequest || workspacePath !== path) return;
			investigations = [record, ...investigations.filter((item) => item.id !== record.id)];
			session.rememberInvestigationNames([record]);
			session.setActiveInvestigation(record.id);
			session.openWorkspaceToolTab('investigation', record.id, conversation.id);
			session.investigationsVersion += 1;
			createOpen = false;
		} catch (cause) {
			if (request === selectionRequest) chooserError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (request === selectionRequest) investigationBusy = false;
		}
	}

	function activateTab(tab: WorkspaceToolTab): void {
		// Tab selection is synchronous navigation. A pending chooser restore must
		// not lock out later clicks or steal focus when it finishes.
		selectionRequest += 1;
		investigationBusy = false;
		chooserError = '';
		session.activateWorkspaceToolTab(tab.id);
		if (
			tab.conversationId &&
			session.activeChat.id !== tab.conversationId &&
			workspacePath &&
			session.conversations.some((conversation) =>
				conversation.id === tab.conversationId && conversation.workspaceScope === workspacePath
			)
		) {
			session.focusWorkspace(workspacePath, tab.conversationId);
		}
	}

	function baseTabLabel(tab: WorkspaceToolTab): string {
		if (tab.kind === 'result') return tab.title || 'Result';
		if (tab.kind === 'studio') return tab.title || 'Report';
		if (tab.kind === 'context') return 'Workspace context';
		if (tab.kind === 'source') return tab.sourceName?.split(/[\\/]/).at(-1) || 'File preview';
		if (tab.kind === 'terminal') return 'Shell';
		if (tab.kind === 'preview') {
			const preview = tab.preview;
			if (preview?.kind === 'source') return preview.name;
			if (preview?.kind === 'chart') return session.conversations.find((item) => item.id === tab.conversationId)?.messages.find((item) => item.id === preview.messageId)?.answer?.evidence[preview.evidenceIndex]?.chart?.title || 'Chart';
			return 'Preview';
		}
		return investigations.find((item) => item.id === tab.investigationId)?.title.trim() || 'Investigation';
	}

	function tabLabel(tab: WorkspaceToolTab): string {
		const label = baseTabLabel(tab);
		const matchingTabs = session.workspaceToolTabs.filter((item) => baseTabLabel(item) === label);
		if (matchingTabs.length < 2) return label;
		return `${label} ${matchingTabs.findIndex((item) => item.id === tab.id) + 1}`;
	}
	function tabScope(tab: WorkspaceToolTab): string {
		if (tab.kind === 'studio') return 'Saved analytical document';
		if (tab.kind === 'context') return 'Shared workspace guide and knowledge';
		if (tab.kind === 'terminal') return 'Independent shell session';
		if (tab.kind === 'source') return tab.sourcePath ?? 'Read-only file preview';
		if (tab.kind === 'preview') return 'Saved answer artifact';
		return 'Investigation and linked conversation';
	}
</script>

<section class="workspace" aria-label="Workspace tools">
	{#if session.workspaceToolTabs.length > 0}
		<div class="workspace-tabs-bar">
			<div bind:this={tabsElement} class="workspace-tool-tabs" role="tablist" aria-label="Open workspace tools">
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
							{#if tab.kind === 'context'}
								<Icon name="table" size={14} />
							{:else if tab.kind === 'source' || tab.kind === 'studio' || tab.kind === 'result'}
								<Icon name="file" size={14} />
							{:else if tab.kind === 'terminal'}
								<span class="workspace-terminal-tab-icon" aria-hidden="true">›_</span>
							{:else}
								<Icon name="bookmark" size={14} />
							{/if}
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
			<DropdownMenu.Root>
				<DropdownMenu.Trigger class="workspace-tool-overview" aria-label="Open tabs" title="Open tabs"><Icon name="chevron-down" size={14} /></DropdownMenu.Trigger>
				<DropdownMenu.Portal>
					<DropdownMenu.Content class="fella-ui-menu-content tool-overview-menu" align="end" sideOffset={6}>
						{#each session.workspaceToolTabs as tab (tab.id)}
							<DropdownMenu.Item class="fella-ui-menu-item tool-overview-item" onSelect={() => activateTab(tab)}>
								<span><strong>{tabLabel(tab)}</strong><small>{tabScope(tab)}</small></span>
								{#if activeTab?.id === tab.id}<Icon name="check" size={14} />{/if}
							</DropdownMenu.Item>
						{/each}
					</DropdownMenu.Content>
				</DropdownMenu.Portal>
			</DropdownMenu.Root>
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

	{#each terminalTabs as tab (tab.id)}
		{@const isActive = activeTab?.id === tab.id && !session.workspaceToolChooserOpen}
		<div
			class="workspace-content terminal-session-pane"
			hidden={!isActive}
			aria-hidden={!isActive}
			id={isActive ? 'workspace-tool-content' : undefined}
			role={isActive ? 'tabpanel' : undefined}
			aria-label={isActive ? tabLabel(tab) : undefined}
			data-terminal-tool-tab={tab.id}
		>
			{#key workspacePath}
				<TerminalToolView active={isActive} tabId={tab.id} />
			{/key}
		</div>
	{/each}

	{#if investigationBusy && !createOpen}
		<div class="workspace-opening" role="status">Opening Investigation…</div>
	{:else if session.workspaceToolChooserOpen}
		<div class="workspace-tool-chooser" data-testid="workspace-tool-chooser">
			<div class="workspace-tool-chooser-inner">
				<header class="workspace-tool-chooser-heading">
					<h2>{notebook ? 'Open a document' : 'Open a tool'}</h2>
					<p>Choose an Investigation, report, context, or Shell.</p>
				</header>
			<div class="workspace-tool-list" aria-label="Available workspace tools">
				<DropdownMenu.Root open={investigationsOpen} onOpenChange={setInvestigationsOpen}>
					<DropdownMenu.Trigger
						class="workspace-tool-option workspace-tool-dropdown-trigger"
						type="button"
						data-testid="workspace-tool-investigations"
						aria-label={`Investigations${investigations.length ? `, ${investigations.length} available` : ''}`}
						aria-controls="workspace-investigations-menu"
					>
						<span class="workspace-tool-option-icon"><Icon name="bookmark" size={16} /></span>
						<span class="workspace-tool-option-copy"><strong>Investigations</strong></span>
						{#if investigations.length}
							<span class="workspace-tool-dropdown-count" aria-hidden="true">{investigations.length}</span>
						{/if}
						<Icon name="chevron-right" size={14} />
					</DropdownMenu.Trigger>
					<DropdownMenu.Portal>
						<DropdownMenu.Content
							id="workspace-investigations-menu"
							class="fella-ui-menu-content workspace-investigations-menu"
							side="bottom"
							align="start"
							sideOffset={6}
						>
							{#if !currentInvestigation}
								<DropdownMenu.Item
									class="fella-ui-menu-item workspace-investigation-menu-item"
									data-testid="workspace-tool-current-conversation"
									onSelect={openCurrentInvestigation}
								>
									<Icon name="ask" size={14} />
									<span class="workspace-investigation-menu-copy">
										<strong>Current conversation</strong>
										<small>Open its Investigation workspace</small>
									</span>
								</DropdownMenu.Item>
							{/if}
							{#if investigations.length}
								{#if !currentInvestigation}<div class="workspace-investigation-menu-separator" role="separator"></div>{/if}
								<DropdownMenu.RadioGroup
									value={currentInvestigation?.id ?? ''}
									onValueChange={(id) => {
										const item = investigations.find((record) => record.id === id);
										if (item) void openInvestigation(item);
									}}
								>
									{#each investigations as item (item.id)}
										<DropdownMenu.RadioItem
											class="fella-ui-menu-item workspace-investigation-menu-item"
											value={item.id}
											data-testid={`workspace-investigation-option-${item.id}`}
											disabled={investigationBusy}
										>
											<Icon name="bookmark" size={14} />
											<span class="workspace-investigation-menu-copy">
												<strong>{item.title || 'Untitled Investigation'}</strong>
												<small>{investigationCoverage(item)}</small>
											</span>
											{#if item.id === currentInvestigation?.id}<Icon name="check" size={14} />{/if}
										</DropdownMenu.RadioItem>
									{/each}
								</DropdownMenu.RadioGroup>
							{:else if investigationsUnavailable}
								<div class="workspace-investigation-menu-note" role="status">Investigations are unavailable right now.</div>
							{:else}
								<div class="workspace-investigation-menu-note">No saved Investigations yet.</div>
							{/if}
							<div class="workspace-investigation-menu-separator" role="separator"></div>
							<DropdownMenu.Item
								class="fella-ui-menu-item workspace-investigation-menu-item workspace-investigation-menu-new"
								data-testid="workspace-tool-new-investigation"
								disabled={investigationBusy}
								onSelect={() => { newName = nextInvestigationTitle(); chooserError = ''; createOpen = true; }}
							>
								<Icon name="plus" size={14} />
								<strong>New Investigation</strong>
							</DropdownMenu.Item>
						</DropdownMenu.Content>
					</DropdownMenu.Portal>
				</DropdownMenu.Root>
				<div class="workspace-tool-group-label workspace-tool-group-secondary">Workspace</div>
				<button class="workspace-tool-option" type="button" data-testid="workspace-tool-studio" onclick={() => session.openStudioToolTab()}>
					<span class="workspace-tool-option-icon"><Icon name="file" size={16} /></span>
					<span class="workspace-tool-option-copy"><strong>Reports</strong><small>Arrange results and narrative</small></span>
					<Icon name="chevron-right" size={16} />
				</button>
				<button
					class="workspace-tool-option"
					type="button"
					data-testid="workspace-tool-context"
					onclick={() => session.openWorkspaceToolTab('context', null, null, true)}
				>
					<span class="workspace-tool-option-icon"><Icon name="table" size={16} /></span>
					<span class="workspace-tool-option-copy">
						<strong>Workspace context</strong>
								<small>Guidance and reusable knowledge</small>
					</span>
					<Icon name="chevron-right" size={16} />
				</button>
				<button
					class="workspace-tool-option"
					type="button"
					data-testid="workspace-tool-terminal"
					onclick={() => session.openWorkspaceToolTab('terminal', null, null, true)}
				>
					<span class="workspace-tool-option-icon workspace-terminal-icon" aria-hidden="true">›_</span>
					<span class="workspace-tool-option-copy">
						<strong>Shell</strong>
						<small>Interactive shell at this workspace</small>
					</span>
					<Icon name="chevron-right" size={16} />
				</button>
			</div>
			{#if chooserError}<p class="workspace-tool-error" role="alert">{chooserError}</p>{/if}
		</div>
	</div>
	{:else if activeTab && activeTab.kind !== 'terminal'}
		<div class="workspace-content" id="workspace-tool-content" role="tabpanel" aria-label={tabLabel(activeTab)}>
			{#if activeTab.kind === 'result' && workspacePath && activeTab.artifactId && activeTab.artifactRevision}
				<ResultDocument workspace={workspacePath} artifactId={activeTab.artifactId} revision={activeTab.artifactRevision} />
			{:else if activeTab.kind === 'studio' && workspacePath}
				{@const ownerTab = activeTab}
				{#key `${workspacePath}:${ownerTab.id}`}
					{#await import('./StudioView.svelte')}
						<p role="status">Opening report…</p>
					{:then module}
						<module.default workspace={workspacePath} documentId={ownerTab.documentId} newReport={ownerTab.newReport} initialArtifactId={ownerTab.initialArtifactId} onopen={(id, title) => session.setStudioTabDocument(ownerTab.id, id, title)} onrequestchange={async (artifact, instruction) => {
							await requestArtifactChange(artifact, instruction);
						}} />
					{:catch error}
						<p role="alert">Could not open report: {String(error)}</p>
					{/await}
				{/key}
			{:else if activeTab.kind === 'preview' && activeTab.preview}
				<CompanionPane reference={activeTab.preview} conversationId={activeTab.conversationId} embedded onclose={() => activeTab && session.closeWorkspaceToolTab(activeTab.id)} />
			{:else if activeTab.kind === 'source'}
				<WorkspaceSourceView source={activeSource} workspacePath={workspacePath} revision={session.catalog.revision ?? null} {onexpand} />
			{:else if activeTab.kind === 'context'}
				<ContextView />
			{:else}
				{#key `${workspacePath}:${activeTab.id}`}
					<NotebookInvestigation workspace={workspacePath!} investigationId={activeTab.investigationId ?? undefined} conversationId={activeTab.conversationId ?? undefined} onopenconversation={() => session.setContentLayout('chat')} />
				{/key}
			{/if}
		</div>
	{/if}
	<Dialog.Root bind:open={createOpen}>
		<Dialog.Portal>
			<Dialog.Overlay class="fella-ui-dialog-overlay" />
			<Dialog.Content class="fella-ui-dialog-content investigation-name-dialog">
				<Dialog.Title>New Investigation</Dialog.Title>
				<Dialog.Description>Name the analysis. It will have its own conversation in this workspace.</Dialog.Description>
				<form onsubmit={(event) => { event.preventDefault(); if (newName.trim()) void createInvestigation(); }}>
					<label for="new-investigation-name">Investigation name</label>
					<Input id="new-investigation-name" bind:value={newName} maxlength={120} disabled={investigationBusy} />
					{#if chooserError}<p role="alert">{chooserError}</p>{/if}
					<div class="name-dialog-actions"><Dialog.Close class="name-cancel" disabled={investigationBusy}>Cancel</Dialog.Close><Button type="submit" disabled={investigationBusy || !newName.trim()}>{investigationBusy ? 'Creating…' : 'Create Investigation'}</Button></div>
				</form>
			</Dialog.Content>
		</Dialog.Portal>
	</Dialog.Root>
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
	.workspace-opening { display: grid; flex: 1; place-content: center; color: var(--text-faint); font-size: var(--fs-sm); }
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
	:global(.workspace-tool-dropdown-trigger) {
		display: flex;
		width: 100%;
		min-width: 0;
		align-items: center;
		min-height: 34px;
		gap: 8px;
		padding: 0 10px;
		border: 1px solid var(--pane-edge);
		border-radius: 999px;
		background: color-mix(in srgb, var(--text) 3%, var(--sidebar-surface));
		color: var(--text-dim);
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	:global(.workspace-tool-dropdown-trigger:hover:not(:disabled)),
	:global(.workspace-tool-dropdown-trigger[data-state='open']) {
		border-color: var(--border-strong);
		background: var(--sidebar-hover);
		color: var(--text);
	}
	:global(.workspace-tool-dropdown-trigger:focus-visible) {
		outline: 2px solid var(--brand);
		outline-offset: 2px;
		box-shadow: none;
	}
	:global(.workspace-tool-dropdown-trigger .workspace-tool-option-icon) {
		width: 16px;
		height: 16px;
		border-radius: 0;
		background: transparent;
		color: inherit;
	}
	:global(.workspace-tool-dropdown-trigger .workspace-tool-option-copy) {
		display: block;
		flex: 1;
	}
	:global(.workspace-tool-dropdown-trigger > svg.icon:last-child) {
		flex: none;
		margin-left: 0;
		color: var(--text-faint);
		transform: rotate(90deg);
		transition: transform var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	:global(.workspace-tool-dropdown-trigger[aria-expanded='true'] > svg.icon:last-child) {
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
	:global(.workspace-investigations-menu) {
		width: min(320px, calc(100vw - 24px));
		max-height: min(420px, calc(100vh - 24px));
		overflow-y: auto;
	}
	:global(.workspace-investigation-menu-item) {
		min-height: 44px;
		gap: 9px;
		padding: 7px 9px;
	}
	:global(.workspace-investigation-menu-item > svg:first-child) {
		flex: none;
		color: var(--text-faint);
	}
	:global(.workspace-investigation-menu-item > svg:last-child) {
		flex: none;
		margin-left: auto;
		color: var(--text-dim);
	}
	:global(.workspace-investigation-menu-copy) {
		display: grid;
		flex: 1;
		min-width: 0;
		gap: 2px;
	}
	:global(.workspace-investigation-menu-copy > strong) {
		overflow: hidden;
		color: var(--text);
		font-size: var(--fs-sm);
		font-weight: 550;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	:global(.workspace-investigation-menu-copy > small) {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	:global(.workspace-investigation-menu-note) {
		padding: 8px 9px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	:global(.workspace-investigation-menu-separator) {
		height: 1px;
		margin: 4px 5px;
		background: var(--border);
	}
	:global(.workspace-investigation-menu-new) {
		color: var(--text-dim);
	}
	.workspace-tool-option:disabled { color: var(--text-faint); cursor: not-allowed; }
	.workspace-tool-error { margin: 6px 10px; color: var(--text-faint); font-size: var(--fs-xs); }
	.workspace-tool-error { color: var(--danger, #b42318); }
	:global(.workspace-tool-overview) { display: inline-flex; align-items: center; justify-content: center; flex: none; width: 28px; height: 28px; border: 0; border-radius: 999px; background: transparent; color: var(--text-dim); cursor: pointer; }
	:global(.workspace-tool-overview:hover) { background: var(--sidebar-hover); }
	:global(.tool-overview-menu) { width: min(360px, calc(100vw - 32px)); max-height: min(480px, 65vh); overflow-y: auto; }
	:global(.tool-overview-item) { display: flex; justify-content: space-between; gap: 12px; }
	:global(.tool-overview-item > span) { display: grid; min-width: 0; gap: 3px; }
	:global(.tool-overview-item strong) { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
	:global(.tool-overview-item small) { color: var(--text-faint); font-size: var(--fs-xs); }
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
	.workspace-terminal-tab-icon { color: var(--text-faint); font-family: var(--mono); font-size: 11px; font-weight: 650; }
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
	.workspace-content[hidden] { display: none; }
</style>
