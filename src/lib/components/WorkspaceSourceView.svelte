<script lang="ts">
	import { ipc } from '$lib/ipc';
	import type { QueryResult, SourceInfo } from '$lib/types';
	import { Spinner } from '$lib/components/ui/spinner';
	import Icon from './Icon.svelte';

	let {
		source,
		workspacePath,
		revision
	}: { source: SourceInfo | null; workspacePath: string | null; revision: string | null } = $props();

	let rows = $state<QueryResult | null>(null);
	let documentText = $state('');
	let truncated = $state(false);
	let loading = $state(false);
	let error = $state('');
	let generation = 0;

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

	$effect(() => {
		const selected = source;
		const root = workspacePath;
		const catalogRevision = revision;
		const request = ++generation;
		rows = null;
		documentText = '';
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
		<header class="source-head">
			<div class="source-title-row">
				<span class="source-icon"><Icon name={source.view ? 'table' : 'file'} size={16} /></span>
				<div class="source-title-copy">
					<h1>{source.name}</h1>
					<p title={source.path}>{relativePath(source.path)}</p>
				</div>
			</div>
			<div class="source-metadata" aria-label="Source metadata">
				<span>{source.kind.toUpperCase()}</span>
				{#if source.view}<span>{new Intl.NumberFormat().format(source.row_count ?? 0)} rows</span>{/if}
				<span>Catalog {revision ?? 'revision unavailable'}</span>
			</div>
		</header>
		{#if loading}
			<div class="source-loading" role="status"><Spinner size={16} />Loading read-only preview…</div>
		{:else if error}
			<div class="source-message" role="alert">
				<strong>Preview unavailable</strong>
				<p>{error}</p>
			</div>
		{:else if source.view && rows}
			<!-- The region is focusable so keyboard users can scroll the wide read-only table. -->
			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<div class="table-scroll" role="region" tabindex="0" aria-label="First 25 rows of {source.name}">
				<table>
					<thead><tr>{#each rows.columns as column (column)}<th scope="col">{column}</th>{/each}</tr></thead>
					<tbody>
						{#each rows.rows as row, rowIndex (rowIndex)}
							<tr>{#each row as cell, columnIndex (`${rowIndex}-${columnIndex}`)}<td title={valueText(cell)}>{valueText(cell)}</td>{/each}</tr>
						{/each}
					</tbody>
				</table>
			</div>
			<p class="source-footnote">Read-only sample from Fella’s normalized table · {rows.rows.length} of {source.row_count ?? 'unknown'} rows. The original file is unchanged.</p>
		{:else if documentText.startsWith('(this PDF has no selectable text')}
			<div class="source-message" role="status">
				<strong>This PDF has no selectable text</strong>
				<p>It may be a scan or contain images of pages. Fella can’t preview or analyze text that isn’t embedded in the PDF.</p>
			</div>
		{:else if documentText}
			<pre class="document-preview">{documentText}</pre>
			{#if truncated}<p class="source-footnote">Preview limited to the first section of this file. Fella’s analysis tools can search the wider document.</p>{/if}
		{:else}
			<div class="source-message" role="status">
				<strong>{source.view ? 'No rows to preview' : 'No readable text found'}</strong>
				<p>This preview is read-only and does not add the file to chat context or change the Investigation’s source scope.</p>
			</div>
		{/if}
	{:else}
		<div class="source-message source-missing" role="status">
			<strong>This file is no longer in the current catalog.</strong>
			<p>The saved analysis and its original source record remain unchanged. Refresh the workspace catalog to check for the file again.</p>
		</div>
	{/if}
</section>

<style>
	.source-view { display: flex; flex: 1; flex-direction: column; min-width: 0; min-height: 0; gap: var(--space-4); padding: var(--space-5); overflow: auto; }
	.source-head { display: grid; gap: var(--space-3); padding-bottom: var(--space-3); border-bottom: 1px solid var(--border); }
	.source-title-row { display: flex; min-width: 0; align-items: flex-start; gap: var(--space-3); }
	.source-icon { display: grid; width: 30px; height: 30px; flex: none; place-items: center; border-radius: var(--radius-sm); background: var(--bg-inset); color: var(--text-dim); }
	.source-title-copy { min-width: 0; }
	.source-title-copy h1 { margin: 0; font-size: var(--fs-md); font-weight: 650; overflow-wrap: anywhere; }
	.source-title-copy p { margin: 2px 0 0; color: var(--text-faint); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.source-metadata { display: flex; flex-wrap: wrap; gap: var(--space-2); color: var(--text-faint); font-size: 11px; }
	.source-metadata span + span::before { content: '·'; margin-right: var(--space-2); color: var(--border-strong); }
	.table-scroll { max-width: 100%; overflow: auto; border: 1px solid var(--border); border-radius: var(--radius-md); }
	table { width: max-content; min-width: 100%; border-collapse: collapse; font-size: var(--fs-xs); }
	th, td { max-width: 240px; padding: 8px 10px; border-bottom: 1px solid var(--border); text-align: left; vertical-align: top; }
	th { position: sticky; top: 0; background: var(--bg-inset); color: var(--text-dim); font-weight: 600; }
	td { overflow: hidden; color: var(--text); text-overflow: ellipsis; white-space: nowrap; }
	tr:last-child td { border-bottom: 0; }
	.document-preview { min-height: 0; margin: 0; padding: var(--space-4); border-radius: var(--radius-md); background: var(--bg-inset); color: var(--text); font: 12px/1.6 var(--mono); white-space: pre-wrap; overflow-wrap: anywhere; }
	.source-footnote { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.source-loading { display: flex; align-items: center; gap: var(--space-2); color: var(--text-dim); font-size: var(--fs-sm); }
	.source-message { display: grid; align-content: start; gap: var(--space-2); max-width: 56ch; padding: var(--space-4) 0; color: var(--text-dim); font-size: var(--fs-sm); }
	.source-message strong { color: var(--text); font-weight: 600; }
	.source-message p { margin: 0; line-height: 1.5; }
</style>
