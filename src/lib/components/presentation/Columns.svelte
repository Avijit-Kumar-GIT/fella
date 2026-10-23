<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props, children }: BaseComponentProps<{ count?: number }> = $props();
</script>
<div class="columns presentation-columns" style:--columns={props.count ?? 2}>
	{@render children?.()}
</div>
<style>
	.columns { display: grid; }
</style>
