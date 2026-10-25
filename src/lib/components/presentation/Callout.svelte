<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props }: BaseComponentProps<{ title?: string; content: string; tone?: 'neutral' | 'info' | 'positive' | 'caution' }> = $props();
</script>
<aside class="presentation-callout" data-tone={props.tone ?? 'neutral'}>{#if props.title}<strong>{props.title}</strong>{/if}<p>{props.content}</p></aside>
<style>
	aside { padding: var(--space-3) var(--space-4); border: 1px solid var(--border); border-left: 3px solid var(--border-strong); border-radius: var(--radius-window); background: var(--bg-inset); }
	strong { display: block; margin-bottom: var(--space-1); color: var(--text); font-size: var(--fs-sm); font-weight: 620; }
	p { margin: 0; color: var(--text-dim); font-size: var(--fs-sm); line-height: 1.55; }
	aside[data-tone='info'] { border-left-color: var(--brand); }
	aside[data-tone='positive'] { border-left-color: var(--ok); }
	aside[data-tone='caution'] { border-left-color: var(--warn); }
</style>
