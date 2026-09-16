<script lang="ts">
	import { scaleLinear, scalePoint } from 'd3-scale';
	import { line as d3line, curveMonotoneX } from 'd3-shape';
	import type { ChartSpec } from '$lib/types';

	let { spec }: { spec: ChartSpec } = $props();

	// A categorical palette built from colors the theme already defines
	// (brand/link/ok/warn/err) rather than inventing new hues -- every one of
	// these already has a light and a dark variant in app.css, so a chart
	// never needs its own theme-awareness. Used per-category on a single
	// series (where color is the only thing distinguishing one bar from the
	// next) and per-series on a multi-series chart; color is always paired
	// with the row/legend label right next to it, never carrying meaning on
	// its own, so this stays inside the house "no color alone" rule even as
	// a deliberate exception to "monochrome first" -- a chart is a different
	// kind of surface than prose or chrome.
	const CAT_COLORS = ['--brand', '--link', '--ok', '--warn', '--err'];
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
					<span class="legend-item"><i class="swatch" style="background:{catColor(si)}"
						></i>{s.name}</span>
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
								<div class="track">
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
	<div class="chart" bind:clientWidth={width}>
		{#if spec.title}<div class="chart-title">{spec.title}</div>{/if}
		{#if spec.series.length > 1}
			<div class="chart-legend">
				{#each spec.series as s, si (s.name)}
					<span class="legend-item"><i class="swatch" style="background:{catColor(si)}"
						></i>{s.name}</span>
				{/each}
			</div>
		{/if}
		<svg
			viewBox="0 0 {width} {height}"
			preserveAspectRatio="none"
			role="img"
			aria-label={spec.title ?? 'Chart'}
		>
			<line
				x1={pad.left}
				x2={width - pad.right}
				y1={y(0)}
				y2={y(0)}
				class="axis"
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
						r="2.4"
						class="dot"
						style="fill:{catColor(si)}"
					/>
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
			<caption class="sr-only">{spec.title ?? 'Chart values'}</caption>
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
		flex: none;
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
		border-radius: var(--radius-sm);
		transition: width var(--dur) var(--ease), left var(--dur) var(--ease);
	}
	.value {
		color: var(--text);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.values {
		margin-top: var(--space-2);
		font-size: var(--fs-xs);
	}
	.values summary {
		color: var(--text-faint);
		cursor: pointer;
		width: fit-content;
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
		stroke-width: 1.6;
	}
	/* a second, non-color cue for the second series -- distinguishable even
	   for a colorblind reader or in a black/white screenshot. */
	.line.dashed {
		stroke-dasharray: 4 3;
	}
	.axis-label {
		fill: var(--text-dim);
		font-size: var(--fs-xs);
	}
</style>
