<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props }: ComponentRenderProps<{ content: string; level?: number }> = $props();
</script>
{#if (props.level ?? 1) === 1}<h2>{props.content}</h2>{:else if props.level === 2}<h3>{props.content}</h3>{:else if props.level === 3}<h4>{props.content}</h4>{:else}<h5>{props.content}</h5>{/if}
<style>
	h2,h3,h4,h5 { margin: 0; color: var(--text); line-height: 1.25; letter-spacing: -.025em; } h2 { font-size: clamp(1.2rem, 3cqi, 1.65rem); font-weight: 650; } h3 { font-size: 1.1rem; font-weight: 620; } h4 { font-size: 1rem; font-weight: 600; } h5 { font-size: .9rem; font-weight: 600; }
</style>
