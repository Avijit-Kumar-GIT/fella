<script lang="ts">
	import {
		baseName,
		COMMAND_DESCRIPTIONS,
		openConversation,
		openFolder,
		SLASH_COMMANDS
	} from '$lib/commands';
	import { ipc, isTauri } from '$lib/ipc';
	import { fadeQuick, pop } from '$lib/motion';
	import { session } from '$lib/session.svelte';
	import type { ConversationSummary, SourceInfo } from '$lib/types';
	import Icon from './Icon.svelte';

	let { open = $bindable(false), onpick }: { open?: boolean; onpick: (cmd: string) => void } =
		$props();

	let query = $state('');
	let sel = $state(0);
	let input: HTMLInputElement | undefined = $state();
	let dialog: HTMLDivElement | undefined = $state();
	let returnFocus: HTMLElement | null = null;
	let history = $state<ConversationSummary[]>([]);

	type SearchResult =
		| { kind: 'repository'; path: string; name: string; count: number }
		| { kind: 'conversation'; item: ConversationSummary }
		| { kind: 'source'; source: SourceInfo }
		| { kind: 'command'; command: string };

	async function loadHistory(): Promise<void> {
		if (!isTauri()) return;
		try {
			history = await ipc.conversationsList();
		} catch {
			/* Keep the last-known search index if the archive is unavailable. */
		}
	}

	let matches = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q) return [];
		return SLASH_COMMANDS.filter((command) => {
			const description = COMMAND_DESCRIPTIONS[command] ?? '';
			return command.includes(q.replace(/^\//, '')) || description.toLowerCase().includes(q);
		}).slice(0, 5);
	});

	let repositories = $derived.by(() => {
		const grouped = new Map<string, { path: string; name: string; count: number }>();
		if (session.catalog.workspace) {
			grouped.set(session.catalog.workspace, {
				path: session.catalog.workspace,
				name: baseName(session.catalog.workspace),
				count: 0
			});
		}
		for (const item of history) {
			if (!item.workspace) continue;
			const repo =
				grouped.get(item.workspace) ??
				{ path: item.workspace, name: baseName(item.workspace), count: 0 };
			repo.count++;
			grouped.set(item.workspace, repo);
		}
		return [...grouped.values()];
	});

	let results = $derived.by((): SearchResult[] => {
		const q = query.trim().toLowerCase();
		if (!q) return [];
		const includes = (value: string) => value.toLowerCase().includes(q);
		const repositoryHits: SearchResult[] = repositories
			.filter((repo) => includes(repo.path) || includes(baseName(repo.path)))
			.slice(0, 5)
			.map((repo) => ({ kind: 'repository', ...repo }));
		const conversationHits: SearchResult[] = history
			.filter(
				(item) =>
					includes(item.title ?? '') ||
					includes(item.preview) ||
					includes(item.workspace ?? '')
			)
			.slice(0, 8)
			.map((item) => ({ kind: 'conversation', item }));
		const sourceHits: SearchResult[] = session.catalog.sources
			.filter((source) => includes(`${source.name} ${source.path} ${source.synopsis ?? ''}`))
			.slice(0, 5)
			.map((source) => ({ kind: 'source', source }));
		const commandHits: SearchResult[] = matches.map((command) => ({ kind: 'command', command }));
		return [...repositoryHits, ...conversationHits, ...sourceHits, ...commandHits];
	});

	$effect(() => {
		if (open) {
			returnFocus = document.activeElement as HTMLElement | null;
			query = '';
			sel = 0;
			queueMicrotask(() => input?.focus());
		} else if (returnFocus) {
			// Put focus back where it was when the palette opened.
			returnFocus.focus();
			returnFocus = null;
		}
	});

	$effect(() => {
		session.historyVersion;
		if (open) void loadHistory();
	});

	function close() {
		open = false;
	}

	function key(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			close();
		} else if (e.key === 'ArrowDown') {
			sel = (sel + 1) % Math.max(results.length, 1);
			e.preventDefault();
		} else if (e.key === 'ArrowUp') {
			sel = (sel - 1 + results.length) % Math.max(results.length, 1);
			e.preventDefault();
		} else if (e.key === 'Enter' && results[sel]) {
			void select(results[sel]);
		}
	}

	function detail(result: SearchResult): string {
		switch (result.kind) {
			case 'repository':
				return `${result.count} conversation${result.count === 1 ? '' : 's'}`;
			case 'conversation':
				return result.item.workspace ? baseName(result.item.workspace) : 'No repository';
			case 'source':
				return result.source.path;
			case 'command':
				return COMMAND_DESCRIPTIONS[result.command] ?? '';
		}
	}

	function label(result: SearchResult): string {
		switch (result.kind) {
			case 'repository':
				return result.name;
			case 'conversation':
				return result.item.title ?? result.item.preview;
			case 'source':
				return result.source.name;
			case 'command':
				return result.command;
		}
	}

	function resultKey(result: SearchResult): string {
		switch (result.kind) {
			case 'repository':
				return `repository:${result.path}`;
			case 'conversation':
				return `conversation:${result.item.id}`;
			case 'source':
				return `source:${result.source.path}`;
			case 'command':
				return `command:${result.command}`;
		}
	}

	function kindLabel(result: SearchResult): string {
		switch (result.kind) {
			case 'repository':
				return 'Repository';
			case 'conversation':
				return 'Conversation';
			case 'source':
				return 'Source';
			case 'command':
				return 'Command';
		}
	}

	function icon(result: SearchResult): 'folder' | 'compose' | 'file' | 'asterisk' {
		switch (result.kind) {
			case 'repository':
				return 'folder';
			case 'conversation':
				return 'compose';
			case 'source':
				return 'file';
			case 'command':
				return 'asterisk';
		}
	}

	async function select(result: SearchResult): Promise<void> {
		close();
		switch (result.kind) {
			case 'command':
				onpick(result.command);
				return;
			case 'repository':
				session.setWorkspaceView('ask');
				await openFolder(result.path);
				return;
			case 'conversation':
				await openConversation(result.item);
				return;
			case 'source':
				session.setWorkspacePane('sources');
				session.openInspector({ kind: 'source', path: result.source.path });
				return;
		}
	}

	// Keep Tab inside the dialog while it's open; Escape closes from anywhere in it.
	function trap(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			close();
			return;
		}
		if (e.key !== 'Tab' || !dialog) return;
		const focusable = dialog.querySelectorAll<HTMLElement>(
			'a[href], button:not([disabled]), input, [tabindex]:not([tabindex="-1"])'
		);
		if (focusable.length === 0) return;
		const first = focusable[0];
		const last = focusable[focusable.length - 1];
		const active = document.activeElement;
		if (e.shiftKey && active === first) {
			e.preventDefault();
			last.focus();
		} else if (!e.shiftKey && active === last) {
			e.preventDefault();
			first.focus();
		}
	}
</script>

{#if open}
	<div class="scrim">
		<button
			type="button"
			class="backdrop"
			aria-label="Close commands"
			onclick={close}
			transition:fadeQuick
		></button>
		<div
			class="palette"
			bind:this={dialog}
			role="dialog"
			aria-modal="true"
			aria-label="Search Fella"
			tabindex="-1"
			onkeydown={trap}
			transition:pop
		>
			<div class="search">
				<Icon name="search" size={15} />
				<input
					bind:this={input}
					bind:value={query}
					onkeydown={key}
					placeholder="Search Fella…"
					spellcheck="false"
					aria-label="Search Fella"
				/>
			</div>
			<ul>
				{#if !query.trim()}
					<li class="hint">Search conversations, repositories, sources, and commands.</li>
				{:else if results.length === 0}
					<li class="empty">No results</li>
				{:else}
					{#each results as result, i (resultKey(result))}
						<li class:sel={i === sel}>
							<button class="search-result" type="button" onclick={() => void select(result)}>
								<span class="result-icon"><Icon name={icon(result)} size={14} /></span>
								<span class="result-copy">
									<strong>{label(result)}</strong>
									<small>{detail(result)}</small>
								</span>
								<span class="result-kind">{kindLabel(result)}</span>
							</button>
						</li>
					{/each}
				{/if}
			</ul>
		</div>
	</div>
{/if}

<style>
	.scrim {
		position: fixed;
		inset: 0;
		display: flex;
		justify-content: center;
		align-items: flex-start;
		padding-top: 12vh;
		z-index: 50;
	}
	.backdrop {
		position: fixed;
		inset: 0;
		border: none;
		padding: 0;
		background: color-mix(in srgb, var(--bg) 55%, transparent);
		cursor: default;
	}
	.palette {
		position: relative;
		width: min(620px, 92vw);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		box-shadow: var(--shadow-pop);
		overflow: hidden;
	}
	.search {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: 0 var(--space-3);
		border-bottom: 1px solid var(--border);
		color: var(--text-faint);
	}
	input {
		flex: 1;
		border: none;
		background: transparent;
		color: var(--text);
		font: inherit;
		padding: var(--space-3) 0;
		outline: none;
	}
	input::placeholder {
		color: var(--text-faint);
	}
	input:focus-visible {
		box-shadow: none;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: var(--space-1);
		max-height: 50vh;
		overflow-y: auto;
	}
	li.sel .search-result {
		background: var(--bg-inset);
	}
	.hint {
		padding: var(--space-3) var(--space-3) var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.search-result {
		width: 100%;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: 8px var(--space-3);
		border-radius: var(--radius-chip);
		text-align: left;
		transition: background var(--dur-fast) var(--ease);
	}
	.search-result:hover {
		background: var(--bg-inset);
	}
	.result-icon {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		flex: none;
		border-radius: var(--radius-chip);
		background: var(--bg-inset);
		color: var(--brand);
	}
	.result-copy {
		min-width: 0;
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.result-copy strong,
	.result-copy small {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.result-copy strong {
		color: var(--accent);
		font-size: var(--fs-sm);
		font-weight: 560;
	}
	.result-copy small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.result-kind {
		flex: none;
		color: var(--text-faint);
		font-size: 10px;
		text-transform: capitalize;
	}
	.empty {
		padding: var(--space-2) var(--space-3);
		color: var(--text-faint);
	}
</style>
