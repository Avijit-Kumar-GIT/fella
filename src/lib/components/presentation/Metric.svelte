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
<dl>
	<dt>{props.title}{#if isDerived}<span class="derived-label">Display-derived</span>{/if}</dt>
	<dd title={formatted.formatted ? `Exact value: ${formatted.exact}` : undefined}>
		{displayValue}
		{#if formatted.formatted && formatted.exact !== displayValue}
			<details class="exact-value"><summary>Exact value</summary><code>{formatted.exact}</code></details>
		{/if}
	</dd>
	{#if props.detail}<dd class="detail">{props.detail}</dd>{/if}
</dl>
<style>
	dl { margin: 0; padding: .25rem 0; min-width: 0; } dt { display: flex; align-items: center; gap: .5rem; color: var(--text-dim); font-size: .8rem; margin-bottom: .3rem; }
	dd { margin: 0; font-size: clamp(1.2rem, 4cqi, 1.8rem); line-height: 1.3; font-weight: 580; letter-spacing: -.025em; overflow-wrap: anywhere; font-variant-numeric: tabular-nums; }
	.derived-label { font-size: .65rem; font-weight: 450; letter-spacing: 0; color: var(--text-faint); }
	dd.detail { margin-top: .3rem; color: var(--text-faint); font-size: .74rem; line-height: 1.45; font-weight: 400; letter-spacing: 0; }
	.exact-value { margin-top: .35rem; color: var(--text-faint); font-size: .72rem; }
	.exact-value summary { width: fit-content; cursor: pointer; }
	.exact-value code { display: block; margin-top: .25rem; color: var(--text-dim); font-size: .74rem; overflow-wrap: anywhere; font-variant-numeric: tabular-nums; }
</style>
