<script lang="ts">
	import { openFolder } from '$lib/commands';
	import type { SourceInfo } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import Icon from './Icon.svelte';
	import WorkspaceEnvironment from './WorkspaceEnvironment.svelte';
	import { session } from '$lib/session.svelte';

	let { selected, onselect, folder = 'all' }: { selected: SourceInfo | null; onselect: (source: SourceInfo) => void; folder?: string } = $props();
	let query = $state('');
	let kind = $state<'all' | 'data' | 'documents'>('all');

	function relativePath(path: string): string {
		const root = session.catalog.workspace?.replace(/[/\\]+$/, '') ?? '';
		const normalizedPath = path.replace(/\\/g, '/');
		const normalizedRoot = root.replace(/\\/g, '/').replace(/\/$/, '');
		return normalizedRoot && normalizedPath.toLocaleLowerCase().startsWith(`${normalizedRoot.toLocaleLowerCase()}/`)
			? normalizedPath.slice(normalizedRoot.length + 1)
			: normalizedPath;
	}

	function sourceType(source: SourceInfo): 'data' | 'document' {
		return source.view ? 'data' : 'document';
	}

	function sourceFolder(source: SourceInfo): string {
		const segments = relativePath(source.path).split('/').filter(Boolean);
		return segments.length > 1 ? segments[0] : '__root';
	}

	function updatedLabel(timestamp: number): string {
		return Number.isFinite(timestamp) && timestamp > 0
			? new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(timestamp))
			: '—';
	}

	let sources = $derived.by(() => {
		const term = query.trim().toLocaleLowerCase();
		return [...session.catalog.sources]
			.filter((source) => folder === 'all' || sourceFolder(source) === folder)
			.filter((source) => kind === 'all' || (kind === 'data' ? sourceType(source) === 'data' : sourceType(source) === 'document'))
			.filter((source) => !term || `${source.name} ${relativePath(source.path)} ${source.synopsis ?? ''} ${source.columns?.map((column) => column.name).join(' ') ?? ''}`.toLocaleLowerCase().includes(term))
			.sort((left, right) => relativePath(left.path).localeCompare(relativePath(right.path), undefined, { numeric: true }) || left.name.localeCompare(right.name, undefined, { numeric: true }));
	});
	let dataCount = $derived(sources.filter((source) => source.view).length);
	let documentCount = $derived(sources.filter((source) => !source.view).length);
	let folderLabel = $derived(folder === 'all' ? 'All files' : folder === '__root' ? 'Workspace root' : folder);
</script>

<main class="files-page" aria-label="Workspace files">
	<header class="files-heading">
		<div>
			<h1>{folderLabel}</h1>
			<p class="files-summary">
				{#if session.catalog.workspace}
					{dataCount} queryable {dataCount === 1 ? 'source' : 'sources'} · {documentCount} readable {documentCount === 1 ? 'document' : 'documents'}
				{:else}
					Open a folder to browse its files.
				{/if}
			</p>
		</div>
			<WorkspaceEnvironment compact />
	</header>

	{#if !session.catalog.workspace}
		<div class="files-empty">
			<span><Icon name="folder" size={20} /></span>
			<h2>Choose a folder to get started</h2>
			<p>Fella will catalog supported files and show you what can be analyzed.</p>
			<Button onclick={() => void openFolder()}>Open folder</Button>
		</div>
	{:else}
		<div class="files-controls">
			<label class="files-search">
				<Icon name="search" size={14} />
				<Input aria-label="Search workspace files" bind:value={query} placeholder="Find a file, field, or folder…" />
			</label>
			<div class="files-filter fella-pill-nav" role="tablist" aria-label="Filter file types">
				{#each [{ id: 'all', label: 'All' }, { id: 'data', label: 'Data' }, { id: 'documents', label: 'Documents' }] as filter (filter.id)}
					<button class="fella-pill-nav-item" type="button" role="tab" aria-selected={kind === filter.id} onclick={() => (kind = filter.id as typeof kind)}>{filter.label}</button>
				{/each}
			</div>
		</div>

		<div class="files-list" role="table" aria-label="Cataloged workspace sources">
			<div class="files-list-head" role="row">
				<span role="columnheader">Name</span><span role="columnheader">Rows</span><span role="columnheader">Updated</span><span role="columnheader">Status</span>
			</div>
			{#each sources as source (`${source.path}:${source.name}`)}
				{@const active = selected?.path === source.path && selected?.name === source.name}
				<button class="file-row" class:selected={active} type="button" role="row" aria-selected={active} onclick={() => onselect(source)}>
					<span class="file-name-cell" role="cell">
						<span class="file-type-icon"><Icon name={source.view ? 'table' : 'file'} size={16} /></span>
						<span class="file-name-copy"><strong>{source.name}</strong><small title={relativePath(source.path)}>{source.kind.toUpperCase()} · {relativePath(source.path)}{#if source.synopsis}<span> · {source.synopsis}</span>{/if}</small></span>
					</span>
					<span class="file-row-count" role="cell">{source.view && source.row_count != null ? new Intl.NumberFormat().format(source.row_count) : '—'}</span>
					<span class="file-updated" role="cell">{updatedLabel(source.mtime)}</span>
					<span class="file-access" class:queryable={source.view} role="cell">{source.view ? 'Queryable' : 'Readable'}</span>
				</button>
			{:else}
				<p class="files-no-results">{query.trim() ? `No files match “${query.trim()}”.` : 'No sources are available in this folder.'}</p>
			{/each}
		</div>
		{#if session.catalog.skipped?.length}
			<details class="files-skipped">
				<summary>{session.catalog.skipped.length} unavailable {session.catalog.skipped.length === 1 ? 'file' : 'files'}</summary>
				<ul>{#each session.catalog.skipped as file (`${file.name}:${file.reason}`)}<li><strong>{relativePath(file.name)}</strong><span>{file.reason}</span></li>{/each}</ul>
			</details>
		{/if}
	{/if}
</main>

<style>
	.files-page { display:flex; flex:1; flex-direction:column; min-width:0; min-height:0; overflow:auto; padding:34px clamp(20px, 3vw, 34px); color:var(--text); }
	.files-heading { display:flex; align-items:flex-start; justify-content:space-between; gap:20px; max-width:980px; width:100%; margin:0 auto; }
	.files-heading h1 { margin:2px 0 0; font-size:clamp(22px, 2.2vw, 26px); font-weight:550; letter-spacing:-.025em; }
	.files-summary { margin:5px 0 0; color:var(--text-dim); font-size:var(--fs-xs); }
	.files-controls { display:flex; align-items:center; gap:12px; width:100%; max-width:980px; margin:22px auto 13px; }
	.files-search { display:flex; flex:1; min-width:120px; align-items:center; gap:8px; padding-left:12px; border:1px solid var(--border); border-radius:999px; background:var(--bg-inset); color:var(--text-faint); }
	.files-search :global(input) { min-width:0; min-height:36px; border:0; background:transparent; box-shadow:none; }
	.files-filter { flex:none; }
	.files-filter .fella-pill-nav-item { min-height:28px; padding-inline:11px; }
	.files-list { width:100%; max-width:980px; margin:0 auto; }
	.files-list-head, .file-row { display:grid; grid-template-columns:minmax(0, 1fr) 76px 96px 86px; align-items:center; gap:12px; }
	.files-list-head { min-height:32px; padding:0 12px; border-bottom:1px solid var(--border); color:var(--text-faint); font-size:11px; }
	.files-list-head span:not(:first-child) { text-align:right; }
	.file-row { width:100%; min-width:0; min-height:58px; padding:7px 12px; border:0; border-bottom:1px solid color-mix(in srgb, var(--border) 75%, transparent); border-radius:10px; background:transparent; color:var(--text-dim); text-align:left; cursor:pointer; }
	.file-row:hover, .file-row.selected { background:var(--bg-raised); }
	.file-name-cell { display:flex; min-width:0; align-items:center; gap:11px; }
	.file-type-icon { display:grid; width:30px; height:30px; flex:none; place-items:center; border-radius:9px; background:var(--bg-raised); color:var(--text-faint); }
	.file-row.selected .file-type-icon { color:var(--brand); }
	.file-name-copy { display:grid; min-width:0; gap:2px; }
	.file-name-copy strong { overflow:hidden; color:var(--text); font-size:var(--fs-sm); font-weight:550; text-overflow:ellipsis; white-space:nowrap; }
	.file-name-copy small { overflow:hidden; color:var(--text-faint); font-size:11px; text-overflow:ellipsis; white-space:nowrap; }
	.file-row-count, .file-updated, .file-access { color:var(--text-dim); font-size:var(--fs-xs); text-align:right; }
	.file-row-count { font-family:var(--mono); }
	.file-access.queryable { color:var(--brand); }
	.files-no-results { padding:28px 12px; color:var(--text-faint); font-size:var(--fs-sm); text-align:center; }
	.files-skipped { width:100%; max-width:980px; margin:14px auto 0; padding-top:12px; border-top:1px solid var(--border); color:var(--text-faint); font-size:var(--fs-xs); }
	.files-skipped summary { cursor:pointer; }
	.files-skipped ul { display:grid; gap:8px; padding-left:18px; }
	.files-skipped li { display:grid; gap:2px; }
	.files-skipped strong { color:var(--text-dim); font-weight:550; }
	.files-empty { display:grid; justify-items:center; align-content:center; flex:1; gap:10px; text-align:center; }
	.files-empty > span { display:grid; width:40px; height:40px; place-items:center; border-radius:14px; background:var(--bg-raised); color:var(--text-dim); }
	.files-empty h2 { margin:4px 0 0; font-size:var(--fs-md); }
	.files-empty p { max-width:42ch; margin:0 0 6px; color:var(--text-dim); font-size:var(--fs-sm); line-height:1.5; }
	@media (max-width: 880px) { .files-list-head, .file-row { grid-template-columns:minmax(0, 1fr) 64px 84px; } .files-list-head span:nth-child(4), .file-access { display:none; } }
	@media (max-width: 560px) { .files-page { padding:16px 12px; } .files-heading { flex-direction:column; gap:12px; } .files-controls { align-items:stretch; flex-direction:column; margin-top:16px; } .files-filter { align-self:flex-start; } .files-list-head, .file-row { grid-template-columns:minmax(0, 1fr) 58px; gap:8px; } .files-list-head span:nth-child(n+3), .file-updated, .file-access { display:none; } }
</style>
