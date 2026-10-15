<script lang="ts">
	import { ipc } from '$lib/ipc';
	import { openFolder } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { Catalog, SourceInfo } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import Icon from './Icon.svelte';

	let { onopen }: { onopen: (source: SourceInfo) => void } = $props();
	type Node = { id: string; name: string; kind: 'folder' | 'file' | 'sheet'; children: Node[]; sources: SourceInfo[]; source?: SourceInfo };
	type Row = { node: Node; depth: number; parentIds: string[] };
	type RefreshSummary = { added: number; changed: number; removed: number; newlySkipped: number; nowReadable: number; changedSources: string[]; staleQuestions: number; staleFindings: number; revision: string | null };

	let expandedFolders = $state<Record<string, boolean>>({});
	let expandedWorkbooks = $state<Record<string, boolean>>({});
	let refreshing = $state(false);
	let refreshError = $state('');
	let refreshSummary = $state<RefreshSummary | null>(null);
	let request = 0;
	let workspacePath = $derived(session.catalog.workspace);

	function relativePath(path: string, root: string): string {
		const normalized = path.replace(/\\/g, '/');
		const normalizedRoot = root.replace(/\\/g, '/').replace(/\/$/, '');
		if (normalized.toLocaleLowerCase().startsWith(`${normalizedRoot.toLocaleLowerCase()}/`)) return normalized.slice(normalizedRoot.length + 1);
		return normalized;
	}

	let tree = $derived.by((): Node[] => {
		const root = workspacePath;
		if (!root) return [];
		const top: Node[] = [];
		const dirs = new Map<string, Node>();
		const files = new Map<string, SourceInfo[]>();
		for (const source of session.catalog.sources) {
			const group = files.get(source.path) ?? [];
			group.push(source);
			files.set(source.path, group);
		}
		for (const [path, sources] of [...files].sort(([left], [right]) => left.localeCompare(right, undefined, { numeric: true }))) {
			const relative = relativePath(path, root);
			const segments = relative.split('/').filter(Boolean);
			let siblings = top;
			let parentKey = '';
			for (const segment of segments.slice(0, -1)) {
				parentKey = parentKey ? `${parentKey}/${segment}` : segment;
				let folder = dirs.get(parentKey);
				if (!folder) {
					folder = { id: `folder:${parentKey}`, name: segment, kind: 'folder', children: [], sources: [] };
					dirs.set(parentKey, folder);
					siblings.push(folder);
				}
				siblings = folder.children;
			}
			const name = segments.at(-1) ?? relative;
			const file: Node = { id: `file:${path}`, name, kind: 'file', children: [], sources: [...sources].sort((a, b) => a.name.localeCompare(b.name)) };
			if (file.sources.length > 1) {
				file.children = file.sources.map((source) => ({ id: `sheet:${source.path}:${source.name}`, name: source.name.includes(' · ') ? source.name.slice(source.name.lastIndexOf(' · ') + 3) : source.name, kind: 'sheet', children: [], sources: [source], source }));
			}
			siblings.push(file);
		}
		const sortNodes = (nodes: Node[]) => {
			nodes.sort((left, right) => left.kind === right.kind ? left.name.localeCompare(right.name, undefined, { numeric: true }) : left.kind === 'folder' ? -1 : 1);
			for (const node of nodes) sortNodes(node.children);
		};
		sortNodes(top);
		return top;
	});

	let rows = $derived.by((): Row[] => {
		const result: Row[] = [];
		const visit = (nodes: Node[], depth: number, ancestors: string[]) => {
			for (const node of nodes) {
				result.push({ node, depth, parentIds: ancestors });
				if (node.kind === 'folder' && expandedFolders[node.id] !== true) continue;
				if (node.kind === 'file' && node.children.length && expandedWorkbooks[node.id] !== true) continue;
				visit(node.children, depth + 1, [...ancestors, node.id]);
			}
		};
		visit(tree, 0, []);
		return result;
	});

	function toggleFolder(id: string): void {
		expandedFolders = { ...expandedFolders, [id]: expandedFolders[id] !== true };
	}

	function toggleWorkbook(id: string): void {
		expandedWorkbooks = { ...expandedWorkbooks, [id]: expandedWorkbooks[id] !== true };
	}

	function selectedSource(source: SourceInfo): boolean {
		const active = session.activeWorkspaceToolTab;
		return active?.kind === 'source' && active.sourcePath === source.path && active.sourceName === source.name;
	}

	function catalogFiles(catalog: Catalog): Map<string, string> {
		return new Map(catalog.sources.map((source) => {
			const signature = [source.kind, source.name, source.view ?? '', source.size_bytes, source.mtime, source.row_count ?? ''].join('|');
			return [source.path, signature];
		}));
	}

	async function freshnessCounts(path: string): Promise<{ questions: number; findings: number }> {
		try {
			const records = (await ipc.investigationsList()).filter((item) => item.workspace_id === path);
			const questionRuns = new Set<string>();
			const findingRuns = new Set<string>();
			for (const item of records) {
				for (const question of item.questions ?? []) if (question.latest_analysis_turn_id) questionRuns.add(question.latest_analysis_turn_id);
				for (const finding of item.findings ?? []) findingRuns.add(finding.analysis_turn_id);
			}
			const all = [...new Set([...questionRuns, ...findingRuns])];
			const results = await Promise.all(all.map(async (turnId) => {
				try { return [turnId, await ipc.analysisTurnReplayStatus(turnId, path)] as const; }
				catch { return [turnId, null] as const; }
			}));
			const stale = new Set(results.filter(([, status]) => status?.revision_changed && status.same_workspace).map(([turnId]) => turnId));
			return {
				questions: [...questionRuns].filter((turnId) => stale.has(turnId)).length,
				findings: [...findingRuns].filter((turnId) => stale.has(turnId)).length
			};
		} catch {
			return { questions: 0, findings: 0 };
		}
	}

	async function refreshWorkspace(): Promise<void> {
		const path = workspacePath;
		if (!path || refreshing) return;
		const refreshRequest = ++request;
		const before = session.catalog;
		refreshing = true;
		refreshError = '';
		refreshSummary = null;
		try {
			const next = await ipc.reindex(path);
			if (refreshRequest !== request || session.catalog.workspace !== path) return;
			const previousFiles = catalogFiles(before);
			const nextFiles = catalogFiles(next);
			const added = [...nextFiles.keys()].filter((file) => !previousFiles.has(file));
			const removed = [...previousFiles.keys()].filter((file) => !nextFiles.has(file));
			const changed = [...nextFiles.keys()].filter((file) => previousFiles.has(file) && previousFiles.get(file) !== nextFiles.get(file));
			const beforeSkipped = new Set(before.skipped?.map((item) => item.name) ?? []);
			const afterSkipped = new Set(next.skipped?.map((item) => item.name) ?? []);
			const newlySkipped = [...afterSkipped].filter((name) => !beforeSkipped.has(name));
			const readablePaths = new Set([...nextFiles.keys()].map((file) => relativePath(file, path)));
			const nowReadable = [...beforeSkipped].filter((name) => !afterSkipped.has(name) && readablePaths.has(name));
			const freshness = await freshnessCounts(path);
			if (refreshRequest !== request || session.catalog.workspace !== path) return;
			session.catalog = next;
			const mounted = session.workspaceAt(path);
			if (mounted) mounted.catalog = next;
			refreshSummary = {
				added: added.length,
				changed: changed.length,
				removed: removed.length,
				newlySkipped: newlySkipped.length,
				nowReadable: nowReadable.length,
				changedSources: [...added, ...changed, ...removed, ...newlySkipped, ...nowReadable].slice(0, 5).map((file) => relativePath(file, path)),
				staleQuestions: freshness.questions,
				staleFindings: freshness.findings,
				revision: next.revision ?? null
			};
		} catch (cause) {
			if (refreshRequest === request) refreshError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (refreshRequest === request) refreshing = false;
		}
	}
</script>

<section class="environment-browser" aria-label="Workspace files">
	<header class="environment-header">
		<div><h2>Environment</h2><p>{workspacePath ? session.workspaceLabel(workspacePath) : 'Workspace files'}</p></div>
		{#if workspacePath}
			<Button variant="ghost" size="icon" type="button" aria-label="Refresh workspace files" title="Check for file changes" disabled={refreshing} onclick={() => void refreshWorkspace()}>
				<Icon name="refresh" size={14} />
			</Button>
		{/if}
	</header>
	{#if !workspacePath}
		<div class="environment-empty">
		<Icon name="folder" size={20} />
			<p>Select a workspace to browse its files.</p>
			<Button type="button" variant="outline" size="sm" onclick={() => void openFolder()}>Choose a folder</Button>
		</div>
	{:else}
		<div class="environment-scroll" role="region" aria-label="Workspace file hierarchy">
			{#each rows as row (row.node.id)}
				{@const node = row.node}
				{@const parentCollapsed = row.parentIds.some((id) => (id.startsWith('folder:') ? expandedFolders[id] !== true : expandedWorkbooks[id] !== true))}
				{#if !parentCollapsed}
					{#if node.kind === 'folder'}
						<button class="tree-row tree-folder" type="button" aria-expanded={expandedFolders[node.id] === true} style={`--tree-depth:${row.depth}`} onclick={() => toggleFolder(node.id)}>
							<Icon name={expandedFolders[node.id] === true ? 'chevron-down' : 'chevron-right'} size={12} /><Icon name="folder" size={14} /><span>{node.name}</span>
						</button>
					{:else if node.kind === 'file' && node.children.length}
						<button class="tree-row tree-file tree-workbook" type="button" aria-expanded={expandedWorkbooks[node.id] === true} style={`--tree-depth:${row.depth}`} onclick={() => toggleWorkbook(node.id)}>
							<Icon name={expandedWorkbooks[node.id] === true ? 'chevron-down' : 'chevron-right'} size={12} /><Icon name="file" size={14} /><span>{node.name}</span><small>{node.children.length} sheets</small>
						</button>
					{:else if node.kind === 'file' && node.sources[0]}
						<button class="tree-row tree-file" type="button" aria-current={selectedSource(node.sources[0]) ? 'page' : undefined} class:selected={selectedSource(node.sources[0])} style={`--tree-depth:${row.depth}`} title={relativePath(node.sources[0].path, workspacePath)} onclick={() => onopen(node.sources[0])}>
							<Icon name="file" size={14} /><span>{node.name}</span>
						</button>
					{:else if node.kind === 'sheet' && node.source}
						<button class="tree-row tree-sheet" type="button" aria-current={selectedSource(node.source) ? 'page' : undefined} class:selected={selectedSource(node.source)} style={`--tree-depth:${row.depth}`} title={node.source.name} onclick={() => onopen(node.source!)}>
							<Icon name="table" size={14} /><span>{node.name}</span>
						</button>
					{/if}
				{/if}
			{/each}
			{#if !rows.length}<p class="environment-note">No supported files were found in this workspace.</p>{/if}
		</div>
		{#if session.catalog.skipped?.length}
			<details class="skipped-files">
				<summary>Unavailable files · {session.catalog.skipped.length}</summary>
				<ul>{#each session.catalog.skipped as file (`${file.name}:${file.reason}`)}<li><strong>{relativePath(file.name, workspacePath)}</strong><span>{file.reason}</span></li>{/each}</ul>
			</details>
		{/if}
		{#if refreshing}<p class="environment-status" role="status"><span class="refresh-spinner"></span>Checking this folder…</p>{/if}
		{#if refreshError}<p class="environment-error" role="alert">Couldn't refresh the workspace: {refreshError}</p>{/if}
		{#if refreshSummary}
			<div class="refresh-summary" role="status" aria-live="polite">
				<strong>Workspace checked</strong>
				<p>{refreshSummary.added} added · {refreshSummary.changed} changed · {refreshSummary.removed} removed{#if refreshSummary.newlySkipped}{' '}· {refreshSummary.newlySkipped} unavailable{/if}{#if refreshSummary.nowReadable}{' '}· {refreshSummary.nowReadable} readable{/if}</p>
				{#if refreshSummary.changedSources.length}<ul>{#each refreshSummary.changedSources as file (file)}<li>{file}</li>{/each}</ul>{/if}
				{#if refreshSummary.staleQuestions || refreshSummary.staleFindings}
					<p>{refreshSummary.staleQuestions} saved Question answer{refreshSummary.staleQuestions === 1 ? '' : 's'} and {refreshSummary.staleFindings} Finding{refreshSummary.staleFindings === 1 ? '' : 's'} use an older catalog revision. Their original runs are unchanged; review freshness in Results before rerunning.</p>
				{/if}
				{#if refreshSummary.revision}<small>Catalog revision {refreshSummary.revision}</small>{/if}
			</div>
		{/if}
	{/if}
</section>

<style>
	.environment-browser { display: flex; min-width: 0; min-height: 0; flex: 1; flex-direction: column; padding: var(--space-2) var(--space-1); }
	.environment-header { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); padding: 0 var(--space-2) var(--space-2); }
	.environment-header h2 { margin: 0; color: var(--text-dim); font-size: var(--fs-sm); font-weight: 600; }
	.environment-header p { margin: 2px 0 0; color: var(--text-faint); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.environment-scroll { flex: 1; min-height: 0; overflow: auto; padding-bottom: var(--space-2); }
	.tree-row { display: flex; width: 100%; min-width: 0; align-items: center; gap: 6px; min-height: 28px; padding: 4px 6px; padding-left: calc(6px + var(--tree-depth) * 14px); border: 0; border-radius: var(--radius-sm); background: transparent; color: var(--text-dim); font: inherit; font-size: var(--fs-xs); text-align: left; cursor: pointer; }
	.tree-row:hover { background: var(--sidebar-hover); color: var(--text); }
	.tree-row.selected, .tree-row[aria-current='page'] { background: var(--sidebar-selected); color: var(--text); }
	.tree-row > :global(svg) { flex: none; color: var(--text-faint); }
	.tree-row > :global(svg:first-child) { margin-left: -2px; }
	.tree-row > span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.tree-folder > span { font-weight: 550; }
	.tree-workbook > :global(svg:nth-child(2)) { margin-left: -3px; }
	.tree-row small { flex: none; color: var(--text-faint); font-size: 10px; }
	.environment-empty { display: grid; align-content: start; justify-items: start; gap: var(--space-3); padding: var(--space-4) var(--space-3); color: var(--text-dim); font-size: var(--fs-xs); }
	.environment-empty p, .environment-note { margin: 0; color: var(--text-faint); line-height: 1.45; }
	.environment-note { padding: var(--space-3); font-size: var(--fs-xs); }
	.skipped-files { flex: none; border-top: 1px solid var(--pane-edge); padding: var(--space-2) var(--space-2) 0; color: var(--text-faint); font-size: 11px; }
	.skipped-files summary { cursor: pointer; }
	.skipped-files ul { display: grid; max-height: 160px; gap: var(--space-2); overflow: auto; margin: var(--space-2) 0 0; padding: 0; list-style: none; }
	.skipped-files li { display: grid; min-width: 0; gap: 2px; }
	.skipped-files strong { color: var(--text-dim); overflow-wrap: anywhere; font-weight: 550; }
	.skipped-files span { overflow-wrap: anywhere; }
	.environment-status, .environment-error { flex: none; margin: var(--space-2) var(--space-2) 0; font-size: 11px; line-height: 1.4; }
	.environment-status { display: flex; align-items: center; gap: 7px; color: var(--text-faint); }
	.environment-error { color: var(--err); overflow-wrap: anywhere; }
	.refresh-spinner { width: 12px; height: 12px; border: 1.5px solid var(--border-strong); border-top-color: var(--brand); border-radius: 50%; animation: spin .8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
	.refresh-summary { flex: none; max-height: 210px; overflow: auto; margin: var(--space-2); padding: var(--space-3); border-radius: var(--radius-md); background: var(--bg-inset); color: var(--text-dim); font-size: 11px; line-height: 1.45; }
	.refresh-summary strong { color: var(--text); font-weight: 600; }
	.refresh-summary p { margin: 4px 0 0; }
	.refresh-summary ul { margin: var(--space-2) 0; padding-left: 16px; }
	.refresh-summary small { color: var(--text-faint); }
</style>
