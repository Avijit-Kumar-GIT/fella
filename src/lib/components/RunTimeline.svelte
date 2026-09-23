<script lang="ts">
	import { session } from '$lib/session.svelte';
	import type { Message, RunStep } from '$lib/types';
	import Icon from './Icon.svelte';
	import DataLoader from './DataLoader.svelte';

	let chat = $derived(session.activeChat);
	let steps = $derived(chat?.runSteps ?? []);
	let running = $derived(chat?.busy ?? false);
	let failed = $derived(steps.some((step) => step.state === 'error'));
	let evidenceSteps = $derived(steps.filter((step) => step.evidence).length);
	let visibleSteps = $derived(steps.slice(-6));
	let expanded = $state(true);
	let previousChatId = $state('');

	$effect(() => {
		const id = chat?.id ?? '';
		if (id !== previousChatId) {
			previousChatId = id;
			expanded = running;
		}
		if (running) expanded = true;
	});

	function lastAnswer(): Message | null {
		if (!chat) return null;
		for (let i = chat.messages.length - 1; i >= 0; i--) {
			const message = chat.messages[i];
			if (message.role === 'assistant') return message;
		}
		return null;
	}

	function inspect(step: RunStep, visibleIndex: number): void {
		const answer = lastAnswer();
		if (!answer) return;
		session.openInspector({
			kind: 'answer',
			messageId: answer.id,
			stepIndex: steps.length - visibleSteps.length + visibleIndex
		});
	}

	function duration(ms: number | null | undefined): string {
		if (ms == null) return '';
		if (ms < 1000) return `${ms}ms`;
		return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)}s`;
	}

	function stepResult(step: RunStep): string {
		if (step.evidence?.error) return step.evidence.error;
		if (step.evidence?.result_summary) return step.evidence.result_summary;
		if (step.state === 'running') return 'In progress';
		if (step.state === 'error') return 'Could not complete this step';
		return 'Complete';
	}

	let hasCompletedAnswer = $derived.by(() => {
		const answer = lastAnswer();
		return !!answer?.answer && !answer.pending;
	});
</script>

{#if steps.length && (running || !hasCompletedAnswer)}
	<section class="run-timeline" class:active={running} aria-label="Question run">
		<div class="timeline-head">
			<div class="run-status">
				{#if running}
					<DataLoader size={18} />
				{:else}
					<span class="run-dot" class:error={failed} aria-hidden="true"></span>
				{/if}
				<strong>{running ? chat?.activity || 'Working through the workspace' : failed ? 'Run needs review' : evidenceSteps ? 'Workspace checked' : 'Question answered'}</strong>
			</div>
			<div class="timeline-tools">
				<span class="run-meta">
					{steps.length} step{steps.length === 1 ? '' : 's'}{#if !running && chat?.runDurationMs != null} · {duration(chat.runDurationMs)}{/if}
				</span>
				<button
					class="disclosure"
					type="button"
					aria-label={expanded ? 'Collapse run details' : 'Show run details'}
					aria-expanded={expanded}
					onclick={() => (expanded = !expanded)}
				>
					<Icon name="chevron-right" size={12} />
				</button>
			</div>
		</div>

		{#if expanded}
			<div class="steps">
				{#each visibleSteps as step, i (step.id)}
					<button
						class="step"
						class:clickable={!!lastAnswer()}
						type="button"
						aria-label={lastAnswer() ? `${step.label}. View details` : step.label}
						title={lastAnswer() ? 'View details' : undefined}
						onclick={() => inspect(step, i)}
					>
						<span class="step-mark" class:running={step.state === 'running'} class:error={step.state === 'error'}>
							{#if step.state === 'complete'}
								<Icon name="check" size={12} />
							{:else if step.state === 'error'}
								<Icon name="alert" size={12} />
							{:else}
								<span class="mini-thinking" aria-hidden="true"></span>
							{/if}
						</span>
						<span class="step-copy">
							<strong>{step.label}</strong>
							<small>{stepResult(step)}</small>
						</span>
						{#if lastAnswer()}<span class="step-info"><Icon name="info" size={12} /></span>{/if}
					</button>
				{/each}
				{#if steps.length > visibleSteps.length}
					<p class="more-steps">Showing the last {visibleSteps.length} steps</p>
				{/if}
			</div>
		{/if}
	</section>
{/if}

<style>
	.run-timeline {
		max-width: 76ch;
		margin: 0 auto var(--space-3);
		color: var(--text-dim);
	}
	.timeline-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		min-height: 30px;
	}
	.run-status,
	.timeline-tools,
	.run-meta {
		display: inline-flex;
		align-items: center;
	}
	.run-status {
		min-width: 0;
		gap: 8px;
	}
	.run-status strong {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.run-dot {
		width: 7px;
		height: 7px;
		flex: none;
		border-radius: 50%;
		background: var(--ok);
	}
	.run-dot.error {
		background: var(--err);
	}
	.timeline-tools {
		gap: var(--space-2);
		flex: none;
	}
	.run-meta {
		color: var(--text-faint);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	.disclosure {
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease), transform var(--dur-fast) var(--ease);
	}
	.disclosure:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.disclosure :global(svg) {
		transition: transform var(--dur-fast) var(--ease);
	}
	.run-timeline:has(.steps) .disclosure :global(svg) {
		transform: rotate(90deg);
	}
	.run-timeline:not(:has(.steps)) .disclosure :global(svg) {
		transform: rotate(0deg);
	}
	.steps {
		position: relative;
		padding: 2px 0 var(--space-2) 15px;
	}
	.steps::before {
		content: '';
		position: absolute;
		top: 0;
		bottom: 16px;
		left: 3px;
		width: 1px;
		background: var(--border);
	}
	.step {
		position: relative;
		width: 100%;
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: 26px;
		padding: 3px var(--space-2);
		border-radius: var(--radius-chip);
		text-align: left;
		color: var(--text-dim);
	}
	.step.clickable:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.step-mark {
		position: relative;
		z-index: 1;
		display: grid;
		place-items: center;
		width: 9px;
		height: 9px;
		margin-left: -16px;
		border: 1px solid var(--border-strong);
		border-radius: 50%;
		background: var(--bg-raised);
		color: var(--ok);
	}
	.step-mark.running {
		border-color: var(--brand);
		color: var(--brand);
	}
	.step-mark.error {
		border-color: var(--err);
		color: var(--err);
	}
	.step-copy {
		min-width: 0;
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		flex: 1;
	}
	.step-copy strong {
		font-size: var(--fs-xs);
		font-weight: 600;
		white-space: nowrap;
	}
	.step-copy small {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.step-info {
		flex: none;
		color: var(--text-faint);
	}
	.more-steps {
		margin: var(--space-1) 0 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.mini-thinking {
		width: 3px;
		height: 3px;
		border-radius: 50%;
		background: currentColor;
		box-shadow: 4px 0 currentColor, -4px 0 currentColor;
		animation: mini-thinking 1.2s ease-in-out infinite;
	}
	@keyframes mini-thinking {
		0%, 100% { opacity: 0.35; }
		50% { opacity: 1; }
	}
	@media (max-width: 600px) {
		.step-copy {
			align-items: flex-start;
			flex-direction: column;
			gap: 0;
		}
		.step-copy small {
			max-width: 100%;
		}
	}
</style>
