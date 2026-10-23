<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { requireResult, exactValue } from '../../presentation/results';
	import { formatPresentationValue } from '../../presentation/format';
	import { usePresentation } from './context';
	let { props }: BaseComponentProps<{ resultId: string; field: string; row: number; title: string; unit?: string; format?: 'number' | 'currency' | 'percent' | 'compact'; detail?: string }> = $props();
	const context = usePresentation();
	let result = $derived(requireResult(context.results, props.resultId));
	let exact = $derived(exactValue(result, props.row, props.field));
	let column = $derived(result.columns.indexOf(props.field));
	let sourceValue = $derived(result.rows[props.row][column] === null ? null : exact);
	let formatOptions = $derived({
		...(props.format ? { format: props.format } : {}),
		...(props.unit ? { unit: props.unit } : {})
	});
	let formatted = $derived(formatPresentationValue(sourceValue, formatOptions));
	let displayValue = $derived(sourceValue === null && !formatted.formatted ? '—' : formatted.display);
</script>
<article class="presentation-metric">
	<dl>
		<dt>{props.title}</dt>
		<dd title={formatted.formatted ? `Exact value: ${formatted.exact}` : undefined}>{displayValue}</dd>
		{#if props.detail}<dd class="detail">{props.detail}</dd>{/if}
	</dl>
	{#if formatted.formatted && formatted.exact !== displayValue}
		<details class="presentation-exact-value"><summary>Exact value</summary><code>{formatted.exact}</code></details>
	{/if}
</article>
