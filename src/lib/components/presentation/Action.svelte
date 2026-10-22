<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { usePresentation } from './context';
	let { props }: ComponentRenderProps<{ label: string; message: string }> = $props();
	const context = usePresentation();
</script>
<button disabled={!context.interactive} onclick={() => context.action({ type: 'continue', message: props.message, state: context.state })}>{props.label}<span aria-hidden="true">↗</span></button>
<style>
	button { display: inline-flex; align-items: center; justify-content: center; gap: .6rem; width: fit-content; padding: .45rem .85rem; border: 1px solid var(--border-strong); border-radius: 999px; color: var(--text); background: var(--bg-raised); cursor: pointer; font-size: .8rem; }
	button:hover:not(:disabled) { background: var(--bg-inset); } button:disabled { opacity: .5; cursor: default; }
</style>
