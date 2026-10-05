<script lang="ts">
	import { scaleLinear, scalePoint } from 'd3-scale';
	import {
		arc,
		area,
		curveMonotoneX,
		line,
		pie,
		type PieArcDatum
	} from 'd3-shape';
	import type { ChartMetadata, ChartPayload, VisualizationSpec } from '$lib/types';

	let {
		spec,
		source = ''
	}: { spec: VisualizationSpec; source?: string } = $props();

	// Chart colors are deliberately separate from verification/status colors.
	// Every mark also gets a label or a data-table equivalent.
	const CATEGORY_COLORS = [
		'--brand',
		'--chart-violet',
		'--chart-cyan',
		'--chart-slate',
		'--chart-plum',
		'--chart-orange',
		'--chart-green',
		'--chart-gold'
	];
	const WIDTH = 520;
	const HEIGHT = 240;
	const PLOT = { top: 16, right: 18, bottom: 42, left: 66 };

	function color(index: number): string {
		return `var(${CATEGORY_COLORS[index % CATEGORY_COLORS.length]})`;
	}

	function formatValue(value: number, unit?: string): string {
		const abs = Math.abs(value);
		const body = Number.isInteger(abs)
			? abs.toLocaleString()
			: abs.toLocaleString(undefined, { maximumFractionDigits: 2 });
		const sign = value < 0 ? '-' : '';
		if (!unit) return sign + body;
		return unit === '$' || unit === '€' || unit === '£'
			? `${sign}${unit}${body}`
			: `${sign}${body}${unit}`;
	}

	function formatCell(value: string | number | null): string {
		return value === null ? '—' : typeof value === 'number' ? formatValue(value, spec.unit) : value;
	}

	function isNumber(value: number | null | undefined): value is number {
		return typeof value === 'number' && Number.isFinite(value);
	}

	let chartTitle = $derived(
		spec.title?.trim() || (spec.kind === 'line' || spec.kind === 'forecast' ? 'Trend over time' : 'Breakdown')
	);
	let payload = $derived(spec.payload);
	let scatterPayload = $derived(payload?.type === 'scatter' ? payload : undefined);
	let boxPayload = $derived(payload?.type === 'box_plot' ? payload : undefined);
	let heatmapPayload = $derived(payload?.type === 'heatmap' ? payload : undefined);
	let forecastPayload = $derived(payload?.type === 'forecast' ? payload : undefined);

	let scatterValues = $derived(
		scatterPayload?.points.flatMap((point) => [point.x, point.y]) ?? []
	);
	let boxValues = $derived(
		boxPayload?.groups.flatMap((group) => [
			group.low_whisker,
			group.high_whisker,
			...group.outliers
		]) ?? []
	);
	let heatmapValues = $derived(
		heatmapPayload?.values.flatMap((row) => row.filter(isNumber)) ?? []
	);
	let forecastValues = $derived(
		forecastPayload
			? [
					...forecastPayload.observed,
					...forecastPayload.forecast,
					...forecastPayload.lower,
					...forecastPayload.upper
				  ].filter(isNumber)
			: []
	);
	let stackedTotals = $derived(
		spec.kind === 'stacked_area'
			? spec.labels
					.map((_, index) =>
						spec.series.every((series) => isNumber(series.values[index]))
							? spec.series.reduce((total, series) => total + (series.values[index] ?? 0), 0)
							: null
					)
					.filter(isNumber)
			: []
	);
	let values = $derived([
		...spec.series.flatMap((series) => series.values.filter(isNumber)),
		...stackedTotals,
		...scatterValues,
		...boxValues,
		...heatmapValues,
		...forecastValues
	]);
	let dataMin = $derived(values.length ? Math.min(0, ...values) : 0);
	let dataMax = $derived(values.length ? Math.max(0, ...values, dataMin + 1) : 1);

	let xScale = $derived(
		scalePoint<string>()
			.domain(spec.labels)
			.range([PLOT.left, WIDTH - PLOT.right])
			.padding(0.45)
	);
	let yScale = $derived(
		scaleLinear()
			.domain([dataMin, dataMax === dataMin ? dataMin + 1 : dataMax])
			.nice(4)
			.range([HEIGHT - PLOT.bottom, PLOT.top])
	);
	let yTicks = $derived(yScale.ticks(4));
	let labelStep = $derived.by(() => {
		const longestLabel = spec.labels.reduce((length, label) => Math.max(length, label.length), 0);
		const labelWidth = Math.max(52, Math.min(120, longestLabel * 6 + 14));
		return Math.max(1, Math.ceil((spec.labels.length * labelWidth) / WIDTH));
	});
	// Markers help short series; on dense series they overlap into visual noise
	// and thousands of extra SVG nodes. The line retains every value either way.
	let showLineMarkers = $derived(spec.labels.length <= 80);

	function pointsLine(series: (number | null)[]) {
		return line<number | null>()
			.defined(isNumber)
			.x((_, index) => xScale(spec.labels[index] ?? '') ?? 0)
			.y((value) => yScale(value ?? 0))
			.curve(curveMonotoneX)(series);
	}

	function pointsArea(series: (number | null)[]) {
		return area<number | null>()
			.defined(isNumber)
			.x((_, index) => xScale(spec.labels[index] ?? '') ?? 0)
			.y0(yScale(0))
			.y1((value) => yScale(value ?? 0))
			.curve(curveMonotoneX)(series);
	}

	// D3's stack layout treats absent values as zero, which would silently
	// misrepresent missing observations. Instead, build only contiguous bands
	// where every series has an observed value; nulls remain actual gaps.
	function makeStackPaths() {
		if (spec.kind !== 'stacked_area') return [];
		const completeIndexes = spec.labels
			.map((_, index) => index)
			.filter((index) => spec.series.every((series) => isNumber(series.values[index])));
		const segments: number[][] = [];
		for (const index of completeIndexes) {
			const previous = segments.at(-1);
			if (previous && previous.at(-1) === index - 1) previous.push(index);
			else segments.push([index]);
		}
		return spec.series.map((series, seriesIndex) => {
			let paths: string[] = [];
			for (const indexes of segments) {
				const band = indexes.map((index) => {
					const bottom = spec.series
						.slice(0, seriesIndex)
						.reduce((total, candidate) => total + (candidate.values[index] ?? 0), 0);
					return {
						index,
						bottom,
						top: bottom + (series.values[index] ?? 0)
					};
				});
				const path = area<(typeof band)[number]>()
					.x((point) => xScale(spec.labels[point.index] ?? '') ?? 0)
					.y0((point) => yScale(point.bottom))
					.y1((point) => yScale(point.top))
					.curve(curveMonotoneX)(band);
				if (path) paths.push(path);
			}
			return { name: series.name, color: color(seriesIndex), paths };
		});
	}
	let stackPaths = $derived(makeStackPaths());

	let scatterX = $derived(
		scaleLinear()
			.domain(scatterPayload?.points.length
				? [
						Math.min(...scatterPayload.points.map((point) => point.x)),
						Math.max(...scatterPayload.points.map((point) => point.x))
					]
				: [0, 1])
			.nice(4)
			.range([PLOT.left, WIDTH - PLOT.right])
	);
	let scatterY = $derived(
		scaleLinear()
			.domain(scatterPayload?.points.length
				? [
						Math.min(...scatterPayload.points.map((point) => point.y)),
						Math.max(...scatterPayload.points.map((point) => point.y))
					]
				: [0, 1])
			.nice(4)
			.range([HEIGHT - PLOT.bottom, PLOT.top])
	);
	let scatterGroups = $derived(
		[...new Set(scatterPayload?.points.map((point) => point.group).filter(Boolean) ?? [])]
	);
	function scatterColor(group: string | undefined, index: number): string {
		return group ? color(Math.max(0, scatterGroups.indexOf(group))) : color(index);
	}

	let boxPlotScale = $derived(
		scaleLinear()
			.domain(
				boxValues.length
					? [
							Math.min(...boxValues) === Math.max(...boxValues) ? Math.min(...boxValues) - 1 : Math.min(...boxValues),
							Math.min(...boxValues) === Math.max(...boxValues) ? Math.max(...boxValues) + 1 : Math.max(...boxValues)
						]
					: [0, 1]
			)
			.nice(4)
			.range([132, 508])
	);
	let boxPlotHeight = $derived(Math.max(140, (boxPayload?.groups.length ?? 0) * 46 + 40));

	let heatMin = $derived(heatmapValues.length ? Math.min(...heatmapValues) : 0);
	let heatMax = $derived(heatmapValues.length ? Math.max(...heatmapValues) : 1);
	function heatOpacity(value: number | null): number {
		if (!isNumber(value)) return 0;
		if (heatMax === heatMin) return 0.62;
		return 0.14 + ((value - heatMin) / (heatMax - heatMin)) * 0.74;
	}

	let pieValues = $derived(spec.series[0]?.values.map((value) => value ?? 0) ?? []);
	let pieSlices = $derived(pie<number>().sort(null).value((value) => Math.max(0, value))(pieValues));
	let pieArc = $derived(arc<PieArcDatum<number>>().innerRadius(spec.kind === 'donut' ? 48 : 0).outerRadius(82));
	let pieTotal = $derived(pieValues.reduce((sum, value) => sum + value, 0));

	let exactTable = $derived.by(() => {
		if (scatterPayload) {
			const hasGroup = scatterPayload.points.some((point) => point.group);
			return {
				headings: [spec.x_label || 'X', spec.y_label || 'Y', 'Point', ...(hasGroup ? ['Group'] : [])],
				rows: scatterPayload.points.map((point) => [
					point.x,
					point.y,
					point.label,
					...(hasGroup ? [point.group ?? '—'] : [])
				])
			};
		}
		if (boxPayload) {
			return {
				headings: ['Group', 'N', 'Low whisker', 'Q1', 'Median', 'Q3', 'High whisker', 'Outliers'],
				rows: boxPayload.groups.map((group) => [
					group.label,
					group.n,
					group.low_whisker,
					group.q1,
					group.median,
					group.q3,
					group.high_whisker,
					group.outliers.length ? group.outliers.join(', ') : 'None'
				])
			};
		}
		if (heatmapPayload) {
			return {
				headings: [spec.y_label || 'Group', ...heatmapPayload.x_labels],
				rows: heatmapPayload.y_labels.map((label, row) => [label, ...heatmapPayload.values[row]])
			};
		}
		if (forecastPayload) {
			return {
				headings: [spec.x_label || 'Period', 'Observed', 'Forecast', 'Lower bound', 'Upper bound'],
				rows: spec.labels.map((label, index) => [
					label,
					forecastPayload.observed[index],
					forecastPayload.forecast[index],
					forecastPayload.lower[index],
					forecastPayload.upper[index]
				])
			};
		}
		return {
			headings: [spec.x_label || (spec.kind === 'line' ? 'Period' : 'Category'), ...spec.series.map((series) => series.name)],
			rows: spec.labels.map((label, index) => [label, ...spec.series.map((series) => series.values[index])])
		};
	});

	function metadataEntries(metadata: ChartMetadata) {
		const fields = metadata.fields ?? [];
		const filters = metadata.filters ?? [];
		return [
			metadata.source_label ? ['Source', metadata.source_label] : null,
			metadata.source_evidence_id ? ['Result', metadata.source_evidence_id] : null,
			fields.length ? ['Fields', fields.join(', ')] : null,
			metadata.aggregation ? ['Aggregation', metadata.aggregation] : null,
			filters.length ? ['Filters', filters.join('; ')] : null,
			metadata.time_range ? ['Period', metadata.time_range] : null,
			metadata.denominator ? ['Whole', metadata.denominator] : null,
			metadata.missing_treatment ? ['Missing values', metadata.missing_treatment] : null
		].filter((entry): entry is [string, string] => entry !== null);
	}
	let chartDetails = $derived(spec.metadata ? metadataEntries(spec.metadata) : []);
</script>

<figure class="chart-card">
	<figcaption class="chart-header">
		<div class="chart-header-copy">
			<div class="chart-title">{chartTitle}</div>
			{#if spec.unit}<div class="chart-unit">Values in {spec.unit}</div>{/if}
			{#if spec.x_label || spec.y_label}
				<div class="chart-axis-summary">
					{#if spec.x_label}<span>{spec.x_label}</span>{/if}
					{#if spec.x_label && spec.y_label}<span aria-hidden="true"> · </span>{/if}
					{#if spec.y_label}<span>{spec.y_label}</span>{/if}
				</div>
			{/if}
		</div>
	</figcaption>
	{#if source}<div class="chart-context">{source}</div>{/if}

	{#if spec.kind === 'bar' || spec.kind === 'histogram'}
		<div class="chart" role="img" aria-label={chartTitle}>
			{#if spec.series.length > 1}
				<div class="chart-legend">
					{#each spec.series as series, index (series.name)}
						<span class="legend-item"><i class="swatch" style={`background:${color(index)}`}></i>{series.name}</span>
					{/each}
				</div>
			{/if}
			<div class="rows">
				{#each spec.labels as label, rowIndex (label + rowIndex)}
					<div class="row">
						<span class="row-label" title={label}>{label}</span>
						<div class="row-tracks">
							{#each spec.series as series, seriesIndex (series.name)}
								{@const value = series.values[rowIndex]}
								{@const span = dataMax - dataMin || 1}
								{@const left = isNumber(value) ? Math.min((0 - dataMin) / span, (value - dataMin) / span) * 100 : 0}
								{@const width = isNumber(value) ? Math.max(Math.abs((value / span) * 100), 0.75) : 0}
								<div class="track-line">
									<div class="track" aria-hidden="true">
										{#if isNumber(value)}<div class="fill" style={`left:${left}%;width:${width}%;background:${color(spec.series.length > 1 ? seriesIndex : rowIndex)}`}></div>{/if}
									</div>
									<span class="value">{isNumber(value) ? formatValue(value, spec.unit) : '—'}</span>
								</div>
							{/each}
						</div>
					</div>
				{/each}
			</div>
		</div>
	{:else if spec.kind === 'pie' || spec.kind === 'donut'}
		<div class="pie-layout">
			<svg viewBox="0 0 200 200" role="img" aria-label={chartTitle}>
				<title>{chartTitle}</title>
				<g transform="translate(100,100)">
					{#each pieSlices as slice, index (spec.labels[index])}
						<path d={pieArc(slice) ?? ''} fill={color(index)} class="pie-slice" aria-label={`${spec.labels[index]}: ${formatValue(pieValues[index] ?? 0, spec.unit)}`}>
							<title>{spec.labels[index]}: {formatValue(pieValues[index] ?? 0, spec.unit)} ({pieTotal ? (((pieValues[index] ?? 0) / pieTotal) * 100).toFixed(1) : '0.0'}%)</title>
						</path>
					{/each}
				</g>
			</svg>
			<div class="pie-legend">
				{#each spec.labels as label, index (label)}
					<div class="pie-legend-row">
						<i class="swatch" style={`background:${color(index)}`}></i>
						<span class="pie-label" title={label}>{label}</span>
						<span class="pie-value">{formatValue(pieValues[index] ?? 0, spec.unit)}</span>
						<span class="pie-percent">{pieTotal ? (((pieValues[index] ?? 0) / pieTotal) * 100).toFixed(1) : '0.0'}%</span>
					</div>
				{/each}
			</div>
		</div>
	{:else if spec.kind === 'scatter' && scatterPayload}
		<div class="chart">
			{#if scatterGroups.length > 0}
				<div class="chart-legend">
					{#each scatterGroups as group, index (group)}
						<span class="legend-item"><i class="swatch" style={`background:${color(index)}`}></i>{group}</span>
					{/each}
				</div>
			{/if}
			<svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label={chartTitle}>
				<title>{chartTitle}</title>
				{#each scatterY.ticks(4) as tick (tick)}
					<line x1={PLOT.left} x2={WIDTH - PLOT.right} y1={scatterY(tick)} y2={scatterY(tick)} class="grid-line" />
					<text x={PLOT.left - 8} y={scatterY(tick) + 4} class="y-axis-label" text-anchor="end">{formatValue(tick, spec.unit)}</text>
				{/each}
				<line x1={PLOT.left} x2={WIDTH - PLOT.right} y1={HEIGHT - PLOT.bottom} y2={HEIGHT - PLOT.bottom} class="zero-axis" />
				{#each scatterX.ticks(5) as tick (tick)}
					<text x={scatterX(tick)} y={HEIGHT - 12} class="axis-label" text-anchor="middle">{formatValue(tick)}</text>
				{/each}
				{#each scatterPayload.points as point, index (`${point.label}-${index}`)}
					<circle cx={scatterX(point.x)} cy={scatterY(point.y)} r="4" class="scatter-dot" style={`fill:${scatterColor(point.group, index)}`}>
						<title>{point.label}: {spec.x_label || 'X'} {formatValue(point.x)}, {spec.y_label || 'Y'} {formatValue(point.y)}{point.group ? ` · ${point.group}` : ''}</title>
					</circle>
				{/each}
				<text x={(PLOT.left + WIDTH - PLOT.right) / 2} y={HEIGHT - 1} class="axis-title" text-anchor="middle">{spec.x_label || 'X'}</text>
				<text transform={`translate(15 ${(PLOT.top + HEIGHT - PLOT.bottom) / 2}) rotate(-90)`} class="axis-title" text-anchor="middle">{spec.y_label || 'Y'}</text>
			</svg>
		</div>
	{:else if spec.kind === 'box_plot' && boxPayload}
		<div class="boxplot-wrap">
			<svg viewBox={`0 0 ${WIDTH} ${boxPlotHeight}`} role="img" aria-label={chartTitle}>
				<title>{chartTitle}</title>
				{#each boxPlotScale.ticks(5) as tick (tick)}
					<line x1={boxPlotScale(tick)} x2={boxPlotScale(tick)} y1="12" y2={boxPlotHeight - 26} class="grid-line" />
					<text x={boxPlotScale(tick)} y={boxPlotHeight - 8} class="axis-label" text-anchor="middle">{formatValue(tick, spec.unit)}</text>
				{/each}
				{#each boxPayload.groups as group, index (group.label)}
					{@const cy = 28 + index * 46}
					<text x="120" y={cy + 4} class="box-label" text-anchor="end">{group.label}</text>
					<line x1={boxPlotScale(group.low_whisker)} x2={boxPlotScale(group.high_whisker)} y1={cy} y2={cy} class="whisker" />
					<line x1={boxPlotScale(group.low_whisker)} x2={boxPlotScale(group.low_whisker)} y1={cy - 6} y2={cy + 6} class="whisker" />
					<line x1={boxPlotScale(group.high_whisker)} x2={boxPlotScale(group.high_whisker)} y1={cy - 6} y2={cy + 6} class="whisker" />
					<rect x={boxPlotScale(group.q1)} y={cy - 9} width={Math.max(1, boxPlotScale(group.q3) - boxPlotScale(group.q1))} height="18" rx="3" class="box-range" />
					<line x1={boxPlotScale(group.median)} x2={boxPlotScale(group.median)} y1={cy - 9} y2={cy + 9} class="median-line" />
					{#each group.outliers as outlier, outlierIndex (`${outlier}-${outlierIndex}`)}
						<circle cx={boxPlotScale(outlier)} cy={cy} r="3.5" class="outlier-dot"><title>{group.label} outlier: {formatValue(outlier, spec.unit)}</title></circle>
					{/each}
				{/each}
			</svg>
			<div class="chart-context">Boxes show the interquartile range; whiskers extend to the furthest observations within 1.5×IQR. Dots are outliers.</div>
		</div>
	{:else if spec.kind === 'heatmap' && heatmapPayload}
		<div class="heatmap-scroll" role="img" aria-label={chartTitle}>
			<table class="heatmap">
				<thead><tr><th scope="col">{spec.y_label || 'Group'} / {spec.x_label || 'Category'}</th>{#each heatmapPayload.x_labels as label (label)}<th scope="col" title={label}>{label}</th>{/each}</tr></thead>
				<tbody>
					{#each heatmapPayload.y_labels as yLabel, rowIndex (yLabel)}
						<tr><th scope="row">{yLabel}</th>
							{#each heatmapPayload.x_labels as xLabel, columnIndex (xLabel)}
								{@const value = heatmapPayload.values[rowIndex]?.[columnIndex] ?? null}
								<td><span class="heat-cell" class:empty={!isNumber(value)} style={`--heat-opacity:${heatOpacity(value)}`} title={`${yLabel} / ${xLabel}: ${isNumber(value) ? formatValue(value, spec.unit) : 'no observed value'}`} aria-label={`${yLabel} / ${xLabel}: ${isNumber(value) ? formatValue(value, spec.unit) : 'no observed value'}`}></span></td>
							{/each}
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{:else}
		<div class="chart">
			{#if spec.series.length > 1 || spec.kind === 'forecast'}
				<div class="chart-legend">
					{#each spec.series as series, index (series.name)}
						<span class="legend-item"><i class="swatch" style={`background:${color(index)}`}></i>{series.name}</span>
					{/each}
					{#if spec.kind === 'forecast'}
						<span class="legend-item"><i class="swatch"></i>Observed</span>
						<span class="legend-item"><i class="swatch forecast-swatch"></i>Forecast</span>
					{/if}
				</div>
			{/if}
			<svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} role="img" aria-label={chartTitle}>
				<title>{chartTitle}</title>
				{#each yTicks as tick (tick)}
					<line x1={PLOT.left} x2={WIDTH - PLOT.right} y1={yScale(tick)} y2={yScale(tick)} class="grid-line" class:zero-grid={tick === 0} />
					<text x={PLOT.left - 8} y={yScale(tick) + 4} class="y-axis-label" text-anchor="end">{formatValue(tick, spec.unit)}</text>
				{/each}
				<line x1={PLOT.left} x2={WIDTH - PLOT.right} y1={yScale(0)} y2={yScale(0)} class="zero-axis" />
				{#if forecastPayload}
					{@const band = area<number | null>()
						.defined((_, index) => isNumber(forecastPayload.lower[index]) && isNumber(forecastPayload.upper[index]))
						.x((_, index) => xScale(spec.labels[index] ?? '') ?? 0)
						.y0((_, index) => yScale(forecastPayload.lower[index] ?? 0))
						.y1((_, index) => yScale(forecastPayload.upper[index] ?? 0))
						.curve(curveMonotoneX)(forecastPayload.forecast)}
					{#if band}<path d={band} class="forecast-band" />{/if}
					<path d={pointsLine(forecastPayload.observed) ?? ''} class="line" style={`stroke:${color(0)}`} />
					<path d={pointsLine(forecastPayload.forecast) ?? ''} class="line dashed" style={`stroke:${color(1)}`} />
				{:else if spec.kind === 'stacked_area'}
					{#each stackPaths as series (series.name)}
						{#each series.paths as path, index (`${series.name}-${index}`)}<path d={path} class="area-fill" style={`fill:${series.color}`} />{/each}
					{/each}
				{:else}
					{#each spec.series as series, seriesIndex (series.name)}
						{#if spec.kind === 'area'}<path d={pointsArea(series.values) ?? ''} class="area-fill" style={`fill:${color(seriesIndex)}`} />{/if}
						<path d={pointsLine(series.values) ?? ''} class="line" class:dashed={seriesIndex > 0} style={`stroke:${color(seriesIndex)}`} />
						{#if showLineMarkers}
							{#each series.values as value, index (`${series.name}-${index}`)}
								{#if isNumber(value)}
									<circle cx={xScale(spec.labels[index] ?? '') ?? 0} cy={yScale(value)} r="3.2" class="line-dot" style={`fill:${color(seriesIndex)}`}>
										<title>{series.name}, {spec.labels[index]}: {formatValue(value, spec.unit)}</title>
									</circle>
								{/if}
							{/each}
						{/if}
					{/each}
				{/if}
				{#each spec.labels as label, index (label + index)}
					{#if index % labelStep === 0 || index === spec.labels.length - 1}<text x={xScale(label) ?? 0} y={HEIGHT - 12} class="axis-label" text-anchor="middle">{label}</text>{/if}
				{/each}
				{#if spec.x_label}<text x={(PLOT.left + WIDTH - PLOT.right) / 2} y={HEIGHT - 1} class="axis-title" text-anchor="middle">{spec.x_label}</text>{/if}
			</svg>
			{#if forecastPayload?.uncertainty_note}<p class="uncertainty-note">{forecastPayload.uncertainty_note}</p>{/if}
		</div>
	{/if}

	{#if chartDetails.length > 0}
		<details class="chart-details">
			<summary>Chart details</summary>
			<dl>{#each chartDetails as [label, value] (label)}<div><dt>{label}</dt><dd>{value}</dd></div>{/each}</dl>
		</details>
	{/if}

	<details class="values">
		<summary>Show exact values</summary>
		<div class="value-table-wrap">
			<table class="value-table">
				<caption class="sr-only">{chartTitle} values</caption>
				<thead><tr>{#each exactTable.headings as heading, index (`${heading}-${index}`)}<th scope="col">{heading}</th>{/each}</tr></thead>
				<tbody>{#each exactTable.rows as row, rowIndex (rowIndex)}<tr>{#each row as cell, cellIndex (`${rowIndex}-${cellIndex}`)}{#if cellIndex === 0}<th scope="row">{formatCell(cell)}</th>{:else}<td>{formatCell(cell)}</td>{/if}{/each}</tr>{/each}</tbody>
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
	.chart-header { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.chart-header-copy { min-width: 0; }
	.chart-title { color: var(--text); font-weight: 600; }
	.chart-unit, .chart-context { margin-top: 2px; color: var(--text-faint); font-size: var(--fs-xs); }
	.chart-axis-summary { margin-top: 2px; color: var(--text-dim); font-size: var(--fs-xs); }
	.chart-context { margin-bottom: var(--space-2); }
	.chart { max-width: 100%; }
	.chart-legend { display: flex; flex-wrap: wrap; gap: var(--space-3); margin-bottom: var(--space-2); color: var(--text-dim); font-size: var(--fs-xs); }
	.legend-item { display: inline-flex; align-items: center; gap: 5px; }
	.swatch { width: 8px; height: 8px; border-radius: 50%; flex: none; background: var(--brand); }
	.forecast-swatch { background: transparent; border: 1px dashed var(--chart-violet); }
	.rows { display: flex; flex-direction: column; gap: var(--space-2); }
	.row { display: grid; grid-template-columns: minmax(0, 30%) 1fr; align-items: center; gap: var(--space-3); }
	.row-label { min-width: 0; overflow: hidden; color: var(--text-dim); text-align: right; text-overflow: ellipsis; white-space: nowrap; }
	.row-tracks { display: flex; min-width: 0; flex-direction: column; gap: 3px; }
	.track-line { display: grid; grid-template-columns: 1fr max-content; align-items: center; gap: var(--space-2); }
	.track { position: relative; height: 12px; overflow: hidden; border-radius: var(--radius-sm); background: var(--bg-inset); }
	.fill { position: absolute; top: 0; bottom: 0; border-radius: var(--radius-sm); transition: width var(--dur) var(--ease), left var(--dur) var(--ease); }
	.value, .pie-value, .pie-percent { color: var(--text); font-variant-numeric: tabular-nums; white-space: nowrap; }
	svg { display: block; width: 100%; height: auto; overflow: visible; }
	.grid-line { stroke: var(--border); stroke-width: 0.75; }
	.grid-line.zero-grid { stroke: var(--border-strong); }
	.zero-axis, .whisker { stroke: var(--border-strong); stroke-width: 1; }
	.line { fill: none; stroke-width: 2; }
	.line.dashed { stroke-dasharray: 5 4; }
	.line-dot, .scatter-dot, .outlier-dot { stroke: var(--bg-raised); stroke-width: 1.4; }
	.axis-label, .y-axis-label { fill: var(--text-dim); font-size: var(--fs-xs); }
	.axis-title { fill: var(--text-dim); font-size: var(--fs-xs); }
	.area-fill { fill-opacity: 0.15; stroke: none; }
	.forecast-band { fill: var(--chart-violet); fill-opacity: 0.13; stroke: none; }
	.uncertainty-note { margin: var(--space-2) 0 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.pie-layout { display: grid; grid-template-columns: minmax(150px, 220px) minmax(0, 1fr); align-items: center; gap: var(--space-4); }
	.pie-layout svg { max-width: 220px; }
	.pie-slice { stroke: var(--bg-raised); stroke-width: 1.5; }
	.pie-legend { display: flex; min-width: 0; flex-direction: column; gap: 7px; }
	.pie-legend-row { display: grid; grid-template-columns: 9px minmax(0, 1fr) max-content 44px; align-items: center; gap: 8px; font-size: var(--fs-xs); }
	.pie-label { overflow: hidden; color: var(--text-dim); text-overflow: ellipsis; white-space: nowrap; }
	.pie-percent { color: var(--text-faint); text-align: right; }
	.boxplot-wrap { max-width: 100%; overflow-x: auto; }
	.boxplot-wrap svg { min-width: 420px; }
	.box-label { fill: var(--text-dim); font-size: var(--fs-xs); }
	.box-range { fill: color-mix(in srgb, var(--brand) 18%, transparent); stroke: var(--brand); stroke-width: 1.4; }
	.median-line { stroke: var(--text); stroke-width: 1.7; }
	.outlier-dot { fill: var(--chart-plum); }
	.heatmap-scroll { max-width: 100%; overflow: auto; }
	.heatmap { border-collapse: separate; border-spacing: 3px; font-size: var(--fs-xs); }
	.heatmap th { max-width: 110px; padding: 2px 5px; overflow: hidden; color: var(--text-dim); text-overflow: ellipsis; white-space: nowrap; }
	.heatmap th:first-child { position: sticky; left: 0; z-index: 1; background: var(--bg-raised); text-align: left; }
	.heatmap td { width: 28px; min-width: 28px; padding: 0; }
	.heat-cell { display: block; width: 26px; height: 22px; border-radius: 4px; background: var(--brand); opacity: var(--heat-opacity); }
	.heat-cell.empty { border: 1px dashed var(--border-strong); background: transparent; opacity: 1; }
	.values, .chart-details { margin-top: var(--space-2); font-size: var(--fs-xs); }
	.values summary, .chart-details summary { width: fit-content; color: var(--text-faint); cursor: pointer; }
	.values summary:hover, .chart-details summary:hover { color: var(--text-dim); }
	.value-table-wrap { margin-top: var(--space-2); overflow-x: auto; }
	.value-table { min-width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; white-space: nowrap; }
	.value-table th, .value-table td { padding: 4px 8px; border-bottom: 1px solid var(--border); text-align: right; }
	.value-table th:first-child, .value-table td:first-child { text-align: left; }
	.value-table thead th { color: var(--text-dim); font-weight: 500; }
	.chart-details dl { display: grid; gap: 5px; margin: var(--space-2) 0 0; }
	.chart-details dl div { display: grid; grid-template-columns: 110px minmax(0, 1fr); gap: var(--space-2); }
	.chart-details dt { color: var(--text-faint); }
	.chart-details dd { margin: 0; color: var(--text-dim); overflow-wrap: anywhere; }
	@media (max-width: 560px) {
		.chart-card { padding-inline: var(--space-3); }
		.row { grid-template-columns: minmax(0, 38%) 1fr; gap: var(--space-2); }
		.pie-layout { grid-template-columns: 150px minmax(0, 1fr); gap: var(--space-2); }
		.pie-legend-row { grid-template-columns: 8px minmax(0, 1fr) 38px; }
		.pie-percent { display: none; }
	}
</style>
