<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props, children }: BaseComponentProps<{ title: string }> = $props();
</script>
<section class="presentation-section"><h3>{props.title}</h3><div class="section-content presentation-section-content">{@render children?.()}</div></section>
<style>
	section { min-width: 0; }
	h3 { margin: 0 0 var(--space-2); font-size: var(--fs-xl); font-weight: 620; }
	.section-content { display: grid; gap: var(--space-3); }
</style>
