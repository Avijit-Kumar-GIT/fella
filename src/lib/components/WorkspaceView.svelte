<script lang="ts">
	import { untrack } from 'svelte';
	import { ipc } from '$lib/ipc';
	import { APP_WORKSPACE_ID } from '$lib/analytics-artifacts';
	import { requestArtifactChange } from '$lib/commands';
	import { session, type WorkspaceTool } from '$lib/session.svelte';
	import type { Investigation, SourceInfo } from '$lib/types';
	import { Dialog } from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import Icon from './Icon.svelte';
	import ContextView from './ContextView.svelte';
	import InvestigationBriefView from './InvestigationBriefView.svelte';
	import ResultDocument from './ResultDocument.svelte';
	import WorkspaceSourceView from './WorkspaceSourceView.svelte';
	import CompanionPane from './CompanionPane.svelte';

	let { onexpand, onasksource, defaultInspector = false, hideToolBar = false, expanded = false }: {
		onexpand?: () => void;
		onasksource?: (source: SourceInfo) => void;
		defaultInspector?: boolean;
		hideToolBar?: boolean;
		expanded?: boolean;
	} = $props();

	let workspacePath = $derived(session.catalog.workspace);
	let investigations = $state<Investigation[]>([]);
	let investigationsLoaded = $state(false);
	let investigationsUnavailable = $state(false);
	let investigationBusy = $state(false);
	let listedWorkspacePath: string | null = null;
	let investigationError = $state('');
	let createOpen = $state(false);
	let newName = $state('');
	let listRequest = 0;
	let selectionRequest = 0;
	let activeTool = $derived(session.activeWorkspaceTool);

	$effect(() => {
		if (!defaultInspector || !workspacePath || !session.newInvestigationRequested) return;
		untrack(() => {
			session.newInvestigationRequested = false;
			newName = nextInvestigationTitle();
			investigationError = '';
			createOpen = true;
		});
	});

	let currentInvestigation = $derived.by(() =>
		investigations.find((item) => item.workspace_id === workspacePath && item.conversation_id === session.activeChat.id) ?? null
	);
	let activeSource = $derived.by(() => {
		if (activeTool?.kind !== 'source' || !activeTool.sourcePath || !activeTool.sourceName) return null;
		return session.catalog.sources.find((item) => item.path === activeTool.sourcePath && item.name === activeTool.sourceName) ?? null;
	});

	$effect(() => {
		const path = workspacePath;
		const version = session.investigationsVersion;
		void version;
		if (!path) {
			listRequest += 1;
			listedWorkspacePath = null;
			investigations = [];
			investigationsUnavailable = false;
			investigationsLoaded = true;
			return;
		}
		const workspaceChanged = path !== listedWorkspacePath;
		const alreadyLoaded = !workspaceChanged && investigationsLoaded;
		if (workspaceChanged) {
			listedWorkspacePath = path;
			investigations = [];
			investigationsLoaded = false;
			investigationsUnavailable = false;
		}
		const request = ++listRequest;
		const activeConversationIds = session.conversations.map((conversation) => conversation.id);
		void ipc.investigationsList(activeConversationIds).then((records) => {
			if (request !== listRequest || path !== workspacePath) return;
			const scoped = records
				.filter((item) => item.workspace_id === path)
				.sort((left, right) => right.updated_at_ms - left.updated_at_ms);
			session.rememberInvestigationNames(scoped);
			investigations = scoped;
			investigationsUnavailable = false;
			investigationsLoaded = true;
		}).catch(() => {
			if (request !== listRequest) return;
			if (!alreadyLoaded) {
				investigations = [];
				investigationsUnavailable = true;
				investigationsLoaded = true;
			}
		});
		return () => {
			if (request === listRequest) listRequest += 1;
		};
	});

	function nextInvestigationTitle(): string {
		const titles = new Set(investigations.map((item) => item.title.trim().toLocaleLowerCase()));
		if (!titles.has('new investigation')) return 'New investigation';
		let suffix = 2;
		while (titles.has(`new investigation ${suffix}`)) suffix += 1;
		return `New investigation ${suffix}`;
	}

	function toolTitle(tool: WorkspaceTool): string {
		if (tool.kind === 'result') return tool.title || 'Analysis result';
		if (tool.kind === 'studio') return tool.title || 'Report';
		if (tool.kind === 'context') return 'Workspace context';
		if (tool.kind === 'source') return tool.sourceName?.split(/[\\/]/).at(-1) || 'File preview';
		if (tool.preview?.kind === 'source') return tool.preview.name;
		const preview = tool.preview;
		if (preview?.kind === 'chart') {
			return session.conversations
				.find((item) => item.id === tool.conversationId)
				?.messages.find((item) => item.id === preview.messageId)
				?.answer?.evidence[preview.evidenceIndex]?.chart?.title || 'Chart';
		}
		return 'Answer preview';
	}

	async function createInvestigation(): Promise<void> {
		const path = workspacePath;
		if (!path || investigationBusy) return;
		const request = ++selectionRequest;
		investigationBusy = true;
		investigationError = '';
		try {
			if (!session.newConversation(path)) throw new Error('Could not start a conversation in this workspace.');
			const conversation = session.activeChat;
			const record = await ipc.investigationCreate(path, newName.trim(), undefined, undefined, conversation.id);
			if (request !== selectionRequest || workspacePath !== path) return;
			investigations = [record, ...investigations.filter((item) => item.id !== record.id)];
			session.rememberInvestigationNames([record]);
			session.setActiveInvestigation(record.id);
			session.setWorkspacePane('investigation');
			session.investigationsVersion += 1;
			createOpen = false;
		} catch (cause) {
			if (request === selectionRequest) investigationError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (request === selectionRequest) investigationBusy = false;
		}
	}
</script>

<section class="workspace" aria-label="Workspace details">
	{#if investigationBusy && !createOpen}
		<div class="workspace-opening" role="status">Creating Investigation…</div>
	{:else if activeTool}
		<div class="workspace-content" id="workspace-tool-content" aria-label={toolTitle(activeTool)}>
			{#if defaultInspector && !hideToolBar}
				<header class="workspace-tool-heading">
					<span class="workspace-tool-title" role="heading" aria-level="1" title={activeTool.kind === 'studio' ? 'Reports' : toolTitle(activeTool)}>{activeTool.kind === 'studio' ? 'Reports' : toolTitle(activeTool)}</span>
					<Button variant="ghost" size="sm" onclick={() => session.closeWorkspaceTool()}>
						<Icon name="chevron-right" size={14} /> Investigation
					</Button>
				</header>
			{/if}
			{#if activeTool.kind === 'result' && workspacePath && activeTool.artifactId && activeTool.artifactRevision}
				<ResultDocument workspace={workspacePath} artifactId={activeTool.artifactId} revision={activeTool.artifactRevision} />
			{:else if activeTool.kind === 'studio'}
				{@const ownerTool = activeTool}
				{#key `${APP_WORKSPACE_ID}:${ownerTool.id}`}
					{#await import('./StudioView.svelte')}
						<p role="status">Opening report…</p>
					{:then module}
						<module.default
							workspace={APP_WORKSPACE_ID}
							documentId={ownerTool.documentId}
							newReport={ownerTool.newReport}
							embedded={defaultInspector}
							initialArtifactId={ownerTool.initialArtifactId}
							onopen={(id, title) => session.setStudioToolDocument(id, title)}
							onrequestchange={async (artifact, instruction) => { await requestArtifactChange(artifact, instruction); }}
						/>
					{:catch error}
						<p role="alert">Could not open report: {String(error)}</p>
					{/await}
				{/key}
			{:else if activeTool.kind === 'preview' && activeTool.preview}
				<CompanionPane reference={activeTool.preview} conversationId={activeTool.conversationId} embedded onclose={() => session.closeWorkspaceTool()} />
			{:else if activeTool.kind === 'source'}
				<WorkspaceSourceView
					source={activeSource}
					workspacePath={workspacePath}
					revision={session.catalog.revision ?? null}
					{onexpand}
					onclose={() => session.closeWorkspaceTool()}
					onask={(source) => onasksource?.(source)}
				/>
			{:else if activeTool.kind === 'context'}
				<ContextView />
			{/if}
		</div>
	{:else if defaultInspector && currentInvestigation}
		<div class="workspace-content investigation-content" id="workspace-tool-content">
			{#key `${workspacePath}:${currentInvestigation.id}`}
				<InvestigationBriefView
					viewKey={`workspace:${workspacePath}:${currentInvestigation.id}`}
					initialInvestigation={currentInvestigation}
					{onexpand}
					{expanded}
					onclose={() => session.setWorkspacePaneOpen(false)}
				/>
			{/key}
		</div>
	{:else if defaultInspector && !investigationsLoaded}
		<div class="workspace-opening" role="status">Loading Investigation…</div>
	{:else if defaultInspector}
		<div class="workspace-investigation-empty">
			<h2>This conversation has no Investigation</h2>
			<p>Start one when you’re ready. It opens its own conversation and keeps the analysis organized here.</p>
			<Button size="sm" onclick={() => { newName = nextInvestigationTitle(); investigationError = ''; createOpen = true; }}>New Investigation</Button>
			{#if investigationsUnavailable}<p role="status">Investigations are temporarily unavailable.</p>{/if}
		</div>
	{:else}
		<div class="workspace-investigation-empty"><h2>Select a report</h2><p>Choose a report from the sidebar to open it here.</p></div>
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
					{#if investigationError}<p role="alert">{investigationError}</p>{/if}
					<div class="name-dialog-actions">
						<Dialog.Close class="name-cancel" disabled={investigationBusy}>Cancel</Dialog.Close>
						<Button type="submit" disabled={investigationBusy || !newName.trim()}>{investigationBusy ? 'Creating…' : 'Create Investigation'}</Button>
					</div>
				</form>
			</Dialog.Content>
		</Dialog.Portal>
	</Dialog.Root>
</section>

<style>
	.workspace { display:flex; flex:1; flex-direction:column; min-width:0; min-height:0; background:var(--workspace-canvas); }
	.workspace-opening { display:grid; flex:1; place-content:center; color:var(--text-faint); font-size:var(--fs-sm); }
	.workspace-content { display:flex; flex:1; flex-direction:column; min-width:0; min-height:0; overflow:hidden; }
	.workspace-tool-heading { display:flex; flex:none; align-items:center; justify-content:space-between; gap:8px; min-width:0; min-height:42px; padding:5px 10px; border-bottom:1px solid var(--border); }
	.workspace-tool-title { overflow:hidden; color:var(--text-dim); font-size:var(--fs-xs); text-overflow:ellipsis; white-space:nowrap; }
	.workspace-tool-heading :global(svg.icon) { transform: rotate(180deg); }
	.workspace-investigation-empty { display:grid; flex:1; align-content:center; justify-items:start; gap:10px; width:min(100%, 420px); margin-inline:auto; padding:clamp(20px, 5vw, 40px); }
	.workspace-investigation-empty h2 { margin:0; color:var(--text); font-size:var(--fs-md); font-weight:600; letter-spacing:-.015em; }
	.workspace-investigation-empty p { max-width:40ch; margin:0; color:var(--text-dim); font-size:var(--fs-sm); line-height:1.55; }
</style>
