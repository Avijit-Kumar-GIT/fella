<script lang="ts">
	import { openFolder } from '$lib/commands';
	import type { SourceInfo } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import Icon from './Icon.svelte';
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
			? new Intl.DateTimeFormat(undefined, { month:'short', day:'numeric' }).format(new Date(timestamp * 1000))
			: '—';
	}

	let sources = $derived.by(() => {
		const term = query.trim().toLocaleLowerCase();
		return [...session.catalog.sources]
			.filter((source) => folder === 'all' || sourceFolder(source) === folder)
			.filter((source) => kind === 'all' || (kind === 'data' ? sourceType(source) === 'data' : sourceType(source) === 'document'))
			.filter((source) => !term || `${source.name} ${relativePath(source.path)} ${source.synopsis ?? ''} ${source.columns?.map((column) => column.name).join(' ') ?? ''}`.toLocaleLowerCase().includes(term))
			.sort((left, right) => left.name.localeCompare(right.name, undefined, { numeric: true }) || relativePath(left.path).localeCompare(relativePath(right.path), undefined, { numeric: true }));
	});
	let dataCount = $derived(sources.filter((source) => source.view).length);
	let documentCount = $derived(sources.filter((source) => !source.view).length);
	let folderLabel = $derived(folder === 'all' ? 'All files' : folder === '__root' ? 'Workspace root' : folder);
</script>

<main class="files-page" aria-label="Workspace files">
	<header class="files-heading">
		<div>
			<h1 class:folder-title={folder !== 'all'}>{folderLabel}</h1>
			<p class="files-summary">
				{#if session.catalog.workspace}
					{dataCount} queryable {dataCount === 1 ? 'source' : 'sources'} · {documentCount} readable {documentCount === 1 ? 'document' : 'documents'}
				{:else}
					Open a folder to browse its files.
				{/if}
			</p>
		</div>
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
				<Input aria-label="Search workspace files" bind:value={query} placeholder="Find a file or column…" />
			</label>
			<div class="files-filter fella-pill-nav" role="tablist" aria-label="Filter file types">
				{#each [{ id: 'all', label: 'All' }, { id: 'data', label: 'Tables' }, { id: 'documents', label: 'Documents' }] as filter (filter.id)}
					<button class="fella-pill-nav-item" type="button" role="tab" aria-selected={kind === filter.id} onclick={() => (kind = filter.id as typeof kind)}>{filter.label}</button>
				{/each}
			</div>
		</div>

		<div class="files-list" role="table" aria-label="Cataloged workspace sources">
			<div class="files-list-head" role="row">
				<span role="columnheader" aria-sort="ascending">Name <Icon name="arrow-up-down" size={12} /></span><span role="columnheader">Rows</span><span role="columnheader">Updated</span><span role="columnheader">Status</span>
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
	.files-page { display:flex; flex:1; flex-direction:column; min-width:0; min-height:0; overflow:hidden; padding:64px 32px 24px; color:var(--text); }
	.files-heading { display:flex; align-items:flex-start; justify-content:space-between; gap:20px; max-width:980px; width:100%; margin:0 auto; }
	.files-heading h1 { margin:2px 0 0; font-size:22px; line-height:33px; font-weight:500; letter-spacing:-.025em; }
	.files-summary { margin:5px 0 0; color:var(--text-dim); font-size:13px; }
	.folder-title { font-family:var(--mono); font-size:20px !important; }
	.files-controls { display:flex; align-items:center; gap:0; width:100%; max-width:980px; margin:24px auto 0; }
	.files-search { display:flex; flex:1; min-width:120px; align-items:center; gap:8px; padding-left:12px; border:1px solid var(--border); border-radius:999px; background:var(--bg-inset); color:var(--text-faint); }
	.files-search :global(input) { min-width:0; min-height:36px; border:0; background:transparent; box-shadow:none; }
	.files-filter { flex:none; }
	.files-filter .fella-pill-nav-item { min-height:28px; padding-inline:11px; }
	.files-list { flex:1; min-height:0; overflow-y:auto; width:100%; max-width:none; margin:20px auto 0; }
	.files-list-head, .file-row { display:grid; grid-template-columns:minmax(0, 1fr) 72px 76px 104px; align-items:center; gap:12px; }
	.files-list-head { position:sticky; top:0; z-index:1; min-height:26px; padding:0 12px 8px; background:var(--bg); border-bottom:1px solid var(--border); color:var(--text-faint); font-size:11px; }
	.files-list-head span:first-child { display:flex; align-items:center; gap:5px; }
	.files-list-head span:not(:first-child) { text-align:right; }
	.file-row { width:100%; min-width:0; min-height:58px; padding:10px 12px; border:0; border-radius:12px; background:transparent; color:var(--text-dim); text-align:left; cursor:pointer; }
	.file-row:hover { background:var(--bg-raised); }
	.file-row.selected { background:var(--bg-raised-2); }
	.file-name-cell { display:flex; min-width:0; align-items:center; gap:11px; }
	.file-type-icon { display:grid; width:15px; height:20px; flex:none; place-items:center; border-radius:0; background:transparent; color:var(--text-faint); }
	.file-row.selected .file-type-icon { color:var(--brand-icon); }
	.file-name-copy { display:grid; min-width:0; gap:2px; }
	.file-name-copy strong { overflow:hidden; color:var(--text); font-size:13px; font-weight:400; text-overflow:ellipsis; white-space:nowrap; }
	.file-name-copy small { overflow:hidden; color:var(--text-faint); font-size:11px; text-overflow:ellipsis; white-space:nowrap; }
	.file-row-count, .file-updated, .file-access { color:var(--text-dim); font-size:var(--fs-xs); text-align:right; }
	.file-row-count { font-family:var(--mono); }
	.file-access.queryable { color:var(--brand-icon); }
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
	@container primary-pane (max-width:600px) {
		.files-page { padding-inline:20px; }
		.files-controls { flex-wrap:wrap; }
		.files-search { flex-basis:100%; }
		.files-list-head, .file-row { grid-template-columns:minmax(0,1fr) 64px 90px; }
		.files-list-head span:nth-child(3), .file-updated { display:none; }
	}
	@container primary-pane (max-width:400px) {
		.files-page { padding-inline:12px; }
		.files-list-head, .file-row { grid-template-columns:minmax(0,1fr) 80px; }
		.files-list-head span:nth-child(2), .file-row-count { display:none; }
	}
</style>
