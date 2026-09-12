<script lang="ts">
	import { ipc, isTauri } from '$lib/ipc';
	import { baseName, errMsg, relativeAge } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { ConversationSummary, Message } from '$lib/types';
	import Icon from './Icon.svelte';

	let list = $state<ConversationSummary[]>([]);
	let query = $state('');

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
						c.preview.toLowerCase().includes(q) || (c.workspace ?? '').toLowerCase().includes(q)
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
			const saved: { workspace?: string | null; messages?: unknown } = JSON.parse(raw);
			const messages = Array.isArray(saved.messages) ? (saved.messages as Message[]) : [];
			session.loadArchivedTab(messages);
			const current = session.catalog.workspace;
			if (saved.workspace && current && saved.workspace !== current) {
				session.addSystem(
					`This conversation was about a different folder (${baseName(saved.workspace)}). ` +
						`Fella is pointed at ${baseName(current)} right now, so a new question here answers ` +
						`from that folder, not the original one. /open ${saved.workspace} first if you want the original.`
				);
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
	<button class="rowbtn new-btn" type="button" onclick={() => session.newTab()}>
		<Icon name="compose" size={14} />
		<span>New conversation</span>
	</button>
	<div class="search">
		<Icon name="search" size={13} />
		<input bind:value={query} placeholder="Search…" spellcheck="false" aria-label="Search history" />
	</div>
	<div class="list">
		{#each groups as group (group.label)}
			<div class="group-label">{group.label}</div>
			{#each group.items as c (c.id)}
				<div class="item-wrap">
					<button class="rowbtn item" type="button" onclick={() => open(c)}>
						<span class="row-top">
							<span class="preview">{c.preview}</span>
						</span>
						<span class="meta">
							<span class="proj">
								<Icon name="folder" size={11} />
								{c.workspace ? baseName(c.workspace) : 'No project'}
							</span>
							<span class="age">{relativeAge(c.saved_at_ms)}</span>
						</span>
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
		padding: var(--space-2);
		/* Same fill as the rest of the shell -- an open floor plan, not a
		   bordered-off compartment. */
		background: var(--bg);
		overflow: hidden;
	}
	.new-btn {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		color: var(--text);
		flex: none;
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
		padding-right: 20px;
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
	.del {
		position: absolute;
		top: var(--space-2);
		right: var(--space-2);
		display: none;
		place-items: center;
		width: 18px;
		height: 18px;
		color: var(--err);
	}
	.item-wrap:hover .del {
		display: grid;
	}
	.del:hover {
		color: var(--text);
	}
	.meta {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.proj {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.age {
		flex: none;
	}
	.empty {
		padding: var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
</style>
