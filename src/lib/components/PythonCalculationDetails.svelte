<script lang="ts">
	import type { EvidenceItem } from '$lib/types';

	let { evidence }: { evidence: EvidenceItem } = $props();

	const code = $derived(
		evidence.tool === 'run_python' && typeof evidence.args?.code === 'string'
			? evidence.args.code
			: undefined
	);

	function forecastCalls(e: EvidenceItem): string[] {
		const args = e.args ?? {};
		const method = typeof args.method === 'string' ? args.method : 'selected method';
		const horizon = typeof args.horizon === 'number' ? args.horizon : 'n';
		const seasonal = typeof args.seasonal_period === 'number' ? `, seasonal_period=${args.seasonal_period}` : '';
		const baseline = typeof args.baseline_method === 'string'
			? args.baseline_method
			: method === 'naive' ? 'mean' : 'naive';
		const calls = [`forecast_series(method="${method}", horizon=${horizon}${seasonal})`];
		if (e.result_summary.includes('rolling-origin comparison attempted')) {
			calls.push(`rolling_origin_backtest(baseline_method="${baseline}")`);
			calls.push('forecast_error_bands(...)');
		} else if (e.result_summary.includes('no chronological holdout available')) {
			calls.push('No chronological holdout available for backtesting.');
		}
		return calls;
	}
</script>

<section class="computation" aria-label="Python calculation">
	<div class="computation-heading">
		<strong>Python</strong>
		<span>{evidence.tool === 'forecast_analysis' ? 'Fella forecast helpers · local sandbox' : 'Generated code · local sandbox'}</span>
	</div>
	{#if code}
		<div class="code-caption">Calculation code</div>
		<pre class="python-code">{code}</pre>
	{:else if evidence.tool === 'forecast_analysis'}
		<div class="python-calls">
			{#each forecastCalls(evidence) as call (call)}<code>{call}</code>{/each}
		</div>
	{/if}
</section>

<style>
	.computation {
		margin: 0 0 9px;
		padding: 8px 10px;
		border-left: 2px solid var(--border-strong);
		background: var(--bg-inset);
	}
	.computation-heading {
		display: flex;
		align-items: baseline;
		gap: 7px;
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.computation-heading strong {
		color: var(--text);
		font-family: var(--mono);
		font-weight: 600;
	}
	.code-caption {
		margin-top: 7px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.python-code {
		max-height: 320px;
		margin: 4px 0 0;
		padding: 8px 10px;
		overflow: auto;
		color: var(--text);
		font-family: var(--mono);
		font-size: var(--fs-xs);
		white-space: pre;
	}
	.python-calls {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		margin-top: 7px;
		color: var(--text-dim);
		font-family: var(--mono);
		font-size: var(--fs-xs);
	}
</style>
