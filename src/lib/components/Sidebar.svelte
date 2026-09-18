<script lang="ts">
	import { ipc, isTauri } from '$lib/ipc';
	import { baseName, errMsg, openContext, openFolder, relativeAge } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { ConversationSummary, Message } from '$lib/types';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let { onsearch }: { onsearch?: () => void } = $props();

	let list = $state<ConversationSummary[]>([]);
	let query = $state('');
	let searchOpen = $state(false);
	let searchInput = $state<HTMLInputElement | null>(null);
	let folderName = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
	let fileCount = $derived(session.catalog.sources.length);
	let analysisCount = $derived(
		session.analyses.filter(
			(analysis) =>
				!session.catalog.workspace ||
				!analysis.answer.workspace?.path ||
				analysis.answer.workspace.path === session.catalog.workspace
		).length
	);
	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent)
			? '⌘'
			: 'Ctrl';

	function toggleSearch() {
		searchOpen = !searchOpen;
		if (searchOpen) queueMicrotask(() => searchInput?.focus());
		else query = '';
	}

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

	function conversationWorkspace(c: ConversationSummary): string {
		return c.workspace ? baseName(c.workspace) : 'No workspace';
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

	/** Today / Yesterday / This week / Older, from local-midnight boundaries. */
	function groupLabel(ms: number): string {
		const startOf = (t: number) => {
			const d = new Date(t);
			return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
		};
		const days = Math.round((startOf(Date.now()) - startOf(ms)) / 86_400_000);
		if (days <= 0) return 'Today';
		if (days === 1) return 'Yesterday';
		if (days < 7) return 'This week';
		return 'Older';
	}

	// `list` is already newest-first from the backend, so consecutive items
	// sharing a label land in the same group without a separate sort pass.
	let groups = $derived.by(() => {
		const q = query.trim().toLowerCase();
		const filtered = q
			? list.filter(
					(c) =>
						c.preview.toLowerCase().includes(q) ||
						(c.workspace ?? '').toLowerCase().includes(q) ||
						(c.title ?? '').toLowerCase().includes(q)
				)
			: list;
		const out: { label: string; items: ConversationSummary[] }[] = [];
		for (const c of filtered) {
			const label = groupLabel(c.saved_at_ms);
			const last = out[out.length - 1];
			if (last && last.label === label) last.items.push(c);
			else out.push({ label, items: [c] });
		}
		return out;
	});

	async function open(c: ConversationSummary) {
		try {
			session.setWorkspaceView('ask');
			const raw = await ipc.conversationLoad(c.id);
			const saved: { workspace?: string | null; messages?: unknown; title?: string | null } =
				JSON.parse(raw);
			const messages = Array.isArray(saved.messages) ? (saved.messages as Message[]) : [];
			session.loadArchivedTab(c.id, messages, saved.title ?? null);
			// Auto-mount the folder this conversation was about, so a follow-up
			// question here answers from the same files it originally did,
			// instead of just telling the user to /open it themselves.
			// openFolder already reports a failure (moved/deleted folder) as a
			// system message, so no separate handling is needed for that.
			if (saved.workspace && saved.workspace !== session.catalog.workspace) {
				await openFolder(saved.workspace);
			}
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
				aria-label="New conversation"
				title="New conversation"
				onclick={() => {
					session.setWorkspaceView('ask');
					session.newTab();
				}}
			>
				<Icon name="plus" size={15} />
			</button>
			<button
				class="icon-btn"
				type="button"
				aria-label="Filter conversations"
				title="Filter conversations"
				aria-pressed={searchOpen}
				onclick={toggleSearch}
			>
				<Icon name="search" size={14} />
			</button>
		</div>
	</div>
	{#if searchOpen}
		<div class="search">
			<Icon name="search" size={13} />
			<input
				bind:this={searchInput}
				bind:value={query}
				placeholder="Search…"
				spellcheck="false"
				aria-label="Search history"
			/>
		</div>
	{/if}
	<div class="workspace-block">
		<div class="section-label">Workspace</div>
		{#if session.catalog.workspace}
			<div class="workspace-card" title={session.catalog.workspace}>
				<span class="workspace-mark"><Icon name="folder" size={15} /></span>
				<span class="workspace-copy">
					<strong>{folderName}</strong>
					<span>{fileCount} file{fileCount === 1 ? '' : 's'} ready</span>
				</span>
				<button
					class="change-folder"
					type="button"
					aria-label="Change workspace folder"
					title="Change folder"
					onclick={() => void openFolder()}
				>
					<Icon name="chevron-right" size={13} />
				</button>
			</div>
		{:else}
			<button class="workspace-empty" type="button" onclick={() => void openFolder()}>
				<span class="workspace-mark"><Icon name="folder" size={15} /></span>
				<span class="workspace-copy">
					<strong>Choose a folder</strong>
					<span>Start a local workspace</span>
				</span>
				<Icon name="chevron-right" size={13} />
			</button>
		{/if}
	</div>
	<nav class="workspace-nav" aria-label="Workspace">
		<button
			class="nav-row"
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
		<button
			class="nav-row"
			class:active={session.workspaceView === 'packs'}
			type="button"
			aria-current={session.workspaceView === 'packs' ? 'page' : undefined}
			onclick={() => session.setWorkspaceView('packs')}
		>
			<Icon name="asterisk" size={14} />
			<span>Packs</span>
			{#if session.packs.length}<small>{session.packs.length}</small>{/if}
		</button>
		<button
			class="nav-row"
			class:active={session.workspaceView === 'sources'}
			type="button"
			disabled={!session.catalog.workspace}
			aria-current={session.workspaceView === 'sources' ? 'page' : undefined}
			onclick={() => session.setWorkspaceView('sources')}
		>
			<Icon name="table" size={14} />
			<span>Sources</span>
			{#if fileCount}<small>{fileCount}</small>{/if}
		</button>
		<button
			class="nav-row"
			class:active={session.workspaceView === 'analyses'}
			type="button"
			disabled={!session.catalog.workspace && analysisCount === 0}
			aria-current={session.workspaceView === 'analyses' ? 'page' : undefined}
			onclick={() => session.setWorkspaceView('analyses')}
		>
			<Icon name="bookmark" size={14} />
			<span>Analyses</span>
			{#if analysisCount}<small>{analysisCount}</small>{/if}
		</button>
		<button
			class="nav-row"
			class:active={session.workspaceView === 'context'}
			type="button"
			disabled={!session.catalog.workspace}
			aria-current={session.workspaceView === 'context' ? 'page' : undefined}
			onclick={() => void openContext()}
		>
			<Icon name="file" size={14} />
			<span>Context</span>
		</button>
	</nav>
	<div class="history-label">
		<span>Recent</span>
		{#if list.length}<span class="history-count">{list.length}</span>{/if}
	</div>
	<div class="list">
		{#each groups as group (group.label)}
			<div class="group-label">{group.label}</div>
			{#each group.items as c (c.id)}
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
							onclick={() => open(c)}
							title={title(c)}
							aria-label={`Open conversation: ${title(c)}`}
						>
							<span class="preview">{title(c)}</span>
							<span class="row-end">
								{#if conversationWorkspace(c) !== folderName}
									<span class="location" title={conversationWorkspace(c)}>{conversationWorkspace(c)}</span>
								{/if}
								<span class="age">{relativeAge(c.saved_at_ms)}</span>
							</span>
						</button>
						<div class="row-actions">
							<button
								class="ren"
								type="button"
								aria-label="Rename conversation"
								title="Rename"
								onclick={(e) => startRename(c, e)}
							>
								<Icon name="pencil" size={12} />
							</button>
							<button
								class="del"
								type="button"
								aria-label="Delete conversation"
								title="Delete"
								onclick={(e) => remove(c, e)}
							>
								<Icon name="x" size={12} />
							</button>
						</div>
					{/if}
				</div>
			{/each}
		{/each}
		{#if groups.length === 0}
			<div class="empty">
				{query ? 'No matches' : "No past conversations yet — they're saved here once you /clear or close a tab."}
			</div>
		{/if}
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
	.search {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-1) var(--space-2);
		color: var(--text-faint);
		flex: none;
	}
	.search input {
		flex: 1;
		border: none;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-sm);
		outline: none;
	}
	.search input::placeholder {
		color: var(--text-faint);
	}
	.workspace-block {
		padding: var(--space-2) var(--space-1) 0;
	}
	.section-label,
	.history-label {
		padding: 0 var(--space-2) var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.workspace-card,
	.workspace-empty {
		width: 100%;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-raised);
		text-align: left;
	}
	.workspace-empty {
		color: var(--text-dim);
		cursor: pointer;
		transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease);
	}
	.workspace-empty:hover {
		border-color: var(--border-strong);
		background: var(--bg-inset);
	}
	.workspace-mark {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		flex: none;
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
		color: var(--brand);
	}
	.workspace-copy {
		min-width: 0;
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.workspace-copy strong {
		overflow: hidden;
		color: var(--text);
		font-size: var(--fs-sm);
		font-weight: 580;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.workspace-copy span {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.change-folder {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		flex: none;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.change-folder:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.workspace-nav {
		display: grid;
		gap: 2px;
		padding: var(--space-2) var(--space-1) var(--space-3);
		border-bottom: 1px solid var(--border);
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
		background: transparent;
		color: var(--text);
		font-weight: 560;
	}
	.nav-row.active::before {
		content: '';
		position: absolute;
		left: 1px;
		top: 7px;
		bottom: 7px;
		width: 2px;
		border-radius: 2px;
		background: var(--brand);
	}
	.nav-row:disabled {
		color: var(--border-strong);
		cursor: default;
	}
	.nav-row small {
		margin-left: auto;
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10px;
	}
	.nav-row kbd {
		margin-left: auto;
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10px;
	}
	.history-label {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding-top: var(--space-3);
	}
	.history-count {
		font-family: var(--mono);
		font-size: 10px;
		font-weight: 500;
		letter-spacing: 0;
		text-transform: none;
	}
	.list {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
	}
	.group-label {
		padding: var(--space-2) var(--space-2) var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 600;
		letter-spacing: 0.01em;
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
	.location {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-faint);
		font-size: 10px;
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
</style>
