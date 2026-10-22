<script lang="ts">
	import type { ComponentRenderProps } from '@openuidev/svelte-lang';
	let { props }: ComponentRenderProps<{ title: string; value: unknown; detail?: string }> = $props();
	let value = $derived(typeof props.value === 'number' && Number.isFinite(props.value) ? Math.max(0, Math.min(100, props.value)) : 0);
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
