<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[]; gap?: number; align?: 'start' | 'center' | 'end' | 'stretch' }> = $props();
</script>
<div class="row" style:--gap={`${Math.min(8, Math.max(0, props.gap ?? 3)) * 4}px`} style:--align={props.align ?? 'center'}>
	{#each props.children as child}<div class="item" data-kind={child && typeof child === 'object' ? (child as { typeName?: string }).typeName : undefined}>{@render renderNode(child)}</div>{/each}
</div>
<style>
	.row { display: flex; flex-wrap: wrap; align-items: var(--align); gap: var(--gap); min-width: 0; }
	.item { min-width: 0; }
	.item[data-kind='Chart'], .item[data-kind='Table'], .item[data-kind='Card'], .item[data-kind='Section'], .item[data-kind='Columns'], .item[data-kind='Grid'], .item[data-kind='Tabs'] { flex: 1 1 15rem; }
</style>
