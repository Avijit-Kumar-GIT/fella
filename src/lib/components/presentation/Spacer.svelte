<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props }: ComponentRenderProps<{ size?: 'sm' | 'md' | 'lg' }> = $props();
</script>
<div class="spacer" data-size={props.size ?? 'md'} aria-hidden="true"></div>
<style>
	.spacer[data-size='sm'] { height: .35rem; } .spacer[data-size='md'] { height: .8rem; } .spacer[data-size='lg'] { height: 1.4rem; }
</style>
