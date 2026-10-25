<script lang="ts">
	import type { VerificationCheck, VerificationFindingCode, VerificationStatus } from '$lib/types';
	import Icon from './Icon.svelte';

	let {
		checks,
		status,
		heading,
		compact = false
	}: {
		checks: VerificationCheck[];
		status?: VerificationStatus;
		heading?: string;
		compact?: boolean;
	} = $props();

	const STATUS_COPY: Record<VerificationStatus, { label: string; explanation: string }> = {
		verified: {
			label: 'Verified under recorded checks',
			explanation: 'This covers only the named checks; it does not certify every interpretation, assumption, or business definition. See any checks marked Not run.'
		},
		needs_review: {
			label: 'Needs review',
			explanation: 'At least one check raised a concern. Review the flagged items before relying on this result.'
		},
		insufficient_data: {
			label: 'Insufficient evidence',
			explanation: 'There was not enough accepted evidence to verify a source-backed result.'
		},
		failed: {
			label: 'Verification failed',
			explanation: 'A blocking check failed. Do not treat this result as verified.'
		}
	};

	const SCOPE_COPY: Record<VerificationFindingCode, { checks: string; limits: string }> = {
		advisory: {
			checks: 'The specific concern named by this check was raised for review.',
			limits: 'An advisory is a review signal, not proof that the concern is true or that unrelated output is wrong.'
		},
		replay_match: {
			checks: 'The recorded query was re-run and matched its saved result at the verifier’s comparison granularity.',
			limits: 'A matching replay does not prove that the query, filters, business definitions, or interpretation answer the right question.'
		},
		replay_mismatch: {
			checks: 'The recorded query failed to run again or its new result differed from the saved result.',
			limits: 'This does not establish which result is correct or invalidate unrelated evidence in the run.'
		},
		unsupported_claim: {
			checks: 'The named claim was not supported by the recorded evidence this check examined.',
			limits: 'Lack of support does not prove the claim false; it remains unverified until supported or revised.'
		},
		semantic_execution_mismatch: {
			checks: 'The recorded execution or result conflicts with a stated part of the requested analysis.',
			limits: 'This does not decide the correct business meaning or invalidate evidence outside the named scope.'
		},
		semantic_caveat: {
			checks: 'A mechanical check could not confirm an interpretation or equivalence from the recorded data alone.',
			limits: 'The check cannot decide whether the analyst’s intended meaning is correct; that may require domain context.'
		},
		chart_mismatch: {
			checks: 'The chart did not match the recorded source result it was built from.',
			limits: 'This concerns the chart artifact, not whether the source query or other outputs are correct.'
		},
		contract_mismatch: {
			checks: 'A specific analysis invariant, reconciliation, or derived claim did not match the recorded contract and evidence.',
			limits: 'It does not certify untested metrics, filters, populations, or business definitions.'
		},
		workspace_stale: {
			checks: 'The workspace changed while this analysis was running, so the run may reflect a stale snapshot.',
			limits: 'This does not identify which values changed; rerun against the current workspace before relying on the result.'
		},
		model_disagreement: {
			checks: 'A second pass from the same model produced a different answer.',
			limits: 'This is not independent evidence and does not establish which answer is correct.'
		}
	};

	function scopeCopy(check: VerificationCheck): { checks: string; limits: string } {
		const code = check.finding?.code;
		return (code && SCOPE_COPY[code]) || {
			checks: 'This check’s label and detail describe the recorded result; no structured scope note was saved.',
			limits: 'The record does not say which other parts of the analysis were covered. This check alone is not a general correctness guarantee.'
		};
	}

	function targetLabel(check: VerificationCheck): string | undefined {
		const finding = check.finding;
		if (!finding) return undefined;
		const target = finding.target.replaceAll('_', ' ');
		return finding.target_id ? `${target}: ${finding.target_id}` : target;
	}

	function effectLabel(check: VerificationCheck): string | undefined {
		const effect = check.finding?.effect;
		if (!effect || effect === 'informational') return undefined;
		if (effect === 'repair') return 'Repair requested';
		if (effect === 'exclude_evidence') return 'Evidence excluded';
		if (effect === 'withhold_artifact') return 'Artifact withheld';
		return 'Answer blocked';
	}

	function outcome(check: VerificationCheck): 'passed' | 'flagged' | 'not_run' | 'informational' {
		if (
			!check.ok &&
			check.finding?.effect === 'informational' &&
			check.finding.code !== 'advisory'
		) return 'informational';
		return check.outcome ?? (check.ok ? 'passed' : 'flagged');
	}

	function outcomeLabel(check: VerificationCheck): string {
		const value = outcome(check);
		return value === 'passed' ? 'Passed' : value === 'flagged' ? 'Flagged' : value === 'not_run' ? 'Not run' : 'Info';
	}
</script>

{#if status || checks.length}
	<div class="verification-results" class:compact>
		{#if status}
			{@const outcome = STATUS_COPY[status]}
			<section class="verification-outcome" data-verification-status={status} aria-label="Verification outcome">
				<div class="verification-outcome-heading">
					<Icon name={status === 'verified' ? 'check' : status === 'needs_review' ? 'info' : 'alert'} size={14} />
					<strong>{outcome.label}</strong>
				</div>
				<p>{outcome.explanation}</p>
			</section>
		{/if}

		{#if checks.length}
			{#if heading}<h4 class="verification-heading">{heading}</h4>{/if}
			<ul class="verification-list">
				{#each checks as check, index (`${check.label}-${index}`)}
					{@const scope = scopeCopy(check)}
					<li data-verification-check data-check-outcome={outcome(check)} data-verification-code={check.finding?.code ?? 'unscoped'}>
						<div class="check-heading">
							<span class="check-outcome" class:passed={outcome(check) === 'passed'} class:flagged={outcome(check) === 'flagged'} class:not-run={outcome(check) === 'not_run'} class:informational={outcome(check) === 'informational'}>
								<Icon name={outcome(check) === 'passed' ? 'check' : outcome(check) === 'flagged' ? 'alert' : 'info'} size={12} />
								{outcomeLabel(check)}
							</span>
							<strong>{check.label}</strong>
						</div>
						{#if check.detail}<p class="check-detail">{check.detail}</p>{/if}
						{#if effectLabel(check) || targetLabel(check)}
							<p class="check-target">{#if effectLabel(check)}{effectLabel(check)}{/if}{#if effectLabel(check) && targetLabel(check)} · {/if}{#if targetLabel(check)}Applies to {targetLabel(check)}{/if}</p>
						{/if}
						<details class="check-scope">
							<summary>What this check tests—and does not</summary>
							<dl>
								<div><dt>Tests</dt><dd>{scope.checks}</dd></div>
								<div><dt>Does not establish</dt><dd>{scope.limits}</dd></div>
							</dl>
							{#if check.finding?.guidance}<p class="check-guidance">Review guidance: {check.finding.guidance}</p>{/if}
						</details>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
{/if}

<style>
	.verification-results { display: grid; gap: var(--space-2); min-width: 0; }
	.verification-results.compact { margin-top: var(--space-2); }
	.verification-outcome { display: grid; gap: 3px; padding: var(--space-2) var(--space-3); border-left: 2px solid var(--border); background: var(--bg-inset); }
	.verification-outcome[data-verification-status='verified'] { border-color: var(--ok); }
	.verification-outcome[data-verification-status='insufficient_data'] { border-color: var(--warn); }
	.verification-outcome[data-verification-status='failed'] { border-color: var(--danger, #b42318); }
	.verification-outcome-heading { display: flex; align-items: center; gap: var(--space-1); color: var(--text); font-size: var(--fs-xs); }
	.verification-outcome[data-verification-status='verified'] .verification-outcome-heading { color: var(--ok); }
	.verification-outcome[data-verification-status='failed'] .verification-outcome-heading { color: var(--danger, #b42318); }
	.verification-outcome p { margin: 0; color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.45; }
	.verification-heading { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); font-weight: 600; }
	.verification-list { display: grid; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
	.verification-list > li { min-width: 0; padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); }
	.verification-results.compact .verification-list > li { padding: var(--space-2); }
	.check-heading { display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px var(--space-2); }
	.check-heading strong { color: var(--text); font-size: var(--fs-xs); font-weight: 600; overflow-wrap: anywhere; }
	.check-outcome { display: inline-flex; flex: none; align-items: center; gap: 4px; font-size: var(--fs-xs); font-weight: 600; }
	.check-outcome.passed { color: var(--ok); }
	.check-outcome.flagged { color: var(--warn); }
	.check-outcome.not-run { color: var(--text-faint); }
	.check-outcome.informational { color: var(--text-faint); }
	.check-detail, .check-target, .check-guidance { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.45; overflow-wrap: anywhere; }
	.check-scope { margin-top: var(--space-1); color: var(--text-dim); font-size: var(--fs-xs); }
	.check-scope summary { width: fit-content; cursor: pointer; color: var(--text-faint); }
	.check-scope summary:hover { color: var(--text); }
	.check-scope dl { display: grid; gap: var(--space-1); margin: var(--space-1) 0 0; }
	.check-scope dl div { display: grid; grid-template-columns: 112px minmax(0, 1fr); gap: var(--space-2); }
	.check-scope dt { color: var(--text-faint); }
	.check-scope dd { margin: 0; overflow-wrap: anywhere; }
	.check-guidance { color: var(--text-faint); }
</style>
