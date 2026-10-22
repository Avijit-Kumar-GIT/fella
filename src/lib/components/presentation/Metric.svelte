<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { requireResult, exactValue } from '../../presentation/results';
	import { formatPresentationValue } from '../../presentation/format';
	import { usePresentation } from './context';
	let { props }: ComponentRenderProps<{ resultId?: string | null; field?: string | null; row?: number | null; title: string; unit?: string | null; value?: unknown; format?: 'number' | 'currency' | 'percent' | 'compact'; detail?: string }> = $props();
	const context = usePresentation();
	let isDerived = $derived(props.value !== undefined);
	let isBound = $derived(!isDerived && Boolean(props.resultId && props.field && typeof props.row === 'number'));
	let sourceValue = $derived.by(() => {
		if (isBound && props.resultId && props.field && typeof props.row === 'number') {
			const result = requireResult(context.results, props.resultId);
			const exact = exactValue(result, props.row, props.field);
			const column = result.columns.indexOf(props.field);
			return result.rows[props.row][column] === null ? null : exact;
		}
		return props.value;
	});
	let formatOptions = $derived({
		...(props.format ? { format: props.format } : {}),
		...(props.unit ? { unit: props.unit } : {})
	});
	let formatted = $derived(formatPresentationValue(sourceValue, formatOptions));
	let displayValue = $derived(
		sourceValue === undefined && !isBound || isDerived && sourceValue === null && !formatted.formatted
			? '—'
			: formatted.display
	);
</script>
<article class="presentation-metric">
	<dl>
		<dt>{props.title}{#if isDerived}<span class="derived-label">Display-derived</span>{/if}</dt>
		<dd title={formatted.formatted ? `Exact value: ${formatted.exact}` : undefined}>{displayValue}</dd>
		{#if props.detail}<dd class="detail">{props.detail}</dd>{/if}
	</dl>
	{#if formatted.formatted && formatted.exact !== displayValue}
		<details class="presentation-exact-value"><summary>Exact value</summary><code>{formatted.exact}</code></details>
	{/if}
</article>
<style>
	.derived-label { color: var(--text-faint); font-size: var(--fs-xs); font-weight: 450; letter-spacing: 0; }
</style>
