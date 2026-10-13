<script lang="ts">
	import { tick } from 'svelte';
	import { session } from '$lib/session.svelte';
	import type { AnalysisTurn, Answer, EvidenceItem, EvidenceSource, Message, RunStep, VerificationCheck } from '$lib/types';
	import { Collapsible } from '$lib/components/ui/collapsible';
	import { Spinner } from '$lib/components/ui/spinner';
	import { analystStepForTool, runStateLabel } from '$lib/run-progress';
	import Icon from './Icon.svelte';

	interface DisplayStep {
		id: string;
		label: string;
		state: RunStep['state'];
		durationMs?: number;
		summary?: string;
		sources: string[];
		evidenceIndex?: number;
	}

	let {
		message,
		turn,
		question = '',
		bodyId,
		onopenevidence
	}: {
		message?: Message;
		turn?: AnalysisTurn;
		question?: string;
		bodyId: string;
		onopenevidence?: () => void;
	} = $props();

	let chat = $derived(session.activeChat);
	let running = $derived(Boolean(message?.pending && chat?.busy && chat.messages.at(-1)?.id === message.id));
	let answer = $derived.by((): Answer | undefined => {
		if (message?.answer) return message.answer;
		if (!turn) return undefined;
		return {
			turn_id: turn.id,
			trace: turn.trace,
			plan: turn.plan,
			contract: turn.contract,
			provenance: turn.provenance,
			text: turn.result.text,
			evidence: turn.result.evidence as EvidenceItem[],
			verification: turn.verification?.checks ?? turn.result.verification,
			status: turn.result.status,
			workspace: turn.workspace && turn.workspace_revision
				? { path: turn.workspace, revision: turn.workspace_revision }
				: undefined,
			usage: turn.result.usage
		};
	});
	let runState = $derived(message?.run_state ?? turn?.state);
	let snapshots = $derived(message?.run_steps ?? []);
	let activeSteps = $derived(running ? (chat?.runSteps ?? []) : []);
	let steps = $derived.by((): DisplayStep[] => {
		const recorded = answer?.trace?.steps ?? [];
		const evidence = answer?.evidence ?? [];
		if (recorded.length) {
			const byId = new Map(recorded.map((step) => [step.id, step]));
			const used = new Set<string>();
			const displayRecorded = (step: (typeof recorded)[number]): DisplayStep => {
				const evidenceIndex = evidence.findIndex((item) => item.id === step.id);
				const item = evidenceIndex >= 0 ? evidence[evidenceIndex] : undefined;
				return {
					id: step.id,
					label: analystStepForTool(step.operation),
					state: !step.success || item?.error ? 'error' : 'complete',
					durationMs: step.duration_ms,
					summary: item?.error ? undefined : item?.result_summary || step.summary,
					sources: item?.sources?.map(sourceLabel) ?? step.sources,
					evidenceIndex: evidenceIndex >= 0 ? evidenceIndex : undefined
				};
			};
			if (snapshots.length) {
				const ordered = snapshots.map((snapshot): DisplayStep => {
					if (!snapshot.tool) {
						return {
							id: snapshot.id,
							label: snapshot.label,
							state: snapshot.state,
							durationMs: stepDuration(snapshot),
							sources: []
						};
					}
					let trace = snapshot.evidence_id ? byId.get(snapshot.evidence_id) : undefined;
					if (!trace) trace = recorded.find((candidate) => !used.has(candidate.id) && candidate.operation === snapshot.tool);
					if (!trace) {
						return {
							id: snapshot.id,
							label: snapshot.label,
							state: snapshot.state,
							durationMs: stepDuration(snapshot),
							summary: snapshot.summary,
							sources: snapshot.sources ?? []
						};
					}
					used.add(trace.id);
					return displayRecorded(trace);
				});
				for (const trace of recorded) if (!used.has(trace.id)) ordered.push(displayRecorded(trace));
				return ordered;
			}
			return recorded.map(displayRecorded);
		}
		return (running ? activeSteps : snapshots).map((step) => ({
			id: step.id,
			label: step.label,
			state: step.state,
			durationMs: stepDuration(step),
			summary: step.summary,
			sources: step.sources ?? [],
			evidenceIndex: step.evidence_id
				? evidence.findIndex((item) => item.id === step.evidence_id)
				: undefined
		}));
	});
	let activeStep = $derived([...steps].reverse().find((step) => step.state === 'running'));
	let failed = $derived(
		steps.some((step) => step.state === 'error') ||
			answer?.status === 'failed' ||
			runState === 'failed'
	);
	let expanded = $state(false);
	let previousRunKey = $state('');

	$effect(() => {
		const runKey = turn?.id ?? message?.id ?? '';
		if (runKey !== previousRunKey) {
			previousRunKey = runKey;
			expanded = Boolean(turn);
		}
		if (running) expanded = true;
	});

	function sourceLabel(source: EvidenceSource): string {
		return source.table ? `${source.source} · ${source.table}` : source.source;
	}

	function stepDuration(step: RunStep): number | undefined {
		if (step.finished_at_ms === undefined) return undefined;
		return Math.max(0, step.finished_at_ms - step.started_at_ms);
	}

	function duration(ms: number | null | undefined): string {
		if (ms == null) return '';
		if (ms < 1000) return `${ms} ms`;
		return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s`;
	}

	function isInformationalCheck(check: VerificationCheck): boolean {
		return !check.ok && check.finding?.effect === 'informational' && check.finding.code !== 'advisory';
	}

	let statusLabel = $derived.by(() => {
		if (running) return activeStep?.label ?? runStateLabel(chat?.runState) ?? 'Preparing the analysis';
		if (answer?.clarification) return 'Waiting for your input';
		if (answer?.status === 'insufficient_data') return 'Insufficient data';
		if (answer?.status === 'failed') return 'Verification failed';
		if (failed) return 'Analysis did not finish';
		if (answer) return answer.status === 'verified' ? 'Analysis checked' : 'Analysis complete';
		if (runState) return runStateLabel(runState) ?? 'Analysis complete';
		return 'Analysis did not finish';
	});

	let verificationSummary = $derived.by(() => {
		const checks = answer?.verification ?? [];
		if (!checks.length) return '';
		const passed = checks.filter((check) => check.outcome === 'passed' || (!check.outcome && check.ok)).length;
		const informational = checks.filter(isInformationalCheck).length;
		const flagged = checks.filter((check) => !isInformationalCheck(check) && (check.outcome === 'flagged' || (!check.outcome && !check.ok))).length;
		const notRun = checks.filter((check) => check.outcome === 'not_run').length;
		return [passed ? `${passed} passed` : '', informational ? `${informational} info` : '', flagged ? `${flagged} flagged` : '', notRun ? `${notRun} not run` : '']
			.filter(Boolean)
			.join(' · ');
	});
	let methodLabel = $derived.by(() => {
		const strategy = answer?.plan?.strategy;
		if (!strategy) return '';
		const labels: Record<string, string> = {
			direct_tools: 'Workspace tools',
			compiled_sql: 'SQL analysis',
			python_fallback: 'Python-assisted analysis',
			document_fallback: 'Document review'
		};
		return labels[strategy] ?? strategy.replaceAll('_', ' ');
	});
	let totalDuration = $derived(
		message?.run_duration_ms ?? answer?.trace?.elapsed_ms ?? chat?.runDurationMs ?? null
	);
	let displayQuestion = $derived(question || turn?.question || '');
	let runQuestionId = $derived(message?.run_question_id ?? (running ? chat?.runQuestionId : undefined) ?? turn?.id);
	let rerunOf = $derived(message?.rerun_of ?? turn?.rerun_of);
	let hasTimeline = $derived(running || steps.length > 0 || Boolean(answer?.clarification) || Boolean(runState));

	async function inspectEvidence(targetId: string): Promise<void> {
		onopenevidence?.();
		await tick();
		const target = document.getElementById(targetId);
		target?.scrollIntoView({ behavior: 'smooth', block: 'center' });
		target?.focus({ preventScroll: true });
	}
</script>

{#if hasTimeline}
	<section
		class="run-timeline"
		class:active={running}
		class:failed
		aria-label="Question run"
		data-question-id={runQuestionId}
		data-run-state={message?.run_state ?? turn?.state ?? (answer?.clarification ? 'clarify' : answer?.status)}
	>
		<Collapsible.Root bind:open={expanded} class="timeline-collapsible">
			<div class="timeline-head">
				<div class="run-status">
					{#if running}
						<Spinner size={16} />
					{:else}
						<span class="run-dot" class:error={failed} aria-hidden="true"></span>
					{/if}
					<strong>{statusLabel}</strong>
					{#if rerunOf}<span class="run-kind">Rerun</span>{/if}
				</div>
				<div class="timeline-tools">
					<span class="run-meta">
						{steps.length} step{steps.length === 1 ? '' : 's'}{#if totalDuration != null} · {duration(totalDuration)}{/if}
					</span>
					<Collapsible.Trigger class="disclosure" aria-label={expanded ? 'Hide analysis steps' : 'Show analysis steps'}>
						<Icon name="chevron-right" size={12} />
					</Collapsible.Trigger>
				</div>
			</div>
			{#if running ? chat?.runQuestion : displayQuestion}
				<p class="run-question" title={running ? chat?.runQuestion : displayQuestion}>
					<span>Question</span>
					<strong>{running ? chat?.runQuestion : displayQuestion}</strong>
				</p>
			{/if}

			<Collapsible.Content class="run-details">
				{#if methodLabel}
					<div class="run-method"><span>Method</span><strong>{methodLabel}</strong></div>
				{/if}
				{#if answer?.plan?.steps.length}
					<details class="planned-steps">
						<summary>Planned approach · {answer.plan.steps.length} step{answer.plan.steps.length === 1 ? '' : 's'} (not execution evidence)</summary>
						<ol>
							{#each answer.plan.steps as planStep, index (`${turn?.id ?? message?.id ?? 'run'}-plan-${index}`)}
								<li>{analystStepForTool(planStep)}</li>
							{/each}
						</ol>
						<p class="planned-note">Recorded work appears separately below; the run may have skipped or changed parts of this plan.</p>
					</details>
				{/if}
				{#if answer?.clarification}
					<div class="clarification-note">
						<strong>Needs your input</strong>
						<span>{answer.clarification.question}</span>
					</div>
				{/if}
				{#if steps.length}
					<h4 class="recorded-steps-heading">Recorded steps</h4>
					<ol class="run-step-list" aria-label="Recorded steps">
						{#each steps as step, index (step.id)}
							<li class="run-step" data-step-state={step.state}>
								<span class="step-mark" class:running={step.state === 'running'} class:error={step.state === 'error'} aria-hidden="true">
									{#if step.state === 'complete'}
										<Icon name="check" size={12} />
									{:else if step.state === 'error'}
										<Icon name="alert" size={12} />
									{:else}
										<span class="mini-thinking"></span>
									{/if}
								</span>
								<div class="step-content">
									<div class="step-heading">
										<strong>{step.label}</strong>
										<small>{step.state === 'running' ? 'In progress' : step.state === 'error' ? 'Failed' : 'Complete'}{#if step.durationMs != null} · {duration(step.durationMs)}{/if}</small>
									</div>
									{#if step.summary}<p class="step-summary"><strong>Recorded output</strong> · {step.summary}</p>{/if}
									{#if step.sources.length}<p class="step-sources"><strong>Source inputs</strong> · {step.sources.join(', ')}</p>{/if}
									{#if step.evidenceIndex !== undefined && step.evidenceIndex >= 0}
										<button type="button" class="evidence-link" onclick={() => void inspectEvidence(`${bodyId}-step-${step.evidenceIndex}`)}>
											{step.state === 'error' ? 'Inspect failure details' : 'Inspect output'}
										</button>
									{/if}
								</div>
							</li>
						{/each}
					</ol>
				{:else if running}
					<p class="recorded-steps-empty">Recorded steps will appear here as the analysis runs.</p>
				{:else if answer}
					<p class="recorded-steps-empty">No execution steps were recorded for this run.</p>
				{/if}
				{#if verificationSummary}
					<div class="verification-summary">
						<span><strong>Quality checks</strong> · {verificationSummary}</span>
						<button type="button" class="evidence-link" onclick={() => void inspectEvidence(bodyId)}>Review checks and evidence</button>
					</div>
				{:else if answer}
					<p class="verification-summary">No verification checks were recorded for this run.</p>
				{/if}
			</Collapsible.Content>
		</Collapsible.Root>
	</section>
{/if}

<style>
	.run-timeline {
		max-width: 76ch;
		margin: 0 auto var(--space-3);
		padding: var(--space-2) 0 0;
		border-top: 1px solid var(--border);
		color: var(--text-dim);
	}
	:global(.timeline-collapsible) { min-width: 0; }
	.timeline-head,
	.run-status,
	.timeline-tools,
	.run-meta,
	.step-heading {
		display: flex;
		align-items: center;
	}
	.timeline-head { justify-content: space-between; gap: var(--space-3); min-height: 28px; }
	.run-status { min-width: 0; gap: var(--space-2); }
	.run-status strong { overflow: hidden; color: var(--text); font-size: var(--fs-sm); font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }
	.run-dot { width: 7px; height: 7px; flex: none; border-radius: 50%; background: var(--ok); }
	.run-dot.error { background: var(--err); }
	.run-kind { flex: none; padding: 2px 7px; border-radius: var(--radius-chip); background: var(--bg-inset); color: var(--text-faint); font-size: var(--fs-xs); }
	.timeline-tools { flex: none; gap: var(--space-2); }
	.run-meta { color: var(--text-faint); font-size: var(--fs-xs); white-space: nowrap; }
	.run-question { display: flex; align-items: flex-start; gap: var(--space-2); max-width: 76ch; margin: var(--space-1) 0 var(--space-2) 24px; color: var(--text-faint); font-size: var(--fs-xs); }
	.run-question span { flex: none; }
	.run-question strong { min-width: 0; color: var(--text-dim); font-weight: 500; overflow-wrap: anywhere; }
	:global(.disclosure) { display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--radius-chip); color: var(--text-faint); transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease), transform var(--dur-fast) var(--ease); }
	:global(.disclosure:hover) { background: var(--bg-inset); color: var(--text); }
	:global(.disclosure svg) { transition: transform var(--dur-fast) var(--ease); }
	:global(.disclosure[data-state='open'] svg) { transform: rotate(90deg); }
	:global(.run-details) { position: relative; padding: var(--space-2) 0 var(--space-2) 15px; }
	:global(.run-details::before) { content: ''; position: absolute; top: 0; bottom: 14px; left: 3px; width: 1px; background: var(--border); }
	.run-method { display: flex; gap: var(--space-2); margin: 0 0 var(--space-2); font-size: var(--fs-xs); }
	.run-method span { color: var(--text-faint); }
	.run-method strong { color: var(--text-dim); font-weight: 600; }
	.planned-steps { margin: 0 0 var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); }
	.planned-steps summary { width: fit-content; cursor: pointer; }
	.planned-steps ol { display: grid; gap: 3px; margin: var(--space-1) 0 0; padding-left: var(--space-5); }
	.planned-note { margin: var(--space-1) 0 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.clarification-note { display: grid; gap: 2px; margin: var(--space-2) 0; font-size: var(--fs-xs); }
	.clarification-note strong { color: var(--text); }
	.clarification-note span { color: var(--text-dim); }
	.run-step-list { display: grid; gap: var(--space-1); margin: 0; padding: 0; list-style: none; }
	.recorded-steps-heading { margin: var(--space-2) 0 var(--space-1); color: var(--text-faint); font-size: var(--fs-xs); font-weight: 600; }
	.recorded-steps-empty { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.run-step { position: relative; display: grid; grid-template-columns: 10px minmax(0, 1fr); gap: var(--space-2); padding: 3px 0; }
	.step-mark { position: relative; z-index: 1; display: grid; place-items: center; width: 9px; height: 9px; margin-top: 4px; margin-left: -16px; border: 1px solid var(--border-strong); border-radius: 50%; background: var(--bg-raised); color: var(--ok); }
	.step-mark.running { border-color: var(--brand); color: var(--brand); }
	.step-mark.error { border-color: var(--err); color: var(--err); }
	.step-content { min-width: 0; }
	.step-heading { flex-wrap: wrap; gap: 2px var(--space-2); }
	.step-heading strong { color: var(--text-dim); font-size: var(--fs-xs); font-weight: 600; }
	.step-heading small { color: var(--text-faint); font-size: var(--fs-xs); }
	.step-summary,
	.step-sources { margin: 3px 0 0; color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.45; }
	.step-summary strong, .step-sources strong { color: var(--text-dim); font-weight: 600; }
	.step-sources { overflow-wrap: anywhere; }
	.evidence-link { display: inline-flex; margin: 4px 0 0; padding: 0; border: 0; background: transparent; color: var(--brand); font: inherit; font-size: var(--fs-xs); cursor: pointer; }
	.evidence-link:hover { color: var(--text); text-decoration: underline; }
	.verification-summary { display: flex; flex-wrap: wrap; align-items: baseline; gap: 0 var(--space-2); margin: var(--space-2) 0 0; padding-top: var(--space-2); border-top: 1px solid var(--border); color: var(--text-faint); font-size: var(--fs-xs); }
	.verification-summary strong { color: var(--text-dim); font-weight: 600; }
	.mini-thinking { width: 3px; height: 3px; border-radius: 50%; background: currentColor; box-shadow: 4px 0 currentColor, -4px 0 currentColor; animation: mini-thinking 1.2s ease-in-out infinite; }
	@keyframes mini-thinking { 0%, 100% { opacity: 0.35; } 50% { opacity: 1; } }
	@media (prefers-reduced-motion: reduce) { .mini-thinking { animation: none; } }
</style>
