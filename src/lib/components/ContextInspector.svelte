<script lang="ts">
	import { session } from '$lib/session.svelte';
	import type { EvidenceItem } from '$lib/types';
	import Icon from './Icon.svelte';

	let selection = $derived(session.inspectorSelection);
	let answerMessage = $derived.by(() => {
		if (selection?.kind !== 'answer') return null;
		return session.activeChat?.messages.find((message) => message.id === selection.messageId) ?? null;
	});
	let answerStep = $derived.by(() => {
		if (selection?.kind !== 'answer' || selection.stepIndex == null) return null;
		return session.activeChat?.runSteps[selection.stepIndex] ?? null;
	});
	let evidence = $derived.by((): EvidenceItem | null => {
		if (answerStep?.evidence) return answerStep.evidence;
		if (selection?.kind !== 'answer' || !answerMessage?.answer) return null;
		return answerMessage.answer.evidence[selection.stepIndex ?? -1] ?? null;
	});

	function baseName(path: string): string {
		return path.replace(/[/\\]+$/, '').split(/[/\\]/).pop() ?? path;
	}

	function answerStatus(): string {
		if (!answerMessage?.answer) return 'Still working';
		if (answerMessage.answer.status === 'verified') return 'Verified';
		if (answerMessage.answer.status === 'needs_review') return 'Needs review';
		if (answerMessage.answer.status === 'insufficient_data') return 'Insufficient data';
		if (answerMessage.answer.status === 'failed') return 'Could not verify';
		return 'Answer received';
	}
</script>

<aside class="inspector" aria-label="Details panel">
	<header class="inspector-head">
		<div>
			<p class="eyebrow">Details</p>
			<h2>{evidence ? 'Run step' : 'Answer details'}</h2>
		</div>
		<button class="close" type="button" aria-label="Close details" title="Close details" onclick={() => session.closeInspector()}>
			<Icon name="x" size={16} />
		</button>
	</header>

	<div class="inspector-body">
		{#if answerMessage?.answer}
			<div class="answer-state"><span class="status-dot"></span><strong>{answerStatus()}</strong></div>
			{#if answerMessage.answer.workspace?.path}
				<p class="path" title={answerMessage.answer.workspace.path}><Icon name="folder" size={12} /> {baseName(answerMessage.answer.workspace.path)}</p>
			{/if}
			{#if evidence}
				<div class="evidence-card">
					<div class="evidence-heading"><span class="type-label">Evidence step</span><code>{evidence.tool}</code></div>
					{#if evidence.note}<p>{evidence.note}</p>{/if}
					<strong>{evidence.result_summary}</strong>
					{#if evidence.row_count != null}<small>{evidence.row_count.toLocaleString()} rows · {evidence.ms}ms</small>{/if}
					{#if evidence.sql}
						<details><summary>View query</summary><pre>{evidence.sql}</pre></details>
					{/if}
					{#if evidence.error}<div class="error">{evidence.error}</div>{/if}
				</div>
			{:else}
				<p class="description">{answerMessage.answer.evidence.length} evidence step{answerMessage.answer.evidence.length === 1 ? '' : 's'} support this answer.</p>
			{/if}
			<div class="section">
				<div class="section-title"><span>Verification</span><span>{answerMessage.answer.verification.length}</span></div>
				{#each answerMessage.answer.verification as check (check.label)}
					<div class="check-row"><Icon name={check.ok ? 'check' : 'alert'} size={12} /><span>{check.label}</span></div>
				{/each}
			</div>
		{/if}
	</div>
</aside>

<style>
	.inspector {
		width: min(330px, 35vw);
		min-width: 280px;
		flex: none;
		display: flex;
		flex-direction: column;
		min-height: 0;
		border-left: 1px solid var(--border);
		background: var(--bg-raised);
		animation: inspector-in var(--dur) var(--ease);
	}
	.inspector-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-3);
		padding: var(--space-4);
		border-bottom: 1px solid var(--border);
	}
	.inspector-head h2 {
		max-width: 22ch;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--fs-lg);
		font-weight: 600;
	}
	.eyebrow,
	.type-label {
		margin: 0 0 var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.close {
		display: grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border-radius: var(--radius-sm);
		color: var(--text-faint);
	}
	.close:hover {
		background: var(--bg-inset);
		color: var(--text);
	}
	.inspector-body {
		min-height: 0;
		overflow: auto;
		padding: var(--space-5) var(--space-4);
	}
	.type-label {
		font-size: 10px;
	}
	.path {
		display: flex;
		align-items: center;
		gap: 5px;
		margin: 0 0 var(--space-4);
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10.5px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.description {
		margin: 0 0 var(--space-4);
		color: var(--text-dim);
		font-size: var(--fs-sm);
	}
	.section {
		margin-top: var(--space-4);
		border-top: 1px solid var(--border);
	}
	.section-title {
		display: flex;
		justify-content: space-between;
		padding: var(--space-3) 0 var(--space-2);
		color: var(--text-dim);
		font-size: 10px;
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	.section-title span:last-child {
		color: var(--text-faint);
		font-weight: 500;
	}
	.check-row {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: 4px 0;
		font-size: var(--fs-xs);
		justify-content: flex-start;
		color: var(--text-dim);
	}
	.check-row :global(svg) {
		color: var(--ok);
	}
	.answer-state {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: var(--space-3);
		font-size: var(--fs-sm);
	}
	.status-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ok);
	}
	.evidence-card {
		padding: var(--space-3);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
	}
	.evidence-heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
	}
	.evidence-heading .type-label {
		margin: 0;
	}
	.evidence-heading code,
	.evidence-card pre {
		font-family: var(--mono);
		font-size: 10px;
	}
	.evidence-card p {
		margin: 0 0 var(--space-2);
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.evidence-card > strong {
		display: block;
		font-size: var(--fs-xs);
		font-weight: 600;
	}
	.evidence-card > small {
		display: block;
		margin-top: 4px;
		color: var(--text-faint);
		font-size: 10px;
	}
	details {
		margin-top: var(--space-2);
		border-top: 1px solid var(--border);
	}
	details summary {
		padding-top: var(--space-2);
		color: var(--link);
		font-size: 10.5px;
		cursor: pointer;
	}
	.evidence-card pre {
		max-height: 180px;
		margin: var(--space-2) 0 0;
		padding: var(--space-2);
		overflow: auto;
		white-space: pre-wrap;
		word-break: break-word;
		background: var(--bg-raised);
		border-radius: var(--radius-chip);
	}
	.error {
		margin-top: var(--space-2);
		color: var(--err);
		font-size: var(--fs-xs);
	}
	@keyframes inspector-in {
		from { opacity: 0; transform: translateX(10px); }
		to { opacity: 1; transform: translateX(0); }
	}
	@media (max-width: 800px) {
		.inspector {
			position: absolute;
			top: 0;
			right: 0;
			bottom: 0;
			z-index: 5;
			width: min(330px, calc(100vw - 48px));
			box-shadow: var(--shadow-pop);
		}
	}
</style>
