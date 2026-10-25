<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props, children }: BaseComponentProps<{ gap?: number; align?: 'start' | 'center' | 'end' | 'stretch' }> = $props();
</script>
<div class="row presentation-row" style:--gap={`${Math.min(8, Math.max(0, props.gap ?? 3)) * 4}px`} style:--align={props.align ?? 'center'}>
	{@render children?.()}
</div>
<style>
	.row { display: flex; }
	.row :global(> *) { min-width: 0; max-width: 100%; }
</style>
