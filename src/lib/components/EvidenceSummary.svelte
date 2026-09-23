<script lang="ts">
	import type { Answer, VerificationStatus } from '$lib/types';
	import { answerStatus } from '$lib/verify';
	import Icon from './Icon.svelte';

	let {
		answer,
		bodyId,
		expanded = false,
		ontoggle
	}: {
		answer: Answer;
		bodyId: string;
		expanded?: boolean;
		ontoggle?: () => void;
	} = $props();

	let stepCount = $derived(answer.evidence.length);
	let durationMs = $derived(answer.evidence.reduce((total, item) => total + item.ms, 0));
	let status = $derived(answerStatus(answer));

	const STATUS_LABEL: Record<VerificationStatus, string> = {
		verified: 'Verified',
		needs_review: 'Needs review',
		insufficient_data: 'Insufficient data',
		failed: 'Could not verify'
	};

	function duration(ms: number): string {
		if (ms < 1000) return `${ms}ms`;
		return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)}s`;
	}

</script>

<div class="evidence-summary" aria-label="Answer evidence">
	<button
		class="summary-toggle"
		type="button"
		aria-expanded={expanded}
		aria-controls={bodyId}
		title={expanded ? 'Hide evidence' : 'Show evidence'}
		onclick={() => ontoggle?.()}
	>
		<span class="status-mark {status}" aria-hidden="true">
			<Icon name={status === 'verified' ? 'check' : 'alert'} size={12} />
		</span>
		<span class="summary-label">{STATUS_LABEL[status]}</span>
			<span class="summary-meta">· {stepCount} step{stepCount === 1 ? '' : 's'} · {duration(durationMs)}</span>
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
	.status-mark {
		display: inline-flex;
		color: var(--text-faint);
	}
	.status-mark.verified {
		color: var(--ok);
	}
	.status-mark.needs_review,
	.status-mark.failed {
		color: var(--warn);
	}
	.status-mark.insufficient_data {
		color: var(--text-faint);
	}
	.summary-label {
		font-weight: 600;
	}
	.summary-meta {
		color: var(--text-faint);
	}
	.caret {
		display: inline-flex;
		color: var(--text-faint);
		transition: transform var(--dur-fast) var(--ease);
	}
	.caret.open {
		transform: rotate(90deg);
	}
	@media (max-width: 600px) {
		.summary-meta {
			display: none;
		}
	}
</style>
