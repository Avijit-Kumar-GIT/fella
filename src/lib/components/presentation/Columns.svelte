<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[]; count?: number }> = $props();
</script>
<div class="columns" style:--columns={props.count ?? 2}>
	{#each props.children as child}<div class="column">{@render renderNode(child)}</div>{/each}
</div>
<style>
	.columns { display: grid; grid-template-columns: repeat(var(--columns), minmax(0, 1fr)); gap: 1.4rem; min-width: 0; }
	.column { min-width: 0; } @container (max-width: 550px) { .columns { grid-template-columns: minmax(0, 1fr); } }
</style>
