<script lang="ts">
	import { ipc, isTauri } from '$lib/ipc';
	import { errMsg, openConversation, openFolder } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { ConversationSummary } from '$lib/types';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let { onsearch, onnewproject }: { onsearch?: () => void; onnewproject?: () => void } = $props();

	type Repository = {
		key: string;
		path: string | null;
		name: string;
		items: ConversationSummary[];
		active: boolean;
		expanded: boolean;
	};

	const LOCAL_REPOSITORY = '__no-repository__';
	let list = $state<ConversationSummary[]>([]);
	let expandedRepos = $state<Record<string, boolean>>({});
	let draggedRepoKey = $state<string | null>(null);
	let dropRepoKey = $state<string | null>(null);
	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent)
			? '⌘'
			: 'Ctrl';

	async function refresh() {
		if (!isTauri()) return;
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
		return path || LOCAL_REPOSITORY;
	}

	function repositoryName(path: string | null): string {
		if (!path) return 'No repository';
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
			if (!item.workspace) add(null, item);
		}

		const current = session.catalog.workspace;
		const order = new Map(session.repositoryPaths.map((path, index) => [path, index]));
		return [...grouped.values()]
			.sort((a, b) => {
				if (a.path === null) return 1;
				if (b.path === null) return -1;
				return (order.get(a.path) ?? Number.MAX_SAFE_INTEGER) - (order.get(b.path) ?? Number.MAX_SAFE_INTEGER);
			})
			.map((group) => {
				const key = repositoryKey(group.path);
				const active = group.path !== null && group.path === current;
				return {
					key,
					path: group.path,
					name: repositoryName(group.path),
					items: group.items,
					active,
					expanded: expandedRepos[key] ?? active
				};
			});
	});

	function toggleRepository(repo: Repository): void {
		expandedRepos = { ...expandedRepos, [repo.key]: !repo.expanded };
		if (repo.expanded || !repo.path) return;
		session.setWorkspaceView('ask');
		if (repo.path !== session.catalog.workspace) void openFolder(repo.path);
	}

	async function openRepositoryPane(repo: Repository, pane: 'sources' | 'context'): Promise<void> {
		expandedRepos = { ...expandedRepos, [repo.key]: true };
		if (repo.path && repo.path !== session.catalog.workspace) await openFolder(repo.path);
		session.setWorkspacePane(pane);
	}

	async function newConversation(repo: Repository): Promise<void> {
		if (repo.path && repo.path !== session.catalog.workspace) await openFolder(repo.path);
		session.setWorkspaceView('ask');
		session.newTab();
	}

	async function addRepository(): Promise<void> {
		session.setWorkspaceView('ask');
		await openFolder();
	}

	function startRepositoryDrag(repo: Repository, event: DragEvent): void {
		if (!repo.path) return;
		draggedRepoKey = repo.key;
		event.dataTransfer?.setData('text/plain', repo.key);
		if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
	}

	function dragOverRepository(repo: Repository, event: DragEvent): void {
		if (!draggedRepoKey || !repo.path || draggedRepoKey === repo.key) return;
		event.preventDefault();
		dropRepoKey = repo.key;
		if (event.dataTransfer) event.dataTransfer.dropEffect = 'move';
	}

	function dropRepository(repo: Repository, event: DragEvent): void {
		event.preventDefault();
		const dragged = repositories.find((item) => item.key === draggedRepoKey);
		if (dragged?.path && repo.path) session.reorderRepositories(dragged.path, repo.path);
		draggedRepoKey = null;
		dropRepoKey = null;
	}

	function endRepositoryDrag(): void {
		draggedRepoKey = null;
		dropRepoKey = null;
	}

	function hideRepository(repo: Repository, event: MouseEvent): void {
		event.stopPropagation();
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
			if (isTauri()) list = await ipc.conversationsList();
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
			// If this conversation is still open as a live tab, close it too --
			// otherwise its very next settle re-archives it, undoing the delete.
			session.removeTabWithoutArchiving(c.id);
		} catch (err) {
			session.addSystem(`error: ${errMsg(err)}`);
		}
	}
</script>

<aside class="sidebar">
	<div class="header">
		<span class="logo"><Logo size={18} /></span>
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
			<button
				class="icon-btn"
				type="button"
				aria-label="New conversation"
				title={`New conversation (${shortcutModifier}+N)`}
				onclick={() => {
					session.setWorkspaceView('ask');
					session.newTab();
				}}
			>
				<Icon name="plus" size={16} />
			</button>
		</div>
	</div>
	<nav class="nav-section" aria-label="General">
		<div class="nav-heading">General</div>
		<button
			class="nav-row"
			title={`Ask (${shortcutModifier}+Shift+A)`}
			aria-keyshortcuts="Control+Shift+A Meta+Shift+A"
			class:active={session.workspaceView === 'ask'}
			type="button"
			aria-current={session.workspaceView === 'ask' ? 'page' : undefined}
			onclick={() => session.setWorkspaceView('ask')}
		>
			<Icon name="compose" size={16} />
			<span>Ask</span>
		</button>
		<button
			class="nav-row"
			type="button"
			title={`Search (${shortcutModifier}+K)`}
			aria-keyshortcuts="Control+K Meta+K"
			aria-haspopup="dialog"
			onclick={() => onsearch?.()}
		>
			<Icon name="search" size={16} />
			<span>Search</span>
		</button>
	</nav>
	<section class="repository-section" aria-labelledby="repositories-heading">
		<div class="section-head">
			<div class="nav-heading" id="repositories-heading">Repositories</div>
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
					class:active={repo.active}
					class:expanded={repo.expanded}
					class:dragging={draggedRepoKey === repo.key}
					class:drop-target={dropRepoKey === repo.key}
					role="listitem"
					draggable={repo.path ? 'true' : undefined}
					ondragstart={(event) => startRepositoryDrag(repo, event)}
					ondragover={(event) => dragOverRepository(repo, event)}
					ondrop={(event) => dropRepository(repo, event)}
					ondragend={endRepositoryDrag}
				>
					<div class="repository-row-wrap">
						<button
							class="repository-row"
							type="button"
							title={repo.path ?? 'No repository'}
							aria-expanded={repo.expanded}
							aria-current={repo.active ? 'true' : undefined}
							onclick={() => toggleRepository(repo)}
						>
							<Icon name="chevron-right" size={12} />
							<span class="repository-icon"><Icon name="folder" size={16} /></span>
							<span class="repository-copy">{repo.name}</span>
						</button>
						{#if repo.path}
							<div class="repository-actions">
								<button
									class="repository-action"
									type="button"
									aria-label="New conversation in repository"
									title="New conversation in repository"
									onclick={(event) => {
										event.stopPropagation();
										void newConversation(repo);
									}}
								>
									<Icon name="plus" size={12} />
								</button>
								{#if !repo.active}
									<button
										class="repository-action"
										type="button"
										aria-label="Hide repository"
										title="Hide repository"
										onclick={(event) => hideRepository(repo, event)}
									>
										<Icon name="x" size={12} />
									</button>
								{/if}
							</div>
						{/if}
					</div>
					{#if repo.expanded}
						<div class="repository-contents">
							{#if repo.path}
								<div class="repository-tools" aria-label={`${repo.name} tools`}>
									<button
										class="repository-tool"
										class:active={repo.active && session.workspaceView === 'workspace' && session.workspacePane === 'sources'}
										type="button"
										aria-label="Sources"
										title={`Sources (${shortcutModifier}+Shift+S)`}
										onclick={() => void openRepositoryPane(repo, 'sources')}
									>
										<Icon name="table" size={16} />
									</button>
									<button
										class="repository-tool"
										class:active={repo.active && session.workspaceView === 'workspace' && session.workspacePane === 'context'}
										type="button"
										aria-label="Context"
										title={`Context (${shortcutModifier}+Shift+C)`}
										onclick={() => void openRepositoryPane(repo, 'context')}
									>
										<Icon name="file" size={16} />
									</button>
									<button
										class="repository-tool"
										type="button"
										aria-label="New conversation"
										title="New conversation"
										onclick={() => void newConversation(repo)}
									>
										<Icon name="plus" size={16} />
									</button>
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
											<span class="preview">{title(c)}</span>
										</button>
										<div class="row-actions">
											<button class="ren" type="button" aria-label="Rename conversation" title="Rename" onclick={(e) => startRename(c, e)}>
												<Icon name="pencil" size={12} />
											</button>
											<button class="del" type="button" aria-label="Delete conversation" title="Delete" onclick={(e) => remove(c, e)}>
												<Icon name="x" size={12} />
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
					<Icon name="folder" size={16} /> Add a repository
				</button>
			{/if}
		</div>
	</section>
	<section class="projects-section" aria-labelledby="projects-heading">
		<div class="section-head">
			<div class="nav-heading" id="projects-heading">Projects</div>
			<button
				class="section-action"
				type="button"
				aria-label="New project"
				title="New project"
				onclick={() => onnewproject?.()}
			>
				<Icon name="plus" size={16} />
			</button>
		</div>
		<div class="project-list">
			{#each session.projects as project (project.id)}
				<button
					class="project-row"
					class:active={session.workspaceView === 'project' && session.activeProjectId === project.id}
					type="button"
					title={project.workspace}
					onclick={() => session.openProject(project.id)}
				>
					<Icon name="bookmark" size={16} />
					<span>{project.name}</span>
				</button>
			{/each}
		</div>
	</section>
	<div class="sidebar-footer">
		<button
			class="icon-btn settings-btn"
			title={`Settings (${shortcutModifier}+,)`}
			aria-label="Settings"
			aria-keyshortcuts="Control+Comma Meta+Comma"
			class:active={session.workspaceView === 'settings'}
			type="button"
			aria-current={session.workspaceView === 'settings' ? 'page' : undefined}
			onclick={() => session.setWorkspaceView('settings')}
		>
			<Icon name="settings" size={16} />
		</button>
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
		/* No top padding: .header is 38px flush against the top edge, to
		   match the titlebar's height exactly across the sidebar seam. */
		background: var(--bg);
		border-right: 1px solid var(--border);
		overflow: hidden;
	}
	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex: none;
		height: 38px;
		padding: 0 var(--space-2);
	}
	.logo {
		display: flex;
		align-items: center;
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
		background: var(--bg-inset);
	}
	.nav-section {
		display: grid;
		gap: 1px;
		padding: var(--space-3) var(--space-1) var(--space-2);
	}
	.projects-section {
		flex: none;
		display: grid;
		gap: 1px;
		padding: var(--space-1) var(--space-1) 0;
	}
	.repository-section {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding: var(--space-2) var(--space-1);
	}
	.projects-section {
		min-height: 24px;
	}
	.section-head {
		display: flex;
		align-items: center;
		min-height: 20px;
	}
	.section-head .nav-heading {
		flex: 1;
	}
	.section-action {
		display: grid;
		place-items: center;
		width: 20px;
		height: 20px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.section-action:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.repositories {
		flex: 1;
		min-height: 0;
		display: grid;
		gap: 1px;
		overflow-y: auto;
	}
	.repository {
		min-width: 0;
	}
	.repository-row-wrap {
		position: relative;
	}
	.repository-row {
		position: relative;
		width: 100%;
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		min-height: 28px;
		padding: 4px 52px 4px var(--space-2);
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		cursor: grab;
		text-align: left;
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.repository-row:active {
		cursor: grabbing;
	}
	.repository-row:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.repository.active .repository-row {
		color: var(--text);
		font-weight: 560;
	}
	.repository.active .repository-row::before {
		content: '';
		position: absolute;
		left: 0;
		top: 7px;
		bottom: 7px;
		width: 2px;
		border-radius: 1px;
		background: var(--brand);
	}
	.repository-row > :global(svg:first-child) {
		flex: none;
		color: var(--text-faint);
		transition: transform var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.repository.expanded .repository-row > :global(svg:first-child) {
		transform: rotate(90deg);
	}
	.repository.active .repository-row > :global(svg:first-child),
	.repository.active .repository-icon {
		color: var(--brand);
	}
	.repository-actions {
		position: absolute;
		top: 2px;
		right: 4px;
		display: none;
		align-items: center;
		gap: 1px;
		padding-left: 5px;
		background: var(--bg);
	}
	.repository:hover .repository-actions,
	.repository:focus-within .repository-actions {
		display: flex;
	}
	.repository-action {
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.repository-action:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.repository.dragging {
		opacity: 0.45;
	}
	.repository.drop-target > .repository-row-wrap {
		box-shadow: inset 0 -2px 0 var(--brand);
	}
	.repository-icon {
		display: grid;
		place-items: center;
		flex: none;
		color: var(--text-faint);
	}
	.repository-copy {
		min-width: 0;
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--fs-sm);
	}
	.repository-contents {
		padding: 0 0 2px 22px;
	}
	.repository-tools {
		display: flex;
		gap: 1px;
		padding: 1px 0 2px;
	}
	.repository-tool {
		display: grid;
		place-items: center;
		width: 24px;
		height: 22px;
		flex: none;
		padding: 0;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.repository-tool:hover,
	.repository-tool.active {
		background: var(--bg-inset);
		color: var(--text);
	}
	.repository-tool :global(svg) {
		color: var(--brand);
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
		background: var(--bg-inset);
		color: var(--text);
	}
	.add-repository :global(svg) {
		color: var(--brand);
	}
	.nav-heading {
		padding: 0 var(--space-2) var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.nav-row {
		position: relative;
		width: 100%;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: 28px;
		padding: 4px var(--space-2);
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		font-size: var(--fs-sm);
		text-align: left;
		white-space: nowrap;
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.nav-row:hover:not(:disabled) {
		background: var(--bg-inset);
		color: var(--text);
	}
	.nav-row.active {
		color: var(--text);
		font-weight: 620;
	}
	.nav-row.active::before {
		content: '';
		position: absolute;
		left: 0;
		top: 7px;
		bottom: 7px;
		width: 2px;
		border-radius: 2px;
		background: var(--brand);
	}
	.nav-row.active :global(svg) {
		color: var(--brand);
	}
	.nav-row:disabled {
		color: var(--border-strong);
		cursor: default;
	}
	.item-wrap {
		position: relative;
	}
	.item {
		position: relative;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		width: 100%;
		min-height: 27px;
		padding-top: 4px;
		padding-bottom: 4px;
		border-radius: var(--radius-sm);
	}
	.item:hover,
	.item:focus-visible {
		background: var(--bg-inset);
	}
	.item.active {
		color: var(--text);
		font-weight: 560;
	}
	.item.active::before {
		content: '';
		position: absolute;
		left: 0;
		top: 7px;
		bottom: 7px;
		width: 2px;
		border-radius: 1px;
		background: var(--brand);
	}
	.item-wrap:focus-within .row-actions,
	.item-wrap:hover .row-actions {
		display: flex;
	}
	.preview {
		flex: 1;
		min-width: 0;
		color: var(--text);
		font-size: var(--fs-sm);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.project-list {
		display: grid;
		gap: 1px;
		max-height: 144px;
		overflow-y: auto;
	}
	.project-row {
		position: relative;
		width: 100%;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
		min-height: 28px;
		padding: 4px var(--space-2);
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		font-size: var(--fs-sm);
		text-align: left;
	}
	.project-row:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.project-row.active {
		color: var(--text);
		font-weight: 560;
	}
	.project-row.active::before {
		content: '';
		position: absolute;
		left: 0;
		top: 7px;
		bottom: 7px;
		width: 2px;
		border-radius: 1px;
		background: var(--brand);
	}
	.project-row :global(svg) {
		flex: none;
		color: var(--text-faint);
	}
	.project-row.active :global(svg) {
		color: var(--brand);
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
		background: var(--bg-inset);
	}
	.sidebar-footer {
		flex: none;
		display: flex;
		justify-content: flex-end;
		padding: var(--space-2) var(--space-1) 0;
		border-top: 1px solid var(--border);
	}
	.settings-btn {
		flex: none;
		width: 30px;
		height: 30px;
	}
	.settings-btn.active {
		background: var(--bg-raised);
		color: var(--text);
	}
	.settings-btn.active :global(svg) {
		color: var(--brand);
	}
</style>
