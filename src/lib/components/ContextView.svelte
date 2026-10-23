<script lang="ts">
	import { onDestroy } from 'svelte';
	import { openFolder } from '$lib/commands';
	import { ipc, isDesktop } from '$lib/ipc';
	import { focusInvestigationConversation } from '$lib/investigation-navigation';
	import { session } from '$lib/session.svelte';
	import type { Investigation, SemanticFact } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import { Spinner } from '$lib/components/ui/spinner';
	import { Textarea } from '$lib/components/ui/textarea';
	import Icon from './Icon.svelte';
	import MarkdownContent from './MarkdownContent.svelte';
	let guideEditing = $state(false);

	let contents = $state('');
	let savedContents = $state('');
	let path = $state<string | null>(null);
	let loading = $state(false);
	let loadFailed = $state(false);
	let saving = $state(false);
	let error = $state('');
	let knowledgeError = $state('');
	let knowledgeLoading = $state(false);
	let memoryPath = $state<string | null>(null);
	let memoryContents = $state<string | null>(null);
	let learnedFacts = $state<SemanticFact[]>([]);
	let investigations = $state<Investigation[]>([]);
	let openingInvestigation = $state<string | null>(null);
	let editingFactId = $state<string | null>(null);
	let editingFactText = $state('');
	let resolvingFactId = $state<string | null>(null);
	let loadedWorkspace = $state<string | null>(null);
	let saveTimer: ReturnType<typeof setTimeout> | undefined;
	let saveQueue: Promise<void> = Promise.resolve();
	let loadRequest = 0;
	let legacyMemorySections = $derived(parseLegacyMemory(memoryContents));
	let currentFacts = $derived(learnedFacts.filter((fact) => fact.state !== 'superseded'));
	let historicalFacts = $derived(learnedFacts.filter((fact) => fact.state === 'superseded'));
	let workspaceInvestigations = $derived(investigations.filter((item) => item.workspace_id === loadedWorkspace));
	let investigationsWithDefinitions = $derived(
		workspaceInvestigations
			.map((item) => ({ ...item, workspaceDefinitions: (item.definitions ?? []).filter((definition) => definition.status === 'active') }))
			.filter((item) => item.workspaceDefinitions.length > 0)
	);

	function parseLegacyMemory(markdown: string | null): { title: string; entries: string[] }[] {
		if (!markdown) return [];
		const sections: { title: string; entries: string[] }[] = [];
		let current: { title: string; entries: string[] } | null = null;
		let inComment = false;
		for (const raw of markdown.split(/\r?\n/)) {
			const line = raw.trim();
			if (line.includes('<!--')) inComment = true;
			if (!inComment && line.startsWith('## ')) {
				if (line.slice(3).trim().toLowerCase() === 'semantic facts') break;
				current = { title: line.slice(3).trim(), entries: [] };
				sections.push(current);
				continue;
			}
			if (inComment) {
				if (line.includes('-->')) inComment = false;
				continue;
			}
			if (!current || !line || line.startsWith('#')) continue;
			const entry = line.replace(/^-\s+/, '').trim();
			if (entry) current.entries.push(entry);
		}
		return sections.filter((section) => section.entries.length > 0);
	}

	function authorityLabel(authority: SemanticFact['authority']): string {
		return ({ user: 'Edited by you', observed: 'Verified analysis', model_suggestion: 'Suggested by Fella', confirmed: 'Confirmed by you' })[authority];
	}

	function factFreshness(fact: SemanticFact): string {
		if (fact.state === 'proposed') {
			if (!fact.investigation_id) return 'Not used as guidance until you accept it';
			const revisionNote = fact.revision && fact.revision !== session.catalog.revision
				? ' · from an earlier data revision'
				: '';
			return `Provisional context in this Investigation${revisionNote} · not shared with others`;
		}
		if (fact.authority !== 'observed' || !fact.revision) return fact.investigation_id ? 'Investigation knowledge' : 'Workspace knowledge';
		if (fact.revision !== session.catalog.revision) return 'From an earlier data revision · re-check before reuse';
		return 'Matches the current data revision';
	}

	function scopeLabel(fact: SemanticFact): string {
		return fact.investigation_id ? 'This Investigation' : 'This workspace';
	}

	function investigationTitle(id?: string): string | null {
		if (!id) return null;
		return investigations.find((item) => item.id === id)?.title || 'Investigation';
	}

	function displayRevision(revision?: string): string {
		if (!revision) return 'Not recorded';
		return revision.length > 18 ? `${revision.slice(0, 10)}…${revision.slice(-5)}` : revision;
	}

	async function load(workspace = loadedWorkspace): Promise<void> {
		if (!isDesktop() || !workspace) return;
		const request = ++loadRequest;
		loading = true;
		knowledgeLoading = true;
		loadFailed = false;
		error = '';
		try {
			await saveQueue.catch(() => {});
			const result = await ipc.contextFile(workspace);
			if (request !== loadRequest || workspace !== loadedWorkspace) return;
			path = result?.[0] ?? null;
			contents = result?.[1] ?? '';
			savedContents = contents;
			knowledgeError = '';
			const [memoryResult, factsResult, investigationResult] = await Promise.allSettled([
				ipc.memoryFile(workspace),
				ipc.memoryFacts(workspace),
				ipc.investigationsList(session.conversations.map((conversation) => conversation.id))
			]);
			if (request !== loadRequest || workspace !== loadedWorkspace) return;
			if (memoryResult.status === 'fulfilled') {
				memoryPath = memoryResult.value?.[0] ?? null;
				memoryContents = memoryResult.value?.[1] ?? null;
			} else {
				memoryPath = null;
				memoryContents = null;
				knowledgeError = memoryResult.reason instanceof Error ? memoryResult.reason.message : String(memoryResult.reason);
			}
			if (factsResult.status === 'fulfilled') learnedFacts = factsResult.value ?? [];
			else {
				learnedFacts = [];
				knowledgeError ||= factsResult.reason instanceof Error ? factsResult.reason.message : String(factsResult.reason);
			}
			if (investigationResult.status === 'fulfilled') investigations = investigationResult.value;
			else {
				investigations = [];
				knowledgeError ||= investigationResult.reason instanceof Error ? investigationResult.reason.message : String(investigationResult.reason);
			}
		} catch (e) {
			if (request !== loadRequest || workspace !== loadedWorkspace) return;
			loadFailed = true;
			error = e instanceof Error ? e.message : String(e);
		} finally {
			if (request === loadRequest) knowledgeLoading = false;
			if (request === loadRequest) loading = false;
		}
	}

	function editFact(fact: SemanticFact): void {
		editingFactId = fact.id;
		editingFactText = fact.statement;
	}

	async function resolveFact(
		fact: SemanticFact,
		action: 'accept' | 'edit' | 'remove',
		targetInvestigationId: string | null,
		statement?: string
	): Promise<void> {
		if (!loadedWorkspace || resolvingFactId) return;
		resolvingFactId = fact.id;
		knowledgeError = '';
		try {
			learnedFacts = await ipc.resolveMemoryFact(
				fact.id,
				action,
				loadedWorkspace,
				statement,
				targetInvestigationId
			);
			const result = await ipc.memoryFile(loadedWorkspace);
			memoryPath = result?.[0] ?? null;
			memoryContents = result?.[1] ?? null;
			editingFactId = null;
			editingFactText = '';
		} catch (cause) {
			knowledgeError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			resolvingFactId = null;
		}
	}

	async function openInvestigation(item: Investigation): Promise<void> {
		if (openingInvestigation || !loadedWorkspace || item.workspace_id !== loadedWorkspace) return;
		openingInvestigation = item.id;
		try {
			const linked = await focusInvestigationConversation(
				item,
				'preserve',
				undefined,
				() => session.catalog.workspace === loadedWorkspace
			);
			if (!linked || session.catalog.workspace !== loadedWorkspace) return;
			session.openWorkspaceToolTab('investigation', linked.id, linked.conversation_id ?? session.activeChat.id);
			session.investigationsVersion += 1;
		} catch (cause) {
			knowledgeError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			openingInvestigation = null;
		}
	}

	async function saveSnapshot(workspace: string | null, value: string): Promise<void> {
		if (!isDesktop() || !workspace || loadFailed) return;
		const visible = workspace === loadedWorkspace;
		if (visible) {
			saving = true;
			error = '';
		}
		try {
			const pending = saveQueue.catch(() => {}).then(() => ipc.saveContext(value, workspace));
			saveQueue = pending;
			await pending;
			if (workspace === loadedWorkspace && value === contents) savedContents = value;
			if (workspace === loadedWorkspace && !path) {
				const result = await ipc.contextFile(workspace);
				if (workspace === loadedWorkspace) path = result?.[0] ?? null;
			}
		} catch (e) {
			if (workspace === loadedWorkspace) error = e instanceof Error ? e.message : String(e);
		} finally {
			if (visible) saving = false;
			if (workspace === loadedWorkspace && contents !== savedContents && !loadFailed && !error) scheduleSave();
		}
	}

	async function save(): Promise<void> {
		if (contents === savedContents) return;
		await saveSnapshot(loadedWorkspace, contents);
	}

	function scheduleSave(): void {
		clearTimeout(saveTimer);
		error = '';
		saveTimer = setTimeout(() => void save(), 700);
	}

	function useTemplate(): void {
		if (contents.trim()) return;
		contents = `# About this workspace\n\n<!-- Tell Fella what these files mean, which fields matter, and any rules it should follow. -->\n`;
		scheduleSave();
	}

	$effect(() => {
		const workspace = session.catalog.workspace;
		if (workspace === loadedWorkspace) return;
		if (loadedWorkspace && contents !== savedContents && !loadFailed) {
			void saveSnapshot(loadedWorkspace, contents);
		}
		loadRequest += 1;
		loading = false;
		clearTimeout(saveTimer);
		contents = '';
		savedContents = '';
		path = null;
		memoryPath = null;
		memoryContents = null;
		learnedFacts = [];
		investigations = [];
		editingFactId = null;
		editingFactText = '';
		resolvingFactId = null;
		knowledgeError = '';
		knowledgeLoading = false;
		loadFailed = false;
		loadedWorkspace = workspace;
		if (workspace) void load(workspace);
	});

	onDestroy(() => {
		clearTimeout(saveTimer);
		if (contents !== savedContents && !loadFailed) void save();
	});
</script>

<section class="context-page" aria-labelledby="context-title">
	<header class="page-head">
		<div>
			<h1 id="context-title">Workspace context</h1>
			<p>Guidance and knowledge for this folder.</p>
		</div>
	</header>

	{#if !session.catalog.workspace}
		<div class="empty-state">
			<h2>Connect a folder to add workspace context</h2>
			<p>Choose a folder to add a short guide that helps Fella interpret its files.</p>
			<Button type="button" onclick={() => void openFolder()}>Choose a folder</Button>
		</div>
	{:else if loadFailed}
		<div class="load-error" role="alert">
			<p>Couldn't load the workspace context: {error}</p>
			<Button type="button" onclick={() => void load(loadedWorkspace)}>Retry</Button>
		</div>
	{:else}
		<div class="editor-card">
			<div class="knowledge-section-intro">
				<h2>Workspace guide</h2>
				<p>Definitions and instructions for this workspace.</p>
			</div>
			<div class="editor-head">
				<div>
					<strong>fella.md</strong>
				</div>
				<div class="editor-actions">
					{#if guideEditing && contents.trim() === ''}
						<Button variant="link" size="sm" type="button" onclick={useTemplate}>Use a template</Button>
					{/if}
					{#if guideEditing}
						<Button variant="ghost" size="sm" disabled={saving || loading} onclick={async () => { await save(); if (!error) guideEditing = false; }}>{error ? 'Retry save' : 'Done'}</Button>
					{:else}
						<Button variant="outline" size="sm" disabled={loading} onclick={() => (guideEditing = true)}><Icon name="pencil" size={14} />{contents.trim() ? 'Edit guide' : 'Add guide'}</Button>
					{/if}
				</div>
			</div>
			{#if loading}
				<div class="loading"><Spinner size={28} /><span>Loading your workspace guide…</span></div>
			{:else if guideEditing}
				<Textarea
					class="guide-textarea"
					bind:value={contents}
					aria-label="Workspace guide"
					placeholder="Explain important fields, definitions, or context Fella should know…"
					oninput={scheduleSave}
				/>
			{:else if contents.trim()}
				<div class="guide-reading"><MarkdownContent text={contents} /></div>
			{:else}
				<p class="knowledge-empty">Add business meaning or rules Fella should follow.</p>
			{/if}
			{#if guideEditing || error}<div class="editor-foot" role={error ? 'alert' : 'status'}><span>{error || (saving ? 'Saving…' : contents === savedContents ? 'Saved in fella.md' : 'Changes save automatically')}</span></div>{/if}
			{#if path}<details class="guide-location"><summary>File location</summary><p>{path}</p></details>{/if}
		</div>
		{#if session.workspaceToolTabs.filter(tab => tab.kind === 'context').length > 1}<p class="shared-context-note">All context tabs show this workspace’s shared guide and knowledge.</p>{/if}

		<section class="knowledge-section" aria-labelledby="learned-knowledge-title" data-testid="workspace-knowledge">
			<header class="knowledge-section-intro">
				<h2 id="learned-knowledge-title">Learned from analysis</h2>
				<p>Investigation notes stay provisional until you choose to share them across this workspace.</p>
			</header>
			{#if knowledgeLoading}
				<div class="knowledge-loading"><Spinner size={20} /><span>Loading workspace knowledge…</span></div>
			{:else}
				{#if knowledgeError}
					<p class="knowledge-error" role="alert">Some workspace knowledge couldn't be loaded: {knowledgeError}</p>
				{/if}
				{#if currentFacts.length}
					<div class="knowledge-facts">
						{#each currentFacts as fact (fact.id)}
							<article class="knowledge-fact" data-testid={`workspace-knowledge-fact-${fact.id}`}>
								<div class="knowledge-fact-heading">
									<strong>{fact.key}</strong>
									<span class="knowledge-state" data-state={fact.state}>{fact.state.replace('_', ' ')}</span>
								</div>
								<p class="knowledge-fact-statement">{fact.statement}</p>
								<p class="knowledge-fact-meta">{authorityLabel(fact.authority)} · {fact.kind.replace('_', ' ')} · {scopeLabel(fact)} · {factFreshness(fact)}</p>
								{#if fact.origin_investigation_id}
									<p class="knowledge-fact-origin">From {investigationTitle(fact.origin_investigation_id)}{fact.investigation_id ? ' · provisional in this Investigation' : ''}</p>
								{/if}
								{#if editingFactId === fact.id}
									<div class="knowledge-edit">
										<Textarea class="knowledge-edit-textarea" bind:value={editingFactText} aria-label={`Edit knowledge: ${fact.key}`} maxlength={1000} />
										<div class="knowledge-actions">
											{#if fact.state === 'proposed' && fact.origin_investigation_id}
												<Button variant="secondary" size="sm" disabled={resolvingFactId !== null} onclick={() => void resolveFact(fact, 'edit', fact.origin_investigation_id!, editingFactText)}>Save in Investigation</Button>
												<Button variant="outline" size="sm" disabled={resolvingFactId !== null} onclick={() => void resolveFact(fact, 'edit', null, editingFactText)}>Save across workspace</Button>
											{:else}
												<Button variant="secondary" size="sm" disabled={resolvingFactId !== null} onclick={() => void resolveFact(fact, 'edit', fact.investigation_id ?? null, editingFactText)}>Save changes</Button>
											{/if}
											<Button variant="ghost" size="sm" disabled={resolvingFactId !== null} onclick={() => { editingFactId = null; editingFactText = ''; }}>Cancel</Button>
										</div>
									</div>
								{:else if fact.state === 'proposed'}
									<div class="knowledge-actions">
										{#if fact.origin_investigation_id}
											<Button variant="secondary" size="sm" disabled={resolvingFactId !== null} onclick={() => void resolveFact(fact, 'accept', fact.origin_investigation_id!)}>Keep in this Investigation</Button>
										{/if}
										<Button variant="outline" size="sm" disabled={resolvingFactId !== null} onclick={() => void resolveFact(fact, 'accept', null)}>Use across workspace</Button>
										<Button variant="ghost" size="sm" disabled={resolvingFactId !== null} onclick={() => void resolveFact(fact, 'remove', null)}>Dismiss</Button>
										<Button variant="ghost" size="sm" disabled={resolvingFactId !== null} onclick={() => editFact(fact)}>Edit</Button>
									</div>
								{:else}
									<div class="knowledge-actions">
										<Button variant="ghost" size="sm" disabled={resolvingFactId !== null} onclick={() => editFact(fact)}>Edit</Button>
										<Button variant="ghost" size="sm" disabled={resolvingFactId !== null} onclick={() => void resolveFact(fact, 'remove', fact.investigation_id ?? null)}>Remove</Button>
									</div>
								{/if}
								<details class="knowledge-provenance">
									<summary>Source and status</summary>
									<dl>
										<div><dt>Scope</dt><dd>{scopeLabel(fact)}{fact.investigation_id ? ` · ${investigationTitle(fact.investigation_id)}` : ''}</dd></div>
										{#if fact.origin_investigation_id}<div><dt>Origin</dt><dd>{investigationTitle(fact.origin_investigation_id)} · <code>{fact.origin_investigation_id}</code></dd></div>{/if}
										<div><dt>Data revision</dt><dd><code>{displayRevision(fact.revision)}</code></dd></div>
										{#if fact.supporting_turn}<div><dt>Analysis turn</dt><dd><code>{fact.supporting_turn}</code></dd></div>{/if}
										{#if fact.evidence_ids?.length}<div><dt>Evidence records</dt><dd>{fact.evidence_ids.join(', ')}</dd></div>{/if}
										{#if fact.conflicts_with?.length}<div><dt>Conflicts with</dt><dd><code>{fact.conflicts_with.join(', ')}</code></dd></div>{/if}
										{#if fact.supersedes?.length}<div><dt>Supersedes</dt><dd><code>{fact.supersedes.join(', ')}</code></dd></div>{/if}
									</dl>
								</details>
							</article>
						{/each}
					</div>
				{:else if !legacyMemorySections.length}
					<p class="knowledge-empty">No reusable knowledge yet. Fella can build it as you analyze this workspace.</p>
				{/if}
				{#each legacyMemorySections as section (section.title)}
					<div class="legacy-memory-section">
						<h3>{section.title}</h3>
						<ul>{#each section.entries as entry, index (`${section.title}:${index}`)}<li>{entry}</li>{/each}</ul>
					</div>
				{/each}
				{#if historicalFacts.length}
					<details class="knowledge-history">
						<summary>Superseded knowledge ({historicalFacts.length})</summary>
						<ul>{#each historicalFacts as fact (fact.id)}<li><strong>{fact.key}</strong> — {fact.statement} <span>· {authorityLabel(fact.authority)} · revision {displayRevision(fact.revision)}</span></li>{/each}</ul>
					</details>
				{/if}
			{/if}
		</section>

		{#if knowledgeLoading || investigationsWithDefinitions.length}<section class="knowledge-section investigation-knowledge" aria-labelledby="investigation-definitions-title" data-testid="workspace-investigation-definitions">
			<header class="knowledge-section-intro">
				<h2 id="investigation-definitions-title">Investigation definitions</h2>
				<p>Definitions and assumptions apply only to their Investigation; they do not silently become workspace-wide rules.</p>
			</header>
			{#if knowledgeLoading}
				<div class="knowledge-loading"><Spinner size={20} /><span>Loading Investigation definitions…</span></div>
			{:else if investigationsWithDefinitions.length}
				{#each investigationsWithDefinitions as item (item.id)}
					<article class="investigation-knowledge-item">
						<div class="investigation-knowledge-heading">
							<h3>{item.title || 'Untitled Investigation'}</h3>
							<Button variant="link" size="sm" type="button" disabled={openingInvestigation !== null} onclick={() => void openInvestigation(item)}>
								{openingInvestigation === item.id ? 'Opening…' : 'Open Investigation'}
							</Button>
						</div>
						<ul class="investigation-definition-list">
							{#each item.workspaceDefinitions as definition (definition.id)}
								<li>
									<div><span class="definition-kind">{definition.kind}</span><span>{definition.text}</span></div>
									<small>Revision {definition.revision}</small>
								</li>
							{/each}
						</ul>
					</article>
				{/each}
			{:else}
				<p class="knowledge-empty">No Investigation definitions saved in this workspace.</p>
			{/if}
		</section>{/if}
	{/if}

</section>

<style>
	.guide-reading { padding: 12px 0; font-size: var(--fs-sm); }
	.guide-location, .shared-context-note { color: var(--text-faint); font-size: var(--fs-xs); }
	.guide-location > summary { cursor: pointer; }
	.guide-location p { overflow-wrap: anywhere; font-family: var(--mono); }
	.context-page {
		flex: 1;
		min-height: 0;
		width: 100%;
		max-width: var(--content-max);
		margin: 0 auto;
		padding: 20px;
		overflow: auto;
	}
	.page-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-5);
		margin-bottom: var(--space-5);
	}
	h1 {
		margin: 0;
		font-size: 18px;
		line-height: 1.4;
		font-weight: 650;
		letter-spacing: -0.03em;
	}
	.page-head p { margin: var(--space-2) 0 0; color: var(--text-dim); line-height: 1.5; }
	.empty-state {
		max-width: 46ch;
		margin: 10vh auto 0;
		text-align: center;
	}
	.load-error {
		display: grid;
		justify-items: start;
		gap: var(--space-3);
		max-width: 100%;
		padding: var(--space-5);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		background: var(--panel);
	}
	.load-error p { margin: 0; color: var(--danger, #b42318); overflow-wrap: anywhere; }
	.empty-state h2 {
		margin: 0;
		font-size: var(--fs-lg);
		font-weight: 650;
	}
	.empty-state p {
		margin: var(--space-2) 0 var(--space-4);
		color: var(--text-dim);
	}
	.editor-card {
		border-block: 1px solid var(--border);
		border-inline: 0;
		border-radius: 0;
		background: transparent;
		overflow: hidden;
	}
	.knowledge-section {
		margin-top: var(--space-7);
		padding-top: var(--space-5);
		border-top: 1px solid var(--border);
	}
	.knowledge-section-intro { margin: 0 0 var(--space-3); }
	.knowledge-section-intro h2 {
		margin: 0;
		font-size: var(--fs-lg);
		font-weight: 620;
		letter-spacing: -0.015em;
	}
	.knowledge-section-intro p {
		max-width: 68ch;
		margin: var(--space-1) 0 0;
		color: var(--text-dim);
		font-size: var(--fs-sm);
		line-height: 1.5;
	}
	.knowledge-loading { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-3) 0; color: var(--text-faint); font-size: var(--fs-sm); }
	.knowledge-error { margin: var(--space-3) 0; color: var(--danger, #b42318); font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.knowledge-empty { margin: var(--space-3) 0 0; color: var(--text-faint); font-size: var(--fs-sm); }
	.knowledge-facts { border-top: 1px solid var(--border); }
	.knowledge-fact,
	.legacy-memory-section,
	.investigation-knowledge-item { padding: var(--space-3) 0; border-bottom: 1px solid var(--border); }
	.knowledge-fact-heading { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
	.knowledge-fact-heading strong { color: var(--text); font-size: var(--fs-sm); font-weight: 600; }
	.knowledge-state {
		flex: none;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		text-transform: capitalize;
	}
	.knowledge-state[data-state='conflict'] { color: var(--warning, #9a6700); }
	.knowledge-state[data-state='proposed'] { color: var(--text-dim); }
	.knowledge-fact-statement { margin: var(--space-1) 0 0; color: var(--text); font-size: var(--fs-sm); line-height: 1.5; }
	.knowledge-fact-meta { margin: var(--space-1) 0 0; color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.45; }
	.knowledge-fact-origin { margin: var(--space-1) 0 0; color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.45; }
	.knowledge-actions { display: flex; flex-wrap: wrap; align-items: center; gap: var(--space-1); margin-top: var(--space-2); }
	.knowledge-edit { display: grid; gap: var(--space-2); margin-top: var(--space-2); }
	:global(.knowledge-edit-textarea) { min-height: 84px; padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--panel); color: var(--text); font-size: var(--fs-sm); line-height: 1.5; }
	:global(.knowledge-edit-textarea:focus-visible) { outline: 2px solid var(--link); outline-offset: 1px; }
	.knowledge-provenance { margin-top: var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); }
	.knowledge-provenance summary,
	.knowledge-history summary { width: fit-content; cursor: pointer; color: var(--text-dim); }
	.knowledge-provenance summary:hover,
	.knowledge-history summary:hover { color: var(--text); }
	.knowledge-provenance dl { display: grid; gap: 5px; margin: var(--space-2) 0 0; }
	.knowledge-provenance dl > div { display: grid; grid-template-columns: minmax(90px, 0.3fr) minmax(0, 1fr); gap: var(--space-2); }
	.knowledge-provenance dt { color: var(--text-faint); }
	.knowledge-provenance dd { min-width: 0; margin: 0; color: var(--text-dim); overflow-wrap: anywhere; }
	.knowledge-provenance code { font-family: var(--mono); }
	.legacy-memory-section h3,
	.investigation-knowledge-heading h3 { margin: 0; font-size: var(--fs-sm); font-weight: 600; }
	.legacy-memory-section ul,
	.knowledge-history ul { display: grid; gap: var(--space-1); margin: var(--space-2) 0 0; padding-left: 18px; color: var(--text-dim); font-size: var(--fs-sm); line-height: 1.5; }
	.knowledge-history { margin-top: var(--space-3); color: var(--text-faint); font-size: var(--fs-xs); }
	.knowledge-history ul { font-size: var(--fs-xs); }
	.knowledge-history li span { color: var(--text-faint); }
	.investigation-knowledge { margin-bottom: var(--space-6); }
	.investigation-knowledge-heading { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
	.investigation-definition-list { display: grid; gap: var(--space-2); margin: var(--space-2) 0 0; padding: 0; list-style: none; }
	.investigation-definition-list li { display: flex; align-items: baseline; justify-content: space-between; gap: var(--space-3); color: var(--text-dim); font-size: var(--fs-sm); line-height: 1.5; }
	.investigation-definition-list li > div { display: flex; align-items: baseline; gap: var(--space-2); min-width: 0; }
	.definition-kind { flex: none; color: var(--text-faint); font-size: var(--fs-xs); text-transform: capitalize; }
	.investigation-definition-list small { flex: none; color: var(--text-faint); font-size: var(--fs-xs); }
	.editor-head,
	.editor-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-3) 0;
	}
	.editor-head {
		border-bottom: 1px solid var(--border);
	}
	.editor-head > div:first-child { min-width: 0; flex: 1; }
	.editor-head strong {
		font-family: var(--mono);
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.editor-actions {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		flex: none;
	}
	:global(.guide-textarea) {
		display: block;
		width: 100%;
		min-height: 160px;
		padding: var(--space-4) 0;
		border: 0;
		outline: 0;
		resize: vertical;
		background: transparent;
		color: var(--text);
		font: 14px/1.65 var(--mono);
	}
	:global(.guide-textarea:focus-visible) {
		box-shadow: inset 0 0 0 2px var(--link);
	}
	:global(.guide-textarea::placeholder) {
		color: var(--text-faint);
	}
	.loading {
		min-height: 160px;
		display: grid;
		place-items: center;
		align-content: center;
		gap: var(--space-3);
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.editor-foot {
		border-top: 1px solid var(--border);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.editor-foot span:first-child {
		color: var(--text-dim);
	}
	@container investigation-pane (max-width: 560px) {
		.page-head,
		.editor-head,
		.editor-foot {
			align-items: flex-start;
			flex-direction: column;
		}
		.editor-actions {
			width: 100%;
			justify-content: space-between;
		}
		.knowledge-actions { align-items: stretch; }
		.investigation-definition-list li { align-items: flex-start; flex-direction: column; gap: 2px; }
	}
</style>
