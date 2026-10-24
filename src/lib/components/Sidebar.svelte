<script lang="ts">
	import { ipc, isDesktop } from '$lib/ipc';
	import { errMsg, openConversation, openRepository, openFolder } from '$lib/commands';
	import { GENERAL_WORKSPACE_ID, session } from '$lib/session.svelte';
	import type { StudioDocument } from '$lib/analytics-artifacts';
	import { Button } from '$lib/components/ui/button';
	import { Dialog } from '$lib/components/ui/dialog';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import { Input } from '$lib/components/ui/input';
	import type { ConversationSummary } from '$lib/types';
	import Icon from './Icon.svelte';
	import WorkspaceSwitcher from './WorkspaceSwitcher.svelte';

	let {
		onsearch,
		onworkspaceselect,
		onsectionselect,
		mode = $bindable<'chat' | 'files' | 'reports'>('chat'),
		selectedFolder = 'all',
		onselectfolder
	}: {
		onsearch?: () => void;
		onworkspaceselect: (id:string) => void | Promise<void>;
		onsectionselect?: (section: 'chat' | 'files' | 'reports') => void;
		mode?: 'chat' | 'files' | 'reports';
		selectedFolder?: string;
		onselectfolder?: (folder: string) => void;
	} = $props();

	let workspaceReports = $state<StudioDocument[]>([]);
	let reportsError = $state('');
	let reportsRequest = 0;
	$effect(() => {
		const path = session.catalog.workspace;
		const reportsVersion = session.reportsVersion;
		void reportsVersion;
		const request = ++reportsRequest;
		if (mode !== 'reports' || !path) { workspaceReports = []; reportsError = ''; return; }
		workspaceReports = [];
		reportsError = '';
		void ipc.studioList(path).then((reports) => {
			if (request === reportsRequest) workspaceReports = reports;
		}).catch(() => { if (request === reportsRequest) reportsError = 'Could not load reports.'; });
	});

	type Repository = {
		key: string;
		path: string | null;
		name: string;
		historyOnly: boolean;
	};

	let list = $state<ConversationSummary[]>([]);
	let investigationConversationIds = $state<Set<string>>(new Set());
	let conversationDeleteTarget = $state<ConversationSummary | null>(null);
	let conversationDeleteDialogOpen = $state(false);
	let deletingConversation = $state(false);
	let conversationDeleteError = $state('');
	let workspaceDeleteTarget = $state<Repository | null>(null);
	let workspaceDeleteDialogOpen = $state(false);
	let deletingWorkspace = $state(false);
	let workspaceDeleteError = $state('');
	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent)
			? '⌘'
			: 'Ctrl';

	async function refresh() {
		if (!isDesktop()) return;
		try {
			const next = await ipc.conversationsList();
			session.rememberRepositories(next.map((item) => item.workspace));
			list = next;
			try {
				const investigations = await ipc.investigationsList(next.map((item) => item.id));
				investigationConversationIds = new Set(investigations.flatMap((item) => item.conversation_id ? [item.conversation_id] : []));
				session.rememberInvestigationNames(investigations);
			} catch {
				// The conversation list remains usable if Investigation history is unavailable.
			}
		} catch {
			/* leave the last-known list rather than blanking it on a hiccup */
		}
	}

	// Runs once on mount, then again whenever a conversation gets archived.
	$effect(() => {
		session.historyVersion;
		session.investigationsVersion;
		void refresh();
	});

	function title(c: ConversationSummary): string {
		return c.title?.trim() || session.investigationNames[c.id]?.trim() || c.preview || 'New conversation';
	}

	function relativeUpdated(timestamp: number): string {
		const elapsed = Math.max(0, Date.now() - timestamp);
		if (elapsed < 60_000) return 'Just now';
		if (elapsed < 3_600_000) return `${Math.floor(elapsed / 60_000)}m ago`;
		if (elapsed < 86_400_000) return `${Math.floor(elapsed / 3_600_000)}h ago`;
		if (elapsed < 172_800_000) return 'Yesterday';
		return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(new Date(timestamp));
	}

	function subtitle(c: ConversationSummary): string {
		const live = session.conversations.find(conversation => conversation.id === c.id);
		const latest = live ? [...live.messages].reverse().find(message => message.role === 'assistant' && message.text.trim()) : undefined;
		if (latest) return latest.text.replace(/[#*_`]/g, '').replace(/\s+/g, ' ').trim().slice(0,160);
		const preview = c.preview.trim();
		const currentTitle = title(c).trim();
		return preview && preview.toLocaleLowerCase() !== currentTitle.toLocaleLowerCase()
			? preview
			: `Updated ${relativeUpdated(c.saved_at_ms)}`;
	}

	function relativePath(path: string): string {
		const root = session.catalog.workspace?.replace(/[/\\]+$/, '') ?? '';
		const normalizedPath = path.replace(/\\/g, '/');
		const normalizedRoot = root.replace(/\\/g, '/').replace(/\/$/, '');
		return normalizedRoot && normalizedPath.toLocaleLowerCase().startsWith(`${normalizedRoot.toLocaleLowerCase()}/`)
			? normalizedPath.slice(normalizedRoot.length + 1)
			: normalizedPath;
	}

	let folders = $derived.by(() => {
		const counts = new Map<string, Set<string>>();
		for (const source of session.catalog.sources) {
			const relative = relativePath(source.path);
			const segments = relative.split('/').filter(Boolean);
			const id = segments.length > 1 ? segments[0] : '__root';
			const paths = counts.get(id) ?? new Set<string>();
			paths.add(source.path);
			counts.set(id, paths);
		}
		return [...counts].map(([id, paths]) => ({ id, label: id === '__root' ? 'Workspace root' : id, count: paths.size }))
			.sort((a, b) => a.label.localeCompare(b.label, undefined, { numeric: true }));
	});
	let catalogFileCount = $derived(new Set(session.catalog.sources.map((source) => source.path)).size);

	let scopedConversations = $derived(list.filter((item) => (item.workspace ?? null) === (session.activeWorkspaceId ?? null)));
	let investigationConversations = $derived(scopedConversations.filter((item) => investigationConversationIds.has(item.id) || Boolean(session.investigationNames[item.id])));
	let questionConversations = $derived(scopedConversations.filter((item) => !investigationConversationIds.has(item.id) && !session.investigationNames[item.id]));


	function repositoryName(path: string | null): string {
		if (!path) return 'General';
		return path.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') || path;
	}

	let repositories = $derived<Repository[]>([{
		key: session.activeWorkspaceId ?? GENERAL_WORKSPACE_ID,
		path: session.activeWorkspaceId ?? null,
		name: repositoryName(session.activeWorkspaceId ?? null),
		historyOnly: Boolean(session.activeWorkspaceId && session.historyOnlyRepositoryPaths.includes(session.activeWorkspaceId))
	}]);

	function newChat(): void {
		mode = 'chat';
		session.newConversation();
	}

	function selectMode(next: 'chat' | 'files' | 'reports'): void {
		mode = next;
		onsectionselect?.(next);
		if (next !== 'reports' || !session.catalog.workspace) return;
		if (session.activeWorkspaceTool?.kind !== 'studio') session.openStudioTool();
	}

	function openReport(id: string): void {
		mode = 'reports';
		onsectionselect?.('reports');
		session.openStudioTool(id);
	}

	function newReport(): void {
		mode = 'reports';
		onsectionselect?.('reports');
		session.openNewReport();
	}

	async function selectRepository(repo: Repository): Promise<boolean> {
		if (!repo.path) {
			return session.focusWorkspace(GENERAL_WORKSPACE_ID);
		}
		const workspace = session.workspaceAt(repo.path);
		if (workspace && !workspace.historyOnly) return session.focusWorkspace(workspace.id);
		return openRepository(repo.path, { reportFailure: true });
	}

	function requestWorkspaceDelete(repo: Repository): void {
		if (!repo.path) return;
		workspaceDeleteTarget = repo;
		workspaceDeleteError = '';
		workspaceDeleteDialogOpen = true;
	}

	async function deleteWorkspaceAndConversations(): Promise<void> {
		const target = workspaceDeleteTarget;
		const path = target?.path;
		if (!target || !path || deletingWorkspace) return;
		deletingWorkspace = true;
		workspaceDeleteError = '';
		const deletedIds: string[] = [];
		let prepared = false;
		try {
			prepared = await session.beginWorkspaceDeletion(path);
			if (!prepared) throw new Error('Deletion could not continue: a conversation is active or a save is in progress.');

			const archived = await ipc.conversationsList();
			const ids = [...new Set(archived.filter((conversation) => conversation.workspace === path).map((conversation) => conversation.id))];
			for (const id of ids) {
				await ipc.deleteConversation(id);
				deletedIds.push(id);
			}

			await session.deleteWorkspaceAndConversations(path);
			list = list.filter((conversation) => conversation.workspace !== path);
			workspaceDeleteDialogOpen = false;
			workspaceDeleteTarget = null;
		} catch (error) {
			if (prepared) session.cancelWorkspaceDeletion(path);
			for (const id of deletedIds) session.removeConversationWithoutArchiving(id);
			try { list = await ipc.conversationsList(); } catch { /* retain the last-known sidebar history */ }
			workspaceDeleteError = deletedIds.length
				? `Deletion stopped after removing ${deletedIds.length} conversation${deletedIds.length === 1 ? '' : 's'}. Try again to remove the rest. ${errMsg(error)}`
				: errMsg(error);
		} finally {
			deletingWorkspace = false;
		}
	}

	let renamingId = $state<string | null>(null);
	let renameValue = $state('');
	let renameInput = $state<HTMLInputElement | null>(null);

	function startRename(c: ConversationSummary, e: MouseEvent) {
		e.stopPropagation();
		renamingId = c.id;
		renameValue = c.title ?? c.preview;
		queueMicrotask(() => renameInput?.select());
	}

	async function commitRename(c: ConversationSummary) {
		const id = renamingId;
		renamingId = null;
		if (id === null) return;
		const next = renameValue.trim();
		const original = c.title ?? c.preview;
		if (next === original) return; // unedited -- nothing to save
		try {
			await session.renameConversation(id, next); // empty clears a custom title
			if (isDesktop()) list = await ipc.conversationsList();
		} catch (e) {
			session.addSystem(`error: ${errMsg(e)}`);
		}
	}

	async function open(c: ConversationSummary, investigation: boolean) {
		try {
			await openConversation(c);
			if (investigation && session.catalog.workspace) session.setWorkspacePane('investigation');
			else session.setWorkspacePaneOpen(false);
		} catch (e) {
			session.addSystem(`error: ${errMsg(e)}`);
		}
	}

	function remove(c: ConversationSummary, e: MouseEvent) {
		e.stopPropagation();
		conversationDeleteTarget = c;
		conversationDeleteError = '';
		conversationDeleteDialogOpen = true;
	}

	async function confirmConversationDelete() {
		const target = conversationDeleteTarget;
		if (!target || deletingConversation) return;
		deletingConversation = true;
		conversationDeleteError = '';
		try {
			await ipc.deleteConversation(target.id);
			list = list.filter((x) => x.id !== target.id);
			// If this conversation is still live, remove it too --
			// otherwise its very next settle re-archives it, undoing the delete.
			session.removeConversationWithoutArchiving(target.id);
			conversationDeleteDialogOpen = false;
			conversationDeleteTarget = null;
		} catch (err) { conversationDeleteError = errMsg(err); }
		finally { deletingConversation = false; }
	}
</script>

	<aside class="sidebar">
			<div class="header">
				<div class="header-actions">
					<Button variant="ghost" size="icon" class="icon-btn collapse-btn" type="button" aria-label="Collapse sidebar" title={`Collapse sidebar (${shortcutModifier}+B)`} onclick={() => session.toggleSidebar()}><Icon name="panel" size={14} /></Button>
					<Button variant="ghost" size="icon" class="icon-btn" type="button" aria-label="Search" title={`Search (${shortcutModifier}+K)`} aria-keyshortcuts="Control+K Meta+K" aria-haspopup="dialog" onclick={() => onsearch?.()}><Icon name="search" size={14} /></Button>
					<Button variant="ghost" size="icon" class="icon-btn" type="button" aria-label="New conversation" title={`New conversation (${shortcutModifier}+Shift+A)`} aria-keyshortcuts="Control+Shift+A Meta+Shift+A" onclick={newChat}><Icon name="plus" size={15} /></Button>
				</div>
			</div>
			<div class="workspace-identity">
				<WorkspaceSwitcher identity onselect={onworkspaceselect} />
				{#each repositories.filter((repo) => repo.path === (session.activeWorkspaceId ?? null)) as repo (repo.key)}
					{#if repo.path}
						<DropdownMenu.Root>
							<DropdownMenu.Trigger class="identity-menu-trigger" aria-label="Workspace settings" title="Workspace settings"><Icon name="more-horizontal" size={16} /></DropdownMenu.Trigger>
							<DropdownMenu.Portal><DropdownMenu.Content class="fella-ui-menu-content" side="bottom" align="end" sideOffset={4}>
								{#if repo.historyOnly}<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => void selectRepository(repo)}>Reconnect folder</DropdownMenu.Item>{/if}
								<DropdownMenu.Item class="fella-ui-menu-item workspace-delete-menu-item" onSelect={() => requestWorkspaceDelete(repo)}>Delete workspace and conversations</DropdownMenu.Item>
							</DropdownMenu.Content></DropdownMenu.Portal>
						</DropdownMenu.Root>
					{/if}
				{/each}
			</div>
			<nav class="sidebar-mode-switch fella-pill-nav" aria-label="Workspace sections">
				<Button variant="ghost" class="sidebar-mode-option fella-pill-nav-item" type="button" aria-pressed={mode === 'chat'} onclick={() => selectMode('chat')}>Chat</Button>
				<Button variant="ghost" class="sidebar-mode-option fella-pill-nav-item" type="button" aria-pressed={mode === 'files'} onclick={() => selectMode('files')}>Files</Button>
				<Button variant="ghost" class="sidebar-mode-option fella-pill-nav-item" type="button" aria-pressed={mode === 'reports'} onclick={() => selectMode('reports')}>Reports</Button>
			</nav>
			{#if mode === 'chat'}
				<section class="repository-section chat-history" aria-label="Workspace conversations">
					{#each [{ label: 'Investigations', rows: investigationConversations }, { label: 'Questions', rows: questionConversations }] as group (group.label)}
						<div class="conversation-group">
							<div class="section-head"><h2>{group.label}</h2>{#if group.label === 'Investigations'}<DropdownMenu.Root><DropdownMenu.Trigger class="section-action investigation-actions" aria-label="Investigation actions" title="Investigation actions"><Icon name="more-horizontal" size={14} /></DropdownMenu.Trigger><DropdownMenu.Portal><DropdownMenu.Content class="fella-ui-menu-content" align="end"><DropdownMenu.Item class="fella-ui-menu-item" disabled={!session.catalog.workspace} onSelect={() => session.requestNewInvestigation()}><Icon name="plus" size={14} />New Investigation…</DropdownMenu.Item></DropdownMenu.Content></DropdownMenu.Portal></DropdownMenu.Root>{/if}</div>
					{#if group.rows.length}
						{#each group.rows as c (c.id)}
							<div class="item-wrap">
								{#if renamingId === c.id}
									<Input class="rename-input" bind:ref={renameInput} bind:value={renameValue} onkeydown={(e) => { if (e.key === 'Enter') commitRename(c); else if (e.key === 'Escape') renamingId = null; }} onblur={() => commitRename(c)} aria-label="Rename conversation" />
								{:else}
									<Button variant="ghost" class={session.conversationId === c.id ? 'conversation-row active' : 'conversation-row'} type="button" onclick={() => void open(c, group.label === 'Investigations')} title={title(c)} aria-label={`Open conversation: ${title(c)}`} aria-current={session.conversationId === c.id ? 'page' : undefined}>
										<span class="conversation-copy"><span class="conversation-title-line"><strong>{title(c)}</strong>{#if session.conversationId === c.id}<Icon name="pencil" size={11} />{/if}</span><small>{subtitle(c)}</small></span>
									</Button>
									<div class="row-actions">
										<Button variant="ghost" size="icon" class="ren" type="button" aria-label="Rename conversation" onclick={(e) => startRename(c, e)}><Icon name="pencil" size={14} /></Button>
										<Button variant="ghost" size="icon" class="del" type="button" aria-label="Delete conversation" title="Delete conversation" onclick={(e) => remove(c, e)}><Icon name="x" size={14} /></Button>
									</div>
								{/if}
							</div>
						{/each}
					{:else}
								<p class="sidebar-empty">{group.label === 'Investigations' ? 'No Investigations yet.' : 'No conversations yet.'}</p>
					{/if}
						</div>
					{/each}
				</section>
			{:else if mode === 'files'}
				<section class="repository-section file-folders" aria-label="Workspace folders">
					<div class="section-head"><h2>Folders</h2><span class="group-count">{catalogFileCount}</span></div>
					<div class="folder-list">
						<button class="folder-row" class:active={selectedFolder === 'all'} type="button" aria-pressed={selectedFolder === 'all'} onclick={() => onselectfolder?.('all')}><strong>All files</strong><small>{catalogFileCount}</small></button>
						{#each folders as folder (folder.id)}
							<button class="folder-row" class:active={selectedFolder === folder.id} type="button" aria-pressed={selectedFolder === folder.id} onclick={() => onselectfolder?.(folder.id)}><strong title={folder.label}>{folder.label}</strong><small>{folder.count}</small></button>
						{/each}
					</div>
					{#if session.catalog.skipped?.length}
						<div class="catalog-warning"><span class="warning-dot"></span><span><strong>{session.catalog.skipped.length} unavailable {session.catalog.skipped.length === 1 ? 'file' : 'files'}</strong><small>Some items couldn’t be cataloged.</small></span></div>
					{/if}
				</section>
			{:else}
				<section class="repository-section report-history" aria-label="Workspace reports">
					<div class="section-head"><h2>Reports</h2><span class="group-count">{workspaceReports.length}</span><Button variant="ghost" size="icon" class="section-action report-add" aria-label="New report" title="New report" disabled={!session.catalog.workspace} onclick={newReport}><Icon name="plus" size={16} /></Button></div>
					{#if !session.catalog.workspace}
						<p class="sidebar-empty">Open a folder to view reports.</p>
					{:else if reportsError}
						<p class="sidebar-empty" role="status">{reportsError}</p>
					{:else}
						<div class="report-list">
							{#each workspaceReports as report (report.id)}
								<Button variant="ghost" class={`report-row${session.activeWorkspaceTool?.kind === 'studio' && session.activeWorkspaceTool.documentId === report.id ? ' active' : ''}`} title={report.title} onclick={() => openReport(report.id)}>
									<span class="report-copy"><strong>{report.title}</strong><small>Updated {relativeUpdated(report.updated_at_ms)}</small></span>
								</Button>
							{:else}<p class="sidebar-empty">No reports yet.</p>{/each}
						</div>
					{/if}
				</section>
			{/if}
			<div class="sidebar-footer">
				<span class="local-profile" title="Local profile"><Icon name="user" size={16} /></span>
				<Button variant="ghost" class="add-folder-button" type="button" onclick={() => void openFolder()}><Icon name="plus" size={13} />Add sources</Button>
				<Button variant="ghost" size="icon" class={`settings-button${session.workspaceView === 'settings' ? ' active' : ''}`} title={`Settings (${shortcutModifier}+,)`} aria-label="Settings" aria-keyshortcuts="Control+Comma Meta+Comma" aria-current={session.workspaceView === 'settings' ? 'page' : undefined} onclick={() => session.setWorkspaceView('settings')}><Icon name="settings" size={15} /></Button>
			</div>
	</aside>

<Dialog.Root bind:open={workspaceDeleteDialogOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="fella-ui-dialog-overlay" />
		<Dialog.Content class="fella-ui-dialog-content workspace-delete-dialog">
			<div>
				<Dialog.Title class="workspace-delete-title" level={2}>
					Delete {workspaceDeleteTarget?.name ?? 'workspace'}?
				</Dialog.Title>
				<Dialog.Description class="workspace-delete-description">
					This removes the workspace from the sidebar and deletes its conversations. Saved results, reports and their supporting records are kept; reconnect the folder to open them. The folder, files, and project notes are not deleted.
				</Dialog.Description>
			</div>
			{#if workspaceDeleteError}
				<p class="workspace-delete-error" role="alert">{workspaceDeleteError}</p>
			{/if}
			<div class="workspace-delete-actions">
				<Button variant="ghost" disabled={deletingWorkspace} onclick={() => { workspaceDeleteDialogOpen = false; }}>Cancel</Button>
				<Button variant="destructive" disabled={deletingWorkspace} onclick={() => void deleteWorkspaceAndConversations()}>
					{deletingWorkspace ? 'Deleting…' : 'Delete workspace and conversations'}
				</Button>
			</div>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

<Dialog.Root bind:open={conversationDeleteDialogOpen}>
	<Dialog.Portal>
		<Dialog.Overlay class="fella-ui-dialog-overlay" />
		<Dialog.Content class="fella-ui-dialog-content conversation-delete-dialog">
			<Dialog.Title class="workspace-delete-title" level={2}>Delete conversation?</Dialog.Title>
			<Dialog.Description class="workspace-delete-description">
				“{conversationDeleteTarget ? title(conversationDeleteTarget) : ''}”, its transcript, and any linked Investigation will be deleted. Saved results, reports, their supporting analysis records, and revisions will remain available in this workspace.
			</Dialog.Description>
			{#if conversationDeleteError}<p class="workspace-delete-error" role="alert">{conversationDeleteError}</p>{/if}
			<div class="workspace-delete-actions">
				<Button variant="ghost" disabled={deletingConversation} onclick={() => { conversationDeleteDialogOpen = false; }}>Cancel</Button>
				<Button variant="destructive" disabled={deletingConversation} onclick={() => void confirmConversationDelete()}>{deletingConversation ? 'Deleting…' : 'Delete conversation'}</Button>
			</div>
		</Dialog.Content>
	</Dialog.Portal>
</Dialog.Root>

<style>
	.sidebar { display:flex; flex-direction:column; flex:none; width:268px; min-width:268px; max-width:268px; height:100%; padding:0; background:var(--sidebar-surface); border-right:1px solid var(--border); }
	.header { display:flex; align-items:center; justify-content:space-between; flex:none; height:48px; padding:16px 16px 0; -webkit-app-region:drag; }
	.header-actions { display:flex; align-items:center; gap:8px; -webkit-app-region:no-drag; }
	.header-actions :global(.icon-btn), :global(.section-action) { display:grid; place-items:center; width:32px; height:32px; min-height:32px; padding:0; border:0; border-radius:999px; background:var(--bg-raised-2); color:var(--text-dim); cursor:pointer; }
	.header-actions :global(.icon-btn:hover), :global(.section-action:hover), :global(.identity-menu-trigger:hover) { background:var(--bg-raised-2, var(--bg-inset)); color:var(--text); }
	.workspace-identity { position:relative; display:flex; min-width:0; align-items:center; gap:8px; margin-top:24px; padding:0 16px; }
	:global(.identity-menu-trigger) { position:absolute; right:12px; opacity:0; display:grid; width:28px; height:28px; flex:none; place-items:center; border:0; border-radius:999px; background:transparent; color:var(--text-faint); cursor:pointer; }
	.workspace-identity:hover :global(.identity-menu-trigger), :global(.identity-menu-trigger:focus-visible), :global(.identity-menu-trigger[data-state='open']) { opacity:1; }
	.sidebar-mode-switch { flex:none; display:flex; width:calc(100% - 24px); gap:0; margin:16px 12px 0; padding:4px; }
	.sidebar :global(.sidebar-mode-option) { flex:1 1 0; justify-content:center; min-width:0; min-height:30px; height:30px; padding:0 6px; font-size:12.5px; font-weight:400; }
	.repository-section { display:flex; flex:1; flex-direction:column; min-height:0; overflow-y:auto; scrollbar-width:thin; padding:0 12px; margin-top:20px; }
	.conversation-group + .conversation-group { margin-top:20px; }
	.section-head { display:flex; align-items:center; gap:8px; min-height:23px; padding:0 8px 6px; }
	.section-head h2 { margin:0; color:var(--text-dim); font-size:11.5px; font-weight:400; }
	.group-count { margin-left:auto; color:var(--text-faint); font-size:11px; }
	:global(.report-add), :global(.investigation-actions) { margin-left:auto; }
	:global(.investigation-actions) { background:transparent; width:20px; height:20px; min-height:20px; }
	.folder-list, .report-list { display:grid; gap:2px; }
	.item-wrap { position:relative; min-width:0; margin-bottom:2px; }
	.sidebar :global(.conversation-row), :global(.report-row) { display:flex; justify-content:flex-start; width:100%; min-width:0; min-height:54px; gap:0; padding:8px; border-radius:12px; color:var(--text-dim); text-align:left; }
	.sidebar :global(.conversation-row:hover), :global(.report-row:hover), .folder-row:hover { background:var(--bg-raised); color:var(--text); }
	.sidebar :global(.conversation-row.active), :global(.report-row.active) { background:var(--bg-raised-2, var(--bg-inset)); color:var(--text); }
	.conversation-copy, .report-copy { display:grid; min-width:0; flex:1; gap:0; text-align:left; }
	.conversation-title-line { display:flex; min-width:0; align-items:center; gap:6px; }
	.conversation-copy strong, .report-copy strong { overflow:hidden; color:var(--text); font-size:13px; font-weight:500; text-overflow:ellipsis; white-space:nowrap; }
	.conversation-copy small, .report-copy small { display:block; overflow:hidden; color:var(--text-dim); font-size:var(--fs-xs); font-weight:400; line-height:1.35; text-overflow:ellipsis; white-space:nowrap; }
	.row-actions { position:absolute; top:11px; right:5px; display:none; gap:1px; padding-left:3px; background:var(--bg-raised-2, var(--bg-inset)); border-radius:var(--radius-sm); }
	.item-wrap:hover .row-actions, .item-wrap:focus-within .row-actions { display:flex; }
	.item-wrap:hover .conversation-copy, .item-wrap:focus-within .conversation-copy { padding-right:56px; }
	.row-actions :global(button) { width:26px; min-height:26px; height:26px; padding:0; color:var(--text-faint); }
	.sidebar :global(.rename-input) { min-height:36px; height:36px; font-size:var(--fs-sm); }
	.sidebar-empty { margin:10px 8px; color:var(--text-faint); font-size:var(--fs-xs); line-height:1.5; }
	.folder-row { display:flex; width:100%; min-width:0; align-items:center; gap:0; min-height:39px; padding:5px 9px; border:0; border-radius:10px; background:transparent; color:var(--text-dim); text-align:left; cursor:pointer; }
	.folder-row.active { background:var(--bg-raised-2, var(--bg-inset)); color:var(--text); }
	.folder-row strong { flex:1; min-width:0; overflow:hidden; font-size:var(--fs-sm); font-weight:450; text-overflow:ellipsis; white-space:nowrap; }
	.folder-row small { color:var(--text-faint); font-size:11px; }
	.catalog-warning { display:flex; gap:9px; padding:14px 8px 0; margin:14px 7px 0; border-top:1px solid var(--border); }
	.catalog-warning .warning-dot { width:7px; height:7px; flex:none; margin-top:5px; border-radius:50%; background:var(--warn); }
	.catalog-warning span:last-child { display:grid; gap:3px; }
	.catalog-warning strong { color:var(--text-dim); font-size:var(--fs-xs); font-weight:500; }
	.catalog-warning small { color:var(--text-faint); font-size:11px; line-height:1.45; }
	:global(.report-row) { min-height:50px; }
	.sidebar-footer { display:flex; align-items:center; gap:12px; padding:12px; margin-top:auto; }
	:global(.add-folder-button) { display:flex; flex:1; justify-content:center; min-width:0; min-height:38px; gap:8px; border:1px solid var(--border); border-radius:999px; background:var(--bg-raised); color:var(--text-dim); font-size:var(--fs-xs); }
	:global(.add-folder-button:hover) { background:var(--bg-raised-2, var(--bg-inset)); color:var(--text); }
	:global(.settings-button) { display:grid; width:15px; min-width:15px; height:32px; flex:none; place-items:center; border-radius:999px; color:var(--text-faint); }
	:global(.settings-button:hover), :global(.settings-button.active) { background:var(--bg-raised); color:var(--text); }
	:global(.collapse-btn) { background:transparent !important; width:14px !important; min-width:14px !important; padding:0 !important; }
	.conversation-title-line :global(svg) { flex:none; color:var(--text-faint); }
	.item-wrap:hover .conversation-title-line > :global(svg), .item-wrap:focus-within .conversation-title-line > :global(svg) { opacity:0; }
	.local-profile { display:grid; width:32px; height:32px; flex:none; place-items:center; border-radius:50%; background:var(--brand); color:var(--on-brand); }

	:global(.workspace-delete-dialog) { width:min(420px,calc(100vw - 32px)); display:grid; gap:18px; padding:22px; }
	:global(.conversation-delete-dialog) { width:min(420px,calc(100vw - 32px)); display:grid; gap:18px; padding:22px; }
	:global(.workspace-delete-title) { margin:0; font-size:18px; font-weight:650; }
	:global(.workspace-delete-description) { display:block; margin-top:8px; color:var(--text-dim); font-size:var(--fs-sm); line-height:1.5; }
	.workspace-delete-actions { display:flex; justify-content:flex-end; gap:8px; }
	.workspace-delete-error { color:var(--err); font-size:var(--fs-sm); }
	@media (max-width: 760px) { .sidebar { width:min(268px, calc(100vw - 48px)); min-width:0; } }
</style>
