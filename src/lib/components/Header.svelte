<script lang="ts">
	import { session } from '$lib/session.svelte';

	let name = $derived.by(() => {
		const w = session.catalog.workspace;
		return w ? w.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') || w : '';
	});
	let count = $derived(session.catalog.sources.length);
</script>

<header data-tauri-drag-region>
	<span class="mark">fella</span>
	{#if name}
		<span class="sep">/</span>
		<span class="folder">{name}</span>
		<span class="count">{count} file{count === 1 ? '' : 's'}</span>
	{/if}
</header>

<style>
	header {
		flex: none;
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		padding: var(--space-2) var(--pad) var(--space-3);
		background: var(--bg);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		white-space: nowrap;
		overflow: hidden;
	}
	.mark {
		color: var(--text-dim);
		font-weight: 550;
		letter-spacing: -0.01em;
	}
	.sep {
		color: var(--border-strong);
	}
	.folder {
		font-family: var(--mono);
		font-size: var(--fs-xs);
		color: var(--text-dim);
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.count {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
</style>
