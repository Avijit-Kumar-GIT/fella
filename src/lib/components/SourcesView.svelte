<script lang="ts">
	import { baseName, openFolder } from '$lib/commands';
	import { session } from '$lib/session.svelte';
	import type { SourceInfo } from '$lib/types';
	import Icon from './Icon.svelte';
	import SourcePreview from './SourcePreview.svelte';

	let query = $state('');
	let selectedPath = $state<string | null>(null);

	let sources = $derived(session.catalog.sources);
	let workspace = $derived(session.catalog.workspace);
	let folderName = $derived(workspace ? baseName(workspace) : 'Your workspace');
	let skipped = $derived(session.catalog.skipped ?? []);
	let tabularCount = $derived(
		sources.filter((s) => ['csv', 'tsv', 'parquet', 'xlsx', 'json', 'ndjson'].includes(s.kind)).length
	);
	let documentCount = $derived(sources.length - tabularCount);

	let filtered = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q) return sources;
		return sources.filter((source) => {
			const haystack = `${source.name} ${source.path} ${source.kind} ${source.synopsis ?? ''}`;
			return haystack.toLowerCase().includes(q);
		});
	});

	let selected = $derived.by(() => sources.find((source) => source.path === selectedPath) ?? null);

	$effect(() => {
		if (selectedPath && sources.some((source) => source.path === selectedPath)) return;
		selectedPath = sources[0]?.path ?? null;
	});

	function select(source: SourceInfo): void {
		selectedPath = source.path;
	}

	function relativePath(path: string): string {
		if (!workspace) return path;
		const root = workspace.replace(/[/\\]+$/, '');
		if (path === root) return baseName(path);
		if (path.startsWith(root + '/') || path.startsWith(root + '\\')) {
			return path.slice(root.length + 1).replace(/\\/g, '/');
		}
		return path;
	}

	function formatBytes(bytes: number): string {
		if (!Number.isFinite(bytes) || bytes < 1024) return `${Math.max(0, bytes || 0)} B`;
		const units = ['KB', 'MB', 'GB'];
		let value = bytes / 1024;
		let unit = units[0];
		for (let i = 1; value >= 1024 && i < units.length; i++) {
			value /= 1024;
			unit = units[i];
		}
		return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${unit}`;
	}

	function formatCount(value: number | undefined): string {
		return value == null ? '—' : new Intl.NumberFormat().format(value);
	}

	function kindLabel(kind: SourceInfo['kind']): string {
		return kind.toUpperCase();
	}

	function formatIndexed(ms: number | undefined): string {
		if (!ms) return 'Not indexed yet';
		return new Date(ms).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' });
	}

	function shortRevision(revision: string | undefined): string {
		return revision ? revision.slice(0, 8) : '—';
	}
</script>

<section class="sources-page" aria-labelledby="sources-title">
	<header class="page-head">
		<div>
			<p class="eyebrow">Workspace</p>
			<h1 id="sources-title">Sources</h1>
			<p class="lede">
				{#if workspace}
					{folderName} · {sources.length} readable file{sources.length === 1 ? '' : 's'}
				{:else}
					Open a folder to see what Fella can work with.
				{/if}
			</p>
		</div>
		<button class="pill ghost" type="button" onclick={() => void openFolder()}>
			<Icon name="folder" size={16} /> {workspace ? 'Change folder' : 'Choose folder'}
		</button>
	</header>

	{#if !workspace}
		<div class="empty-state">
			<div class="empty-icon"><Icon name="folder" size={20} /></div>
			<h2>Your workspace is still empty</h2>
			<p>Choose a folder and Fella will catalog spreadsheets, documents, and notes without changing them.</p>
			<button class="pill primary" type="button" onclick={() => void openFolder()}>Choose a folder</button>
		</div>
	{:else}
		<div class="summary" aria-label="Source summary">
			<div class="summary-item"><strong>{sources.length}</strong><span>files</span></div>
			<div class="summary-item"><strong>{tabularCount}</strong><span>data files</span></div>
			<div class="summary-item"><strong>{documentCount}</strong><span>documents</span></div>
			{#if skipped.length}<div class="summary-item warn"><strong>{skipped.length}</strong><span>skipped</span></div>{/if}
		</div>
		<div class="freshness" title={session.catalog.revision ?? undefined}>
			<Icon name="check" size={16} />
			<span>Indexed {formatIndexed(session.catalog.indexed_at_ms)}</span>
			<span class="dot">·</span>
			<span>Snapshot <code>{shortRevision(session.catalog.revision)}</code></span>
		</div>

		<div class="toolbar">
			<label class="searchbox">
				<Icon name="search" size={16} />
				<span class="sr-only">Filter sources</span>
				<input bind:value={query} placeholder="Filter sources…" spellcheck="false" />
			</label>
			<span class="result-count">{filtered.length} shown</span>
		</div>

		{#if sources.length}
		<div class="source-layout">
			<div class="source-list" role="listbox" aria-label="Workspace sources">
				{#each filtered as source (source.path)}
					<button
						class="source-row"
						class:selected={selectedPath === source.path}
						type="button"
						role="option"
						aria-selected={selectedPath === source.path}
						onclick={() => select(source)}
					>
				<span class="source-icon"><Icon name={source.view ? 'table' : 'file'} size={16} /></span>
						<span class="source-copy">
							<strong>{source.name}</strong>
						</span>
						<span class="source-meta">
							<small>{kindLabel(source.kind)}</small>
							{#if source.row_count != null}<small>{formatCount(source.row_count)} rows</small>{/if}
						</span>
					</button>
				{:else}
					<div class="no-results">No sources match “{query}”.</div>
				{/each}
			</div>

			<aside class="detail" aria-label="Selected source details">
				{#if selected}
					<div class="detail-head">
				<div class="detail-icon"><Icon name={selected.view ? 'table' : 'file'} size={20} /></div>
						<div>
							<p class="eyebrow">{kindLabel(selected.kind)}</p>
							<h2>{selected.name}</h2>
						</div>
					</div>
					{#if relativePath(selected.path) !== selected.name}
						<p class="path">{relativePath(selected.path)}</p>
					{/if}

					<div class="facts">
						<div><span>Size</span><strong>{formatBytes(selected.size_bytes)}</strong></div>
						<div><span>Rows</span><strong>{formatCount(selected.row_count)}</strong></div>
						<div><span>Columns</span><strong>{formatCount(selected.columns?.length)}</strong></div>
					</div>
					<p class="source-freshness">File updated {selected.mtime ? new Date(selected.mtime * 1000).toLocaleString([], { dateStyle: 'medium' }) : 'unknown'}</p>

					{#if selected.synopsis}
						<p class="synopsis">{selected.synopsis}</p>
					{/if}
					{#if selected.note}
						<div class="note"><span>Note</span>{selected.note}</div>
					{/if}

					<SourcePreview source={selected} />

					{#if selected.columns?.length}
						<div class="columns">
							<div class="subhead"><span>Columns</span><span>{selected.columns.length}</span></div>
							{#each selected.columns.slice(0, 16) as column (column.name)}
								<div class="column-row">
									<div class="column-main"><code>{column.name}</code><span>{column.type}</span></div>
									{#if column.distinct != null}<small>{formatCount(column.distinct)} values</small>{/if}
									{#if column.common_values?.length}
										<div class="common-values" title="Frequent values from the latest index">
											<span>Common</span>{column.common_values.slice(0, 4).join(' · ')}
										</div>
									{/if}
								</div>
							{/each}
							{#if selected.columns.length > 16}<p class="more">+ {selected.columns.length - 16} more columns</p>{/if}
						</div>
					{/if}
				{:else}
					<div class="detail-empty">Select a source to inspect its shape and caveats.</div>
				{/if}
			</aside>
		</div>
		{:else}
			<div class="no-sources">
				<div class="empty-icon"><Icon name="folder" size={20} /></div>
				<h2>No readable sources yet</h2>
				<p>Fella found this folder, but it could not use any files in it yet.</p>
				<button class="pill ghost" type="button" onclick={() => void openFolder()}>Choose another folder</button>
			</div>
		{/if}

		{#if skipped.length}
			<details class="skipped">
				<summary><span>{skipped.length} skipped file{skipped.length === 1 ? '' : 's'}</span><span>Why?</span></summary>
				{#each skipped as item (item.name)}
					<div><code>{item.name}</code><span>{item.reason}</span></div>
				{/each}
			</details>
		{/if}
	{/if}
</section>

<style>
	.sources-page {
		flex: 1;
		min-height: 0;
		width: 100%;
		max-width: var(--content-max);
		margin: 0 auto;
		padding: var(--space-6) var(--pad) var(--space-6);
		overflow: auto;
	}
	.page-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-5);
		margin-bottom: var(--space-5);
	}
	.eyebrow {
		margin: 0 0 var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	h1,
	h2 {
		margin: 0;
		font-weight: 650;
		letter-spacing: -0.03em;
	}
	h1 {
		font-size: clamp(24px, 3vw, 32px);
		line-height: 1.15;
	}
	h2 {
		font-size: var(--fs-lg);
		line-height: 1.3;
	}
	.lede {
		margin: var(--space-2) 0 0;
		color: var(--text-dim);
	}
	.summary {
		display: flex;
		width: fit-content;
		max-width: 100%;
		align-items: stretch;
		gap: var(--space-5);
		margin-bottom: var(--space-5);
		background: transparent;
		border: 0;
		border-radius: 0;
		overflow: visible;
	}
	.summary-item {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 0;
		background: transparent;
	}
	.summary-item + .summary-item {
		padding-left: var(--space-5);
		border-left: 1px solid var(--border);
	}
	.summary-item strong {
		font-size: var(--fs-lg);
		font-weight: 650;
	}
	.summary-item span {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.summary-item.warn strong {
		color: var(--warn);
	}
	.freshness {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: -12px 0 var(--space-4);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.freshness :global(svg) {
		color: var(--ok);
		flex: none;
	}
	.freshness .dot {
		color: var(--border-strong);
	}
	.freshness code {
		font-family: var(--mono);
		font-size: var(--fs-xs);
	}
	.toolbar {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		margin-bottom: var(--space-2);
	}
	.searchbox {
		flex: 1;
		max-width: 360px;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		color: var(--text-faint);
		background: var(--bg-inset);
		border: 1px solid transparent;
		border-radius: var(--radius-sm);
	}
	.searchbox:focus-within {
		border-color: var(--border-strong);
		box-shadow: var(--focus-ring);
	}
	.searchbox input {
		width: 100%;
		border: 0;
		outline: 0;
		background: transparent;
		color: var(--text);
		font: inherit;
		font-size: var(--fs-sm);
	}
	.searchbox input::placeholder {
		color: var(--text-faint);
	}
	.result-count {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.source-layout {
		display: grid;
		grid-template-columns: minmax(0, 1.15fr) minmax(260px, 0.85fr);
		min-height: 340px;
		border-block: 1px solid var(--border);
		border-inline: 0;
		border-radius: 0;
		overflow: hidden;
		background: transparent;
	}
	.source-list {
		min-width: 0;
		padding: var(--space-2) 0;
		overflow: auto;
	}
	.source-row {
		width: 100%;
		display: grid;
		grid-template-columns: 24px minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-3);
		border-radius: var(--radius-sm);
		text-align: left;
	}
	.source-row:hover,
	.source-row.selected {
		background: var(--bg-inset);
	}
	.source-icon,
	.detail-icon,
	.empty-icon {
		display: grid;
		place-items: center;
		color: var(--text-faint);
	}
	.source-copy {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.source-copy strong {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.source-meta small,
	.path,
	.more {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.source-meta {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 2px;
		white-space: nowrap;
	}
	.source-meta small:first-child {
		color: var(--text-dim);
		font-family: var(--mono);
		font-size: var(--fs-xs);
	}
	.detail {
		min-width: 0;
		padding: var(--space-5);
		border-left: 1px solid var(--border);
		background: transparent;
		overflow: auto;
	}
	.detail-head {
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
	}
	.detail-icon {
		width: 30px;
		height: 30px;
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
	}
	.detail-head h2 {
		max-width: 24ch;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.path {
		margin: var(--space-2) 0 var(--space-4);
		font-family: var(--mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.facts {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-2);
		margin-bottom: var(--space-4);
	}
	.facts div {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding-top: var(--space-2);
		border-top: 1px solid var(--border);
	}
	.facts span {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.facts strong {
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.source-freshness {
		margin: -8px 0 var(--space-4);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.synopsis {
		margin: 0 0 var(--space-4);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.note {
		margin-bottom: var(--space-4);
		padding: var(--space-2) var(--space-3);
		border-left: 2px solid var(--warn);
		background: var(--bg-inset);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.note span {
		display: block;
		margin-bottom: 2px;
		color: var(--warn);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.columns {
		border-top: 1px solid var(--border);
	}
	.subhead {
		display: flex;
		justify-content: space-between;
		padding: var(--space-3) 0 var(--space-2);
		color: var(--text-dim);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.subhead span:last-child {
		color: var(--text-faint);
		font-weight: 500;
	}
	.column-row {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		justify-content: space-between;
		gap: 3px;
		padding: 4px 0;
		font-size: var(--fs-sm);
	}
	.column-main {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		min-width: 0;
	}
	.column-main span {
		font-family: var(--sans);
	}
	.column-row > small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.column-row code {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.column-row span {
		flex: none;
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: var(--fs-xs);
	}
	.common-values {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.common-values span {
		margin-right: 6px;
		color: var(--text-faint);
		font-family: var(--sans);
		font-weight: 600;
	}
	.more {
		margin: var(--space-2) 0 0;
	}
	.skipped {
		margin-top: var(--space-3);
		border-top: 1px solid var(--border);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.skipped summary {
		display: flex;
		justify-content: space-between;
		padding: var(--space-3) 0;
		color: var(--warn);
		cursor: pointer;
		list-style: none;
	}
	.skipped summary::-webkit-details-marker {
		display: none;
	}
	.skipped summary span:last-child {
		color: var(--text-faint);
	}
	.skipped div {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-2) 0;
		border-top: 1px solid var(--border);
	}
	.skipped div span {
		color: var(--text-faint);
		text-align: right;
	}
	.no-results,
	.detail-empty {
		padding: var(--space-5);
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.empty-state {
		max-width: 46ch;
		margin: 8vh auto 0;
		text-align: center;
	}
	.empty-icon {
		width: 42px;
		height: 42px;
		margin: 0 auto var(--space-3);
		border-radius: 50%;
		background: var(--bg-inset);
	}
	.empty-state h2 {
		font-size: var(--fs-xl);
	}
	.empty-state p {
		margin: var(--space-2) 0 var(--space-4);
		color: var(--text-dim);
	}
	.no-sources {
		padding: var(--space-6) var(--space-5);
		border-block: 1px solid var(--border);
		border-inline: 0;
		border-radius: 0;
		background: transparent;
		text-align: center;
	}
	.no-sources .empty-icon {
		width: 36px;
		height: 36px;
		margin-bottom: var(--space-2);
	}
	.no-sources h2 {
		font-size: var(--fs-lg);
	}
	.no-sources p {
		margin: var(--space-2) auto var(--space-4);
		max-width: 44ch;
		color: var(--text-dim);
	}
	@media (max-width: 760px) {
		.sources-page {
			padding-inline: var(--space-4);
		}
		.page-head {
			align-items: stretch;
			flex-direction: column;
		}
		.page-head .pill {
			align-self: flex-start;
		}
		.summary {
			width: 100%;
			flex-wrap: wrap;
		}
		.summary-item {
			flex: 1 0 40%;
		}
		.source-layout {
			grid-template-columns: 1fr;
		}
		.detail {
			border-top: 1px solid var(--border);
			border-left: 0;
		}
	}
</style>
