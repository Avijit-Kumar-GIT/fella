<script lang="ts">
	import { session } from '$lib/session.svelte';

	let up = $derived(session.health?.reachable ?? null);
	let rejected = $derived(session.health?.rejected === true);
	let providerId = $derived(session.settings?.provider ?? 'ollama');
	let providerName = $derived(
		session.providers.find((p) => p.id === providerId)?.display ?? providerId
	);

	let hasFolder = $derived(!!session.catalog.workspace);
	let folder = $derived(
		session.catalog.workspace?.replace(/[/\\]+$/, '').replace(/^.*[/\\]/, '') ?? ''
	);
	let fileCount = $derived(session.catalog.sources.length);

	// provider/model, only once the provider has actually answered. Shows the
	// active tab's model (each tab can pick its own).
	let modelLabel = $derived(up === true && session.model ? `${providerName}/${session.model}` : '');

	// The trailing crumb, only when something needs doing. Points at the fix.
	let state = $derived.by(() => {
		if (up === null) return 'connecting…';
		if (rejected) return 'key refused — /login';
		if (up === false) return 'offline';
		if (up === true && !session.model) return 'pick a model — /model';
		return '';
	});

	let busy = $derived(session.busy);
	let note = $derived.by(() => {
		if (session.activity) return session.activity;
		if (session.busy) return 'working…';
		if ((up === false || rejected) && providerId !== 'ollama') return providerName;
		if (session.focus) return 'focus mode · /focus to exit';
		return null;
	});

	let parts = $derived.by(() => {
		const p: { text: string; cls: string }[] = [];
		if (modelLabel) p.push({ text: modelLabel, cls: 'model' });
		if (hasFolder) {
			p.push({ text: folder, cls: 'crumb' });
			p.push({ text: `${fileCount} file${fileCount === 1 ? '' : 's'}`, cls: 'crumb' });
		}
		if (state) p.push({ text: state, cls: 'state' });
		return p;
	});
</script>

<div class="status">
	<span class="dot" class:up={up === true} class:down={up === false}></span>
	{#each parts as part, i (i)}
		{#if i > 0}<span class="sep">·</span>{/if}
		<span class={part.cls}>{part.text}</span>
	{/each}
	{#if note}
		{#if parts.length}<span class="sep">·</span>{/if}
		{#if busy}<span class="thinking" aria-hidden="true"></span>{/if}
		<span class="note">{note}</span>
	{/if}
</div>

<style>
	.status {
		flex: none;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-2) var(--pad);
		background: var(--bg);
		border-top: 1px solid var(--border);
		color: var(--text-faint);
		font-size: var(--fs-sm);
		letter-spacing: 0.005em;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.model {
		color: var(--text-dim);
		font-family: var(--mono);
		font-size: var(--fs-xs);
	}
	.crumb,
	.state,
	.sep,
	.note {
		color: var(--text-faint);
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--text-faint);
		flex: none;
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--text-faint) 18%, transparent);
	}
	.dot.up {
		background: var(--ok);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 20%, transparent);
	}
	.dot.down {
		background: var(--err);
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--err) 20%, transparent);
	}
</style>
