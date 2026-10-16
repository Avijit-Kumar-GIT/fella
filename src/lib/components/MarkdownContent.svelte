<script lang="ts">
	import { renderMarkdown } from '$lib/markdown';
	let { text }: { text: string } = $props();
</script>

<div class="answer-prose rich">{@html renderMarkdown(text)}</div>

<style>
	.answer-prose { min-width: 0; line-height: 1.6; overflow-wrap: anywhere; }
	.answer-prose :global(p) { margin: 0 0 0.5em; }
	.answer-prose :global(p:last-child) { margin-bottom: 0; }
	.answer-prose :global(ul), .answer-prose :global(ol) { margin: 0.25em 0; padding-left: 1.4em; }
	.answer-prose :global(h1), .answer-prose :global(h2), .answer-prose :global(h3), .answer-prose :global(h4) { margin: 0.6em 0 0.25em; font-size: 1.05em; font-weight: 650; color: var(--text); }
	.answer-prose :global(blockquote) { margin: 0.4em 0; padding-left: 0.6em; border-left: 2px solid var(--border-strong); color: var(--text-dim); }
	.answer-prose :global(table) { display: block; max-width: 100%; overflow-x: auto; margin: 4px 0; }
	.answer-prose :global(img) { max-width: 100%; height: auto; }
</style>
