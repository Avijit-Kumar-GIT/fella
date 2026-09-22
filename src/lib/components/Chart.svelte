<script lang="ts">
	import { scaleLinear, scalePoint } from 'd3-scale';
	import { line as d3line, curveMonotoneX } from 'd3-shape';
	import type { VisualizationSpec } from '$lib/types';

	let {
		spec,
		source = '',
		verified = false
	}: { spec: VisualizationSpec; source?: string; verified?: boolean } = $props();

	// Keep chart series distinct from semantic status colors. A label and legend
	// always accompany color, so color never carries the only meaning.
	const CAT_COLORS = ['--brand', '--chart-violet', '--chart-cyan', '--chart-slate', '--chart-plum'];
	function catColor(i: number): string {
		return `var(${CAT_COLORS[i % CAT_COLORS.length]})`;
	}

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

	let chartTitle = $derived(
		spec.title?.trim() || (spec.kind === 'line' ? 'Trend over time' : 'Breakdown')
	);
	let allValues = $derived(spec.series.flatMap((s) => s.values));
	// Include zero so bars have an honest baseline and lines remain easy to
	// read when the result contains only positive or only negative values.
	let dataMin = $derived(allValues.length ? Math.min(0, ...allValues) : 0);
	let dataMax = $derived(allValues.length ? Math.max(0, ...allValues, dataMin + 1) : 1);

	function barMetrics(v: number): { left: number; width: number } {
		const span = dataMax - dataMin || 1;
		const zero = ((0 - dataMin) / span) * 100;
		const val = ((v - dataMin) / span) * 100;
		const left = Math.min(zero, val);
		const width = Math.max(Math.abs(val - zero), 0.75);
		return { left, width };
	}

	// Line charts use real container width and d3's scale math. The y-axis is
	// intentionally rendered here, alongside the exact-value table, so the
	// visual can be understood without hovering.
	let width = $state(480);
	const height = 176;
	const pad = { top: 12, right: 16, bottom: 34, left: 58 };

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
	let yTicks = $derived(y.ticks(4));
	let lineGen = $derived(
		d3line<number>()
			.x((_, i) => x(spec.labels[i]) ?? 0)
			.y((v) => y(v))
			.curve(curveMonotoneX)
	);
	// Thin out x-axis labels once they'd crowd each other, keeping them
	// readable instead of shrinking the whole chart to fit every label.
	let labelStep = $derived(Math.max(1, Math.ceil((spec.labels.length * 46) / width)));

	function pointLabel(series: string, label: string, value: number): string {
		return `${series}, ${label}: ${formatValue(value, spec.unit)}`;
	}
</script>

	<figure class="chart-card">
	<figcaption class="chart-header">
		<div class="chart-header-copy">
			<div class="chart-title">{chartTitle}</div>
			{#if spec.unit}<div class="chart-unit">Values in {spec.unit}</div>{/if}
		</div>
		{#if verified}
			<span class="chart-status"><span class="status-dot" aria-hidden="true"></span>Checked</span>
		{/if}
	</figcaption>
	{#if source}<div class="chart-context">{source}</div>{/if}

	{#if spec.kind === 'bar'}
		<div class="chart" aria-label={chartTitle}>
			{#if spec.series.length > 1}
				<div class="chart-legend">
					{#each spec.series as s, si (s.name)}
						<span class="legend-item"><i class="swatch" style="background:{catColor(si)}"></i>{s.name}</span>
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
								{@const color = catColor(spec.series.length > 1 ? si : i)}
								<div class="track-line">
									<div class="track" aria-hidden="true">
										<div
											class="fill"
											style="left:{m.left}%;width:{m.width}%;background:{color}"
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
		<div class="chart" bind:clientWidth={width} aria-label={chartTitle}>
			{#if spec.series.length > 1}
				<div class="chart-legend">
					{#each spec.series as s, si (s.name)}
						<span class="legend-item"><i class="swatch" style="background:{catColor(si)}"></i>{s.name}</span>
					{/each}
				</div>
			{/if}
			<svg
				viewBox="0 0 {width} {height}"
				preserveAspectRatio="none"
				role="img"
				aria-label={chartTitle}
			>
				<title>{chartTitle}</title>
				{#each yTicks as tick (tick)}
					<line
						x1={pad.left}
						x2={width - pad.right}
						y1={y(tick)}
						y2={y(tick)}
						class="grid-line"
						class:zero-grid={tick === 0}
					/>
					<text x={pad.left - 8} y={y(tick) + 4} class="y-axis-label" text-anchor="end">
						{formatValue(tick, spec.unit)}
					</text>
				{/each}
				<line
					x1={pad.left}
					x2={width - pad.right}
					y1={y(0)}
					y2={y(0)}
					class="zero-axis"
				/>
				{#each spec.series as s, si (s.name)}
					<path
						d={lineGen(s.values) ?? ''}
						class="line"
						class:dashed={si > 0}
						style="stroke:{catColor(si)}"
					/>
					{#each s.values as v, i (i)}
						<circle
							cx={x(spec.labels[i]) ?? 0}
							cy={y(v)}
							r="3"
							class="dot"
							style="fill:{catColor(si)}"
							aria-label={pointLabel(s.name, spec.labels[i], v)}
						>
							<title>{pointLabel(s.name, spec.labels[i], v)}</title>
						</circle>
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

	<details class="values">
		<summary>Show exact values</summary>
		<div class="value-table-wrap">
			<table class="value-table">
				<caption class="sr-only">{chartTitle} values</caption>
				<thead>
					<tr>
						<th scope="col">{spec.kind === 'line' ? 'Label' : 'Category'}</th>
						{#each spec.series as s (s.name)}<th scope="col">{s.name}</th>{/each}
					</tr>
				</thead>
				<tbody>
					{#each spec.labels as label, i (label + i)}
						<tr>
							<th scope="row">{label}</th>
							{#each spec.series as s (s.name)}
								<td>{formatValue(s.values[i], spec.unit)}</td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</details>
</figure>

<style>
	.chart-card {
		margin: var(--space-4) 0;
		padding: var(--space-3) var(--space-4) var(--space-2);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
		font-size: var(--fs-sm);
	}
	.chart-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-3);
	}
	.chart-header-copy {
		min-width: 0;
	}
	.chart-title {
		font-weight: 600;
		color: var(--text);
	}
	.chart-unit,
	.chart-context {
		margin-top: 2px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.chart-context {
		margin-bottom: var(--space-2);
	}
	.chart-status {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		flex: none;
		color: var(--ok);
		font-size: var(--fs-xs);
		white-space: nowrap;
	}
	.status-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: currentColor;
	}
	.chart {
		max-width: 100%;
	}
	.chart-legend {
		display: flex;
		flex-wrap: wrap;
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
		flex: none;
	}

	/* Bar chart: HTML/CSS keeps labels and values aligned at every width. */
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
		min-width: 0;
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
		min-width: 0;
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
		border-radius: var(--radius-sm);
		transition: width var(--dur) var(--ease), left var(--dur) var(--ease);
	}
	.value {
		color: var(--text);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	/* Line chart: grid and y-axis labels make the scale understandable without
		requiring the user to hover a point. */
	svg {
		display: block;
		width: 100%;
		height: auto;
		overflow: visible;
	}
	.grid-line {
		stroke: var(--border);
		stroke-width: 0.75;
	}
	.grid-line.zero-grid {
		stroke: var(--border-strong);
	}
	.zero-axis {
		stroke: var(--border-strong);
		stroke-width: 1;
	}
	.line {
		fill: none;
		stroke-width: 1.8;
	}
	.line.dashed {
		stroke-dasharray: 4 3;
	}
	.dot {
		stroke: var(--bg-raised);
		stroke-width: 1.5;
		outline: none;
		transition: r var(--dur-fast) var(--ease);
	}
	.dot:hover,
	.dot:focus-visible {
		r: 4.5;
	}
	.axis-label,
	.y-axis-label {
		fill: var(--text-dim);
		font-size: var(--fs-xs);
	}

	.values {
		margin-top: var(--space-2);
		font-size: var(--fs-xs);
	}
	.values summary {
		width: fit-content;
		color: var(--text-faint);
		cursor: pointer;
	}
	.values summary:hover {
		color: var(--text-dim);
	}
	.value-table-wrap {
		margin-top: var(--space-2);
		overflow-x: auto;
	}
	.value-table {
		border-collapse: collapse;
		min-width: 100%;
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.value-table th,
	.value-table td {
		padding: 4px 8px;
		border-bottom: 1px solid var(--border);
		text-align: right;
	}
	.value-table th:first-child,
	.value-table td:first-child {
		text-align: left;
	}
	.value-table thead th {
		color: var(--text-dim);
		font-weight: 500;
	}

	@media (max-width: 560px) {
		.chart-card {
			padding-inline: var(--space-3);
		}
		.row {
			grid-template-columns: minmax(0, 38%) 1fr;
			gap: var(--space-2);
		}
	}
</style>
