<script lang="ts">
	import type { Message } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import EvidenceBlock from './EvidenceBlock.svelte';
	import EvidenceSummary, { answerNeedsReview } from './EvidenceSummary.svelte';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';
	import MarkdownContent from './MarkdownContent.svelte';
	import AnalyticalAnswer from './AnalyticalAnswer.svelte';
	import { enterUp } from '$lib/motion';

	let {
		message,
		expanded = false,
		ontoggle,
		onrerun,
		onopeninvestigationrun,
	}: {
		message: Message;
		expanded?: boolean;
		ontoggle?: () => void;
		onrerun?: () => Promise<void>;
		onopeninvestigationrun?: (turnId: string) => void;
	} = $props();
	let presentationPrimary = $derived(!!message.answer?.workspace && !message.answer?.clarification);
	let needsAttention = $derived(!!message.answer && answerNeedsReview(message.answer)
		&& message.answer.status !== 'failed' && message.answer.status !== 'insufficient_data');

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

	// Keep the useful trust signal close to the finding. This deliberately uses
	// only evidence already returned by the engine; it never invents a row count
	// or claims that a selected source was a hard filter when it was only a
	// starting point for the model.
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
				{#if needsAttention}
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
		{#if split.background && !presentationPrimary}
			<div class="background">{split.background}</div>
		{/if}
		{#if presentationPrimary}
			<AnalyticalAnswer {message} />
		{:else}
			<div class="text rich" class:pending={message.pending}>
				{#if message.answer?.clarification}
					<p class="clarification-transcript-note">Choose an option below to continue.</p>
				{:else}
					<MarkdownContent text={split.body} />
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
			suppressReviewStatus={needsAttention}
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
	.investigation-run-link {
		margin-top: var(--space-1);
		color: var(--text-dim);
	}
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
