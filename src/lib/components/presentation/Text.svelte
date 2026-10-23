<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	let { props }: BaseComponentProps<{ content: string }> = $props();
</script>
<p class="presentation-text">{props.content}</p>
<style>p { margin: 0; color: var(--text-dim); font-size: var(--fs); line-height: 1.62; white-space: pre-wrap; overflow-wrap: anywhere; }</style>
