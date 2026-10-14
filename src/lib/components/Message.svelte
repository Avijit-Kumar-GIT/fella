<script lang="ts">
	import type { EvidenceItem, Message } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import EvidenceBlock from './EvidenceBlock.svelte';
	import EvidenceSummary from './EvidenceSummary.svelte';
	import Chart from './Chart.svelte';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';
	import { renderMarkdown } from '$lib/markdown';
	import { enterUp } from '$lib/motion';
	import { isRenderableChartEvidence } from '$lib/workbench';

	let {
		message,
		expanded = false,
		ontoggle,
		onrerun,
		onopenchart,
		onopeninvestigationrun,
		openChartIndex = null
	}: {
		message: Message;
		expanded?: boolean;
		ontoggle?: () => void;
		onrerun?: () => Promise<void>;
		onopenchart?: (messageId: string, evidenceIndex: number) => void;
		onopeninvestigationrun?: (turnId: string) => void;
		openChartIndex?: number | null;
	} = $props();

	// The model marks a one-line general-knowledge aside with "Background:" on
	// its own line. Peel any leading such lines off so they render muted, apart
	// from the computed answer.
	let split = $derived.by(() => {
		if (message.role !== 'assistant') return { background: '', body: message.text };
		const lines = message.text.split('\n');
		let i = 0;
		while (i < lines.length && lines[i].trimStart().startsWith('Background:')) i++;
		return { background: lines.slice(0, i).join('\n'), body: lines.slice(i).join('\n') };
	});

	// Only the assistant's prose is markdown. When a chart is present, keep the
	// first plain paragraph as the takeaway and place the visual immediately
	// after it; the remaining prose follows as supporting detail. This is a UI
	// composition rule, not a new model-facing markup language.
	function composeAnswer(body: string): { lead: string; remainder: string } {
		const trimmed = body.trim();
		if (!trimmed) return { lead: '', remainder: '' };
		const boundary = trimmed.search(/\n\s*\n/);
		if (boundary < 0) return { lead: trimmed, remainder: '' };
		const lead = trimmed.slice(0, boundary).trim();
		// Keep headings, lists, quotes, and code blocks together. Splitting one of
		// those before the chart would make the answer feel arbitrary.
		if (/^(?:#{1,6}\s|[-*+]\s|>\s|```)/.test(lead)) {
			return { lead: trimmed, remainder: '' };
		}
		return { lead, remainder: trimmed.slice(boundary).trim() };
	}

	let composition = $derived(composeAnswer(split.body));
	let bodyHtml = $derived(renderMarkdown(split.body));
	let leadHtml = $derived(renderMarkdown(composition.lead));
	let remainderHtml = $derived(renderMarkdown(composition.remainder));

	// A chart is selected from evidence, independent of whether the model's text
	// references it. Correctness shouldn't depend on a small model correctly
	// placing a chart mention in freeform text.
	let chartItems = $derived.by(() => {
		const seenVisuals = new Set<string>();
		return (message.answer?.evidence ?? []).filter((e) => {
			if (!isRenderableChartEvidence(e, message.answer?.verification ?? [])) return false;
			const visual = e.chart;
			if (!visual) return false;

			// A repair iteration can leave several evidence entries that render the
			// same visual while using different SQL aliases/source metadata. Keep
			// the first visible instance; all original calls remain in Analysis
			// Details so this presentation cleanup doesn't discard audit history.
			const key = JSON.stringify({
				kind: visual.kind,
				title: visual.title,
				labels: visual.labels,
				series: visual.series,
				unit: visual.unit,
				x_label: visual.x_label,
				y_label: visual.y_label,
				payload: visual.payload
			});
			if (seenVisuals.has(key)) return false;
			seenVisuals.add(key);
			return true;
		});
	});
	function sourceEvidenceForChart(chartEvidence: EvidenceItem): EvidenceItem {
		const sourceId = chartEvidence.chart?.metadata?.source_evidence_id;
		return sourceId
			? message.answer?.evidence.find((item) => item.id === sourceId) ?? chartEvidence
			: chartEvidence;
	}
	let hasVisualAnswer = $derived(chartItems.length > 0 && !message.pending);

	// Keep the useful trust signal close to the finding. This deliberately uses
	// only evidence already returned by the engine; it never invents a row count
	// or claims that a selected source was a hard filter when it was only a
	// starting point for the model.
	let answerSources = $derived.by(() => {
		const names = new Set<string>();
		for (const item of message.answer?.evidence ?? []) {
			if (item.error || item.verifier_disposition?.state === 'excluded') continue;
			for (const source of item.sources ?? []) {
				if (source.source.trim()) names.add(source.source.trim());
			}
		}
		return [...names];
	});
	let scopeLabel = $derived.by(() => {
		if (answerSources.length === 1) return `Based on ${answerSources[0]}`;
		if (answerSources.length > 1) return `Based on ${answerSources.length} sources`;
		return message.answer?.workspace ? 'Based on this workspace' : 'Based on the available evidence';
	});
	let detailsLabel = $derived.by(() => {
		switch (message.answer?.trace?.mode) {
			case 'model_only':
				return 'General answer';
			case 'workspace_inspect':
				return 'Inspection details';
			default:
				return 'How this was analyzed';
		}
	});
	let answerCaveat = $derived.by(() => {
		switch (message.answer?.status) {
			case 'insufficient_data':
				return {
					title: 'Evidence is incomplete',
					body: 'There is not enough accepted evidence to verify this result. The answer remains available, but treat it as incomplete.'
				};
			case 'failed':
				return {
					title: 'Verification failed',
					body: 'A blocking verification check failed. The answer remains available, but it is not verified; review the details before relying on it.'
				};
			default:
				return null;
		}
	});
</script>

<div class="msg {message.role}" data-message-id={message.id} data-analysis-turn-id={message.role === 'assistant' ? message.answer?.turn_id : undefined} transition:enterUp>
	{#if message.role === 'user'}
		<span class="sr-only">You asked: </span>
		<div class="you">
			<div class="you-copy">{message.text}</div>
		</div>
	{:else if message.role === 'assistant'}
		<span class="sr-only">Fella replied: </span>
		<div class="assistant-heading">
			<div class="assistant-identity">
				<Logo size={18} active={message.pending} />
				<strong>Fella</strong>
				{#if message.answer?.status === 'needs_review'}
					<span
						class="review-status"
						role="status"
						aria-label="Answer status"
						data-testid="answer-review-status"
						data-answer-status="needs_review"
					>Needs review</span>
				{/if}
				{#if message.pending}<span class="status">Working through the workspace</span>{/if}
			</div>
			{#if message.answer}
				<EvidenceSummary
					bodyId={`evidence-${message.id}`}
					label={detailsLabel}
					{expanded}
					{ontoggle}
				/>
			{/if}
		</div>
		{#if split.background}
			<div class="background">{split.background}</div>
		{/if}
		{#if hasVisualAnswer}
			<div class="text rich answer-lead">{@html leadHtml}</div>
			<div class="answer-visuals" aria-label="Visual answer">
				{#each chartItems as e, i (e.id ?? `chart-${i}`)}
					{#if e.chart}
						{@const evidenceIndex = message.answer?.evidence.indexOf(e) ?? i}
						{@const sourceEvidence = sourceEvidenceForChart(e)}
						{#if openChartIndex === evidenceIndex}
							<div class="chart-reference" aria-label="Chart open beside conversation">
								<span>{e.chart.title?.trim() || 'Chart'} is open beside the conversation.</span>
								<span>Close the side pane to return it here.</span>
							</div>
						{:else}
							<Chart
								spec={e.chart}
								source={scopeLabel}
								sourceEvidence={sourceEvidence}
								sourceRunId={message.answer?.turn_id}
								findingSnapshot={composition.lead}
								onopen={onopenchart ? () => onopenchart(message.id, evidenceIndex) : undefined}
							/>
						{/if}
					{/if}
				{/each}
			</div>
			{#if composition.remainder}
				<div class="text rich answer-supporting">{@html remainderHtml}</div>
			{/if}
		{:else}
			<div class="text rich" class:pending={message.pending}>
				{#if message.answer?.clarification}
					<p class="clarification-transcript-note">Choose an option below to continue.</p>
				{:else}
					{@html bodyHtml}
				{/if}
				{#if message.pending}<span class="thinking" aria-hidden="true"></span>{/if}
			</div>
		{/if}
	{:else}
		<div class="text">{message.text}</div>
	{/if}
	{#if message.answer && answerCaveat}
		<div
			class="answer-caveat"
			role="status"
			aria-label="Answer limitation"
			data-answer-status={message.answer.status}
		>
			<Icon name="alert" size={14} />
			<div>
				<strong>{answerCaveat.title}</strong>
				<p>{answerCaveat.body}</p>
			</div>
		</div>
	{/if}
	{#if message.answer}
		<EvidenceBlock
			answer={message.answer}
			bodyId={`evidence-${message.id}`}
			{expanded}
			suppressReviewStatus={message.answer.status === 'needs_review'}
			onrerun={message.answer.turn_id ? onrerun : undefined}
		/>
		{#if message.answer.turn_id && onopeninvestigationrun}
			<Button type="button" variant="ghost" size="sm" class="investigation-run-link" onclick={() => onopeninvestigationrun?.(message.answer!.turn_id!)}>
				<Icon name="bookmark" size={14} />Open in Investigation
			</Button>
		{/if}
	{/if}
</div>

<style>
	.msg {
		padding: var(--space-3) 0;
	}
	.chart-reference {
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: var(--space-3);
		border-left: 2px solid var(--brand);
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.investigation-run-link {
		margin-top: var(--space-1);
		color: var(--text-dim);
	}
	.chart-reference span:first-child { color: var(--text); font-weight: 600; }
	.msg.user {
		display: flex;
		justify-content: flex-end;
		padding: var(--space-4) 0 var(--space-2);
	}
	.you {
		max-width: min(72%, 58ch);
		padding: 9px 14px 10px;
		border-radius: var(--radius-conversation) var(--radius-conversation) var(--radius-sm) var(--radius-conversation);
		background: var(--user-bubble);
		color: var(--user-bubble-text);
		font-size: var(--fs);
		line-height: 1.48;
		text-align: left;
	}
	.you-copy {
		white-space: pre-wrap;
		word-break: break-word;
	}
	.assistant-heading {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
		color: var(--chat-meta);
		font-size: var(--fs-sm);
	}
	.assistant-identity {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-width: 0;
	}
	.assistant-heading strong {
		color: var(--chat-meta);
		font-weight: 600;
	}
	.assistant-heading .status {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.review-status {
		display: inline-flex;
		align-items: center;
		padding: 2px 7px;
		border: 1px solid var(--border);
		border-radius: var(--radius-chip);
		background: var(--bg-inset);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 500;
		line-height: 1.35;
		white-space: nowrap;
	}
	.text {
		word-break: break-word;
	}
	.msg.assistant .text {
		font-size: var(--fs-lg);
		line-height: 1.6;
		color: var(--chat-body);
	}
	/* Markdown emphasis is deliberately brighter, not dramatically heavier.
	   The color step makes a finding legible in both themes without turning
	   every bold phrase into an accent-colored badge. */
	.msg.assistant .text :global(strong) {
		color: var(--chat-strong);
		font-weight: 650;
	}
	/* A "Background:" aside general knowledge, not computed from the files. */
	.background {
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--chat-meta);
		font-size: 0.95em;
		margin-bottom: 4px;
	}
	/* System notes (catalog dumps, /sql output) are raw text, not markdown
	   keep the newlines they were written with, and stay mono since they're
	   pre-aligned. */
	.msg.system .text {
		white-space: pre-wrap;
		/* a long path or URL in a system line must not widen the transcript
		   (the body can't scroll sideways to recover it). */
		overflow-wrap: anywhere;
		font-family: var(--mono);
		font-size: var(--fs-sm);
		color: var(--text-dim);
	}
	.text.pending {
		color: var(--text-dim);
	}
	.answer-visuals {
		margin-top: var(--space-3);
	}
	.answer-caveat {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		margin: var(--space-3) 0;
		padding: var(--space-2) var(--space-3);
		border-left: 2px solid var(--warn);
		background: var(--bg-inset);
		color: var(--warn);
	}
	.answer-caveat :global(svg) { flex: none; margin-top: 2px; }
	.answer-caveat strong { color: var(--text); font-size: var(--fs-sm); font-weight: 600; }
	.answer-caveat p { margin: 2px 0 0; color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.45; }
	.answer-visuals :global(.chart-card) {
		margin-top: 0;
	}
	.answer-supporting {
		margin-top: var(--space-3);
	}
	.clarification-transcript-note {
		margin: 0;
		color: var(--chat-meta);
		font-size: var(--fs-sm);
		font-style: italic;
	}

	/* The assistant's answer is rendered from markdown (see markdown.ts). Code,
	   pre and tables come from the shared `.rich` rules in app.css; these are
	   the prose-only bits. */
	.msg.assistant .text :global(p) {
		margin: 0 0 0.5em;
	}
	.msg.assistant .text :global(p:last-child) {
		margin-bottom: 0;
	}
	.msg.assistant .text :global(ul),
	.msg.assistant .text :global(ol) {
		margin: 0.25em 0;
		padding-left: 1.4em;
	}
	.msg.assistant .text :global(h1),
	.msg.assistant .text :global(h2),
	.msg.assistant .text :global(h3),
	.msg.assistant .text :global(h4) {
		font-size: 1.05em;
		color: var(--chat-strong);
		font-weight: 650;
		margin: 0.6em 0 0.25em;
	}
	.msg.assistant .text :global(blockquote) {
		margin: 0.4em 0;
		padding-left: 0.6em;
		border-left: 2px solid var(--border-strong);
		color: var(--chat-meta);
	}
	/* markdown tables can't be wrapped in a scroll container (they come from
	   @html), so let the table itself scroll. */
	.msg.assistant .text :global(table) {
		display: block;
		overflow-x: auto;
		margin: 4px 0;
	}
	/* the shared .thinking dots sit on the baseline after streamed text */
	.text .thinking {
		margin-left: 10px;
		color: var(--text-faint);
	}
</style>
