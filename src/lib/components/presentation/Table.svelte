<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { exactRowFilterValuesFromRow, filteredRows, incompatibleInteractionFilters, requireResult } from '../../presentation/results';
	import type { PresentationFormatOptions } from '../../presentation/format';
	import { usePresentation } from './context';
	import ResultTable from './ResultTable.svelte';
	let { props }: BaseComponentProps<{ resultId: string; fields?: string[]; title?: string; columnFormats?: Record<string, PresentationFormatOptions>; interactionGroup?: string; linkBy?: string[] }> = $props();
	const context = usePresentation();
	let result = $derived(requireResult(context.results, props.resultId));
	let linkBy = $derived(props.linkBy ?? []);
	let indices = $derived(filteredRows(result, context.state, props.interactionGroup, linkBy, context.interactionScope));
	let incompatibleFilters = $derived(incompatibleInteractionFilters(result, context.state, props.interactionGroup, linkBy, context.interactionScope));
</script>
{#if props.title}<h3 class="presentation-table-title">{props.title}</h3>{/if}
{#if incompatibleFilters.length}<p class="filter-limitation" role="status">This output is unchanged by {incompatibleFilters.map(filter => filter.field).join(', ')} because those fields are not available here.</p>{/if}
<ResultTable {result} fields={props.fields ?? result.columns} {indices} columnFormats={props.columnFormats} filterFields={linkBy} onrowfilter={props.interactionGroup && linkBy.length ? row => context.selectRows(result.id, props.interactionGroup, exactRowFilterValuesFromRow(result, row, linkBy)) : undefined} />
<style>
	h3 { margin: 0 0 var(--space-2); color: var(--text); font-size: var(--fs-lg); font-weight: 600; line-height: 1.4; overflow-wrap: anywhere; }
	.filter-limitation { margin: 0 0 var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.5; }
</style>
