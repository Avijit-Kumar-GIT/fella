<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[]; columns?: number }> = $props();
</script>
<div class="grid" style:--columns={props.columns ?? 2}>{#each props.children as child}<div>{@render renderNode(child)}</div>{/each}</div>
<style>
	.grid { display: grid; grid-template-columns: repeat(var(--columns), minmax(0, 1fr)); gap: 1rem; min-width: 0; }
	.grid > div { min-width: 0; }
	@container (max-width: 620px) { .grid { grid-template-columns: repeat(min(var(--columns), 2), minmax(0, 1fr)); } }
	@container (max-width: 420px) { .grid { grid-template-columns: minmax(0, 1fr); } }
</style>
