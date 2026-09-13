<script lang="ts">
	import { ipc, isTauri } from '$lib/ipc';
	import { baseName, errMsg, openFolder, relativeAge } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { ConversationSummary, Message } from '$lib/types';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';

	let list = $state<ConversationSummary[]>([]);
	let query = $state('');
	let searchOpen = $state(false);
	let searchInput = $state<HTMLInputElement | null>(null);

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

	/** A custom name, if renamed; otherwise "folder — first message" -- the
	 *  folder is what actually distinguishes two similarly-phrased
	 *  conversations. */
	function title(c: ConversationSummary): string {
		if (c.title) return c.title;
		const folder = c.workspace ? baseName(c.workspace) : 'No project';
		return `${folder} — ${c.preview}`;
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
				onclick={() => session.newTab()}
			>
				<Icon name="plus" size={15} />
			</button>
			<button
				class="icon-btn"
				type="button"
				aria-label="Search history"
				title="Search"
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
						<button class="rowbtn item" type="button" onclick={() => open(c)} title={title(c)}>
							<span class="row-top">
								<span class="preview">{title(c)}</span>
							</span>
							<span class="meta">
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
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.item-wrap {
		position: relative;
	}
	.item {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 2px;
		width: 100%;
	}
	.row-top {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding-right: 40px;
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
		top: var(--space-2);
		right: var(--space-2);
		display: none;
		align-items: center;
		gap: 2px;
	}
	.item-wrap:hover .row-actions {
		display: flex;
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
	.meta {
		display: flex;
		align-items: center;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.empty {
		padding: var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
</style>
