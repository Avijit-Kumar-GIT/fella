<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { ipc } from '$lib/ipc';
	import type { AnalysisTurn, InvestigationQuestion, InvestigationQuestionAnswerStatus } from '$lib/types';

	let {
		question,
		questionLabel,
		onopenrun
	}: {
		question: InvestigationQuestion;
		questionLabel: string;
		onopenrun: (turnId: string) => void;
	} = $props();

	let expanded = $state(false);
	let loading = $state(false);
	let error = $state('');
	let loadedTurnId = $state<string | null>(null);
	let run = $state<AnalysisTurn | null>(null);
	let requestSerial = 0;
	let turnId = $derived(question.latest_analysis_turn_id ?? null);
	let answerLabel = $derived(answerStatusLabel(question.answer_status));
	let toggleLabel = $derived(answerStatusToggleLabel(question.answer_status));

	function answerStatusLabel(status: InvestigationQuestionAnswerStatus): string {
		return {
			not_started: 'Not answered',
			running: 'Running',
			answered: 'Complete answer',
			incomplete: 'Partial answer · needs review',
			unsupported: 'Unsupported by available data',
			blocked: 'No complete answer',
			outdated: 'Previous answer · outdated'
		}[status];
	}

	function answerStatusToggleLabel(status: InvestigationQuestionAnswerStatus): string {
		return {
			not_started: 'View answer',
			running: 'View answer',
			answered: 'View complete answer',
			incomplete: 'View partial answer',
			unsupported: 'View run response',
			blocked: 'View run response',
			outdated: 'View previous answer'
		}[status];
	}

	function openSavedRun(): void {
		if (run) onopenrun(run.id);
	}

	async function loadAnswer(): Promise<void> {
		if (!turnId || loading) return;
		error = '';
		if (loadedTurnId === turnId && run) return;

		const serial = ++requestSerial;
		loading = true;
		try {
			const saved = await ipc.analysisTurnLoad(turnId);
			if (serial !== requestSerial) return;
			run = saved;
			loadedTurnId = turnId;
		} catch (cause) {
			if (serial === requestSerial) error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (serial === requestSerial) loading = false;
		}
	}

	async function toggleAnswer(): Promise<void> {
		if (!turnId || loading) return;
		if (expanded) {
			expanded = false;
			return;
		}
		expanded = true;
		await loadAnswer();
	}
</script>

{#if turnId && question.answer_status !== 'running'}
	<section class="question-answer" data-answer-status={question.answer_status} aria-label="Latest answer for {questionLabel}">
		<header>
			<div>
				<strong>{answerLabel}</strong>
				<span>From the latest saved run for this Question</span>
			</div>
			<Button
				type="button"
				variant="outline"
				size="sm"
				aria-expanded={expanded}
				aria-label="{expanded ? 'Hide' : toggleLabel} for {questionLabel}"
				disabled={loading}
				onclick={() => void toggleAnswer()}
			>
				{loading ? 'Loading answer…' : expanded ? 'Hide answer' : toggleLabel}
			</Button>
		</header>
		{#if expanded}
			{#if loading}
				<p class="question-answer-state" role="status">Loading the answer from its saved run…</p>
			{:else if error}
				<div class="question-answer-error" role="alert">
					<p>Couldn't load this Question's saved answer: {error}</p>
					<Button type="button" variant="outline" size="sm" onclick={() => void loadAnswer()}>Retry</Button>
				</div>
			{:else if run}
				<div class="question-answer-output" aria-label="Saved response for {questionLabel}">{run.result.text}</div>
				<footer>
					<span>Run <code>{run.id}</code></span>
					<Button type="button" variant="ghost" size="sm" onclick={openSavedRun}>Open run details</Button>
				</footer>
			{/if}
		{/if}
	</section>
{/if}

<style>
	.question-answer { display: grid; gap: var(--space-2); min-width: 0; padding: var(--space-3); margin: var(--space-2) 0; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg-inset); }
	.question-answer > header, .question-answer > footer { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); }
	.question-answer > header > div { display: grid; min-width: 0; gap: 2px; }
	.question-answer strong { font-size: var(--fs-xs); font-weight: 650; }
	.question-answer header span, .question-answer footer > span { color: var(--text-faint); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.question-answer-output { max-height: 280px; overflow: auto; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); font-size: var(--fs-sm); line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; }
	.question-answer footer { justify-content: flex-end; flex-wrap: wrap; }
	.question-answer footer > span { margin-right: auto; }
	.question-answer code { font-size: var(--fs-xs); }
	.question-answer-state, .question-answer-error { margin: 0; font-size: var(--fs-xs); }
	.question-answer-error { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); color: var(--danger, #b42318); overflow-wrap: anywhere; }
	.question-answer-error p { margin: 0; }
	@media (max-width: 720px) {
		.question-answer > header { align-items: flex-start; flex-direction: column; }
	}
</style>
