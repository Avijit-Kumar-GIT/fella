<script lang="ts">
	import { ipc, isDesktop } from '$lib/ipc';
	import { errMsg, openConversation, openFolder, openRepository } from '$lib/commands';
	import { GENERAL_WORKSPACE_ID, session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import { Tooltip } from '$lib/components/ui/tooltip';
	import type { ConversationSummary } from '$lib/types';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let { onsearch, onnewproject }: { onsearch?: () => void; onnewproject?: (workspace?: string | null) => void } = $props();

	type Repository = {
		key: string;
		path: string | null;
		name: string;
		items: ConversationSummary[];
		current: boolean;
		expanded: boolean;
		historyOnly: boolean;
		project: ReturnType<typeof session.projectForWorkspace>;
	};

	let list = $state<ConversationSummary[]>([]);
	let expandedRepos = $state<Record<string, boolean>>({});
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
		return c.title ?? c.preview;
	}

	function repositoryKey(path: string | null | undefined): string {
		return path ?? GENERAL_WORKSPACE_ID;
	}

	function repositoryName(path: string | null): string {
		if (!path) return 'General';
		return path.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') || path;
	}

	let repositories = $derived.by((): Repository[] => {
		const grouped = new Map<string, { path: string | null; items: ConversationSummary[] }>();
		const add = (path: string | null, item?: ConversationSummary) => {
			const key = repositoryKey(path);
			const group = grouped.get(key) ?? { path, items: [] };
			if (item) group.items.push(item);
			grouped.set(key, group);
		};

		for (const path of session.repositoryPaths) add(path);
		if (session.catalog.workspace) add(session.catalog.workspace);
		for (const item of list) {
			if (!item.workspace || !session.hiddenRepositoryPaths.includes(item.workspace)) {
				add(item.workspace, item);
			}
		}
		const order = new Map(session.repositoryPaths.map((path, index) => [path, index]));
		add(null);
		return [...grouped.values()]
			.sort((a, b) => {
				if (a.path === null) return -1;
				if (b.path === null) return 1;
				return (order.get(a.path) ?? Number.MAX_SAFE_INTEGER) - (order.get(b.path) ?? Number.MAX_SAFE_INTEGER);
			})
			.map((group) => {
				const key = repositoryKey(group.path);
				const active = group.path
					? session.activePane?.workspaceId === group.path || (session.workspaceView === 'project' && session.activeProject?.workspace === group.path)
					: session.activePane?.workspaceId === null && session.activeChat?.workspaceScope == null && session.workspaceView !== 'settings';
				return {
					key,
					path: group.path,
					name: repositoryName(group.path),
					items: group.items,
					current: active,
					historyOnly: !!group.path && session.historyOnlyRepositoryPaths.includes(group.path),
					project: group.path ? session.projectForWorkspace(group.path) : null,
					expanded: expandedRepos[key] ?? (active || group.path === null)
				};
			});
	});

	function newChat(): void {
		if (!session.newConversation()) {
			session.addSystem('This environment already has four workspaces. Create or switch environments to add another.');
		}
	}

	async function newWorkspaceChat(repo: Repository): Promise<void> {
		if (!repo.path) {
			if (!session.newConversation(null)) session.addSystem('Could not start a new conversation.');
			return;
		}
		if (!(await selectRepository(repo))) return;
		if (!session.newConversation(repo.path)) session.addSystem('Open the workspace before starting a conversation in it.');
	}

	async function selectRepository(repo: Repository): Promise<boolean> {
		if (!repo.path) {
			const selected = session.focusWorkspace(GENERAL_WORKSPACE_ID);
			if (!selected) session.addSystem('This environment already has four workspaces. Create or switch environments to add General.');
			return selected;
		}
		const workspace = session.workspaceAt(repo.path);
		if (workspace && !workspace.historyOnly) return session.focusWorkspace(workspace.id);
		return openRepository(repo.path, { reportFailure: true });
	}

	async function toggleRepository(repo: Repository): Promise<void> {
		await selectRepository(repo);
	}

	function toggleExpansion(repo: Repository, event: MouseEvent): void {
		event.stopPropagation();
		expandedRepos = { ...expandedRepos, [repo.key]: !repo.expanded };
	}

	async function openRepositoryWorkspace(repo: Repository): Promise<void> {
		expandedRepos = { ...expandedRepos, [repo.key]: true };
		if (!(await selectRepository(repo))) return;
		if (!repo.path) return;
		session.setWorkspacePane('sources');
	}

	function beginRepositoryDrag(event: DragEvent, repo: Repository): void {
		if (!repo.path || !event.dataTransfer) return;
		const alreadyOpen = !!session.paneForWorkspace(repo.path);
		event.dataTransfer.effectAllowed = alreadyOpen ? 'move' : 'copy';
		event.dataTransfer.setData('application/x-fella-workspace', repo.path);
		if (alreadyOpen) event.dataTransfer.setData('application/x-fella-existing-workspace', repo.path);
	}

	async function addRepository(): Promise<void> {
		await openFolder();
	}

	function hideRepository(repo: Repository): void {
		if (repo.path && repo.path !== session.catalog.workspace) session.forgetRepository(repo.path);
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
			expandedRepos = { ...expandedRepos, [repositoryKey(c.workspace)]: true };
			await openConversation(c);
		} catch (e) {
			session.addSystem(`error: ${errMsg(e)}`);
		}
	}

	async function remove(c: ConversationSummary, e: MouseEvent) {
		e.stopPropagation();
		try {
			await ipc.deleteConversation(c.id);
			list = list.filter((x) => x.id !== c.id);
			// If this conversation is still live, remove it too --
			// otherwise its very next settle re-archives it, undoing the delete.
			session.removeConversationWithoutArchiving(c.id);
		} catch (err) {
			session.addSystem(`error: ${errMsg(err)}`);
		}
	}
</script>

<aside class="sidebar">
	<div class="header">
		<span class="logo"><Logo size={18} active={session.busy} /></span>
		<div class="header-actions">
			<button
				class="icon-btn"
				type="button"
				aria-label="Collapse sidebar"
				title={`Collapse sidebar (${shortcutModifier}+B)`}
				onclick={() => session.toggleSidebar()}
			>
				<Icon name="panel" size={16} />
			</button>
		</div>
	</div>
	<nav class="nav-section" aria-label="General">
		<div class="nav-heading">General</div>
		<Button
			variant="ghost"
			class="nav-row"
			title={`New conversation (${shortcutModifier}+Shift+A)`}
			aria-label="New conversation"
			aria-keyshortcuts="Control+Shift+A Meta+Shift+A"
			type="button"
			onclick={newChat}
		>
			<Icon name="ask" size={16} />
			<span>Ask</span>
		</Button>
		<Button
			variant="ghost"
			class="nav-row"
			type="button"
			title={`Search (${shortcutModifier}+K)`}
			aria-keyshortcuts="Control+K Meta+K"
			aria-haspopup="dialog"
			onclick={() => onsearch?.()}
		>
			<Icon name="search" size={16} />
			<span>Search</span>
		</Button>
	</nav>
	<section class="repository-section" aria-labelledby="repositories-heading">
		<div class="section-head">
			<div class="nav-heading" id="repositories-heading">Workspaces</div>
			<button
				class="section-action"
				type="button"
				aria-label="Add repository"
				title={`Add repository (${shortcutModifier}+O)`}
				onclick={() => void addRepository()}
			>
				<Icon name="plus" size={16} />
			</button>
		</div>
		<div class="repositories">
			{#each repositories as repo (repo.key)}
				<div
					class="repository"
					class:current={repo.current}
					class:expanded={repo.expanded}
				>
					<div class="repository-row-wrap">
						<button
							class="repository-disclosure"
							type="button"
							aria-label={`${repo.expanded ? 'Collapse' : 'Expand'} ${repo.name}`}
							aria-expanded={repo.expanded}
							title={`${repo.expanded ? 'Collapse' : 'Expand'} workspace contents`}
							onclick={(event) => toggleExpansion(repo, event)}
						>
							<span class="row-slot row-chevron"><Icon name="chevron-right" size={12} /></span>
						</button>
						<button
							class="repository-row"
							type="button"
							draggable={!!repo.path}
							title={repo.path ?? 'General · conversations without a mounted folder'}
							aria-pressed={repo.current}
							aria-label={repo.path ? `Open workspace ${repo.name}` : 'General'}
							ondragstart={(event) => beginRepositoryDrag(event, repo)}
							onclick={() => toggleRepository(repo)}
						>
							<span class="row-slot row-icon"><Icon name={repo.path ? 'repository' : 'ask'} size={16} solid={repo.current} /></span>
							<span class="repository-copy">
								<span class="repository-name">{repo.name}</span>
								{#if repo.historyOnly}<span class="history-badge" role="status" aria-label="History only. The folder could not be opened; saved conversations remain available." title="Saved conversations are available; the folder is offline">History</span>{/if}
							</span>
						</button>
						<div class="repository-actions">
							{#if !repo.historyOnly}
								<Tooltip.Root>
									<Tooltip.Trigger
										class="repository-action"
										type="button"
										aria-label={`New conversation in ${repo.name}`}
										onclick={(event) => { event.stopPropagation(); void newWorkspaceChat(repo); }}
									>
										<Icon name="plus" size={14} />
									</Tooltip.Trigger>
									<Tooltip.Portal>
										<Tooltip.Content class="fella-ui-tooltip-content" side="right" sideOffset={8}>
											New conversation in {repo.name}
										</Tooltip.Content>
									</Tooltip.Portal>
								</Tooltip.Root>
							{/if}
							{#if repo.path && !repo.current && !repo.historyOnly}
								<DropdownMenu.Root>
									<DropdownMenu.Trigger class="repository-action" type="button" aria-label="Repository actions" title="Repository actions">
										<Icon name="more-horizontal" size={14} />
									</DropdownMenu.Trigger>
									<DropdownMenu.Portal>
										<DropdownMenu.Content class="fella-ui-menu-content" side="bottom" align="end" sideOffset={4}>
											<DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => hideRepository(repo)}>
												Hide repository
											</DropdownMenu.Item>
										</DropdownMenu.Content>
									</DropdownMenu.Portal>
								</DropdownMenu.Root>
							{/if}
							{#if repo.path && repo.historyOnly}
								<button
									class="repository-action reconnect-action"
									type="button"
									aria-label={`Reconnect ${repo.name}`}
									title="Try the saved folder location again"
									onclick={(event) => { event.stopPropagation(); void selectRepository(repo); }}
								>
									<Icon name="refresh" size={14} />
								</button>
							{/if}
						</div>
					</div>
					{#if repo.expanded}
						<div class="repository-contents">
							{#if repo.path && !repo.historyOnly}
								<div class="repository-tools" aria-label={`${repo.name} tools`}>
									<button
										class="repository-tool"
										class:active={repo.current && session.workspaceView === 'workspace'}
										type="button"
										aria-label={`Open sources in ${repo.name}`}
										aria-current={repo.current && session.workspaceView === 'workspace' ? 'page' : undefined}
										title={`Open sources (${shortcutModifier}+Shift+S)`}
										onclick={() => void openRepositoryWorkspace(repo)}
									>
										<span class="row-slot row-icon"><Icon name="table" size={16} /></span>
										<span>Sources</span>
									</button>
								</div>
							{/if}
							{#if repo.path}
								<div class="repository-tools" aria-label={`${repo.name} project`}>
									{#if repo.project}
										<button
											class="repository-tool"
											class:active={session.workspaceView === 'project' && session.activeProjectId === repo.project.id}
											type="button"
											title={repo.project.workspace}
											aria-label={`Open project ${repo.project.name}`}
											onclick={() => session.openProject(repo.project!.id)}
										>
											<span class="row-slot row-icon"><Icon name="project" size={16} solid={session.workspaceView === 'project' && session.activeProjectId === repo.project.id} /></span>
											<span>{repo.project.name}</span>
										</button>
									{:else}
										<button
											class="repository-tool project-create"
											type="button"
											aria-label={`Create project for ${repo.name}`}
											onclick={() => onnewproject?.(repo.path)}
										>
											<span class="row-slot row-icon"><Icon name="plus" size={16} /></span>
											<span>Add project</span>
										</button>
									{/if}
								</div>
							{/if}
							{#each repo.items as c (c.id)}
								<div class="item-wrap">
									{#if renamingId === c.id}
										<input
											class="rename-input"
											bind:this={renameInput}
											bind:value={renameValue}
											onkeydown={(e) => {
												if (e.key === 'Enter') commitRename(c);
												else if (e.key === 'Escape') renamingId = null;
											}}
											onblur={() => commitRename(c)}
											aria-label="Rename conversation"
										/>
									{:else}
										<button
											class="rowbtn item"
											class:active={session.conversationId === c.id}
											type="button"
											onclick={() => void open(c)}
											title={title(c)}
											aria-label={`Open conversation: ${title(c)}`}
											aria-current={session.conversationId === c.id ? 'page' : undefined}
										>
											<span class="row-slot row-icon row-placeholder" aria-hidden="true"></span>
											<span class="preview">{title(c)}</span>
										</button>
										<div class="row-actions">
											<button class="ren" type="button" aria-label="Rename conversation" title="Rename" onclick={(e) => startRename(c, e)}>
												<Icon name="pencil" size={14} />
											</button>
											<button class="del" type="button" aria-label="Delete conversation" title="Delete" onclick={(e) => remove(c, e)}>
												<Icon name="x" size={14} />
											</button>
										</div>
									{/if}
								</div>
							{/each}
						</div>
					{/if}
				</div>
			{/each}
			{#if repositories.length === 0}
				<button class="add-repository" type="button" onclick={() => void addRepository()}>
					<span class="row-slot row-icon"><Icon name="folder" size={16} /></span> Add a repository
				</button>
			{/if}
		</div>
	</section>
	<div class="sidebar-footer">
		<Button
			variant="ghost"
			class={`nav-row settings-row${session.workspaceView === 'settings' ? ' active' : ''}`}
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

<style>
	.sidebar {
		flex: none;
		width: 240px;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: 0 var(--space-2) var(--space-2);
		--sidebar-hover: color-mix(in srgb, var(--text) 4%, var(--sidebar-surface));
		--sidebar-selected: color-mix(in srgb, var(--text) 7%, var(--sidebar-surface));
		/* No top padding: .header is 42px flush against the top edge, to
		   match the titlebar's height exactly across the sidebar seam. */
		background: var(--sidebar-surface);
		border-right: 1px solid var(--pane-edge);
		overflow: hidden;
	}
	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex: none;
		height: 42px;
		padding: 0 var(--space-2);
	}
	.logo {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
	}
	.header-actions {
		display: flex;
		align-items: center;
		gap: var(--space-1);
	}
	.icon-btn {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.icon-btn:hover {
		color: var(--text);
		background: var(--sidebar-hover);
	}
	.nav-section {
		display: grid;
		gap: 1px;
		padding: var(--space-3) var(--space-1) var(--space-2);
	}
	.repository-section {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding: var(--space-2) var(--space-1);
	}
	.section-head {
		display: flex;
		align-items: center;
		min-height: 24px;
	}
	.section-head .nav-heading {
		flex: 1;
		padding: 0 var(--space-2);
		line-height: 24px;
	}
	.section-action {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.section-action:hover {
		background: var(--sidebar-hover);
		color: var(--text);
	}
	.repositories {
		flex: 1;
		min-height: 0;
		display: grid;
		align-content: start;
		grid-auto-rows: max-content;
		gap: 1px;
		overflow-y: auto;
	}
	.repository {
		min-width: 0;
	}
	.repository + .repository { margin-top: 3px; }
	.repository-row-wrap {
		position: relative;
	}
	.repository-disclosure {
		position: absolute;
		z-index: 1;
		top: 4px;
		left: 4px;
		display: grid;
		place-items: center;
		width: 18px;
		height: 20px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.repository-disclosure:hover { background: var(--sidebar-hover); color: var(--text); }
	.repository-row {
		position: relative;
		width: 100%;
		display: grid;
		grid-template-columns: 16px minmax(0, 1fr);
		align-items: center;
		gap: 6px;
		min-width: 0;
		min-height: 28px;
		padding: 4px 32px 4px 28px;
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		text-align: left;
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.repository-row:hover {
		background: var(--sidebar-hover);
		color: var(--text);
	}
	.repository.current .repository-row {
		background: var(--sidebar-selected);
		color: var(--text);
	}
	.row-slot {
		display: grid;
		place-items: center;
		width: 16px;
		height: 20px;
		flex: none;
		color: var(--text-faint);
	}
	.row-slot :global(svg) {
		display: block;
	}
	.row-chevron {
		transition: color var(--dur-fast) var(--ease);
	}
	.repository-row:hover .row-slot,
	.repository-row:focus-visible .row-slot,
	.repository-disclosure:focus-visible .row-slot {
		color: var(--text-dim);
	}
	.repository.expanded .row-chevron :global(svg) {
		transform: rotate(90deg);
		transition: transform var(--dur-fast) var(--ease);
	}
	.repository.current .row-chevron {
		color: var(--text-dim);
	}
	.repository.current .row-icon {
		color: var(--brand-icon);
	}
	.repository-copy {
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 6px;
		overflow: hidden;
	}
	.repository-name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-dim);
		font-size: var(--fs-sm);
		font-weight: 550;
	}
	.history-badge {
		flex: none;
		padding: 1px 5px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--text-faint) 10%, transparent);
		color: var(--text-faint);
		font-size: 9px;
		font-weight: 550;
		letter-spacing: .01em;
		line-height: 1.4;
	}
	.repository-row:hover .repository-name,
	.repository-row:focus-visible .repository-name {
		color: var(--text);
	}
	.repository-actions {
		position: absolute;
		top: 3px;
		right: 4px;
		display: none;
		align-items: center;
		gap: 2px;
		padding-left: 4px;
		background: transparent;
		color: var(--text-faint);
	}
	.repository:hover .repository-actions,
	.repository:focus-within .repository-actions {
		display: flex;
		color: var(--text);
	}
	.repository-action {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border-radius: var(--radius-chip);
		color: inherit;
	}
	.repository-action:hover {
		background: var(--sidebar-hover);
		color: inherit;
	}
	.repository-action :global(svg) {
		flex: none;
		color: inherit;
	}
	.repository-contents {
		margin: 2px 4px 5px 4px;
		padding: 3px 5px 4px 25px;
		border-left: 1px solid var(--pane-edge);
		border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
		background: color-mix(in srgb, var(--sidebar-surface) 62%, var(--workspace-canvas));
	}
	.repository-tools {
		display: grid;
		padding: 1px 0 2px;
	}
	.repository-tool {
		display: grid;
		grid-template-columns: 16px minmax(0, 1fr);
		align-items: center;
		gap: 6px;
		width: 100%;
		min-height: 27px;
		padding: 4px 4px 4px 0;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		text-align: left;
	}
	.repository-tool:hover {
		background: var(--sidebar-hover);
		color: var(--text);
	}
	.repository-tool.active {
		background: var(--sidebar-selected);
		color: var(--text);
	}
	.add-repository {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px var(--space-2);
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		font-size: var(--fs-sm);
		text-align: left;
	}
	.add-repository:hover {
		background: var(--sidebar-hover);
		color: var(--text);
	}
	.nav-heading {
		padding: 0 var(--space-2) var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.sidebar :global(.nav-row) {
		position: relative;
		width: 100%;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		height: auto;
		min-height: 28px;
		padding: 4px var(--space-2);
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		font-size: var(--fs-sm);
		text-align: left;
		white-space: nowrap;
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.sidebar :global(.nav-row:hover:not(:disabled)) {
		background: var(--sidebar-hover);
		color: var(--text);
	}
	.sidebar :global(.nav-row.active) {
		background: var(--sidebar-selected);
		color: var(--text);
		font-weight: 500;
	}
	.sidebar :global(.nav-row:disabled) {
		color: var(--border-strong);
		cursor: default;
	}
	.item-wrap {
		position: relative;
	}
	.item {
		position: relative;
		display: grid;
		grid-template-columns: 16px minmax(0, 1fr);
		align-items: center;
		gap: 6px;
		width: 100%;
		min-height: 27px;
		padding-left: 0;
		padding-right: 48px;
		padding-top: 4px;
		padding-bottom: 4px;
		border-radius: var(--radius-sm);
	}
	.item:hover,
	.item:focus-visible {
		background: var(--sidebar-hover);
	}
	.item.active {
		background: var(--sidebar-selected);
		color: var(--text);
	}
	.item-wrap:focus-within .row-actions,
	.item-wrap:hover .row-actions {
		display: flex;
	}
	.preview {
		min-width: 0;
		color: var(--text-dim);
		font-size: var(--fs-sm);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.item.active .preview {
		color: var(--text);
	}
	.rename-input {
		width: 100%;
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--border-strong);
		border-radius: var(--radius-chip);
		background: var(--bg-raised);
		color: var(--text);
		font: inherit;
		font-size: var(--fs-sm);
		outline: none;
	}
	.row-actions {
		position: absolute;
		top: 5px;
		right: 6px;
		display: none;
		align-items: center;
		gap: 2px;
	}
	.ren,
	.del {
		display: grid;
		place-items: center;
		width: 18px;
		height: 18px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.del {
		color: var(--err);
	}
	.ren:hover,
	.del:hover {
		color: var(--text);
		background: var(--sidebar-hover);
	}
	.sidebar-footer {
		flex: none;
		display: flex;
		justify-content: flex-start;
		padding: var(--space-1) var(--space-1) 0;
	}
</style>
