<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { openFolder } from '$lib/commands';
	import { ipc } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import type {
		ColumnInfo,
		Investigation,
		InvestigationSourceSelection,
		SkippedFile,
		SourceInfo
	} from '$lib/types';
	import Icon from './Icon.svelte';
	import SourcePreview from './SourcePreview.svelte';

	let {
		workspacePath,
		investigation = null,
		onUpdated
	}: {
		workspacePath: string;
		investigation?: Investigation | null;
		onUpdated?: (investigation: Investigation) => void;
	} = $props();
	let selectedKey = $state<string | null>(null);
	let selectionSaving = $state(false);
	let selectionError = $state('');
	let showAllSources = $state(false);
	let showAllSkipped = $state(false);
	let showAllColumns = $state(false);
	let sourceQuery = $state('');
	let skippedQuery = $state('');
	let catalogIdentity = '';
	const SOURCE_LIMIT = 5;
	const SKIPPED_LIMIT = 5;
	const FIELD_LIMIT = 12;

	let catalog = $derived(session.catalog.workspace === workspacePath ? session.catalog : null);
	let sources = $derived(catalog?.sources ?? []);
	let skipped = $derived(catalog?.skipped ?? []);
	let matchingSources = $derived.by(() => {
		const query = sourceQuery.trim().toLowerCase();
		if (!query) return sources;
		return sources.filter((source) => {
			const columns = (source.columns ?? []).flatMap((column) => [
				column.name,
				column.type,
				column.note,
				column.min,
				column.max,
				...(column.common_values ?? [])
			]);
			const haystack = [
				source.name,
				relativePath(source.path),
				source.kind,
				source.note,
				source.synopsis,
				...columns
			].filter(Boolean).join(' ').toLowerCase();
			return haystack.includes(query);
		});
	});
	let visibleSources = $derived(
		showAllSources ? matchingSources : matchingSources.slice(0, SOURCE_LIMIT)
	);
	let matchingSkipped = $derived.by(() => {
		const query = skippedQuery.trim().toLowerCase();
		return query
			? skipped.filter((file) => `${file.name} ${file.reason}`.toLowerCase().includes(query))
			: skipped;
	});
	let visibleSkipped = $derived(showAllSkipped ? matchingSkipped : matchingSkipped.slice(0, SKIPPED_LIMIT));
	let selected = $derived(sources.find((source) => sourceKey(source) === selectedKey) ?? null);
	let matchingFields = $derived.by(() => {
		const query = sourceQuery.trim().toLowerCase();
		if (!query || !selected) return [];
		return (selected.columns ?? []).filter((column) =>
			[
				column.name,
				column.type,
				column.note,
				column.min,
				column.max,
				...(column.common_values ?? [])
			]
				.filter(Boolean).join(' ').toLowerCase().includes(query)
		);
	});
	let profileColumns = $derived(
		sourceQuery.trim() && matchingFields.length ? matchingFields : selected?.columns ?? []
	);
	let visibleColumns = $derived(
		showAllColumns || (sourceQuery.trim() && matchingFields.length > 0)
			? profileColumns
			: profileColumns.slice(0, FIELD_LIMIT)
	);
	let readyTables = $derived(sources.filter((source) => source.view && isReady(source)).length);
	let readyDocuments = $derived(sources.filter((source) => !source.view && isReady(source)).length);

	$effect(() => {
		const identity = `${workspacePath}\u0000${catalog?.revision ?? ''}\u0000${investigation?.id ?? ''}`;
		if (identity === catalogIdentity) return;
		catalogIdentity = identity;
		selectedKey = null;
		showAllSources = false;
		showAllSkipped = false;
		sourceQuery = '';
		skippedQuery = '';
	});

	$effect(() => {
		const query = sourceQuery;
		if (!query.trim()) return;
		if (selectedKey && !matchingSources.some((source) => sourceKey(source) === selectedKey)) {
			selectedKey = null;
		}
		showAllSources = false;
	});

	$effect(() => {
		const path = session.selectedSourcePath;
		if (!path) return;
		const source = sources.find((item) => item.path === path);
		if (source) selectedKey = sourceKey(source);
	});

	function sourceKey(source: SourceInfo): string {
		return `${source.path}\u0000${source.view ?? source.name}`;
	}

	function isReady(source: SourceInfo): boolean {
		const capabilities = session.settings?.capabilities;
		if (!capabilities) return true;
		return source.view ? capabilities.table_analysis : capabilities.document_analysis;
	}

	function readinessLabel(source: SourceInfo): string {
		if (!isReady(source)) return source.view ? 'Table analysis off' : 'Document analysis off';
		return source.view ? 'Queryable' : 'Readable';
	}

	function skippedLabel(file: SkippedFile): string {
		const reason = file.reason.toLowerCase();
		if (reason.includes("can't read this file type")) return 'Unsupported';
		if (reason.includes("couldn't be read") || reason.includes("couldn't be opened")) return 'Unavailable';
		return 'Could not load';
	}

	function kindLabel(source: SourceInfo): string {
		return source.kind.toUpperCase();
	}

	function toggleSource(source: SourceInfo): void {
		const key = sourceKey(source);
		selectedKey = selectedKey === key ? null : key;
		session.selectSource(selectedKey ? source.path : null);
		showAllColumns = false;
	}

	function closeSourceDetails(): void {
		selectedKey = null;
		session.selectSource(null);
	}

	function missingCoverage(column: ColumnInfo, rowCount: number | undefined): string {
		if (rowCount === 0) return 'No rows loaded';
		if (column.null_fraction == null) return 'Not profiled';
		const fraction = Math.max(0, Math.min(1, column.null_fraction));
		if (fraction === 0) return 'None observed';
		const missing = rowCount == null ? null : Math.round(rowCount * fraction);
		const percent = Math.round(fraction * 100);
		const percentage = percent === 0 ? '<1%' : `${percent}%`;
		return missing == null
			? `${percentage} missing`
			: `${new Intl.NumberFormat().format(missing)} missing · ${percentage}`;
	}

	function observedRange(column: ColumnInfo, rowCount: number | undefined): string {
		if (rowCount === 0) return 'No observed values';
		if (column.min == null && column.max == null) return 'Not available';
		if (column.min == null) return `Through ${column.max}`;
		if (column.max == null || column.min === column.max) return column.min;
		return `${column.min} – ${column.max}`;
	}

	function relativePath(path: string): string {
		const root = workspacePath.replace(/[/\\]+$/, '');
		if (path === root) return path;
		if (path.startsWith(`${root}/`) || path.startsWith(`${root}\\`)) {
			return path.slice(root.length + 1).replace(/\\/g, '/');
		}
		return path;
	}

	function formatCount(value: number | undefined): string {
		return value == null ? '—' : new Intl.NumberFormat().format(value);
	}

	function formatBytes(value: number): string {
		if (!Number.isFinite(value) || value < 1024) return `${Math.max(0, value || 0)} B`;
		const units = ['KB', 'MB', 'GB'];
		let amount = value / 1024;
		let unit = units[0];
		for (let index = 1; amount >= 1024 && index < units.length; index++) {
			amount /= 1024;
			unit = units[index];
		}
		return `${amount < 10 ? amount.toFixed(1) : Math.round(amount)} ${unit}`;
	}

	function isSourceIncluded(source: SourceInfo): boolean {
		const selection = investigation?.source_selection;
		return !selection || selection.mode === 'all_sources' ||
			selection.included_paths.includes(relativePath(source.path));
	}

	function uniqueSourcePaths(): string[] {
		return [...new Set(sources.map((source) => relativePath(source.path)))];
	}

	function isFirstSourceForPath(source: SourceInfo): boolean {
		const path = relativePath(source.path);
		return sources.find((item) => relativePath(item.path) === path) === source;
	}

	async function saveSourceSelection(sourceSelection: InvestigationSourceSelection): Promise<void> {
		if (!investigation || selectionSaving) return;
		selectionSaving = true;
		selectionError = '';
		try {
			const updated = await ipc.investigationUpdateSourceSelection(investigation.id, sourceSelection);
			onUpdated?.(updated);
		} catch (cause) {
			selectionError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			selectionSaving = false;
		}
	}

	function chooseAllSources(): void {
		void saveSourceSelection({ mode: 'all_sources', included_paths: [] });
	}

	function chooseSpecificSources(): void {
		if (!investigation) return;
		const includedPaths = investigation.source_selection.mode === 'all_sources'
			? uniqueSourcePaths()
			: investigation.source_selection.included_paths;
		void saveSourceSelection({ mode: 'selected_sources', included_paths: includedPaths });
	}

	function setSourceIncluded(source: SourceInfo, included: boolean): void {
		if (!investigation) return;
		const path = relativePath(source.path);
		const selectedPaths = new Set(
			investigation.source_selection.mode === 'all_sources'
				? uniqueSourcePaths()
				: investigation.source_selection.included_paths
		);
		if (included) selectedPaths.add(path);
		else selectedPaths.delete(path);
		void saveSourceSelection({
			mode: 'selected_sources',
			included_paths: [...selectedPaths]
		});
	}
</script>

<section
	class="source-readiness"
	aria-labelledby="source-readiness-title"
	data-testid="investigation-data-readiness"
>
	<header class="readiness-heading">
		<h3 id="source-readiness-title">Data available</h3>
	</header>
	{#if catalog}
		<section class="availability-card" aria-label="Available data summary">
			<div class="availability-metrics">
				<div><strong>{readyTables}</strong><span>queryable table{readyTables === 1 ? '' : 's'}</span></div>
				<div><strong>{readyDocuments}</strong><span>readable document{readyDocuments === 1 ? '' : 's'}</span></div>
			</div>
			<details class="catalog-details"><summary><Icon name="chevron-right" size={12} />Catalog details</summary><div class="catalog-meta" aria-label="Current source catalog metadata">
				<span class="catalog-revision" data-testid="current-catalog-revision">
					Catalog revision
					{#if catalog.revision}<code title={catalog.revision}>{catalog.revision}</code>{:else}<span>not recorded</span>{/if}
				</span>
				{#if catalog.indexed_at_ms}
					<span class="indexed-at">Indexed {new Date(catalog.indexed_at_ms).toLocaleDateString()}</span>
				{/if}
			</div><p class="readiness-caveat">Loaded files and profiled rows do not guarantee a source can answer this Investigation.</p></details>
		</section>
	{:else}
		<p class="catalog-loading" role="status">Loading this workspace’s catalog…</p>
	{/if}
	{#if catalog && investigation}
		<fieldset class="scope-controls" disabled={selectionSaving}>
			<legend>Sources for future runs</legend>
			<div class="scope-segment">
			<label class:active={investigation.source_selection.mode === 'all_sources'}>
				<input
					type="radio"
					name={`source-scope-${investigation.id}`}
					checked={investigation.source_selection.mode === 'all_sources'}
					onchange={chooseAllSources}
				/>
				<span>All sources</span>
			</label>
			<label class:active={investigation.source_selection.mode === 'selected_sources'}>
				<input
					type="radio"
					name={`source-scope-${investigation.id}`}
					checked={investigation.source_selection.mode === 'selected_sources'}
					onchange={chooseSpecificSources}
				/>
				<span>Choose files</span>
			</label>
			</div>
			<span class="sr-only" role="status">
				{investigation.source_selection.mode === 'all_sources'
					? `All ${uniqueSourcePaths().length} cataloged files are selected for future runs.`
					: `${investigation.source_selection.included_paths.length} of ${uniqueSourcePaths().length} cataloged files selected for future runs.`}
			</span>
		</fieldset>
		{#if selectionError}
			<p class="scope-error" role="alert">{selectionError}</p>
		{/if}
	{/if}

	{#if catalog && sources.length}
		<div class="source-search">
			<Icon name="search" size={16} />
			<Input
				class="source-search-input"
				aria-label="Find a source or field"
				bind:value={sourceQuery}
				placeholder="Find a source or field…"
				spellcheck="false"
			/>
			{#if sourceQuery.trim()}
				<span class="source-search-count">{matchingSources.length} match{matchingSources.length === 1 ? '' : 'es'}</span>
			{/if}
		</div>
		<div class="source-list-heading">
			<h4>Cataloged sources</h4>
			<span>{sources.length}</span>
		</div>
		<ul class="source-list" aria-label="Cataloged data sources">
			{#each visibleSources as source (sourceKey(source))}
				<li>
					<div class="source-row">
						{#if investigation && isFirstSourceForPath(source)}
							<label class="source-scope-toggle">
								<input
									type="checkbox"
									checked={isSourceIncluded(source)}
									disabled={selectionSaving}
									aria-label={`Include ${relativePath(source.path)} in this Investigation`}
									onchange={(event) => setSourceIncluded(source, event.currentTarget.checked)}
								/>
							</label>
						{:else if investigation}
							<span class="source-scope-toggle source-scope-toggle-spacer" aria-hidden="true"></span>
						{/if}
						<button
							class="source-option"
							class:selected={selectedKey === sourceKey(source)}
							class:disabled={!isReady(source)}
							type="button"
							aria-expanded={selectedKey === sourceKey(source)}
							aria-controls={selectedKey === sourceKey(source) ? 'investigation-source-detail' : undefined}
							onclick={() => toggleSource(source)}
						>
							<span class="source-main">
								<strong>{source.name}</strong>
								<small class="source-relative-path" title={source.path}>{relativePath(source.path)} · {kindLabel(source)}{source.view && source.row_count != null ? ` · ${formatCount(source.row_count)} rows` : ''}</small>
							</span>
							<span class="source-readiness-label" data-state={isReady(source) ? 'ready' : 'disabled'}>{readinessLabel(source)}</span>
						</button>
					</div>
				</li>
			{/each}
		</ul>
		{#if matchingSources.length > SOURCE_LIMIT}
			<Button class="show-more" variant="ghost" size="sm" type="button" onclick={() => showAllSources = !showAllSources}>
				{showAllSources ? 'Show fewer sources' : `Show all ${matchingSources.length} matching sources`}
			</Button>
		{/if}
		{#if !matchingSources.length}
			<p class="empty-sources">No sources or fields match “{sourceQuery}”.</p>
		{/if}
		{#if selected}
			<section id="investigation-source-detail" class="source-detail" aria-label={`Details for ${selected.name}`}>
				<header class="detail-heading">
					<div>
						<h4>{selected.name}</h4>
						<p>{relativePath(selected.path)}</p>
					</div>
					<div class="detail-actions">
						<Button variant="outline" size="sm" type="button" aria-label="Open source preview beside the active conversation" onclick={() => session.openSourcePane(selected)}>Open beside</Button>
						<Button variant="ghost" size="sm" type="button" aria-label="Close source details" onclick={closeSourceDetails}>Close</Button>
					</div>
				</header>
				<div class="source-facts">
					<span><small>Type</small><strong>{kindLabel(selected)}</strong></span>
					{#if selected.view}
						<span><small>Rows</small><strong>{formatCount(selected.row_count)}</strong></span>
						<span><small>Columns</small><strong>{formatCount(selected.columns?.length)}</strong></span>
					{/if}
					<span><small>Size</small><strong>{formatBytes(selected.size_bytes)}</strong></span>
				</div>
				{#if selected.note}
					<p class="source-note"><strong>Ingest note</strong>{selected.note}</p>
				{/if}
				{#if selected.view && selected.columns?.length}
					<details class="field-profile">
						<summary>
							Observed field profile
							{#if sourceQuery.trim() && matchingFields.length}
								({matchingFields.length} matching of {selected.columns.length} fields)
							{:else}
								({selected.columns.length} fields)
							{/if}
						</summary>
						<p class="profile-caveat">
							Missingness and minimum/maximum values are measured from the loaded rows only. Compare them with the Question’s population and time period; they do not establish complete coverage.
						</p>
						<div class="profile-table-wrap">
							<table aria-label={`Observed field profile for ${selected.name}`}>
								<thead>
									<tr>
										<th scope="col">Field</th>
										<th scope="col">Type</th>
										<th scope="col">Missing</th>
										<th scope="col">Observed range</th>
										<th scope="col">Distinct</th>
										<th scope="col">Ingest note</th>
									</tr>
								</thead>
								<tbody>
									{#each visibleColumns as column (column.name)}
										<tr>
											<th scope="row">{column.name}</th>
											<td>{column.type}</td>
										<td
											data-missing={column.null_fraction != null && column.null_fraction > 0 ? 'true' : 'false'}
										>
											{missingCoverage(column, selected.row_count)}
											</td>
											<td>{observedRange(column, selected.row_count)}</td>
											<td>{column.distinct == null ? '—' : formatCount(column.distinct)}</td>
											<td>{column.note ?? '—'}</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
						{#if selected.columns.length > FIELD_LIMIT && !(sourceQuery.trim() && matchingFields.length)}
							<Button
								class="show-more"
								variant="ghost"
								size="sm"
								type="button"
								onclick={() => showAllColumns = !showAllColumns}
							>
								{showAllColumns ? 'Show fewer fields' : `Show all ${selected.columns.length} fields`}
							</Button>
						{/if}
					</details>
				{/if}
				{#if selected.synopsis}
					<p class="source-synopsis">{selected.synopsis}</p>
				{/if}
				{#if !isReady(selected)}
					<p class="capability-note">This source is cataloged, but its analysis capability is turned off in Settings.</p>
				{:else if selected.view}
					<SourcePreview source={selected} workspacePath={workspacePath} revision={catalog?.revision ?? null} />
				{:else if !selected.synopsis}
					<p class="capability-note">This document is cataloged and available to document analysis.</p>
				{/if}
			</section>
		{/if}
	{:else if catalog}
		<p class="empty-sources">Fella could not load any usable sources from this workspace.</p>
		<Button class="show-more" variant="outline" size="sm" type="button" onclick={() => void openFolder()}>
			Choose another folder
		</Button>
	{/if}

	{#if catalog && skipped.length}
		<details class="skipped-sources">
			<summary>Files not available to Fella ({skipped.length})</summary>
			<label class="skipped-search">
				<Icon name="search" size={16} />
				<Input
					class="skipped-filter-input"
					aria-label="Filter unavailable files"
					bind:value={skippedQuery}
					placeholder="Find an unavailable file…"
					spellcheck="false"
				/>
			</label>
			<p class="skipped-count">{matchingSkipped.length} matching file{matchingSkipped.length === 1 ? '' : 's'}</p>
			<ul>
				{#each visibleSkipped as file (file.name)}
					<li>
						<span class="skipped-name">{file.name}</span>
						<span class="skipped-status">{skippedLabel(file)}</span>
						<small>{file.reason}</small>
					</li>
				{/each}
			</ul>
			{#if matchingSkipped.length > SKIPPED_LIMIT}
				<Button class="show-more" variant="ghost" size="sm" type="button" onclick={() => showAllSkipped = !showAllSkipped}>
					{showAllSkipped ? 'Show fewer files' : `Show all ${matchingSkipped.length} files`}
				</Button>
			{/if}
		</details>
	{/if}
</section>

<style>
	.catalog-details { color: var(--text-dim); font-size: var(--fs-xs); }
	.catalog-details > summary { display:flex; width:fit-content; align-items:center; gap:5px; cursor:pointer; list-style:none; }
	.catalog-details > summary::-webkit-details-marker { display:none; }
	.catalog-details > summary :global(svg) { flex:none; transition:transform 120ms ease; }
	.catalog-details[open] > summary :global(svg) { transform:rotate(90deg); }
	.catalog-details .catalog-meta { margin-top: 8px; }
	.source-relative-path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.source-readiness { padding: 0; border: 0; }
	.readiness-heading { margin: 0 4px 7px; }
	.readiness-heading h3 { margin: 0; color: var(--text-faint); font-size:var(--fs-xs); font-weight:400; }
	.catalog-loading { margin: 0; padding: 14px; border-radius: var(--radius-window); background: var(--bg-raised); color: var(--text-faint); font-size: var(--fs-xs); }
	.availability-card { display:grid; gap:var(--space-2); padding:13px 14px 11px; border-radius:var(--radius-window); background:var(--bg-raised); }
	.availability-metrics { display:flex; align-items:baseline; flex-wrap:wrap; gap:5px 16px; }
	.availability-metrics > div { display:flex; min-width:0; align-items:baseline; gap:5px; }
	.availability-metrics strong { color:var(--text); font-size:20px; font-weight:500; font-variant-numeric:tabular-nums; }
	.availability-metrics span { color:var(--text-dim); font-size:var(--fs-xs); }
	.readiness-caveat, .profile-caveat { margin: var(--space-2) 0 0; color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.45; }
	.scope-controls { display:block; min-width:0; margin:18px 0 0; padding:0; border:0; color:var(--text-dim); font-size:var(--fs-xs); }
	.scope-controls legend { width:100%; padding:0; margin:0 4px 7px; color:var(--text-faint); font-size:var(--fs-xs); font-weight:400; }
	.scope-segment { display:flex; min-width:0; padding:4px; border-radius:999px; background:var(--bg-raised); }
	.scope-controls label { position:relative; display:flex; min-width:0; min-height:32px; flex:1; align-items:center; justify-content:center; padding:4px 8px; border-radius:999px; color:var(--text-dim); text-align:center; cursor:pointer; }
	.scope-controls label.active { background:var(--bg-raised-2); color:var(--text); }
	.scope-controls input { position:absolute; width:1px; height:1px; opacity:0; }
	.scope-controls label:focus-within { outline:2px solid var(--brand); outline-offset:2px; }
	.source-scope-toggle { display: inline-flex; align-items: center; justify-content:center; flex:none; padding:8px 2px; cursor:pointer; }
	.source-scope-toggle input { display:grid; width:16px; height:16px; place-content:center; margin:0; appearance:none; border:1px solid var(--border-strong, var(--border)); border-radius:5px; background:transparent; cursor:pointer; }
	.source-scope-toggle input:checked { border-color:var(--brand); background:var(--brand); }
	.source-scope-toggle input:checked::after { width:7px; height:4px; content:''; transform:rotate(-45deg) translateY(-1px); border-bottom:2px solid white; border-left:2px solid white; }
	.source-scope-toggle input:focus-visible { outline:2px solid var(--brand); outline-offset:2px; }
	.source-scope-toggle input:disabled { opacity:.55; cursor:default; }
	.scope-error { margin: var(--space-2) 0 0; color: var(--warning, #9b6500); font-size: var(--fs-xs); }
	.catalog-meta { display: flex; flex: none; flex-wrap: wrap; justify-content: flex-end; gap: 4px var(--space-3); color: var(--text-faint); font-size: var(--fs-xs); }
	.catalog-revision { display: inline-flex; align-items: baseline; gap: 4px; }
	.catalog-revision code { max-width: min(42vw, 360px); color: var(--text-dim); overflow-wrap: anywhere; }
	.indexed-at { flex: none; }
	.source-list-heading { display: flex; align-items: baseline; justify-content: space-between; gap: var(--space-3); margin: 18px 4px 7px; }
	.source-list-heading h4 { margin: 0; color: var(--text); font-size:var(--fs-xs); font-weight:400; }
	.source-list-heading span { color: var(--text-faint); font-size: 11px; }
	.source-list { display: grid; grid-template-columns: minmax(0, 1fr); gap: 0; margin: 0; padding:5px 8px; border-radius:var(--radius-window); background:var(--bg-raised); list-style: none; }
	.source-list li + li { border-top:0; }
	.source-search { display: flex; min-width:0; align-items: center; gap: var(--space-2); margin-top: 11px; padding:0 11px; border:1px solid var(--border); border-radius:999px; background:var(--bg-inset); color:var(--text-faint); }
	.source-search :global(input) { min-width:0; min-height:34px; height:34px; padding:7px 0; border:0; background:transparent; box-shadow:none; font-size:12.5px; }
	.source-search-count { flex: none; font-size: var(--fs-xs); }
	.source-row { display: flex; min-width: 0; align-items: center; gap: 7px; }
	.source-scope-toggle-spacer { width:20px; flex:none; }
	.source-option { display: flex; flex: 1; width: 100%; min-width: 0; align-items: center; gap: 8px; padding: 8px 2px; border: 0; border-radius: var(--radius-md); background: transparent; color: var(--text); text-align: left; cursor: pointer; }
	.source-option:hover, .source-option.selected { background: var(--bg-raised-2); }
	.source-option.disabled { opacity: .72; }
	.source-main { display: grid; min-width: 0; flex: 1; gap: 2px; overflow:hidden; }
	.source-main strong { overflow:hidden; font-size:var(--fs-xs); font-weight:400; text-overflow:ellipsis; white-space:nowrap; }
	.source-main small { overflow:hidden; color: var(--text-faint); font-size: 11px; text-overflow:ellipsis; white-space:nowrap; }
	.source-readiness-label, .skipped-status { flex: none; color: var(--text-dim); font-size: 11px; }
	.source-readiness-label[data-state='ready'] { color: var(--brand-icon, var(--brand)); }
	.source-readiness-label[data-state='disabled'], .skipped-status { color: var(--warning, #9b6500); }
	:global(.show-more) { margin-top: var(--space-2); }
	.source-detail { margin-top: var(--space-3); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-window); background: var(--bg-inset); }
	.detail-heading { align-items: center; }
	.detail-actions { display: flex; flex: none; align-items: center; gap: var(--space-1); }
	.detail-heading h4 { margin: 0; overflow-wrap: anywhere; font-size: var(--fs-sm); font-weight: 650; }
	.detail-heading p { overflow-wrap: anywhere; }
	.source-facts { display: flex; flex-wrap: wrap; gap: var(--space-4); margin-top: var(--space-3); }
	.source-facts span { display: grid; gap: 2px; }
	.source-facts small { color: var(--text-faint); font-size: 11px; }
	.source-facts strong { font-size:var(--fs-xs); font-weight:400; }
	.source-note, .source-synopsis, .capability-note, .empty-sources { margin: var(--space-3) 0 0; color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.source-note strong { margin-right: var(--space-2); color: var(--text); }
	.field-profile { margin-top: var(--space-3); color: var(--text-dim); font-size: var(--fs-xs); }
	.field-profile summary { width: fit-content; cursor: pointer; color: var(--text); font-weight: 600; }
	.profile-table-wrap { max-width: 100%; margin-top: var(--space-2); overflow: auto; border: 1px solid var(--border); border-radius: var(--radius-sm); }
	.profile-table-wrap table { width: 100%; border-collapse: collapse; font-size: 11px; text-align: left; }
	.profile-table-wrap th, .profile-table-wrap td { max-width: 240px; padding: 6px 8px; border-bottom: 1px solid var(--border); overflow-wrap: anywhere; vertical-align: top; }
	.profile-table-wrap thead th { position: sticky; top: 0; background: var(--bg-inset); color: var(--text-dim); font-weight: 600; white-space: nowrap; }
	.profile-table-wrap tbody th { color: var(--text); font-weight: 600; }
	.profile-table-wrap tbody tr:last-child th, .profile-table-wrap tbody tr:last-child td { border-bottom: 0; }
	.profile-table-wrap td[data-missing='true'] { color: var(--warning, #9b6500); }
	.skipped-sources { margin-top: var(--space-3); color: var(--text-dim); font-size: var(--fs-xs); }
	.skipped-sources summary { width: fit-content; cursor: pointer; }
	.skipped-search { display: flex; align-items: center; gap: var(--space-2); max-width: 520px; margin-top: var(--space-2); color: var(--text-faint); }
	.skipped-search :global(input) { min-width: 0; }
	.skipped-count { margin: var(--space-2) 0 0; color: var(--text-faint); font-size: 11px; }
	.skipped-sources ul { display: grid; gap: var(--space-2); margin: var(--space-2) 0 0; padding: 0; list-style: none; }
	.skipped-sources li { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px var(--space-3); padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--radius-sm); }
	.skipped-name { min-width: 0; color: var(--text); overflow-wrap: anywhere; }
	.skipped-sources li small { grid-column: 1 / -1; color: var(--text-faint); overflow-wrap: anywhere; }
	@container investigation-pane (max-width: 900px) {
		.catalog-meta { justify-content: flex-start; }
		.scope-segment { width:100%; }
		.source-readiness-label { font-size:11px; }
	}
</style>
