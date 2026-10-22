<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props }: ComponentRenderProps<{ content: string }> = $props();
</script>
<p>{props.content}</p>
<style>p { margin: 0; color: var(--text-faint); font-size: .76rem; line-height: 1.5; }</style>
