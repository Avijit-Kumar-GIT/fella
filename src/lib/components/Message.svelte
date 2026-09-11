<script lang="ts">
	import type { Message } from '$lib/types';
	import EvidenceBlock from './EvidenceBlock.svelte';
	import Chart from './Chart.svelte';
	import Icon from './Icon.svelte';
	import { renderMarkdown } from '$lib/markdown';
	import { enterUp } from '$lib/motion';
	import { hardFail } from '$lib/verify';

	let { message, expanded = false, ontoggle }: {
		message: Message;
		expanded?: boolean;
		ontoggle?: () => void;
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

	// Only the assistant's prose is markdown the model is asked to structure
	// its final answer, and rendering it lets that structure actually show.
	// User input and system/`/sql` dumps stay plain text (see below).
	let bodyHtml = $derived(renderMarkdown(split.body));

	// Set when a query behind the answer still disagrees after the agent's
	// one-shot corrective re-ask the trust gap the verification system
	// exists to close, surfaced at the point the user actually reads it.
	let unconfirmed = $derived(
		message.role === 'assistant' && message.answer
			? hardFail(message.answer.verification)
			: undefined
	);

	// A chart renders itself below the prose, independent of whether the
	// model's text references it correctness shouldn't depend on a small
	// model correctly placing a chart mention in freeform text.
	let chartItems = $derived(
		(message.answer?.evidence ?? []).filter((e) => e.tool === 'make_chart' && e.chart)
	);
</script>

<div class="msg {message.role}" transition:enterUp>
	{#if message.role === 'user'}
		<span class="sr-only">You asked: </span>
		<div class="you">{message.text}</div>
	{:else if message.role === 'assistant'}
		<span class="sr-only">Fella replied: </span>
		{#if message.plan}
			<div class="plan">{message.plan}</div>
		{/if}
		{#if split.background}
			<div class="background">{split.background}</div>
		{/if}
		{#if unconfirmed}
			<div class="unconfirmed">
				<Icon name="alert" size={13} />
				<span>Fella couldn't confirm this figure against the data — here's its best answer.</span>
			</div>
		{/if}
		<div class="text rich" class:pending={message.pending}>{@html bodyHtml}{#if message.pending}<span
					class="thinking" aria-hidden="true"></span
				>{/if}</div>
		{#each chartItems as e, i (i)}
			{#if e.chart}<Chart spec={e.chart} />{/if}
		{/each}
	{:else}
		<div class="text">{message.text}</div>
	{/if}
	{#if message.answer}
		<EvidenceBlock answer={message.answer} {expanded} {ontoggle} />
	{/if}
</div>

<style>
	.msg {
		padding: var(--space-3) 0;
	}
	.msg.user {
		padding-top: var(--space-4);
	}
	.you {
		color: var(--text-dim);
		white-space: pre-wrap;
		word-break: break-word;
	}
	/* The user's turn is marked like a shell prompt; Fella's reply is unprefixed. */
	.you::before {
		content: '❯ ';
		font-family: var(--mono);
		color: var(--text-dim);
	}
	.text {
		word-break: break-word;
	}
	/* The model's one-line plan, shown dimmed while its tools run. */
	.plan {
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--text-faint);
		font-size: 0.95em;
		margin-bottom: 4px;
		font-style: italic;
	}
	/* A "Background:" aside general knowledge, not computed from the files. */
	.background {
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--text-faint);
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
	/* A hard-failed answer (verify's re-ask still disagreed) reads distinctly
	   from a clean one, at the point the user actually reads it. */
	.unconfirmed {
		display: flex;
		align-items: baseline;
		gap: var(--space-2);
		color: var(--warn);
		font-size: var(--fs-sm);
		margin-bottom: 4px;
	}
	.unconfirmed :global(svg) {
		align-self: center;
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
		font-weight: 600;
		margin: 0.6em 0 0.25em;
	}
	.msg.assistant .text :global(blockquote) {
		margin: 0.4em 0;
		padding-left: 0.6em;
		border-left: 2px solid var(--border-strong);
		color: var(--text-dim);
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
