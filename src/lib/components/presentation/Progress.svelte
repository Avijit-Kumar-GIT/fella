<script lang="ts">
	import type { BaseComponentProps } from '@json-render/svelte';
	import { exactValue, requireResult } from '../../presentation/results';
	import { usePresentation } from './context';
	let { props }: BaseComponentProps<{ resultId: string; field: string; row: number; title: string; maximum?: number; detail?: string }> = $props();
	const context = usePresentation();
	let result = $derived(requireResult(context.results, props.resultId));
	let exact = $derived(exactValue(result, props.row, props.field));
	let raw = $derived(Number(exact));
	let value = $derived(Number.isFinite(raw) ? Math.max(0, Math.min(100, raw / (props.maximum ?? 100) * 100)) : 0);
</script>
<div class="progress"><div class="line"><span>{props.title}</span><strong>{Math.round(value)}%</strong></div><div class="track" role="progressbar" aria-label={props.title} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(value)}><div style:width={`${value}%`}></div></div>{#if props.detail}<p>{props.detail}</p>{/if}</div>
<style>
	.progress { min-width: 0; }
	.line { display: flex; justify-content: space-between; gap: var(--space-3); color: var(--text-dim); font-size: var(--fs-sm); }
	strong { color: var(--text); font-weight: 600; font-variant-numeric: tabular-nums; }
	.track { height: 7px; margin-top: var(--space-2); overflow: hidden; border-radius: 999px; background: var(--bg-inset); }
	.track div { height: 100%; border-radius: inherit; background: var(--brand); transition: width var(--dur) var(--ease); }
	p { margin: var(--space-1) 0 0; color: var(--text-faint); font-size: var(--fs-xs); line-height: 1.5; }
</style>
