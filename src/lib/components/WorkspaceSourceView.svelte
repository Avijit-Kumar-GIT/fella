<script lang="ts">
	import { ipc } from '$lib/ipc';
	import type { Investigation, QueryResult, SourceInfo } from '$lib/types';
	import { session } from '$lib/session.svelte';
	import { focusInvestigationConversation } from '$lib/investigation-navigation';
	import { Spinner } from '$lib/components/ui/spinner';
	import { Input } from '$lib/components/ui/input';
	import { Button } from '$lib/components/ui/button';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import Icon from './Icon.svelte';

	let {
		source,
		workspacePath,
		revision,
		onexpand,
		onclose,
		onask
	}: { source: SourceInfo | null; workspacePath: string | null; revision: string | null; onexpand?: () => void; onclose?: () => void; onask?: (source: SourceInfo) => void } = $props();

	let rows = $state<QueryResult | null>(null);
	let documentText = $state('');
	let showFullDocument = $state(false);
	let usages = $state<Investigation[] | null>(null);
	let usageLoading = $state(false);
	let usageError = $state('');
	let truncated = $state(false);
	let loading = $state(false);
	let error = $state('');
	let generation = 0;
	let query = $state('');
	let copyStatus = $state('');
	let showAllSourceColumns = $state(false);
	let hiddenColumns = $state<number[]>([]);
	let sourceColumns = $derived(source?.columns ?? []);
	let shownSourceColumns = $derived(showAllSourceColumns ? sourceColumns : sourceColumns.slice(0, 12));
	let visibleColumns = $derived((rows?.columns ?? []).map((name, index) => ({ name, index })).filter(column => !hiddenColumns.includes(column.index)));
	let matchingRows = $derived((rows?.rows ?? []).filter(row => !query.trim() || row.some(value => valueText(value).toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))));

	function relativePath(path: string): string {
		const root = workspacePath?.replace(/[/\\]+$/, '') ?? '';
		if (!root) return path;
		const normalizedPath = path.replace(/\\/g, '/');
		const normalizedRoot = root.replace(/\\/g, '/').replace(/\/$/, '');
		return normalizedPath.toLocaleLowerCase().startsWith(`${normalizedRoot.toLocaleLowerCase()}/`)
			? normalizedPath.slice(normalizedRoot.length + 1)
			: normalizedPath;
	}

	function valueText(value: unknown): string {
		if (value == null) return '—';
		if (typeof value === 'object') return JSON.stringify(value);
		return String(value);
	}

	function fileSize(bytes: number): string {
		if (!Number.isFinite(bytes) || bytes < 0) return '—';
		if (bytes < 1024) return `${bytes} B`;
		const units = ['KB', 'MB', 'GB', 'TB'];
		let value = bytes / 1024;
		let unit = 0;
		while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1; }
		return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value)} ${units[unit]}`;
	}

	async function copySourcePath(): Promise<void> {
		if (!source) return;
		try {
			await navigator.clipboard.writeText(source.path);
			copyStatus = 'File path copied';
		} catch {
			copyStatus = 'Could not access the clipboard';
		}
	}

	async function loadUsages(): Promise<void> {
		const chosen = source, root = workspacePath;
		if (!chosen || !root || usageLoading || usages !== null) return;
		usageLoading = true; usageError = '';
		try {
			const investigations = (await ipc.investigationsList(session.conversations.map(conversation => conversation.id))).filter(item => item.workspace_id === root);
			const idsFor = (item:Investigation) => [...new Set([...(item.analysis_turn_ids ?? []), ...(item.questions ?? []).flatMap(question => question.latest_analysis_turn_id ? [question.latest_analysis_turn_id] : []), ...(item.findings ?? []).map(finding => finding.analysis_turn_id)])];
			const ids = [...new Set(investigations.flatMap(idsFor))];
			let unavailable = 0;
			const runs = await Promise.all(ids.map(id => ipc.analysisTurnLoad(id).catch(() => { unavailable++; return null; })));
			if (source?.path !== chosen.path || workspacePath !== root) return;
			const linked = new Set(runs.filter(run => run?.source_scope?.used_sources.some(input => input.source.replaceAll('\\','/') === chosen.path.replaceAll('\\','/') || input.source.replaceAll('\\','/') === relativePath(chosen.path) || (chosen.view && input.table === chosen.view))).map(run => run!.id));
			usages = investigations.filter(item => idsFor(item).some(id => linked.has(id)));
			if (unavailable) usageError = `${unavailable} saved runs could not be checked.`;
		} catch (cause) { if (source?.path === chosen.path) usageError = cause instanceof Error ? cause.message : String(cause); }
		finally { if (source?.path === chosen.path) usageLoading = false; }
	}
	async function openUsage(item:Investigation): Promise<void> {
		await focusInvestigationConversation(item, 'open');
		session.workspaceSection = 'chat';
	}

	$effect(() => {
		const selected = source;
		const root = workspacePath;
		const catalogRevision = revision;
		const request = ++generation;
		rows = null;
		query = ''; hiddenColumns = [];
		copyStatus = '';
		showAllSourceColumns = false;
		documentText = ''; showFullDocument = false; usages = null; usageLoading = false; usageError = '';
		truncated = false;
		error = '';
		if (!selected || !root) {
			loading = false;
			return;
		}
		loading = true;
		const load = selected.view
			? ipc.sampleSource(selected.name, 25, root).then((result) => {
				if (request === generation && catalogRevision === revision) rows = result;
			})
			: ipc.previewFile(relativePath(selected.path), root).then((result) => {
				if (request === generation && catalogRevision === revision) {
					documentText = result.text;
					truncated = result.truncated;
				}
			});
		void load.catch((cause) => {
			if (request === generation) error = cause instanceof Error ? cause.message : String(cause);
		}).finally(() => {
			if (request === generation) loading = false;
		});
	});
</script>

<section class="source-view" aria-label={source ? `File preview: ${source.name}` : 'File preview'}>
	{#if source}
		<div class="source-toolbar">
			<span>File details</span>
			<div class="source-toolbar-actions">
				<DropdownMenu.Root>
					<DropdownMenu.Trigger class="source-more-button" aria-label="More file actions" title="More file actions"><Icon name="more-horizontal" size={16} /></DropdownMenu.Trigger>
					<DropdownMenu.Portal><DropdownMenu.Content class="fella-ui-menu-content" align="end"><DropdownMenu.Item class="fella-ui-menu-item" onSelect={() => void copySourcePath()}><Icon name="file" size={14} />Copy file path</DropdownMenu.Item></DropdownMenu.Content></DropdownMenu.Portal>
				</DropdownMenu.Root>
				<Button variant="ghost" size="icon" aria-label="Close file details" title="Close file details" onclick={() => onclose?.()}><Icon name="x" size={14} /></Button>
			</div>
		</div>
		<div class="source-scroll">
		<header class="source-head">
			<div class="source-title-row">
				<span class="source-icon"><Icon name={source.view ? 'table' : 'file'} size={19} /></span>
				<div class="source-title-copy">
					<h1>{source.name}</h1>
					<p title={source.path}>{relativePath(source.path)}</p>
				</div>
			</div>
			<div class="source-metadata" aria-label="Source metadata">
				<span>{source.kind.toUpperCase()}</span>
				{#if source.view}<span>{new Intl.NumberFormat().format(source.row_count ?? 0)} rows</span>{/if}
				<span>{fileSize(source.size_bytes)}</span>
				<span>Updated {new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(source.mtime * 1000))}</span>
			</div>
			{#if copyStatus}<p class="source-copy-status" role="status">{copyStatus}</p>{/if}
			<Button class="source-ask-button" onclick={() => onask?.(source)}><Icon name="ask" size={14} />Ask about this file</Button>
		</header>
		{#if source.note}
			<div class="source-caveat"><Icon name="alert" size={14} /><p>{source.note}</p></div>
		{/if}
		{#if source.view && sourceColumns.length}
			<section class="source-schema" aria-label="Source columns">
				<div class="source-schema-heading"><span>Columns</span><small>{sourceColumns.length}</small></div>
				<ul>
					{#each shownSourceColumns as column (column.name)}
						<li><span class="source-column-copy"><code>{column.name}</code>{#if column.note}<small class:column-caveat={/text|coerc|mixed|date|type/i.test(column.note)}>{column.note}</small>{/if}</span><code class="source-column-type">{column.type}</code></li>
					{/each}
				</ul>
				{#if sourceColumns.length > 12}<button class="source-show-columns" type="button" aria-expanded={showAllSourceColumns} onclick={() => (showAllSourceColumns = !showAllSourceColumns)}>{showAllSourceColumns ? 'Show fewer columns' : `Show all ${sourceColumns.length} columns`}</button>{/if}
			</section>
		{/if}
		{#if loading}
			<div class="source-loading" role="status"><Spinner size={16} />Loading read-only preview…</div>
		{:else if error}
			<div class="source-message" role="alert">
				<strong>Preview unavailable</strong>
				<p>{error}</p>
			</div>
		{:else if source.view && rows}
			<div class="source-section-label preview-heading">Preview</div>
			<details class="preview-options"><summary>Preview options</summary><div class="table-controls">
				<Input aria-label="Filter preview rows" bind:value={query} placeholder="Find in this sample…" />
				<DropdownMenu.Root>
					<DropdownMenu.Trigger class="source-columns-trigger">Columns <Icon name="chevron-down" size={14} /></DropdownMenu.Trigger>
					<DropdownMenu.Portal><DropdownMenu.Content class="fella-ui-menu-content source-columns-menu" align="end">
						{#each rows.columns as column, index (index)}
							<DropdownMenu.CheckboxItem class="fella-ui-menu-item" checked={!hiddenColumns.includes(index)} disabled={visibleColumns.length === 1 && !hiddenColumns.includes(index)} onCheckedChange={(checked) => { hiddenColumns = checked ? hiddenColumns.filter(item => item !== index) : [...hiddenColumns, index]; }} closeOnSelect={false}>{column}{#if !hiddenColumns.includes(index)}<Icon name="check" size={14} />{/if}</DropdownMenu.CheckboxItem>
						{/each}
					</DropdownMenu.Content></DropdownMenu.Portal>
				</DropdownMenu.Root>
				{#if onexpand}<Button variant="ghost" size="sm" aria-label="Expand file preview" onclick={onexpand}><Icon name="arrow-up-right" size={14} /></Button>{/if}
			</div>
			</details>
			<!-- The region is focusable so keyboard users can scroll the wide read-only table. -->
			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<div class="table-scroll" role="region" tabindex="0" aria-label="First 25 rows of {source.name}">
				<table>
					<thead><tr>{#each visibleColumns as column (column.index)}<th scope="col">{column.name}</th>{/each}</tr></thead>
					<tbody>
						{#each matchingRows as row, rowIndex (rowIndex)}
							<tr>{#each visibleColumns as column (column.index)}<td title={valueText(row[column.index])}>{valueText(row[column.index])}</td>{/each}</tr>
						{/each}
						{#if !matchingRows.length}<tr><td colspan={visibleColumns.length}>No matching rows in this sample.</td></tr>{/if}
					</tbody>
				</table>
			</div>
			<p class="source-footnote">Read-only sample · {query.trim() ? `${matchingRows.length} matching of ${rows.rows.length} sampled rows` : `${rows.rows.length} of ${source.row_count ?? 'unknown'} rows`}. The original file is unchanged.</p>
		{:else if documentText.startsWith('(this PDF has no selectable text')}
			<div class="source-message" role="status">
				<strong>This PDF has no selectable text</strong>
				<p>It may be a scan or contain images of pages. Fella can’t preview or analyze text that isn’t embedded in the PDF.</p>
			</div>
		{:else if documentText}
			<div class="document-excerpt"><div class="source-section-label">Excerpt</div><p class="document-preview">{showFullDocument ? documentText : documentText.slice(0,800)}{#if !showFullDocument && documentText.length > 800}…{/if}</p>{#if documentText.length > 800}<button class="source-show-columns" onclick={() => (showFullDocument = !showFullDocument)}>{showFullDocument ? 'Show less' : 'Read more'}</button>{/if}</div>
			{#if truncated}<p class="source-footnote">Preview limited to the first section of this file. Fella’s analysis tools can search the wider document.</p>{/if}
		{:else}
			<div class="source-message" role="status">
				<strong>{source.view ? 'No rows to preview' : 'No readable text found'}</strong>
				<p>This preview is read-only and does not add the file to chat context or change the Investigation’s source scope.</p>
			</div>
		{/if}
		<details class="source-usage" ontoggle={(event) => { if (event.currentTarget.open) void loadUsages(); }}>
			<summary>Used in</summary>
			{#if usageLoading}<p role="status">Checking saved runs…</p>{:else if usages?.length}{#each usages as item (item.id)}<button onclick={() => void openUsage(item)}><Icon name="flask" size={13} /><span>{item.title}</span></button>{/each}{:else if usages}<p>No recorded Investigation uses this file yet.</p>{/if}
			{#if usageError}<p role="status">{usageError}</p>{/if}
		</details>
		</div>
	{:else}
		<div class="source-message source-missing" role="status">
			<strong>This file is no longer in the current catalog.</strong>
			<p>The saved analysis and its original source record remain unchanged. Refresh the workspace catalog to check for the file again.</p>
		</div>
	{/if}
</section>

<style>
	.table-controls { display: flex; align-items: center; gap: 8px; min-width: 0; }
	.table-controls :global(input) { min-width: 0; flex: 1; }
	:global(.source-columns-trigger) { display: inline-flex; align-items: center; gap: 6px; border: 1px solid var(--border); border-radius: var(--radius); padding: 7px 10px; background: var(--bg-raised); color: var(--text-dim); font: inherit; font-size: var(--fs-xs); cursor: pointer; }
	:global(.source-columns-menu) { max-height: 360px; overflow: auto; }
	.source-view { display: flex; flex: 1; flex-direction: column; min-width: 0; min-height: 0; gap:0; padding:0; overflow:hidden; color:var(--text); }
	.source-toolbar { display:flex; min-height:52px; flex:none; padding:16px 16px 0; align-items:center; justify-content:space-between; color:var(--text-dim); font-size:var(--fs-xs); }
	.source-toolbar-actions { display:flex; align-items:center; gap:6px; }
	.source-toolbar-actions :global(button), :global(.source-more-button) { display:grid; width:36px; min-height:36px; height:36px; place-items:center; border:0; border-radius:50%; background:var(--bg-raised); color:var(--text-dim); cursor:pointer; }
	.source-toolbar-actions :global(button:hover), :global(.source-more-button:hover) { background:var(--bg-raised-2); color:var(--text); }
	.source-scroll { flex:1; min-height:0; overflow-y:auto; overflow-x:hidden; padding:0 20px 20px; }
	.source-head { display: grid; gap:12px; padding:20px 0 0; }
	.source-title-row { display:flex; min-width:0; flex-direction:column; align-items:flex-start; gap:12px; }
	.source-icon { display: grid; width: 44px; height: 44px; flex: none; place-items: center; border-radius: 12px; background:var(--bg-raised-2); color:var(--brand-icon); }
	.source-title-copy { min-width: 0; }
	.source-title-copy h1 { margin: 0; font-size:16px; font-weight:500; overflow-wrap: anywhere; }
	.source-title-copy p { margin:2px 0 0; color:var(--text-faint); font:11.5px/18px var(--mono); overflow-wrap: anywhere; }
	.source-metadata { display: flex; flex-wrap: wrap; gap:4px 12px; color:var(--text-dim); font-size:12px; }
	.source-copy-status { margin:-7px 0 0; color:var(--brand-icon); font-size:11px; }
	:global(.source-ask-button) { display:flex; width:100%; min-height:40px; margin-top:8px; align-items:center; justify-content:center; gap:8px; border-radius:999px; background:var(--brand); color:var(--on-brand); font-size:var(--fs-xs); }
	:global(.source-ask-button:hover) { filter:brightness(.94); }
	.source-caveat { display:flex; align-items:flex-start; gap:9px; padding:11px 13px; border-radius:16px; background:color-mix(in srgb,var(--warn) 10%,var(--bg-raised)); color:var(--warn); font-size:var(--fs-xs); line-height:1.5; }
	.source-caveat :global(svg) { flex:none; margin-top:2px; }
	.source-caveat p { margin:0; color:var(--text-dim); }
	.source-schema { margin:24px 0 0; }
	.source-schema-heading { display:flex; justify-content:space-between; padding:0 4px 7px; color:var(--text-dim); font-size:var(--fs-xs); }
	.source-schema-heading small { color:var(--text-faint); }
	.source-schema ul { margin:0; padding:0; list-style:none; }
	.source-schema li { display:flex; min-width:0; align-items:flex-start; gap:12px; padding:8px 4px; border-bottom:1px solid var(--border); }
	.source-column-copy { display:grid; min-width:0; flex:1; gap:2px; }
	.source-column-copy > code { color:var(--text); font:12px var(--mono); overflow-wrap:anywhere; }
	.source-column-copy > small { color:var(--text-faint); font-size:11px; line-height:1.35; overflow-wrap:anywhere; }
	.source-column-copy > small.column-caveat { color:var(--warn); }
	.source-column-type { flex:none; color:var(--text-dim); font:11px var(--mono); }
	.source-show-columns { width:100%; padding:8px 4px; border:0; border-radius:9px; background:transparent; color:var(--text-dim); font:inherit; font-size:var(--fs-xs); text-align:left; cursor:pointer; }
	.source-show-columns:hover { background:var(--bg-raised); color:var(--text); }
	.table-scroll { max-width: 100%; overflow: auto; border: 1px solid var(--border); border-radius: 12px; }
	table { font-family:var(--mono); width:max-content; min-width: 100%; border-collapse: collapse; font-size: var(--fs-xs); }
	th, td { max-width: 240px; padding:6px 10px; border-bottom: 1px solid var(--border); text-align: left; vertical-align: top; }
	th { position: sticky; top: 0; background: var(--bg-inset); color: var(--text-dim); font-weight: 600; }
	td { overflow: hidden; color: var(--text); text-overflow: ellipsis; white-space: nowrap; }
	tr:last-child td { border-bottom: 0; }
	.document-excerpt { display:grid; gap:8px; margin-top:2px; }
	.source-section-label { padding:0 4px; color:var(--text-dim); font-size:var(--fs-xs); }
	.document-preview { min-height: 0; margin: 0; padding: 14px 15px; border-radius: 16px; background: var(--bg-raised); color: var(--chat-body); font: var(--fs-sm)/1.58 var(--sans); white-space: pre-wrap; overflow-wrap: anywhere; }
	.source-footnote { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.source-loading { display: flex; align-items: center; gap: var(--space-2); padding-top:16px; color: var(--text-dim); font-size: var(--fs-sm); }
	.source-message { display: grid; align-content: start; gap: var(--space-2); max-width: 56ch; padding: var(--space-4) 0; color: var(--text-dim); font-size: var(--fs-sm); }
	.source-message strong { color: var(--text); font-weight: 600; }
	.source-message p { margin: 0; line-height: 1.5; }
	.preview-heading { margin-top:24px; }
	.preview-options { margin:4px 0 8px; font-size:11.5px; color:var(--text-faint); }
	.preview-options summary { cursor:pointer; }
	.table-controls { margin-top:8px; }
	.source-usage { margin-top:24px; }
	.source-usage summary { padding:0 4px 8px; color:var(--text-dim); font-size:12px; cursor:pointer; }
	.source-usage > p { margin:4px; color:var(--text-faint); font-size:12.5px; line-height:19px; }
	.source-usage > button { display:flex; width:100%; min-width:0; align-items:center; gap:10px; padding:6px; border-radius:12px; color:var(--chat-body); text-align:left; font-size:12.5px; }
	.source-usage > button:hover { background:var(--bg-raised); color:var(--text); }
	.source-usage > button > span { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
</style>
