<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { usePresentation } from './context';
	let { props }: ComponentRenderProps<{ label: string; action?: unknown }> = $props();
	const context = usePresentation();
</script>
<button type="button" onclick={() => context.runAction(props.action)}>{props.label}<span aria-hidden="true">↗</span></button>
<style>
	button { display: inline-flex; align-items: center; gap: .5rem; min-height: 2.1rem; padding: .4rem .8rem; border: 1px solid var(--border); border-radius: 999px; color: var(--text); background: var(--bg-raised); font: inherit; font-size: .8rem; cursor: pointer; }
	button:hover { background: var(--bg-inset); } button:focus-visible { outline: 2px solid color-mix(in srgb, var(--accent) 60%, transparent); outline-offset: 2px; } span { color: var(--text-faint); }
</style>
