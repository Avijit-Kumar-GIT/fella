<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { usePresentation } from './context';
	let { props }: ComponentRenderProps<{ label: string; message: string }> = $props();
	const context = usePresentation();
</script>
<button class="presentation-action" disabled={!context.interactive} onclick={() => context.action({ type: 'continue', message: props.message, state: context.state })}>{props.label}<span aria-hidden="true">↗</span></button>
<style>
	button { cursor: pointer; }
	span { color: var(--text-faint); }
</style>
