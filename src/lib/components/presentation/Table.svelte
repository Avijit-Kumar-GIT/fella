<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { filteredRows, requireResult } from '../../presentation/results';
	import type { PresentationFormatOptions } from '../../presentation/format';
	import { usePresentation } from './context';
	import ResultTable from './ResultTable.svelte';
	let { props }: BaseComponentProps<{ resultId: string; fields?: string[]; title?: string; columnFormats?: Record<string, PresentationFormatOptions> }> = $props();
	const context = usePresentation();
	let result = $derived(requireResult(context.results, props.resultId));
	let indices = $derived(filteredRows(result, context.state));
</script>
{#if props.title}<h3 class="presentation-table-title">{props.title}</h3>{/if}
<ResultTable {result} fields={props.fields ?? result.columns} {indices} columnFormats={props.columnFormats} />
<style>
	h3 { margin: 0 0 var(--space-2); color: var(--text); font-size: var(--fs-lg); font-weight: 600; line-height: 1.4; overflow-wrap: anywhere; }
</style>
