<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[]; count?: number }> = $props();
</script>
<div class="columns presentation-columns" style:--columns={props.count ?? 2}>
	{#each props.children as child}<div class="column presentation-column">{@render renderNode(child)}</div>{/each}
</div>
<style>
	.columns { display: grid; }
</style>
