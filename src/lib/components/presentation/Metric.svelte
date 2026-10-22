<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { requireResult, exactValue } from '../../presentation/results';
	import { usePresentation } from './context';
	let { props }: ComponentRenderProps<{ resultId: string; field: string; row: number; title: string; unit?: string }> = $props();
	const context = usePresentation();
	let value = $derived(exactValue(requireResult(context.results, props.resultId), props.row, props.field));
</script>
<dl><dt>{props.title}</dt><dd>{value}{#if props.unit}<span>{props.unit}</span>{/if}</dd></dl>
<style>
	dl { margin: 0; padding: .25rem 0; min-width: 0; } dt { color: var(--text-dim); font-size: .8rem; margin-bottom: .3rem; }
	dd { margin: 0; font-size: clamp(1.2rem, 4cqi, 1.8rem); line-height: 1.3; font-weight: 580; letter-spacing: -.025em; overflow-wrap: anywhere; font-variant-numeric: tabular-nums; }
	dd span { margin-left: .45rem; font-size: .75rem; font-weight: 400; letter-spacing: 0; color: var(--text-dim); }
</style>
