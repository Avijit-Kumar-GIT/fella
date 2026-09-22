<script lang="ts">
	import { onDestroy } from 'svelte';
	import { openFolder } from '$lib/commands';
	import { ipc, isTauri } from '$lib/ipc';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';

	let contents = $state('');
	let savedContents = $state('');
	let path = $state<string | null>(null);
	let loading = $state(false);
	let saving = $state(false);
	let error = $state('');
	let loadedWorkspace = $state<string | null>(null);
	let saveTimer: ReturnType<typeof setTimeout> | undefined;

	async function load(): Promise<void> {
		if (!isTauri() || !session.catalog.workspace) return;
		loading = true;
		error = '';
		try {
			const result = await ipc.contextFile();
			path = result?.[0] ?? null;
			contents = result?.[1] ?? '';
			savedContents = contents;
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	async function save(): Promise<void> {
		if (!isTauri() || !session.catalog.workspace || contents === savedContents) return;
		saving = true;
		error = '';
		try {
			await ipc.saveContext(contents);
			savedContents = contents;
			if (!path) {
				const result = await ipc.contextFile();
				path = result?.[0] ?? null;
			}
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			saving = false;
		}
	}

	function scheduleSave(): void {
		clearTimeout(saveTimer);
		saveTimer = setTimeout(() => void save(), 700);
	}

	function useTemplate(): void {
		if (contents.trim()) return;
		contents = `# About this workspace\n\n<!-- Tell Fella what these files mean, which fields matter, and any rules it should follow. -->\n`;
		scheduleSave();
	}

	$effect(() => {
		const workspace = session.catalog.workspace;
		if (!workspace || workspace === loadedWorkspace) return;
		clearTimeout(saveTimer);
		contents = '';
		savedContents = '';
		path = null;
		loadedWorkspace = workspace;
		void load();
	});

	onDestroy(() => {
		clearTimeout(saveTimer);
	});
</script>

<section class="context-page" aria-labelledby="context-title">
	<header class="page-head">
		<div>
			<p class="eyebrow">Workspace</p>
			<h1 id="context-title">Context</h1>
			<p class="lede">A short note about your files that Fella reads before it answers.</p>
		</div>
		<button class="pill ghost" type="button" onclick={() => void openFolder()}>
			<Icon name="folder" size={16} /> Change folder
		</button>
	</header>

	<div class="editor-card">
		<div class="editor-head">
			<div>
				<strong>fella.md</strong>
				{#if path}<p>{path}</p>{/if}
			</div>
			<div class="editor-actions">
				{#if contents.trim() === ''}
					<button class="text-button" type="button" onclick={useTemplate}>Use a template</button>
				{/if}
				<button class="pill primary" type="button" disabled={saving || loading || contents === savedContents} onclick={() => void save()}>
					<Icon name="check" size={16} />
					{saving ? 'Saving…' : 'Save'}
				</button>
			</div>
		</div>
		{#if loading}
			<div class="loading">Loading your workspace context…</div>
		{:else}
			<textarea
				bind:value={contents}
				aria-label="Workspace context"
				placeholder="Tell Fella what your files mean, which fields matter, and what it should keep in mind…"
				oninput={scheduleSave}
			></textarea>
		{/if}
		<div class="editor-foot">
			<span>{error || (saving ? 'Saving to the workspace…' : contents === savedContents ? 'Saved locally in this folder' : 'Unsaved changes')}</span>
			<span>Only you can edit this file.</span>
		</div>
	</div>

	<div class="tip">
		<Icon name="info" size={16} />
		<span>Use context for definitions and guidance. Fella can read it, but it does not rewrite it.</span>
	</div>
</section>

<style>
	.context-page {
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
	h1 {
		margin: 0;
		font-size: clamp(24px, 3vw, 32px);
		font-weight: 650;
		letter-spacing: -0.03em;
	}
	.lede {
		margin: var(--space-2) 0 0;
		color: var(--text-dim);
	}
	.editor-card {
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
		overflow: hidden;
	}
	.editor-head,
	.editor-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
	}
	.editor-head {
		border-bottom: 1px solid var(--border);
	}
	.editor-head strong {
		font-family: var(--mono);
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.editor-head p {
		max-width: 48ch;
		margin: 3px 0 0;
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.editor-actions {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		flex: none;
	}
	.text-button {
		color: var(--link);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	.text-button:hover {
		text-decoration: underline;
	}
	.editor-head .pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		font-size: var(--fs-xs);
	}
	textarea {
		display: block;
		width: 100%;
		min-height: 360px;
		padding: var(--space-4);
		border: 0;
		outline: 0;
		resize: vertical;
		background: var(--bg-raised);
		color: var(--text);
		font: 14px/1.65 var(--mono);
	}
	textarea:focus {
		box-shadow: inset 0 0 0 2px var(--brand);
	}
	textarea::placeholder {
		color: var(--text-faint);
	}
	.loading {
		min-height: 360px;
		display: grid;
		place-items: center;
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.editor-foot {
		border-top: 1px solid var(--border);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.editor-foot span:first-child {
		color: var(--text-dim);
	}
	.tip {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		margin-top: var(--space-4);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.tip :global(svg) {
		flex: none;
		color: var(--brand);
	}
	@media (max-width: 680px) {
		.page-head,
		.editor-head,
		.editor-foot {
			align-items: flex-start;
			flex-direction: column;
		}
		.editor-actions {
			width: 100%;
			justify-content: space-between;
		}
	}
</style>
