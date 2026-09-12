<script lang="ts">
	import { session } from '$lib/session.svelte';
	import EvidenceBlock from './EvidenceBlock.svelte';

	// The last answer in the active tab, i.e. the same "last answer" the
	// titlebar's info popover already summarizes reuse that derivation.
	let lastAnswer = $derived.by(() => {
		const msgs = session.activeChat?.messages ?? [];
		for (let i = msgs.length - 1; i >= 0; i--) {
			if (msgs[i].answer) return msgs[i].answer;
		}
		return null;
	});

	let expanded = $state(true);
</script>

<aside class="panel">
	<div class="head">Evidence</div>
	<div class="body">
		{#if lastAnswer}
			<EvidenceBlock answer={lastAnswer} {expanded} ontoggle={() => (expanded = !expanded)} />
		{:else}
			<p class="empty">No evidence yet — ask something that needs a lookup or a calculation.</p>
		{/if}
	</div>
</aside>

<style>
	.panel {
		flex: none;
		width: 280px;
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-3);
		border-left: 1px solid var(--border);
		background: var(--bg);
		overflow-y: auto;
	}
	.head {
		flex: none;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.body {
		flex: 1;
		min-height: 0;
	}
	.empty {
		margin: 0;
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
</style>
