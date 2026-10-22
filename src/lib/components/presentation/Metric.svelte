<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	import { requireResult, exactValue } from '../../presentation/results';
	import { usePresentation } from './context';
	let { props }: ComponentRenderProps<{ resultId?: string | null; field?: string | null; row?: number | null; title: string; unit?: string | null; value?: unknown; format?: 'number' | 'currency' | 'percent' | 'compact'; detail?: string }> = $props();
	const context = usePresentation();
	let isDerived = $derived(props.value !== undefined);
	let value = $derived.by(() => {
		if (!isDerived && props.resultId && props.field && typeof props.row === 'number') return exactValue(requireResult(context.results, props.resultId), props.row, props.field);
		if (props.value === null || props.value === undefined) return '—';
		if (typeof props.value !== 'number' || !Number.isFinite(props.value)) return String(props.value);
		const options: Intl.NumberFormatOptions = { maximumFractionDigits: 2 };
		if (props.format === 'currency' && props.unit && /^[A-Z]{3}$/.test(props.unit)) { options.style = 'currency'; options.currency = props.unit; options.maximumFractionDigits = 2; }
		else if (props.format === 'percent') { options.style = 'percent'; options.maximumFractionDigits = 2; }
		else if (props.format === 'compact') options.notation = 'compact';
		let formatted = new Intl.NumberFormat(undefined, options).format(props.value);
		if (props.unit && !(props.format === 'currency' && /^[A-Z]{3}$/.test(props.unit)) && props.format !== 'percent') formatted += ` ${props.unit}`;
		return formatted;
	});
</script>
<dl><dt>{props.title}{#if isDerived}<span class="derived-label">Display-derived</span>{/if}</dt><dd>{value}</dd>{#if props.detail}<p>{props.detail}</p>{/if}</dl>
<style>
	dl { margin: 0; padding: .25rem 0; min-width: 0; } dt { display: flex; align-items: center; gap: .5rem; color: var(--text-dim); font-size: .8rem; margin-bottom: .3rem; }
	dd { margin: 0; font-size: clamp(1.2rem, 4cqi, 1.8rem); line-height: 1.3; font-weight: 580; letter-spacing: -.025em; overflow-wrap: anywhere; font-variant-numeric: tabular-nums; }
	.derived-label { font-size: .65rem; font-weight: 450; letter-spacing: 0; color: var(--text-faint); }
	p { margin: .3rem 0 0; color: var(--text-faint); font-size: .74rem; line-height: 1.45; }
</style>
