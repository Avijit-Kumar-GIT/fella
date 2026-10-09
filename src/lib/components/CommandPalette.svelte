<script lang="ts">
	import {
		baseName,
		COMMAND_DESCRIPTIONS,
		openConversation,
		openRepository,
		SLASH_COMMANDS
	} from '$lib/commands';
	import { ipc, isDesktop } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import { Dialog } from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import type { ConversationSummary, Project, SourceInfo } from '$lib/types';
	import Icon from './Icon.svelte';

	let { open = $bindable(false), onpick }: { open?: boolean; onpick: (cmd: string) => void } =
		$props();

	let query = $state('');
	let sel = $state(0);
	let input: HTMLInputElement | undefined = $state();
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
		if (!isDesktop()) return;
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
			query = '';
			sel = 0;
			activeFilter = 'all';
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
		if ((e.ctrlKey || e.metaKey) && e.key === '[') {
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
				return result.item.workspace ? baseName(result.item.workspace) : 'General';
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
				await openRepository(result.path);
				return;
			case 'conversation':
				await openConversation(result.item);
				return;
			case 'source':
				session.selectSource(result.source.path);
				session.setWorkspacePane('sources');
				return;
			case 'project':
				session.openProject(result.project.id);
				return;
		}
	}

</script>

	<Dialog.Root bind:open>
		<Dialog.Portal>
			<Dialog.Overlay class="fella-ui-dialog-overlay" />
			<Dialog.Content
				class="fella-ui-dialog-content palette"
				aria-label="Search Fella"
				onOpenAutoFocus={(event) => {
					event.preventDefault();
					queueMicrotask(() => input?.focus());
				}}
			>
			<div class="search">
				<Icon name="search" size={16} />
				<Input
					class="palette-search-input"
					bind:ref={input}
					bind:value={query}
					onkeydown={key}
					placeholder="Search Fella…"
					spellcheck="false"
					aria-label="Search Fella"
				/>
			</div>
			<div class="filters" aria-label="Search filters" role="group">
				{#each filters as filter}
					<Button
						variant="ghost"
						class={`filter${activeFilter === filter.id ? ' active' : ''}`}
						aria-pressed={activeFilter === filter.id}
						onclick={() => setFilter(filter.id)}
					>
						{filter.label}
					</Button>
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
								<Button variant="ghost" class="search-result" onclick={() => void select(result)}>
									<span class="result-icon">
										<Icon name={icon(result)} size={16} />
									</span>
									<span class="result-copy">
										<strong>{label(result)}</strong>
										{#if detail(result)}<small>{detail(result)}</small>{/if}
									</span>
								</Button>
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
			</Dialog.Content>
		</Dialog.Portal>
	</Dialog.Root>

<style>
	:global(.palette) {
		top: 10vh;
		left: 50%;
		transform: translateX(-50%);
		width: min(680px, 92vw);
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
	:global(.palette-search-input) {
		flex: 1;
		min-width: 0;
		min-height: 0;
		padding: 0;
		border: 0;
		border-radius: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: 15px;
		outline: none;
	}
	:global(.palette-search-input::placeholder) {
		color: var(--text-faint);
	}
	:global(.palette-search-input:focus-visible) {
		box-shadow: none;
		border-color: transparent;
	}
	.filters {
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 7px 10px;
		border-bottom: 1px solid var(--border);
	}
	.filters :global(.filter) {
		height: auto;
		min-height: 0;
		justify-content: flex-start;
		padding: 5px 10px;
		border-radius: 5px;
		background: transparent;
		color: var(--text-faint);
		font-size: var(--fs-sm);
		white-space: nowrap;
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.filters :global(.filter:hover) {
		color: var(--text);
		background: var(--bg-inset);
	}
	.filters :global(.filter.active) {
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
	li.sel :global(.search-result) {
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
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.02em;
	}
	ul li :global(.search-result) {
		width: 100%;
		display: flex;
		align-items: center;
		justify-content: flex-start;
		gap: 10px;
		min-height: 42px;
		height: auto;
		white-space: normal;
		padding: 6px 10px;
		border-radius: 6px;
		background: transparent;
		color: var(--text);
		text-align: left;
		transition: background var(--dur-fast) var(--ease);
	}
	ul li :global(.search-result:hover) {
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
		font-size: var(--fs-xs);
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
		font-size: var(--fs-xs);
	}
	.footer-spacer {
		flex: 1;
	}
</style>
