<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { usePresentation } from './context';
	let { props }: ComponentRenderProps<{ label: string; action?: unknown }> = $props();
	const context = usePresentation();
</script>
<button class="presentation-button" type="button" onclick={() => context.runAction(props.action)}>{props.label}<span aria-hidden="true">↗</span></button>
<style>
	button { font: inherit; cursor: pointer; }
	span { color: var(--text-faint); }
</style>
