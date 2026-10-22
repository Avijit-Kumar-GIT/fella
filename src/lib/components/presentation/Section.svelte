<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props, renderNode }: ComponentRenderProps<{ title: string; children: unknown[] }> = $props();
</script>
<section><h3>{props.title}</h3><div class="section-content">{@render renderNode(props.children)}</div></section>
<style>
	section { min-width: 0; } h3 { margin: 0 0 .8rem; font-size: 1rem; font-weight: 600; line-height: 1.4; letter-spacing: -.015em; }
	.section-content { display: grid; gap: 1.1rem; min-width: 0; }
</style>
