<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { usePresentation } from './context';
	let { props, children }: BaseComponentProps<{ name: string; index: number }> = $props();
	const context = usePresentation();
	let selected = $derived(Number(context.state[`tab:${props.name}`] ?? 0) === props.index);
</script>
{#if selected}{@render children?.()}{/if}
