<script lang="ts">
	import { scaleLinear, scalePoint } from 'd3-scale';
	import { line as d3line, curveMonotoneX } from 'd3-shape';
	import type { ChartSpec } from '$lib/types';

	let { spec }: { spec: ChartSpec } = $props();

	function formatValue(v: number, unit?: string): string {
		const abs = Math.abs(v);
		const body = Number.isInteger(abs)
			? abs.toLocaleString()
			: abs.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
		const sign = v < 0 ? '-' : '';
		if (!unit) return sign + body;
		return unit === '$' || unit === '€' || unit === '£'
			? `${sign}${unit}${body}`
			: `${sign}${body}${unit}`;
	}

	// Zero-inclusive range across every series, matching how a bar/line chart
	// reads (a chart of only-positive values still baselines at zero).
	let allValues = $derived(spec.series.flatMap((s) => s.values));
	let dataMin = $derived(Math.min(0, ...allValues));
	let dataMax = $derived(Math.max(0, ...allValues, dataMin + 1));

	function barMetrics(v: number): { left: number; width: number } {
		const span = dataMax - dataMin || 1;
		const zero = ((0 - dataMin) / span) * 100;
		const val = ((v - dataMin) / span) * 100;
		const left = Math.min(zero, val);
		const width = Math.max(Math.abs(val - zero), 0.75);
		return { left, width };
	}

	// --- line chart: real container width via bind:clientWidth, d3-scale for
	// the axis math, d3-shape for the path -- no more guessing character
	// widths into a hand-built viewBox.
	let width = $state(480);
	const height = 148;
	let pad = $derived({ top: spec.title ? 28 : 12, right: 14, bottom: 26, left: 14 });

	let x = $derived(
		scalePoint<string>()
			.domain(spec.labels)
			.range([pad.left, Math.max(pad.left + 1, width - pad.right)])
			.padding(0.5)
	);
	let y = $derived(
		scaleLinear()
			.domain([dataMin, dataMax])
			.nice()
			.range([height - pad.bottom, pad.top])
	);
	let lineGen = $derived(
		d3line<number>()
			.x((_, i) => x(spec.labels[i]) ?? 0)
			.y((v) => y(v))
			.curve(curveMonotoneX)
	);
	// Thin out x-axis labels once they'd crowd each other -- keeps every
	// label legible instead of shrinking text to fit.
	let labelStep = $derived(Math.max(1, Math.ceil((spec.labels.length * 46) / width)));
</script>

{#if spec.kind === 'bar'}
	<div class="chart">
		{#if spec.title}<div class="chart-title">{spec.title}</div>{/if}
		{#if spec.series.length > 1}
			<div class="chart-legend">
				{#each spec.series as s, si (s.name)}
					<span class="legend-item"><i class="swatch" class:b={si === 1}></i>{s.name}</span>
				{/each}
			</div>
		{/if}
		<div class="rows">
			{#each spec.labels as label, i (label + i)}
				<div class="row">
					<span class="row-label" title={label}>{label}</span>
					<div class="row-tracks">
						{#each spec.series as s, si (s.name)}
							{@const m = barMetrics(s.values[i])}
							<div class="track-line">
								<div class="track">
									<div
										class="fill"
										class:b={si === 1}
										style="left:{m.left}%;width:{m.width}%"
									></div>
								</div>
								<span class="value">{formatValue(s.values[i], spec.unit)}</span>
							</div>
						{/each}
					</div>
				</div>
			{/each}
		</div>
	</div>
{:else}
	<div class="chart" bind:clientWidth={width}>
		{#if spec.title}<div class="chart-title">{spec.title}</div>{/if}
		{#if spec.series.length > 1}
			<div class="chart-legend">
				{#each spec.series as s, si (s.name)}
					<span class="legend-item"><i class="swatch" class:b={si === 1}></i>{s.name}</span>
				{/each}
			</div>
		{/if}
		<svg viewBox="0 0 {width} {height}" preserveAspectRatio="none">
			<line
				x1={pad.left}
				x2={width - pad.right}
				y1={y(0)}
				y2={y(0)}
				class="axis"
			/>
			{#each spec.series as s, si (s.name)}
				<path d={lineGen(s.values) ?? ''} class="line" class:b={si === 1} />
				{#each s.values as v, i (i)}
					<circle cx={x(spec.labels[i]) ?? 0} cy={y(v)} r="2.4" class="dot" class:b={si === 1} />
				{/each}
			{/each}
			{#each spec.labels as label, i (label + i)}
				{#if i % labelStep === 0}
					<text x={x(label) ?? 0} y={height - 8} class="axis-label" text-anchor="middle">
						{label}
					</text>
				{/if}
			{/each}
		</svg>
	</div>
{/if}

<style>
	.chart {
		margin: var(--space-2) 0;
		max-width: 100%;
		font-size: var(--fs-sm);
	}
	.chart-title {
		font-weight: 600;
		color: var(--text);
		margin-bottom: var(--space-2);
	}
	.chart-legend {
		display: flex;
		gap: var(--space-3);
		margin-bottom: var(--space-2);
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.legend-item {
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}
	.swatch {
		width: 7px;
		height: 7px;
		border-radius: 2px;
		background: var(--border-strong);
		flex: none;
	}
	.swatch.b {
		background: var(--text-dim);
	}

	/* --- bar chart: plain HTML/CSS, real layout -- no SVG, no estimated
	   character widths. Each row is label | tracks | (values live inside
	   the track column so multi-series rows still line up). */
	.rows {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}
	.row {
		display: grid;
		grid-template-columns: minmax(0, 30%) 1fr;
		align-items: center;
		gap: var(--space-3);
	}
	.row-label {
		color: var(--text-dim);
		text-align: right;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.row-tracks {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.track-line {
		display: grid;
		grid-template-columns: 1fr max-content;
		align-items: center;
		gap: var(--space-2);
	}
	.track {
		position: relative;
		height: 12px;
		background: var(--bg-inset);
		border-radius: var(--radius-sm);
		overflow: hidden;
	}
	.fill {
		position: absolute;
		top: 0;
		bottom: 0;
		background: var(--border-strong);
		border-radius: var(--radius-sm);
		transition: width var(--dur) var(--ease), left var(--dur) var(--ease);
	}
	.fill.b {
		background: var(--text-dim);
	}
	.value {
		color: var(--text);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	/* --- line chart: SVG hand-written here (trusted Svelte markup, not
	   {@html}), theme colors from CSS as usual. */
	svg {
		display: block;
		width: 100%;
		height: auto;
		overflow: visible;
	}
	.axis {
		stroke: var(--border);
	}
	.line {
		fill: none;
		stroke: var(--text);
		stroke-width: 1.6;
	}
	.line.b {
		stroke: var(--text-dim);
		stroke-dasharray: 4 3;
	}
	.dot {
		fill: var(--text);
	}
	.dot.b {
		fill: var(--text-dim);
	}
	.axis-label {
		fill: var(--text-dim);
		font-size: var(--fs-xs);
	}
</style>
