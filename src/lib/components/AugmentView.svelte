<script lang="ts">
	import { onDestroy } from 'svelte';
	import { ipc, isTauri } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import type { AugmentTab } from '$lib/session.svelte';

	let { tab }: { tab: AugmentTab } = $props();

	let isGrid = $derived(tab.capability === 'grid');

	let ta = $state<HTMLTextAreaElement | undefined>();
	let gridWrap = $state<HTMLDivElement | undefined>();
	let saveTimer: ReturnType<typeof setTimeout> | undefined;
	let rescanTimer: ReturnType<typeof setTimeout> | undefined;
	let error = $state<string | null>(null);

	const SAVE_DEBOUNCE = 600;
	const RESCAN_IDLE = 3000;

	// --- csv (grid only) ------------------------------------------------
	function parseCsv(s: string): string[][] {
		const rows: string[][] = [];
		let row: string[] = [];
		let cell = '';
		let q = false;
		for (let i = 0; i < s.length; i++) {
			const c = s[i];
			if (q) {
				if (c === '"') {
					if (s[i + 1] === '"') {
						cell += '"';
						i++;
					} else q = false;
				} else cell += c;
			} else if (c === '"') q = true;
			else if (c === ',') {
				row.push(cell);
				cell = '';
			} else if (c === '\n') {
				row.push(cell);
				rows.push(row);
				row = [];
				cell = '';
			} else if (c !== '\r') cell += c;
		}
		if (cell !== '' || row.length) {
			row.push(cell);
			rows.push(row);
		}
		return rows;
	}
	function toCsv(rows: string[][]): string {
		return (
			rows
				.map((r) =>
					r.map((c) => (/[",\n]/.test(c) ? '"' + c.replace(/"/g, '""') + '"' : c)).join(',')
				)
				.join('\n') + '\n'
		);
	}

	let rows = $state<string[][]>([]);
	let gridReady = false;
	$effect(() => {
		if (!isGrid || gridReady) return;
		const parsed = parseCsv(tab.text).filter((r) => r.length);
		rows = parsed.length ? normalize(parsed) : [['', ''], ['', '']];
		gridReady = true;
		// Same "ready to type the moment the tab opens" behaviour as the
		// textarea's autofocus, once the grid has rendered.
		queueMicrotask(() => gridWrap?.querySelector('input')?.focus());
	});
	function normalize(r: string[][]): string[][] {
		const w = Math.max(1, ...r.map((row) => row.length));
		return r.map((row) => [...row, ...Array(w - row.length).fill('')]);
	}
	function pushGrid(): void {
		tab.text = toCsv(rows);
		schedule();
	}
	function addRow(): void {
		rows.push(Array(rows[0]?.length ?? 1).fill(''));
		pushGrid();
	}
	function addCol(): void {
		for (const row of rows) row.push('');
		pushGrid();
	}
	function onCellKey(e: KeyboardEvent, ri: number, ci: number): void {
		if (e.key !== 'Enter') return;
		e.preventDefault();
		const next = e.currentTarget as HTMLElement;
		const table = next.closest('table');
		if (ri + 1 >= rows.length) addRow();
		queueMicrotask(() => {
			table
				?.querySelectorAll<HTMLInputElement>('tbody tr')
				[ri + 1]?.querySelectorAll('input')
				[ci]?.focus();
		});
	}

	// --- save + rescan -------------------------------------------------
	async function save(): Promise<void> {
		if (!isTauri() || !tab.dirty) return;
		const snapshot = tab.text;
		tab.saving = true;
		try {
			await ipc.augmentSave(tab.capability, tab.file, snapshot);
			tab.saved = snapshot;
			tab.savedAt = Date.now();
			error = null;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			tab.saving = false;
		}
	}
	async function rescan(): Promise<void> {
		if (!isTauri()) return;
		try {
			session.catalog = await ipc.reindex();
		} catch {
			/* the user can still run /reindex by hand */
		}
	}
	// Debounced autosave, then a longer idle before a folder re-scan
	// (a full re-scan is O(folder), too heavy per keystroke).
	function schedule(): void {
		clearTimeout(saveTimer);
		clearTimeout(rescanTimer);
		saveTimer = setTimeout(save, SAVE_DEBOUNCE);
		rescanTimer = setTimeout(async () => {
			await save();
			await rescan();
		}, RESCAN_IDLE);
	}

	onDestroy(() => {
		clearTimeout(saveTimer);
		clearTimeout(rescanTimer);
		// closeTab() flushes a dirty buffer already; still re-scan so a freshly
		// written file is visible without a manual /reindex.
		void (async () => {
			await save();
			await rescan();
		})();
	});

	let status = $derived.by(() => {
		if (error) return { text: error, cls: 'err' };
		if (tab.loadError) return { text: tab.loadError, cls: 'err' };
		if (tab.saving) return { text: 'Saving…', cls: 'dim' };
		if (tab.dirty) return { text: 'Unsaved', cls: 'dim' };
		if (tab.savedAt) {
			const d = new Date(tab.savedAt);
			const hh = String(d.getHours()).padStart(2, '0');
			const mm = String(d.getMinutes()).padStart(2, '0');
			return { text: `Saved · ${hh}:${mm}`, cls: 'dim' };
		}
		return { text: 'Saved', cls: 'dim' };
	});

	let dims = $derived.by(() => {
		if (isGrid) {
			const cols = rows[0]?.length ?? 0;
			return `${rows.length} row${rows.length === 1 ? '' : 's'} · ${cols} col${cols === 1 ? '' : 's'}`;
		}
		if (tab.syntax !== 'csv') return null;
		const lines = tab.text.split('\n').filter((l) => l.trim() !== '');
		if (!lines.length) return null;
		const cols = Math.max(...lines.map((l) => l.split(',').length));
		return `${lines.length} row${lines.length === 1 ? '' : 's'} · ${cols} col${cols === 1 ? '' : 's'}`;
	});

	$effect(() => {
		if (!isGrid && ta) ta.focus();
	});
</script>

<div class="augment">
	<div class="head">
		<span class="file">{tab.file}</span>
		{#if isGrid}
			<span class="spacer"></span>
			<button class="pill ghost" onclick={addRow}>+ row</button>
			<button class="pill ghost" onclick={addCol}>+ column</button>
		{/if}
	</div>

	{#if isGrid}
		<div class="editor gridwrap" bind:this={gridWrap}>
			<table>
				<tbody>
					{#each rows as row, ri (ri)}
						<tr>
							{#each row as _cell, ci (ci)}
								<td>
									<input
										bind:value={rows[ri][ci]}
										oninput={pushGrid}
										onkeydown={(e) => onCellKey(e, ri, ci)}
									/>
								</td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{:else}
		<div class="editor">
			<textarea
				bind:this={ta}
				bind:value={tab.text}
				oninput={schedule}
				spellcheck={tab.syntax === 'markdown'}
				class:mono={tab.syntax === 'csv'}
				placeholder={tab.syntax === 'csv'
					? 'one row per line, values separated by commas'
					: 'type here it saves into the folder as you go'}
			></textarea>
		</div>
	{/if}

	<div class="foot">
		<span class={status.cls}>{status.text}</span>
		{#if dims}<span class="dim">· {dims}</span>{/if}
	</div>
</div>

<style>
	.augment {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		padding: var(--space-5) var(--pad) 0;
	}
	.head {
		display: flex;
		align-items: baseline;
		gap: var(--space-3);
		min-width: 0;
		padding-bottom: var(--space-3);
	}
	.file {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--mono);
		font-size: var(--fs-sm);
		color: var(--text);
	}
	/* Outlined but unfilled same "grouped surface" treatment as the composer's
	   .field: a border for structure, no card colour, one ring on focus. */
	.editor {
		flex: 1;
		min-height: 0;
		display: flex;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		transition:
			border-color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
	}
	.editor:focus-within {
		border-color: var(--link);
		box-shadow: var(--focus-ring);
	}
	/* The grid rings its one focused cell instead (below) two rings at
	   once the container's and the cell's would break "one focus ring". */
	.editor.gridwrap:focus-within {
		border-color: var(--border);
		box-shadow: none;
	}
	textarea {
		flex: 1;
		min-height: 0;
		width: 100%;
		resize: none;
		border: none;
		outline: none;
		background: transparent;
		color: var(--text);
		font: inherit;
		line-height: var(--lh);
		padding: var(--space-3);
	}
	textarea:focus-visible {
		box-shadow: none;
	}
	textarea::placeholder {
		color: var(--text-faint);
	}
	textarea.mono {
		font-family: var(--mono);
		font-size: var(--fs-sm);
		white-space: pre;
		overflow-wrap: normal;
	}
	.gridwrap {
		overflow: auto;
	}
	table {
		border-collapse: collapse;
		width: 100%;
	}
	td {
		border: 1px solid var(--border);
		padding: 0;
	}
	td:first-child {
		border-left: none;
	}
	tr:first-child td {
		border-top: none;
	}
	td input {
		width: 100%;
		min-width: 8ch;
		border: none;
		background: transparent;
		color: var(--text);
		font-family: var(--mono);
		font-size: var(--fs-sm);
		font-variant-numeric: tabular-nums;
		padding: 4px 7px;
	}
	td input:focus-visible {
		position: relative;
		z-index: 1;
	}
	.foot {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) 0 var(--space-3);
		font-size: var(--fs-xs);
	}
	.foot .dim {
		color: var(--text-faint);
	}
	.foot .err {
		color: var(--err);
	}
	.spacer {
		flex: 1;
	}
</style>
