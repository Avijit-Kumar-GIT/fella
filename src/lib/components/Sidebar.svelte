<script lang="ts">
	import { ipc, isDesktop } from '$lib/ipc';
	import { errMsg, openConversation, openRepository } from '$lib/commands';
	import { GENERAL_WORKSPACE_ID, session } from '$lib/session.svelte';
	import type { StudioDocument } from '$lib/analytics-artifacts';
	import { Button } from '$lib/components/ui/button';
	import { Dialog } from '$lib/components/ui/dialog';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import { Input } from '$lib/components/ui/input';
	import type { ConversationSummary } from '$lib/types';
	import WorkspaceEnvironment from './WorkspaceEnvironment.svelte';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let { onsearch }: { onsearch?: () => void } = $props();

	let sidebarMode = $state<'chat' | 'files' | 'reports'>('chat');
	let workspaceReports = $state<StudioDocument[]>([]);
	let reportsError = $state('');
	let reportsRequest = 0;
	$effect(() => {
		const path = session.catalog.workspace;
		const reportsVersion = session.reportsVersion;
		const mode = sidebarMode;
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
		} catch {
			/* leave the last-known list rather than blanking it on a hiccup */
		}
	}

	// Runs once on mount, then again whenever a conversation gets archived.
	$effect(() => {
		session.historyVersion;
		void refresh();
	});

	/** Keep the question as the primary history label. Workspace identity lives
	 *  in the workspace card and only appears in a row when it disambiguates a
	 *  conversation from the currently open folder. */
	function title(c: ConversationSummary): string {
		return c.title ?? session.investigationNames[c.id] ?? c.preview;
	}


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
		session.newConversation();
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

	async function open(c: ConversationSummary) {
		try {
			await openConversation(c);
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
		<div class="sidebar-brand"><Logo size={18} active={session.busy} /><span>Fella</span></div>
		<div class="header-actions">
			<Button
				variant="ghost"
				size="icon"
				class="icon-btn"
				type="button"
				aria-label="Collapse sidebar"
				title={`Collapse sidebar (${shortcutModifier}+B)`}
				onclick={() => session.toggleSidebar()}
			>
				<Icon name="panel" size={16} />
			</Button>
		</div>
	</div>
	<nav class="sidebar-mode-switch fella-pill-nav" aria-label="Sidebar">
		<Button variant="ghost" class="sidebar-mode-option fella-pill-nav-item" type="button" aria-pressed={sidebarMode === 'chat'} onclick={() => (sidebarMode = 'chat')}>Chat</Button>
		<Button variant="ghost" class="sidebar-mode-option fella-pill-nav-item" type="button" aria-pressed={sidebarMode === 'files'} onclick={() => (sidebarMode = 'files')}>Files</Button>
		<Button variant="ghost" class="sidebar-mode-option fella-pill-nav-item" type="button" aria-pressed={sidebarMode === 'reports'} onclick={() => (sidebarMode = 'reports')}>Reports</Button>
	</nav>
	{#if sidebarMode === 'chat'}
	<section class="repository-section" aria-label="Workspace conversations">
		<div class="section-head">
			<h2>Conversations</h2>
			<div class="section-actions">
				<Button variant="ghost" size="sm" class="section-action new-conversation-action" type="button" aria-label="New conversation" title={`New conversation (${shortcutModifier}+Shift+A)`} aria-keyshortcuts="Control+Shift+A Meta+Shift+A" onclick={newChat}><Icon name="plus" size={14} /><span>New</span></Button>
			{#each repositories.filter((repo) => repo.path === (session.activeWorkspaceId ?? null)) as repo (repo.key)}
				{#if repo.path}
					<DropdownMenu.Root>
						<DropdownMenu.Trigger class="section-action" aria-label="Workspace actions" title="Workspace actions"><Icon name="more-horizontal" size={16} /></DropdownMenu.Trigger>
						<DropdownMenu.Portal>
							<DropdownMenu.Content class="fella-ui-menu-content" side="bottom" align="end" sideOffset={4}>
								{#if repo.historyOnly}<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => void selectRepository(repo)}>Reconnect folder</DropdownMenu.Item>{/if}
								<DropdownMenu.Item class="fella-ui-menu-item workspace-delete-menu-item" onSelect={() => requestWorkspaceDelete(repo)}>Delete workspace and conversations</DropdownMenu.Item>
							</DropdownMenu.Content>
						</DropdownMenu.Portal>
					</DropdownMenu.Root>
				{/if}
			{/each}
			</div>
		</div>
		<div class="repositories">
			{#each list.filter((item) => (item.workspace ?? null) === (session.activeWorkspaceId ?? null)) as c (c.id)}
				<div class="item-wrap">
					{#if renamingId === c.id}
						<Input class="rename-input" bind:ref={renameInput} bind:value={renameValue} onkeydown={(e) => { if (e.key === 'Enter') commitRename(c); else if (e.key === 'Escape') renamingId = null; }} onblur={() => commitRename(c)} aria-label="Rename conversation" />
					{:else}
						<Button variant="ghost" class={session.conversationId === c.id ? 'conversation-row active' : 'conversation-row'} type="button" onclick={() => void open(c)} title={title(c)} aria-label={`Open conversation: ${title(c)}`} aria-current={session.conversationId === c.id ? 'page' : undefined}>
							<span class="preview">{title(c)}</span>
						</Button>
						<div class="row-actions">
							<Button variant="ghost" size="icon" class="ren" type="button" aria-label="Rename conversation" onclick={(e) => startRename(c, e)}><Icon name="pencil" size={14} /></Button>
							<Button variant="ghost" size="icon" class="del" type="button" aria-label="Delete conversation" title="Delete conversation" onclick={(e) => remove(c, e)}><Icon name="x" size={14} /></Button>
						</div>
					{/if}
				</div>
			{:else}
				<p class="sidebar-empty">No conversations yet.</p>
			{/each}
		</div>
	</section>
	{:else if sidebarMode === 'reports'}
		<section class="repository-section" aria-label="Workspace reports">
			<div class="section-head"><h2>Reports</h2><Button variant="ghost" size="icon" class="section-action" aria-label="New report" title="New report" disabled={!session.catalog.workspace} onclick={() => session.openNewReportTab()}><Icon name="plus" size={16} /></Button></div>
			{#if !session.catalog.workspace}
				<p class="sidebar-empty">Open a folder to view its reports.</p>
			{:else if reportsError}
				<p class="sidebar-empty" role="status">{reportsError}</p>
			{:else}
				<div class="repositories">
					{#each workspaceReports as report (report.id)}<Button variant="ghost" class="conversation-row" title={report.title} onclick={() => session.openStudioToolTab(report.id)}><span class="preview">{report.title}</span></Button>{:else}<p class="sidebar-empty">No reports yet.</p>{/each}
				</div>
			{/if}
		</section>
	{:else}
		<WorkspaceEnvironment onopen={(source) => session.openSourceToolTab(source)} />
	{/if}
	<div class="sidebar-footer">
		<Button
			variant="ghost"
			class="nav-row utility-row"
			type="button"
			title={`Search (${shortcutModifier}+K)`}
			aria-label="Search"
			aria-keyshortcuts="Control+K Meta+K"
			aria-haspopup="dialog"
			onclick={() => onsearch?.()}
		>
			<Icon name="search" size={16} />
			<span>Search</span>
		</Button>
		<Button
			variant="ghost"
			class={`nav-row utility-row settings-row${session.workspaceView === 'settings' ? ' active' : ''}`}
			title={`Settings (${shortcutModifier}+,)`}
			aria-label="Settings"
			aria-keyshortcuts="Control+Comma Meta+Comma"
			type="button"
			aria-current={session.workspaceView === 'settings' ? 'page' : undefined}
			onclick={() => session.setWorkspaceView('settings')}
		>
			<Icon name="settings" size={16} />
			<span>Settings</span>
		</Button>
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
				“{conversationDeleteTarget ? title(conversationDeleteTarget) : ''}” and its transcript will be deleted. Saved results, reports, their supporting analysis records, and revisions will remain available in this workspace.
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
	.sidebar { display:flex; flex-direction:column; flex:none; width:244px; min-width:0; height:100%; padding:0 8px 8px; background:var(--sidebar-surface); border-right:1px solid var(--border); }
	.header { display:flex; align-items:center; justify-content:space-between; flex:none; height:44px; padding:0 8px; }
	.sidebar-brand { display:flex; align-items:center; gap:8px; color:var(--text-dim); font-size:var(--fs-sm); font-weight:500; }
	.header-actions :global(.icon-btn), :global(.section-action) { display:grid; place-items:center; width:28px; height:28px; min-height:28px; padding:0; border:0; border-radius:var(--radius-sm); background:transparent; color:var(--text-faint); cursor:pointer; }
	.sidebar :global(.nav-row) { display:flex; justify-content:flex-start; width:100%; min-height:36px; padding:7px 10px; gap:9px; border-radius:var(--radius-sm); color:var(--text-dim); font-size:var(--fs-sm); font-weight:450; }
	.sidebar :global(.nav-row:hover), .sidebar :global(.nav-row.active) { background:var(--sidebar-hover); color:var(--text); }
	.sidebar-mode-switch { flex:none; display:flex; width:100%; gap:2px; margin:2px 0 10px; }
	.sidebar :global(.sidebar-mode-option) { flex:1 1 0; justify-content:center; min-width:0; min-height:28px; height:28px; padding:0 6px; font-size:var(--fs-xs); }
	.repository-section { display:flex; flex:1; flex-direction:column; min-height:0; }
	.section-head { display:flex; align-items:center; justify-content:space-between; min-height:36px; padding:0 4px; }
	.section-actions { display:flex; align-items:center; gap:2px; }
	.sidebar :global(.new-conversation-action) { width:auto; min-height:28px; height:28px; gap:4px; padding:0 8px; border-radius:var(--radius-chip); color:var(--text-dim); font-size:var(--fs-xs); }
	.section-head h2 { margin:0; color:var(--text-faint); font-size:var(--fs-xs); font-weight:550; }
	.repositories { flex:1; min-height:0; overflow-y:auto; padding-top:4px; }
	.item-wrap { position:relative; min-width:0; margin-bottom:2px; }
	.sidebar :global(.conversation-row) { display:flex; justify-content:flex-start; width:100%; min-height:36px; padding:7px 10px; border-radius:var(--radius-sm); color:var(--text-dim); font-size:var(--fs-sm); font-weight:450; }
	.sidebar :global(.conversation-row.active) { background:var(--sidebar-selected, var(--sidebar-hover)); color:var(--text); font-weight:550; }
	.preview { display:block; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
	.row-actions { position:absolute; top:4px; right:4px; display:none; gap:2px; background:var(--sidebar-surface); border-radius:var(--radius-sm); }
	.item-wrap:hover .row-actions, .item-wrap:focus-within .row-actions { display:flex; }
	.item-wrap:hover .preview, .item-wrap:focus-within .preview { padding-right:56px; }
	.row-actions :global(button) { width:28px; min-height:28px; height:28px; padding:0; color:var(--text-faint); }
	.sidebar :global(.rename-input) { min-height:36px; height:36px; font-size:var(--fs-sm); }
	.sidebar-empty { margin:12px 10px; color:var(--text-faint); font-size:var(--fs-xs); }
	.sidebar-footer { display:grid; gap:2px; padding-top:8px; margin-top:auto; }
	:global(.workspace-delete-dialog) { width:min(420px,calc(100vw - 32px)); display:grid; gap:18px; padding:22px; }
	:global(.conversation-delete-dialog) { width:min(420px,calc(100vw - 32px)); display:grid; gap:18px; padding:22px; }
	:global(.workspace-delete-title) { margin:0; font-size:18px; font-weight:650; }
	:global(.workspace-delete-description) { display:block; margin-top:8px; color:var(--text-dim); font-size:var(--fs-sm); line-height:1.5; }
	.workspace-delete-actions { display:flex; justify-content:flex-end; gap:8px; }
	.workspace-delete-error { color:var(--err); font-size:var(--fs-sm); }
</style>
