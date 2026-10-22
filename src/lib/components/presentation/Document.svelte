<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[] }> = $props();
</script>
<div class="analytical-document">{@render renderNode(props.children)}</div>
<style>
	.analytical-document { display: grid; gap: 1.5rem; min-width: 0; color: var(--text); line-height: 1.55; }
</style>
