<script module lang="ts">
	import type { Answer } from '$lib/types';

	/** Explicit unresolved meaning is actionable; missing independent grounding
	 * alone is a verification limit, not proof that a check raised a concern. */
	export function answerSemanticGaps(answer: Answer): string[] {
		const gaps = [...(answer.contract?.unresolved ?? []), ...(answer.grounding?.unresolved ?? [])]
			.filter((gap) => gap.trim().length > 0);
		if (answer.contract?.interpretation === 'ambiguous' && !gaps.length) {
			gaps.push('The recorded interpretation is ambiguous. Clarify the intended meaning before relying on this answer.');
		} else if (answer.contract?.interpretation === 'unsupported' && !gaps.length) {
			gaps.push('The recorded interpretation is unsupported by the available evidence.');
		}
		return [...new Set(gaps)];
	}

	/** UI attention only. Never changes the engine's status or upgrades a run. */
	export function answerNeedsReview(answer: Answer): boolean {
		if (answer.clarification) return false; // The existing clarification UI owns this decision.
		if (answerSemanticGaps(answer).length) return true;
		if (answer.verification.some((check) => {
			if (check.outcome === 'not_run') return false;
			if (check.finding?.effect === 'informational' && check.finding.code !== 'advisory') return false;
			return check.outcome === 'flagged' || !check.ok;
		})) return true;
		return answer.status === 'needs_review' && answer.evidence.length > 0
			&& answer.evidence.every((item) => !!item.error || item.verifier_disposition?.state === 'excluded');
	}
</script>

<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import Icon from './Icon.svelte';

	let {
		bodyId,
		label = 'How this was analyzed',
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
	<Button
		variant="ghost"
		size="sm"
		class="summary-toggle"
		aria-expanded={expanded}
		aria-controls={bodyId}
		title={`${expanded ? 'Hide' : 'Show'} ${label.toLowerCase()}`}
		onclick={() => ontoggle?.()}
	>
		<span class="summary-label">{label}</span>
		<span class="caret" class:open={expanded} aria-hidden="true"><Icon name="chevron-right" size={12} /></span>
	</Button>
</div>

<style>
	.evidence-summary {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-width: 0;
		margin-left: auto;
	}
	.evidence-summary :global(.summary-toggle) {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		height: auto;
		min-height: 0;
		min-width: 0;
		padding: 3px 5px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	.evidence-summary :global(.summary-toggle:hover),
	.evidence-summary :global(.summary-toggle[aria-expanded='true']) {
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
	.caret :global(svg) {
		width: 12px;
		height: 12px;
	}
	.caret.open {
		transform: rotate(90deg);
	}
</style>
