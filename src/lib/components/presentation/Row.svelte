<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[]; gap?: number; align?: 'start' | 'center' | 'end' | 'stretch' }> = $props();
</script>
<div class="row presentation-row" style:--gap={`${Math.min(8, Math.max(0, props.gap ?? 3)) * 4}px`} style:--align={props.align ?? 'center'}>
	{#each props.children as child}<div class="item presentation-row-item" data-kind={child && typeof child === 'object' ? (child as { typeName?: string }).typeName : undefined}>{@render renderNode(child)}</div>{/each}
</div>
<style>
	.row { display: flex; }
	.item { min-width: 0; max-width: 100%; }
</style>
