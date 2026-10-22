<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props }: ComponentRenderProps<{ content: string }> = $props();
</script>
<p>{props.content}</p>
<style>p { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--text-dim); line-height: 1.65; }</style>
