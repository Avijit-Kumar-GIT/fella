<script lang="ts">
	import { ipc, isDesktop } from '$lib/ipc';
	import { Spinner } from '$lib/components/ui/spinner';
	import type { QueryResult, SourceInfo } from '$lib/types';

	let {
		source,
		workspacePath = null,
		revision = null
	}: { source: SourceInfo; workspacePath?: string | null; revision?: string | null } = $props();
	let preview = $state<QueryResult | null>(null);
	let loading = $state(false);
	let error = $state('');
	let loadedIdentity = $state('');
	let requestGeneration = 0;

	function cellValue(value: unknown): string {
		if (value == null) return '—';
		if (typeof value === 'object') return JSON.stringify(value);
		return String(value);
	}

	async function load(identity: string, name: string, root: string | null): Promise<void> {
		const generation = ++requestGeneration;
		preview = null;
		error = '';
		loadedIdentity = identity;
		if (!isDesktop()) {
			error = 'Preview is available in the Fella desktop app.';
			return;
		}
		loading = true;
		try {
			const result = await ipc.sampleSource(name, 5, root ?? undefined);
			if (requestGeneration === generation && loadedIdentity === identity) preview = result;
		} catch (e) {
			if (requestGeneration === generation && loadedIdentity === identity) {
				error = e instanceof Error ? e.message : String(e);
			}
		} finally {
			if (requestGeneration === generation && loadedIdentity === identity) loading = false;
		}
	}

	$effect(() => {
		const path = source.path;
		const root = workspacePath;
		const sourceRevision = revision;
		const identity = `${root ?? ''}\u0000${sourceRevision ?? ''}\u0000${path}`;
		if (!source.view) {
			requestGeneration++;
			preview = null;
			error = '';
			loadedIdentity = identity;
			loading = false;
			return;
		}
		void load(identity, source.name, root);
	});
</script>

{#if source.view}
	<section class="preview" aria-label={`Preview of ${source.name}`}>
		<div class="preview-head">
			<span>First look</span>
			<span>{preview ? `${preview.rows.length} rows shown` : ''}</span>
		</div>
		{#if loading}
			<div class="preview-loading"><Spinner size={18} /><span>Loading a few rows…</span></div>
		{:else if error}
			<p class="preview-note">{error}</p>
		{:else if preview?.columns.length}
			<div class="table-wrap">
				<table>
					<thead>
						<tr>
							{#each preview.columns as column (column)}<th scope="col">{column}</th>{/each}
						</tr>
					</thead>
					<tbody>
						{#each preview.rows as row, rowIndex (rowIndex)}
							<tr>
								{#each row as value, columnIndex (columnIndex)}<td>{cellValue(value)}</td>{/each}
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{:else}
			<p class="preview-note">No rows to preview yet.</p>
		{/if}
	</section>
{/if}

<style>
	.preview {
		margin: var(--space-4) 0;
		padding-top: var(--space-3);
		border-top: 1px solid var(--border);
	}
	.preview-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
		color: var(--text-dim);
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	.preview-head span:last-child {
		color: var(--text-faint);
		font-weight: 500;
	}
	.preview-note {
		margin: 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.preview-loading {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.table-wrap {
		max-width: 100%;
		overflow-x: auto;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
	}
	table {
		border-collapse: collapse;
		min-width: 100%;
		font-family: var(--mono);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	th,
	td {
		max-width: 150px;
		padding: 5px 7px;
		border-bottom: 1px solid var(--border);
		overflow: hidden;
		text-overflow: ellipsis;
		text-align: left;
	}
	th {
		background: var(--bg-inset);
		color: var(--text-dim);
		font-weight: 600;
	}
	td {
		color: var(--text-dim);
	}
	tr:last-child td {
		border-bottom: 0;
	}
</style>
