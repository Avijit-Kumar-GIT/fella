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
	import type { ConversationSummary, Project, SourceInfo } from '$lib/types';
	import Icon from './Icon.svelte';

	let { open = $bindable(false), onpick }: { open?: boolean; onpick: (cmd: string) => void } =
		$props();

	let query = $state('');
	let sel = $state(0);
	let input: HTMLInputElement | undefined = $state();
	let dialog: HTMLDivElement | undefined = $state();
	let returnFocus: HTMLElement | null = null;
	let history = $state<ConversationSummary[]>([]);
	let activeFilter = $state<SearchFilter>('all');
	const shortcutModifier =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent)
			? '⌘'
			: 'Ctrl';

	type SearchResult =
		| { kind: 'repository'; path: string; name: string; count: number }
		| { kind: 'conversation'; item: ConversationSummary }
		| { kind: 'source'; source: SourceInfo }
		| { kind: 'project'; project: Project }
		| { kind: 'command'; command: string };
	type SearchKind = SearchResult['kind'];
	type SearchFilter = 'all' | SearchKind;

	const filters: { id: SearchFilter; label: string }[] = [
		{ id: 'all', label: 'All' },
		{ id: 'repository', label: 'Repositories' },
		{ id: 'conversation', label: 'Conversations' },
		{ id: 'source', label: 'Files' },
		{ id: 'project', label: 'Projects' },
		{ id: 'command', label: 'Actions' }
	];

	async function loadHistory(): Promise<void> {
		if (!isTauri()) return;
		try {
			const next = await ipc.conversationsList();
			session.rememberRepositories(next.map((item) => item.workspace));
			history = next;
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
		for (const path of session.repositoryPaths) {
			grouped.set(path, { path, name: baseName(path), count: 0 });
		}
		if (session.catalog.workspace && !grouped.has(session.catalog.workspace)) {
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

	let allResults = $derived.by((): SearchResult[] => {
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
		const projectHits: SearchResult[] = session.projects
			.filter((project) => includes(`${project.name} ${project.workspace} ${project.body}`))
			.slice(0, 5)
			.map((project) => ({ kind: 'project', project }));
		const commandHits: SearchResult[] = matches.map((command) => ({ kind: 'command', command }));
		return [...repositoryHits, ...conversationHits, ...sourceHits, ...projectHits, ...commandHits];
	});

	let resultGroups = $derived.by(() => {
		const kinds: SearchKind[] = ['repository', 'conversation', 'source', 'project', 'command'];
		return kinds
			.filter((kind) => activeFilter === 'all' || activeFilter === kind)
			.map((kind) => ({
				kind,
				label: filters.find((filter) => filter.id === kind)?.label ?? kind,
				items: allResults.filter((result) => result.kind === kind)
			}))
			.filter((group) => group.items.length > 0);
	});

	let results = $derived.by(() => resultGroups.flatMap((group) => group.items));

	$effect(() => {
		if (open) {
			returnFocus = document.activeElement as HTMLElement | null;
			query = '';
			sel = 0;
			activeFilter = 'all';
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

	function setFilter(filter: SearchFilter): void {
		activeFilter = filter;
		sel = 0;
	}

	function cycleFilter(direction: 1 | -1): void {
		const index = filters.findIndex((filter) => filter.id === activeFilter);
		const next = (index + direction + filters.length) % filters.length;
		setFilter(filters[next].id);
	}

	function key(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			close();
		} else if ((e.ctrlKey || e.metaKey) && e.key === '[') {
			cycleFilter(-1);
			e.preventDefault();
		} else if ((e.ctrlKey || e.metaKey) && e.key === ']') {
			cycleFilter(1);
			e.preventDefault();
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
			case 'project':
				return baseName(result.project.workspace);
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
			case 'project':
				return result.project.name;
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
			case 'project':
				return `project:${result.project.id}`;
			case 'command':
				return `command:${result.command}`;
		}
	}

	function isSelected(result: SearchResult): boolean {
		const current = results[sel];
		return current ? resultKey(current) === resultKey(result) : false;
	}

	function icon(result: SearchResult): 'repository' | 'ask' | 'file' | 'project' | 'asterisk' {
		switch (result.kind) {
			case 'repository':
				return 'repository';
			case 'conversation':
				return 'ask';
			case 'source':
				return 'file';
			case 'project':
				return 'project';
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
			case 'project':
				session.openProject(result.project.id);
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
				<Icon name="search" size={16} />
				<input
					bind:this={input}
					bind:value={query}
					onkeydown={key}
					placeholder="Search Fella…"
					spellcheck="false"
					aria-label="Search Fella"
				/>
			</div>
			<div class="filters" aria-label="Search filters" role="tablist">
				{#each filters as filter}
					<button
						class:active={activeFilter === filter.id}
						class="filter"
						type="button"
						role="tab"
						aria-selected={activeFilter === filter.id}
						onclick={() => setFilter(filter.id)}
					>
						{filter.label}
					</button>
				{/each}
			</div>
			<ul>
				{#if !query.trim()}
					<li class="hint"><Icon name="search" size={16} /> <span>Search your workspace, conversations, files, and actions.</span></li>
				{:else if results.length === 0}
					<li class="empty">No results</li>
				{:else}
					{#each resultGroups as group}
						<li class="group-label">{group.label}</li>
						{#each group.items as result (resultKey(result))}
							<li class:sel={isSelected(result)}>
								<button class="search-result" type="button" onclick={() => void select(result)}>
									<span class="result-icon">
										<Icon name={icon(result)} size={16} />
									</span>
									<span class="result-copy">
										<strong>{label(result)}</strong>
										{#if detail(result)}<small>{detail(result)}</small>{/if}
									</span>
								</button>
							</li>
						{/each}
					{/each}
				{/if}
			</ul>
			<div class="palette-footer" aria-hidden="true">
				<span><kbd>↑↓</kbd> Select</span>
				<span><kbd>↵</kbd> Open</span>
				<span class="footer-spacer"></span>
				<span><kbd>{shortcutModifier}+[ / ]</kbd> Change filter</span>
			</div>
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
		padding-top: 10vh;
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
		width: min(680px, 92vw);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		box-shadow: var(--shadow-pop);
		overflow: hidden;
	}
	.search {
		display: flex;
		align-items: center;
		gap: 10px;
		min-height: 52px;
		padding: 0 16px;
		border-bottom: 1px solid var(--border);
		color: var(--text-faint);
	}
	input {
		flex: 1;
		border: none;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: 15px;
		padding: 0;
		outline: none;
	}
	input::placeholder {
		color: var(--text-faint);
	}
	input:focus-visible {
		box-shadow: none;
	}
	.filters {
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 7px 10px;
		border-bottom: 1px solid var(--border);
	}
	.filter {
		padding: 5px 10px;
		border-radius: 5px;
		color: var(--text-faint);
		font-size: var(--fs-sm);
		white-space: nowrap;
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.filter:hover {
		color: var(--text);
		background: var(--bg-inset);
	}
	.filter.active {
		background: var(--bg-inset);
		color: var(--text);
		font-weight: 600;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 6px;
		max-height: 52vh;
		overflow-y: auto;
	}
	li.sel .search-result {
		background: color-mix(in srgb, var(--brand) 8%, var(--bg-inset));
	}
	.hint {
		display: flex;
		align-items: center;
		gap: 10px;
		min-height: 72px;
		padding: 16px 12px;
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.hint :global(svg) {
		flex: none;
		color: var(--text-faint);
	}
	.group-label {
		padding: 9px 10px 4px;
		color: var(--text-faint);
		font-size: 10.5px;
		font-weight: 650;
		letter-spacing: 0.02em;
	}
	.search-result {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 10px;
		min-height: 42px;
		padding: 6px 10px;
		border-radius: 6px;
		text-align: left;
		transition: background var(--dur-fast) var(--ease);
	}
	.search-result:hover {
		background: var(--bg-inset);
	}
	.result-icon {
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		flex: none;
		border-radius: var(--radius-chip);
		background: transparent;
		color: var(--text-faint);
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
		color: var(--text);
		font-size: var(--fs);
		font-weight: 600;
	}
	.result-copy small {
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.empty {
		padding: 24px 12px;
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.palette-footer {
		display: flex;
		align-items: center;
		gap: 14px;
		min-height: 32px;
		padding: 0 12px;
		border-top: 1px solid var(--border);
		background: var(--bg-inset);
		color: var(--text-faint);
		font-size: 10.5px;
	}
	.palette-footer span {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		white-space: nowrap;
	}
	.palette-footer kbd {
		color: var(--text-dim);
		font-family: var(--mono);
		font-size: 10px;
	}
	.footer-spacer {
		flex: 1;
	}
</style>
