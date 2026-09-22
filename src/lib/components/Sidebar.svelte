<script lang="ts">
	import { ipc, isTauri } from '$lib/ipc';
	import { errMsg, openConversation, openFolder, relativeAge } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { ConversationSummary } from '$lib/types';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let { onsearch }: { onsearch?: () => void } = $props();

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
	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent)
			? '⌘'
			: 'Ctrl';

	async function refresh() {
		if (!isTauri()) return;
		try {
			list = await ipc.conversationsList();
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

		if (session.catalog.workspace) add(session.catalog.workspace);
		for (const item of list) add(item.workspace, item);

		const current = session.catalog.workspace;
		return [...grouped.values()]
			.sort((a, b) => {
				if (a.path === current && b.path !== current) return -1;
				if (b.path === current && a.path !== current) return 1;
				return (b.items[0]?.saved_at_ms ?? 0) - (a.items[0]?.saved_at_ms ?? 0);
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
				<Icon name="panel" size={15} />
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
				<Icon name="plus" size={15} />
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
			<Icon name="compose" size={14} />
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
			<Icon name="search" size={14} />
			<span>Search</span>
			<kbd>{shortcutModifier}K</kbd>
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
				<Icon name="plus" size={14} />
			</button>
		</div>
		<div class="repositories">
			{#each repositories as repo (repo.key)}
				<div class="repository" class:active={repo.active} class:expanded={repo.expanded}>
					<button
						class="repository-row"
						type="button"
						aria-expanded={repo.expanded}
						aria-current={repo.active ? 'true' : undefined}
						onclick={() => toggleRepository(repo)}
					>
						<Icon name="chevron-right" size={12} />
						<span class="repository-icon"><Icon name="folder" size={14} /></span>
						<span class="repository-copy">
							<strong>{repo.name}</strong>
							<small>
								{repo.items.length
									? `${repo.items.length} conversation${repo.items.length === 1 ? '' : 's'}`
									: repo.active
										? 'Mounted now'
										: 'No conversations yet'}
							</small>
						</span>
					</button>
					{#if repo.expanded}
						<div class="repository-contents">
							{#if repo.path}
								<div class="repository-tools" aria-label={`${repo.name} tools`}>
									<button
										class="repository-tool"
										class:active={repo.active && session.workspaceView === 'workspace' && session.workspacePane === 'sources'}
										type="button"
										title={`Sources (${shortcutModifier}+Shift+S)`}
										onclick={() => void openRepositoryPane(repo, 'sources')}
									>
										<Icon name="table" size={12} />
										<span>Sources</span>
										{#if repo.active && session.catalog.sources.length}<small>{session.catalog.sources.length}</small>{/if}
									</button>
									<button
										class="repository-tool"
										class:active={repo.active && session.workspaceView === 'workspace' && session.workspacePane === 'context'}
										type="button"
										title={`Context (${shortcutModifier}+Shift+C)`}
										onclick={() => void openRepositoryPane(repo, 'context')}
									>
										<Icon name="file" size={12} />
										<span>Context</span>
									</button>
								</div>
							{/if}
							<button class="repository-new" type="button" onclick={() => void newConversation(repo)}>
								<Icon name="plus" size={12} /> New conversation
							</button>
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
											type="button"
											onclick={() => void open(c)}
											title={title(c)}
											aria-label={`Open conversation: ${title(c)}`}
										>
											<span class="preview">{title(c)}</span>
											<span class="row-end"><span class="age">{relativeAge(c.saved_at_ms)}</span></span>
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
							{#if repo.items.length === 0}
								<div class="empty">No conversations in this repository yet.</div>
							{/if}
						</div>
					{/if}
				</div>
			{/each}
			{#if repositories.length === 0}
				<button class="add-repository" type="button" onclick={() => void addRepository()}>
					<Icon name="folder" size={14} /> Add a repository
				</button>
			{/if}
		</div>
	</section>
	<section class="projects-section" aria-labelledby="projects-heading">
		<div class="section-head">
			<div class="nav-heading" id="projects-heading">Projects</div>
			<span class="coming-soon">Future</span>
		</div>
		<div class="projects-empty">
			<Icon name="bookmark" size={13} />
			<span>Repository wikis will live here.</span>
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
			<Icon name="settings" size={14} />
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
		gap: 2px;
		padding: var(--space-3) var(--space-1) var(--space-2);
	}
	.projects-section {
		flex: none;
		display: grid;
		gap: 2px;
		padding: var(--space-2) var(--space-1);
	}
	.repository-section {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: var(--space-2) var(--space-1);
		border-top: 1px solid var(--border);
	}
	.projects-section {
		border-top: 1px solid var(--border);
	}
	.section-head {
		display: flex;
		align-items: center;
		min-height: 22px;
	}
	.section-head .nav-heading {
		flex: 1;
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
		background: var(--bg-inset);
		color: var(--text);
	}
	.repositories {
		flex: 1;
		min-height: 0;
		display: grid;
		gap: 2px;
		overflow-y: auto;
	}
	.repository {
		min-width: 0;
	}
	.repository-row {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		padding: 7px var(--space-2);
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		text-align: left;
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.repository-row:hover,
	.repository.active > .repository-row {
		background: var(--bg-raised);
		color: var(--text);
	}
	.repository-row > :global(svg:first-child) {
		flex: none;
		color: var(--text-faint);
		transition: transform var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.repository.expanded > .repository-row > :global(svg:first-child) {
		transform: rotate(90deg);
	}
	.repository.active > .repository-row > :global(svg:first-child),
	.repository.active .repository-icon {
		color: var(--brand);
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
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.repository-copy strong,
	.repository-copy small {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.repository-copy strong {
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.repository-copy small {
		color: var(--text-faint);
		font-size: 10px;
	}
	.repository-contents {
		padding: 0 0 4px 22px;
	}
	.repository-tools {
		display: flex;
		gap: 2px;
		padding: 0 0 2px;
	}
	.repository-tool,
	.repository-new {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		min-height: 24px;
		padding: 3px 6px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		font-size: 10.5px;
		white-space: nowrap;
	}
	.repository-tool {
		flex: 1;
	}
	.repository-tool:hover,
	.repository-tool.active,
	.repository-new:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.repository-tool :global(svg),
	.repository-new :global(svg) {
		color: var(--brand);
	}
	.repository-tool small {
		margin-left: auto;
		font-family: var(--mono);
		font-size: 9px;
	}
	.repository-new {
		width: 100%;
		margin-bottom: 2px;
		text-align: left;
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
	.coming-soon {
		color: var(--text-faint);
		font-size: 10px;
	}
	.projects-empty {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.projects-empty :global(svg) {
		color: var(--text-faint);
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
		padding: var(--space-2) var(--space-2);
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
		background: var(--bg-raised);
		color: var(--text);
		font-weight: 620;
	}
	.nav-row.active::before {
		content: '';
		position: absolute;
		left: 0;
		top: 6px;
		bottom: 6px;
		width: 3px;
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
	.nav-row kbd {
		margin-left: auto;
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10px;
	}
	.item-wrap {
		position: relative;
	}
	.item {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		width: 100%;
		min-height: 30px;
		padding-top: 6px;
		padding-bottom: 6px;
		border-radius: var(--radius-sm);
	}
	.item:hover,
	.item:focus-visible {
		background: var(--bg-inset);
	}
	.item-wrap:focus-within .row-actions,
	.item-wrap:hover .row-actions {
		display: flex;
	}
	.item-wrap:hover .row-end,
	.item-wrap:focus-within .row-end {
		opacity: 0;
	}
	.row-end {
		display: flex;
		align-items: center;
		gap: 6px;
		flex: none;
		max-width: 42%;
		transition: opacity var(--dur-fast) var(--ease);
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
	.age {
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10px;
		white-space: nowrap;
	}
	.empty {
		padding: var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-sm);
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
