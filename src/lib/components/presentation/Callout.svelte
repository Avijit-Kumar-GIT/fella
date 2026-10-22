<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props }: ComponentRenderProps<{ title?: string; content: string; tone?: 'neutral' | 'info' | 'positive' | 'caution' }> = $props();
</script>
<aside data-tone={props.tone ?? 'neutral'}>{#if props.title}<strong>{props.title}</strong>{/if}<p>{props.content}</p></aside>
<style>
	aside { padding: .8rem 1rem; border-left: 2px solid var(--border-strong); border-radius: 0 var(--radius, 10px) var(--radius, 10px) 0; background: var(--bg-inset); }
	strong { display: block; margin-bottom: .2rem; color: var(--text); font-size: .84rem; font-weight: 600; } p { margin: 0; color: var(--text-dim); font-size: .81rem; line-height: 1.55; }
	aside[data-tone='info'] { border-color: var(--accent); } aside[data-tone='positive'] { border-color: var(--success, #4e9b76); } aside[data-tone='caution'] { border-color: var(--warning, #b18a37); }
</style>
