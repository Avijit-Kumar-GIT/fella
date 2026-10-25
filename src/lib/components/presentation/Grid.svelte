<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props, children }: BaseComponentProps<{ columns?: number }> = $props();
</script>
<div class="grid presentation-grid" style:--columns={props.columns ?? 2}>{@render children?.()}</div>
<style>
	.grid { display: grid; }
	.grid > :global(*) { min-width: 0; }
</style>
