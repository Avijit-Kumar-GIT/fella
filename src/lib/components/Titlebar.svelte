<script lang="ts">
	import { session } from '$lib/session.svelte';
	import { isTauri, win } from '$lib/ipc';
	import Icon from './Icon.svelte';

	let { onpalette }: { onpalette: () => void } = $props();

	// macOS keeps its native traffic lights (titleBarStyle: Overlay), so leave a
	// gutter for them. Windows/Linux draw nothing on the left.
	const isMac =
		typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
	// Windows is frameless (decorations:false) and needs our own controls; Linux
	// keeps its native frame, so it doesn't.
	const isWindows =
		typeof navigator !== 'undefined' && /Win/i.test(navigator.platform || navigator.userAgent);

	let folder = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
</script>

<div class="titlebar" class:mac={isMac} data-tauri-drag-region>
	{#if isMac}<span class="lights" aria-hidden="true"></span>{/if}

	<span class="id" data-tauri-drag-region>
		{#if folder}
			<span class="prompt" aria-hidden="true">❯</span>
			<span class="folder" title={session.catalog.workspace}>{folder}</span>
		{:else}
			<span class="wordmark">Fella</span>
		{/if}
	</span>

	<span class="spacer" data-tauri-drag-region></span>

	<button class="hint" data-tauri-drag-region="false" onclick={onpalette} title="Command palette">
		<kbd>Ctrl</kbd><kbd>K</kbd>
	</button>

	{#if isWindows && isTauri()}
		<div class="winctl" data-tauri-drag-region="false">
			<button aria-label="Minimize" onclick={() => void win.minimize()}>
				<Icon name="minus" size={14} />
			</button>
			<button aria-label="Maximize" onclick={() => void win.toggleMaximize()}>
				<Icon name="square" size={11} />
			</button>
			<button class="x" aria-label="Close" onclick={() => void win.close()}>
				<Icon name="x" size={14} />
			</button>
		</div>
	{/if}
</div>

<style>
	.titlebar {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		height: 38px;
		padding: 0 var(--space-2) 0 var(--pad);
		background: var(--bg);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		user-select: none;
		white-space: nowrap;
	}
	.titlebar.mac {
		padding-left: 0;
	}
	.lights {
		flex: none;
		width: 78px;
	}
	.id {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		min-width: 0;
	}
	.prompt {
		font-family: var(--mono);
		color: var(--text-dim);
	}
	.wordmark {
		color: var(--text-dim);
		font-weight: 560;
		letter-spacing: -0.02em;
	}
	.folder {
		font-family: var(--mono);
		font-size: var(--fs-xs);
		color: var(--text-dim);
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 0;
	}
	.spacer {
		flex: 1;
		align-self: stretch;
	}
	.hint {
		display: inline-flex;
		gap: 2px;
		padding: 2px 6px;
		color: var(--text-faint);
		border-radius: var(--radius-chip);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.hint:hover {
		background: var(--bg-inset);
		color: var(--text-dim);
	}
	.winctl {
		display: flex;
		align-self: stretch;
	}
	.winctl button {
		width: 42px;
		display: grid;
		place-items: center;
		color: var(--text-faint);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
	}
	.winctl button:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.winctl button.x:hover {
		background: var(--err);
		color: #fff;
	}
</style>
