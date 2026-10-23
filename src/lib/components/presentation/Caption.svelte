<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props }: BaseComponentProps<{ content: string }> = $props();
</script>
<p class="presentation-caption">{props.content}</p>
<style>p { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.5; overflow-wrap: anywhere; }</style>
