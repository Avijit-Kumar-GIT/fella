<script lang="ts">
	import Icon from './Icon.svelte';

	let {
		bodyId,
		label = 'Analysis details',
		expanded = false,
		ontoggle
	}: {
		bodyId: string;
		label?: string;
		expanded?: boolean;
		ontoggle?: () => void;
	} = $props();

</script>

<div class="evidence-summary">
	<button
		class="summary-toggle"
		type="button"
		aria-expanded={expanded}
		aria-controls={bodyId}
		title={`${expanded ? 'Hide' : 'Show'} ${label.toLowerCase()}`}
		onclick={() => ontoggle?.()}
	>
		<span class="summary-label">{label}</span>
		<span class="caret" class:open={expanded} aria-hidden="true"><Icon name="chevron-right" size={12} /></span>
	</button>
</div>

<style>
	.evidence-summary {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-width: 0;
		margin-left: auto;
	}
	.summary-toggle {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		min-width: 0;
		padding: 3px 5px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	.summary-toggle:hover,
	.summary-toggle[aria-expanded='true'] {
		background: var(--bg-inset);
		color: var(--text);
	}
	.summary-label {
		font-weight: 600;
	}
	.caret {
		display: inline-flex;
		color: var(--text-faint);
		transition: transform var(--dur-fast) var(--ease);
	}
	.caret.open {
		transform: rotate(90deg);
	}
</style>
