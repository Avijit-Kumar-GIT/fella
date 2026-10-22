<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[]; gap?: number; align?: 'start' | 'center' | 'end' | 'stretch' }> = $props();
</script>
<div class="row" style:--gap={`${(props.gap ?? 3) * 4}px`} style:--align={props.align ?? 'center'}>
	{#each props.children as child}<div class="item">{@render renderNode(child)}</div>{/each}
</div>
<style>
	.row { display: flex; flex-wrap: wrap; align-items: var(--align); gap: var(--gap); min-width: 0; }
	.item { min-width: 0; }
</style>
