<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { filteredRows, requireResult } from '../../presentation/results';
	import { usePresentation } from './context';
	import ResultTable from './ResultTable.svelte';
	let { props }: ComponentRenderProps<{ resultId: string; fields?: string[] }> = $props();
	const context = usePresentation();
	let result = $derived(requireResult(context.results, props.resultId));
	let indices = $derived(filteredRows(result, context.state));
</script>
<ResultTable {result} fields={props.fields ?? result.columns} {indices} />
