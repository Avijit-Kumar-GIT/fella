import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createServer } from 'vite';

const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	server: { middlewareMode: true, hmr: false }
});

function count(html, pattern) {
	return [...html.matchAll(pattern)].length;
}

function token(block, name) {
	const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
	const match = block.match(new RegExp(`${escaped}:\\s*(#[0-9a-fA-F]{6})`));
	assert.ok(match, `missing color token ${name}`);
	return match[1];
}

function luminance(hex) {
	const channels = [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16) / 255);
	const linear = channels.map((value) =>
		value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4
	);
	return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
}

function contrast(a, b) {
	const values = [luminance(a), luminance(b)].sort((left, right) => right - left);
	return (values[0] + 0.05) / (values[1] + 0.05);
}

try {
	const { render } = await server.ssrLoadModule('svelte/server');
	const { default: Chart } = await server.ssrLoadModule('/src/lib/components/Chart.svelte');
	const { default: Message } = await server.ssrLoadModule('/src/lib/components/Message.svelte');
	const renderChart = (spec) => render(Chart, { props: { spec } }).body;
	const metadata = {
		source_label: 'fixture.csv',
		fields: ['period', 'value'],
		aggregation: 'monthly sum',
		filters: ['year = 2025'],
		time_range: '2025-01 through 2025-03',
		part_to_whole: false,
		missing_treatment: 'unobserved cells remain blank, not zero'
	};
	const generic = (kind, labels, series, extra = {}) => ({
		kind,
		title: `${kind} validation`,
		labels,
		series,
		metadata,
		...extra
	});

	const bar = renderChart(generic('bar', ['Rent', 'Food'], [{ name: 'Spend', values: [120, 45] }]));
	assert.match(bar, /role="img" aria-label="bar validation"/);
	assert.match(bar, /Show exact values/);
	assert.match(bar, /monthly sum/);
	assert.match(bar, /year = 2025/);
	assert.match(bar, /120/);

	// Older serialized chart metadata may omit empty `fields`/`filters` arrays
	// (the Rust wire shape historically skipped empty vectors). It must remain
	// renderable instead of crashing the entire transcript on `.length`.
	const sparseMetadataBar = renderChart(
		generic('bar', ['Rent'], [{ name: 'Spend', values: [120] }], {
			metadata: { part_to_whole: false }
		})
	);
	assert.match(sparseMetadataBar, /role="img" aria-label="bar validation"/);

	// Superseded charts remain in evidence for auditability, but only the final
	// accepted chart should appear in the visible answer.
	const finalChart = generic('bar', ['A', 'B'], [{ name: 'Metric', values: [10, 20] }]);
	const renderedMessage = render(Message, {
		props: {
			message: {
				id: 'chart-lifecycle',
				role: 'assistant',
				text: 'Metric totals by segment.',
				ts: 1,
				answer: {
					text: 'Metric totals by segment.',
					verification: [],
					evidence: [
						{ id: 'old-chart-1', tool: 'make_chart', args: {}, result_summary: 'old', error: 'superseded', chart: finalChart },
						{ id: 'old-chart-2', tool: 'make_chart', args: {}, result_summary: 'old', error: 'superseded', chart: finalChart },
						{ id: 'accepted-chart', tool: 'make_chart', args: {}, result_summary: 'accepted', chart: finalChart }
					]
				}
			}
		}
	}).body;
	assert.equal(count(renderedMessage, /class="[^"]*\bchart-card\b[^"]*"/g), 1);

	const line = renderChart(generic('line', ['Jan', 'Feb'], [{ name: 'Observed', values: [2, 4] }], {
		x_label: 'Month',
		y_label: 'Count'
	}));
	assert.match(line, /<svg[^>]+role="img" aria-label="line validation"/);
	assert.match(line, /Month/);
	assert.match(line, /Observed/);

	for (const kind of ['pie', 'donut']) {
		const html = renderChart(generic(kind, ['A', 'B'], [{ name: 'Share', values: [3, 1] }], {
			metadata: { ...metadata, denominator: 'all observations', part_to_whole: true }
		}));
		assert.equal(count(html, /class="[^"]*\bpie-slice\b[^"]*"/g), 2, `${kind} has two rendered slices`);
		assert.match(html, /A/);
		assert.match(html, /75\.0%/);
	}

	const histogram = renderChart(generic('histogram', ['10–25', '25–40'], [{ name: 'Count', values: [4, 2] }], {
		x_label: 'Response time',
		y_label: 'Frequency'
	}));
	assert.equal(count(histogram, /class="row(?: [^"]*)?"/g), 2);
	assert.match(histogram, /role="img" aria-label="histogram validation"/);
	assert.match(histogram, /Frequency/);

	const scatter = renderChart(generic('scatter', [], [], {
		x_label: 'Study hours',
		y_label: 'Exam score',
		payload: {
			type: 'scatter',
			points: [
				{ x: 1, y: 55, label: 'P1', group: 'A' },
				{ x: 2, y: 60, label: 'P2', group: 'A' }
			]
		}
	}));
	assert.equal(count(scatter, /class="[^"]*\bscatter-dot\b[^"]*"/g), 2);
	assert.match(scatter, /Study hours/);
	assert.match(scatter, /Exam score/);
	assert.match(scatter, /P1/);
	assert.match(scatter, /<th scope="col"[^>]*>Group<\/th>/);

	const boxPlot = renderChart(generic('box_plot', [], [], {
		payload: {
			type: 'box_plot',
			groups: [
				{ label: 'A', low_whisker: 1, q1: 2, median: 3, q3: 4, high_whisker: 4, outliers: [20], n: 5 },
				{ label: 'B', low_whisker: 2, q1: 3, median: 6, q3: 9, high_whisker: 10, outliers: [], n: 5 }
			]
		}
	}));
	assert.equal(count(boxPlot, /class="[^"]*\bbox-range\b[^"]*"/g), 2);
	assert.equal(count(boxPlot, /class="[^"]*\bmedian-line\b[^"]*"/g), 2);
	assert.match(boxPlot, /A outlier: 20/);
	assert.match(boxPlot, /<th scope="col"[^>]*>Q1<\/th>/);
	assert.match(boxPlot, /<th scope="col"[^>]*>Median<\/th>/);

	const area = renderChart(generic('area', ['Jan', 'Feb', 'Mar'], [{ name: 'Revenue', values: [20, 30, 25] }]));
	assert.match(area, /class="[^"]*\barea-fill\b[^"]*"/);
	assert.match(area, /Jan/);

	const stacked = renderChart(generic('stacked_area', ['Jan', 'Feb'], [
		{ name: 'Email', values: [10, 14] },
		{ name: 'Search', values: [15, 13] }
	]));
	assert.equal(count(stacked, /class="[^"]*\barea-fill\b[^"]*"/g), 2);
	assert.match(stacked, /Email/);
	assert.match(stacked, /Search/);

	const heatmap = renderChart(generic('heatmap', [], [], {
		x_label: 'Daypart',
		y_label: 'Weekday',
		payload: {
			type: 'heatmap',
			x_labels: ['AM', 'PM'],
			y_labels: ['Mon', 'Tue'],
			values: [[10, 30], [20, null]]
		}
	}));
	assert.equal(count(heatmap, /class="[^"]*\bheat-cell\b[^\"]*\bempty\b[^\"]*"/g), 1);
	assert.match(heatmap, /Tue \/ PM: no observed value/);
	assert.match(heatmap, /<th scope="col"[^>]*>AM<\/th>/);
	assert.match(heatmap, /<th scope="col"[^>]*>PM<\/th>/);
	assert.match(heatmap, /<caption class="sr-only">heatmap validation values<\/caption>/);

	const forecast = renderChart(generic('forecast', ['Jan', 'Feb', 'Mar', 'Apr'], [], {
		x_label: 'Month',
		payload: {
			type: 'forecast',
			observed: [10, 12, null, null],
			forecast: [null, null, 15, 16],
			lower: [null, null, 13, 13.5],
			upper: [null, null, 17, 18.5],
			uncertainty_note: undefined
		}
	}));
	assert.match(forecast, /class="[^"]*\bforecast-band\b[^"]*"/);
	assert.match(forecast, /class="[^"]*\bline\b[^"]*\bdashed\b[^"]*"/);
	assert.match(forecast, /Lower bound/);
	assert.match(forecast, /Upper bound/);
	assert.match(forecast, /18\.5/);

	// Chart series use application tokens, with separate values in both themes.
	// This is a structural theme check; visual contrast still needs a human pass.
	const css = await readFile('src/app.css', 'utf8');
	const componentCss = await readFile('src/lib/components/Chart.svelte', 'utf8');
	const light = css.match(/:root\s*\{([\s\S]*?)\n\}/)?.[1];
	const dark = css.match(/:root\[data-color-mode='dark'\]\s*\{([\s\S]*?)\n\}/)?.[1];
	assert.ok(light, 'light theme tokens exist');
	assert.ok(dark, 'dark theme tokens exist');
	for (const block of [light, dark]) {
		const background = token(block, '--bg-raised');
		for (const name of [
			'--brand',
			'--chart-violet',
			'--chart-cyan',
			'--chart-slate',
			'--chart-plum',
			'--chart-orange',
			'--chart-green',
			'--chart-gold'
		]) {
			assert.ok(contrast(token(block, name), background) >= 3, `${name} has 3:1 graphical contrast`);
		}
	}
	assert.match(componentCss, /\.chart\s*\{[^}]*max-width:\s*100%/);
	assert.match(componentCss, /\.boxplot-wrap\s*\{[^}]*overflow-x:\s*auto/);
	assert.match(componentCss, /\.heatmap-scroll\s*\{[^}]*overflow:\s*auto/);
	assert.match(componentCss, /@media\s*\(max-width:\s*560px\)/);
	console.log('Chart renderer SSR checks passed for bar, line, pie, donut, histogram, scatter, box, area, stacked area, heatmap, and forecast.');
} finally {
	await server.close();
}
