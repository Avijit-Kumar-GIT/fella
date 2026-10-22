<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { filteredRows, requireResult } from '../../presentation/results';
	import type { PresentationFormatOptions } from '../../presentation/format';
	import { usePresentation } from './context';
	import ResultTable from './ResultTable.svelte';
	import DerivedTable from './DerivedTable.svelte';
	let { props }: ComponentRenderProps<{ resultId: string; fields?: string[]; rows?: unknown[]; title?: string; columnFormats?: Record<string, PresentationFormatOptions> }> = $props();
	const context = usePresentation();
	let result = $derived(requireResult(context.results, props.resultId));
	let indices = $derived(filteredRows(result, context.state));
</script>
{#if props.title}<h3>{props.title}</h3>{/if}
{#if props.rows !== undefined}
	<DerivedTable source={result} rows={props.rows} fields={props.fields} columnFormats={props.columnFormats} />
{:else}
	<ResultTable {result} fields={props.fields ?? result.columns} {indices} columnFormats={props.columnFormats} />
{/if}
<style>h3 { margin: 0 0 .5rem; font-size: .88rem; font-weight: 550; }</style>
