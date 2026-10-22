<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props }: ComponentRenderProps<{ size?: 'sm' | 'md' | 'lg' }> = $props();
</script>
<div class="spacer" data-size={props.size ?? 'md'} aria-hidden="true"></div>
<style>
	.spacer[data-size='sm'] { height: var(--space-1); }
	.spacer[data-size='md'] { height: var(--space-2); }
	.spacer[data-size='lg'] { height: var(--space-3); }
</style>
