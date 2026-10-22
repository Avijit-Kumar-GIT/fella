<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[]; columns?: number }> = $props();
</script>
<div class="grid presentation-grid" style:--columns={props.columns ?? 2}>{#each props.children as child}<div>{@render renderNode(child)}</div>{/each}</div>
<style>
	.grid { display: grid; }
	.grid > div { min-width: 0; }
</style>
