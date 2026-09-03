<script lang="ts">
	import { session } from '$lib/session.svelte';

	let up = $derived(session.health?.reachable ?? null);
	let providerId = $derived(session.settings?.provider ?? 'ollama');
	let providerName = $derived(
		session.providers.find((p) => p.id === providerId)?.display ?? providerId
	);

	let rejected = $derived(session.health?.rejected === true);

	// Only name a model when the provider actually answered. Shows the active
	// tab's model (each tab can pick its own).
	let label = $derived.by(() => {
		if (up === true) return session.model || 'no model set';
		if (rejected) return 'key refused';
		if (up === null) return 'connecting…';
		return 'not connected';
	});
	let busy = $derived(session.busy);
	let note = $derived.by(() => {
		if (session.activity) return session.activity;
		if (session.busy) return 'working…';
		// Name the service when it's a hosted one you're not reaching / refused.
		if ((up === false || rejected) && providerId !== 'ollama') return providerName;
		// The only reminder that focus mode is on (the tabs/header are hidden).
		if (session.focus) return 'focus mode · /focus to exit';
		return null;
	});
</script>

<div class="status">
	<span class="dot" class:up={up === true} class:down={up === false}></span>
	<span class="model">{label}</span>
	{#if note}
		<span class="sep">·</span>
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
