<script lang="ts">
	import { onDestroy } from 'svelte';
	import { ipc, isTauri } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import type { AugmentTab } from '$lib/session.svelte';

	let { tab }: { tab: AugmentTab } = $props();

	let isGrid = $derived(tab.capability === 'grid');

	let ta = $state<HTMLTextAreaElement | undefined>();
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
		<span class="hint">saved into this folder as you type</span>
		{#if isGrid}
			<span class="spacer"></span>
			<button class="pill ghost" onclick={addRow}>+ row</button>
			<button class="pill ghost" onclick={addCol}>+ column</button>
		{/if}
	</div>

	{#if isGrid}
		<div class="gridwrap">
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
	{/if}

	<div class="foot">
		<span class={status.cls}>{status.text}</span>
		{#if dims}<span class="dim">· {dims}</span>{/if}
		<span class="spacer"></span>
		<span class="dim">Fella re-checks the folder when you pause or close this tab</span>
	</div>
</div>

<style>
	.augment {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
		padding: var(--space-4) var(--pad) 0;
	}
	.head {
		display: flex;
		align-items: baseline;
		gap: var(--space-3);
		padding-bottom: var(--space-3);
	}
	.file {
		font-family: var(--mono);
		font-size: var(--fs-sm);
		color: var(--text);
	}
	.hint {
		font-size: var(--fs-xs);
		color: var(--text-faint);
	}
	textarea {
		flex: 1;
		min-height: 0;
		width: 100%;
		resize: none;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-inset);
		color: var(--text);
		font: inherit;
		line-height: var(--lh);
		padding: var(--space-3);
	}
	textarea.mono {
		font-family: var(--mono);
		font-size: var(--fs-sm);
		white-space: pre;
		overflow-wrap: normal;
	}
	textarea:focus {
		outline: none;
		border-color: var(--link);
		box-shadow: var(--focus-ring);
	}
	.gridwrap {
		flex: 1;
		min-height: 0;
		overflow: auto;
		border: 1px solid var(--border);
		border-radius: var(--radius);
	}
	table {
		border-collapse: collapse;
		width: 100%;
	}
	td {
		border: 1px solid var(--border);
		padding: 0;
	}
	td input {
		width: 100%;
		min-width: 8ch;
		border: none;
		background: transparent;
		color: var(--text);
		font-family: var(--mono);
		font-size: var(--fs-sm);
		padding: 4px 7px;
	}
	td input:focus {
		outline: 2px solid var(--link);
		outline-offset: -2px;
		background: var(--bg-inset);
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
