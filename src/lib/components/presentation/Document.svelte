<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import './presentation-theme.css';
	let { props, renderNode }: ComponentRenderProps<{ children: unknown[] }> = $props();
</script>
<div class="analytical-document">{@render renderNode(props.children)}</div>
