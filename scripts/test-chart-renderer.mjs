import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createServer } from 'vite';

const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	server: { middlewareMode: true, hmr: false, ws: false }
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
	const { default: EvidenceBlock } = await server.ssrLoadModule('/src/lib/components/EvidenceBlock.svelte');
	const { default: PythonCalculationDetails } = await server.ssrLoadModule('/src/lib/components/PythonCalculationDetails.svelte');
	const renderChart = (spec) => render(Chart, { props: { spec } }).body;
	const componentCss = await readFile('src/lib/components/Chart.svelte', 'utf8');
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
	const openableChartMessage = render(Message, {
		props: {
			onopenchart: () => {},
			message: {
				id: 'openable-chart', role: 'assistant', text: 'Metric totals by segment.', ts: 2,
				answer: {
					text: 'Metric totals by segment.', verification: [],
					evidence: [{ id: 'chart-evidence', tool: 'make_chart', args: {}, result_summary: 'accepted', chart: finalChart }]
				}
			}
		}
	}).body;
	assert.equal(count(openableChartMessage, /class="[^"]*\bchart-card\b[^"]*"/g), 1);
	assert.match(openableChartMessage, />Open beside<\/button>/);

	// In the composable Ask layout, opening a chart beside the conversation
	// moves the full visual into the companion pane and leaves a compact
	// transcript reference instead of rendering the same chart twice.
	const movedChartMessage = render(Message, {
		props: {
			openChartIndex: 0,
			onopenchart: () => {},
			message: {
				id: 'moved-chart',
				role: 'assistant',
				text: 'Metric totals by segment.',
				ts: 2,
				answer: {
					text: 'Metric totals by segment.',
					verification: [],
					evidence: [{
						id: 'chart-evidence', tool: 'make_chart', args: {},
						result_summary: 'accepted', chart: finalChart
					}]
				}
			}
		}
	}).body;
	assert.equal(count(movedChartMessage, /class="[^\"]*\bchart-card\b[^\"]*"/g), 0);
	assert.match(movedChartMessage, /Chart open beside conversation/);
	assert.doesNotMatch(movedChartMessage, /Open beside/);

	// The evidence disclosure names the answer mode instead of presenting the
	// same generic analysis label for general answers, inspections, and analysis.
	for (const [mode, label] of [
		['model_only', 'General answer'],
		['workspace_inspect', 'Inspection details'],
		['workspace_ask', 'Analysis details']
	]) {
		const modeMessage = render(Message, {
			props: {
				message: {
					id: `mode-${mode}`,
					role: 'assistant',
					text: 'A short answer.',
					ts: 1,
					answer: {
						text: 'A short answer.',
						evidence: [],
						verification: [],
						trace: { mode }
					}
				}
			}
		}).body;
		assert.match(modeMessage, new RegExp(label));
	}

	// A stopped run can still contain completed read-only work. The transcript
	// must keep its details disclosure and source visible instead of dropping
	// the already-finished evidence with the cancelled model response.
	const stoppedMessage = render(Message, {
		props: {
			expanded: true,
			message: {
				id: 'stopped-with-evidence',
				role: 'assistant',
				text: 'Stopped.',
				ts: 3,
				answer: {
					text: 'Stopped.',
					evidence: [{
						id: 'completed-before-stop',
						tool: 'run_sql',
						args: {},
						note: 'Calculate total sales',
						result_summary: '1 row',
						sources: [{ source: 'sales.csv', table: 'sales' }],
						columns: ['total'],
						rows: [[450]],
						row_count: 1
					}],
					verification: [],
					trace: { mode: 'workspace_ask' }
				}
			}
		}
	}).body;
	assert.match(stoppedMessage, /Stopped\./);
	assert.match(stoppedMessage, /Analysis details/);
	assert.match(stoppedMessage, /Calculate total sales/);
	assert.match(stoppedMessage, /sales\.csv/);

	// Clarification is a typed part of the answer, not just prose in the body.
	// Its controls live in the dock composer; the transcript should point there
	// without duplicating the question and choices beside the assistant answer.
	const clarificationMessage = render(Message, {
		props: {
			showFollowups: true,
			onfollowup: () => {},
			message: {
				id: 'typed-clarification',
				role: 'assistant',
				text: 'The scope changes the total.',
				ts: 4,
				answer: {
					turn_id: 'parent-turn',
					text: 'The scope changes the total.',
					clarification: {
						question: 'Which categories should count?',
						options: ['Rent and utilities', 'All housing-related costs'],
						reason: 'These choices produce different totals.'
					},
					evidence: [],
					verification: []
				}
			}
		}
	}).body;
	assert.match(clarificationMessage, /Choose an option below to continue\./);
	assert.doesNotMatch(clarificationMessage, /Choose an interpretation/);
	assert.doesNotMatch(clarificationMessage, /Rent and utilities/);
	assert.doesNotMatch(clarificationMessage, /Answer the clarification in your own words/);

	// Real runs can emit multiple independent checks with identical labels.
	// Opening Analysis Details must not crash, and the answer's line chart must
	// still render alongside both checks.
	const repeatedCheck = 'grounded raw observations were selected without aggregation';
	const lineAnswer = {
		text: 'Values rose from January to February.',
		evidence: [
			{
				id: 'line-evidence',
				tool: 'make_chart',
				args: {},
				result_summary: 'line chart',
				chart: generic('line', ['Jan', 'Feb'], [{ name: 'Observed', values: [2, 4] }])
			}
		],
		verification: [
			{ label: repeatedCheck, ok: true },
			{ label: 'grouped totals reconciled', ok: true },
			{ label: repeatedCheck, ok: true }
		]
	};
	const lineMessage = render(Message, {
		props: {
			message: {
				id: 'line-chart-with-duplicate-checks',
				role: 'assistant',
				text: lineAnswer.text,
				ts: 2,
				answer: lineAnswer
			}
		}
	}).body;
	assert.equal(count(lineMessage, /class="[^"]*\bchart-card\b[^"]*"/g), 1);
	assert.match(lineMessage, /aria-label="line validation"/);
	const detailsWithDuplicateChecks = render(EvidenceBlock, {
		props: { answer: lineAnswer, expanded: true, bodyId: 'duplicate-checks' }
	}).body;
	assert.equal(count(detailsWithDuplicateChecks, /grounded raw observations were selected without aggregation/g), 2);

	// Python must be visible as the execution language, with generated code
	// separated from its read-only SQL inputs and printed result.
	const pythonSnippet = [
		"rows = sql('SELECT rating FROM books')",
		"values = [row['rating'] for row in rows]",
		"print(median(values))"
	].join('\n');
	const pythonAnswer = {
		text: 'The median rating is 4.',
		evidence: [
			{
				id: 'python-statistic',
				tool: 'run_python',
				args: { note: 'Calculate median rating', code: pythonSnippet },
				note: 'Calculate median rating',
				result_summary: 'python finished in 8ms · local sandbox',
				output: '4',
				python_input_trace: {
					complete: true,
					queries: [{ sql: 'SELECT rating FROM books', columns: ['rating'], row_count: 5, truncated: false }]
				}
			}
		],
		verification: []
	};
	const pythonDetails = render(EvidenceBlock, {
		props: { answer: pythonAnswer, expanded: true, bodyId: 'python-details' }
	}).body;
	assert.match(pythonDetails, /Python/);
	assert.match(pythonDetails, /show calculation/);
	const pythonCodeDetails = render(PythonCalculationDetails, {
		props: { evidence: pythonAnswer.evidence[0] }
	}).body;
	assert.match(pythonCodeDetails, /Generated code · local sandbox/);
	assert.match(pythonCodeDetails, /Calculation code/);
	assert.match(pythonCodeDetails, /print\(median\(values\)\)/);
	assert.equal(count(pythonCodeDetails, /print\(median\(values\)\)/g), 1, 'code is not duplicated as a generic argument');

	// The built-in forecast tool performs its calculation in Python too. Show
	// the actual selected method and evaluation helpers without dumping an
	// opaque internal wrapper script.
	const forecastSql = 'SELECT period, value FROM metrics ORDER BY period';
	const forecastEvidence = {
					id: 'forecast-python',
					tool: 'forecast_analysis',
					args: { sql: forecastSql, method: 'linear_trend', horizon: 3, baseline_method: 'naive' },
					sql: forecastSql,
					result_summary: '24 observations; linear_trend method, 3-period horizon; rolling-origin comparison attempted',
					output: 'forecast_values=[31, 32, 33]',
					python_input_trace: {
						complete: true,
						queries: [{ sql: forecastSql, columns: ['period', 'value'], row_count: 24, truncated: false }]
					}
				};
	const forecastDetails = render(PythonCalculationDetails, {
		props: { evidence: forecastEvidence }
	}).body;
	const forecastEvidenceBlock = render(EvidenceBlock, {
		props: {
			answer: { text: 'Forecast summary.', evidence: [forecastEvidence], verification: [] },
			expanded: true,
			bodyId: 'forecast-evidence'
		}
	}).body;
	assert.match(forecastEvidenceBlock, /Forecast/);
	assert.match(forecastEvidenceBlock, /show forecast method/);
	assert.match(forecastDetails, /Fella forecast helpers · local sandbox/);
	assert.match(forecastDetails, /forecast_series\(method="linear_trend", horizon=3\)/);
	assert.match(forecastDetails, /rolling_origin_backtest\(baseline_method="naive"\)/);
	assert.match(forecastDetails, /forecast_error_bands/);

	const line = renderChart(generic('line', ['Jan', 'Feb'], [{ name: 'Observed', values: [2, 4] }], {
		x_label: 'Month',
		y_label: 'Count'
	}));
	assert.match(line, /<svg[^>]+role="img" aria-label="line validation"/);
	assert.match(line, /Month/);
	assert.match(line, /Observed/);

	// Axis ticks stay compact and numeric when a long unit is already stated in
	// the chart header; exact values retain their unit in the table. This keeps
	// y-axis text within the plot's reserved left gutter instead of bleeding
	// across the card edge.
	const rentalsLine = renderChart(generic('line', ['2012-01', '2012-02', '2012-03'], [{
		name: 'Rentals', values: [50000, 150000, 250000]
	}], { unit: 'rentals', x_label: 'Month' }));
	assert.match(rentalsLine, /Values in rentals/);
	assert.match(rentalsLine, />January</, 'exact values retain full month names');
	assert.doesNotMatch(rentalsLine, />2012-01</, 'the display should not expose raw ISO labels on a month-name axis');
	const rentalAxisLabels = [...rentalsLine.matchAll(/<text\b([^>]*)>([^<]*)<\/text>/g)]
		.filter((match) => /\bclass="axis-label(?:\s|")/.test(match[1]))
		.map((match) => match[2]);
	assert.deepEqual(rentalAxisLabels.slice(0, 3), ['Jan', 'Feb', 'Mar'],
		'month axis uses readable compact names without discarding full source periods');
	const rentalsTicks = [...rentalsLine.matchAll(/<text\b([^>]*)>([^<]*)<\/text>/g)]
		.filter((match) => /\bclass="y-axis-label(?:\s|")/.test(match[1]))
		.map((match) => match[2]);
	assert.ok(rentalsTicks.length > 0, 'expected y-axis ticks');
	assert.ok(rentalsTicks.every((tick) => !tick.includes('rentals')),
		'long units belong in the chart header, not concatenated to every tick');
	assert.ok(rentalsTicks.includes('250k'), 'large y-axis values should use compact notation');
	assert.match(rentalsLine, /250,000 rentals/, 'exact-value table preserves the full value and separates the unit');
	assert.doesNotMatch(rentalsLine, /class="chart-axis-summary"/,
		'the x-axis name should not be repeated above and below the plot');
	assert.match(componentCss, /\.chart-card\s*\{[^}]*min-width:\s*0;[^}]*max-width:\s*100%;[^}]*overflow:\s*hidden/s,
		'chart frame contains oversized content instead of bleeding into the transcript');
	const rentalsBars = renderChart(generic('bar', ['2012-01', '2012-02'], [{
		name: 'Rentals', values: [50000, 150000]
	}], { unit: 'rentals', x_label: 'Month' }));
	assert.match(rentalsBars, /class="row-label(?: [^"]+)?"[^>]*>January<\/span>/,
		'categorical chart labels use the readable month display formatter too');
	assert.match(rentalsBars, /class="value(?: [^"]+)?"[^>]*>50,000 rentals<\/span>/,
		'bar values separate word units while retaining exact magnitudes');
	const crossYearMonths = renderChart(generic('line', ['2024-12', '2025-01'], [{
		name: 'Rentals', values: [200, 240]
	}], { x_label: 'Month' }));
	assert.match(crossYearMonths, />Dec 24</);
	assert.match(crossYearMonths, />Jan 25</);
	assert.match(crossYearMonths, />December 2024</, 'exact values preserve a year for multi-year periods');

	// Period labels should remain separated on a 24-month line chart. In
	// particular, do not force a final tick into the narrow tail gap when the
	// nearest regular tick is already within one label-width of the chart edge.
	const monthlyLabels = Array.from({ length: 24 }, (_, index) => {
		const date = new Date(Date.UTC(2024, index, 1));
		return date.toISOString().slice(0, 7);
	});
	const monthlyTrend = renderChart(generic('line', monthlyLabels, [{
		name: 'Net revenue',
		values: monthlyLabels.map((_, index) => 66000 + index * 700)
	}]));
	const monthlyAxisLabels = [...monthlyTrend.matchAll(/<text\b([^>]*)>([^<]*)<\/text>/g)]
		.filter((match) => /\bclass="axis-label(?:\s|")/.test(match[1]))
		.map((match) => ({ x: Number(match[1].match(/\bx="([\d.]+)"/)?.[1]), label: match[2] }));
	assert.ok(
		monthlyAxisLabels.length >= 6 && monthlyAxisLabels.length < monthlyLabels.length,
		'expected thinned monthly ticks; found ' + monthlyAxisLabels.map((item) => item.label).join(', ')
	);
	for (let index = 1; index < monthlyAxisLabels.length; index += 1) {
		const gap = monthlyAxisLabels[index].x - monthlyAxisLabels[index - 1].x;
		assert.ok(gap >= 55, `monthly x-axis labels should not collide (gap=${gap})`);
	}
	assert.notEqual(monthlyAxisLabels.at(-1).label, '2025-12', 'omit a crowded final tick, not the data point');

	// A full-archive daily series keeps every point in its paths and exact-value
	// table, while the visible axis labels are thinned and dense point markers
	// are omitted to keep the chart legible and the SVG light.
	const archiveDates = Array.from({ length: 731 }, (_, index) =>
		new Date(Date.UTC(2024, 0, 1 + index)).toISOString().slice(0, 10)
	);
	const archiveSeries = ['Direct', 'Partner', 'Retail', 'Search'].map((name, seriesIndex) => ({
		name,
		values: archiveDates.map((_, dayIndex) => 100 + dayIndex * 10 + seriesIndex)
	}));
	const archive = renderChart(
		generic('line', archiveDates, archiveSeries, { x_label: 'Date', y_label: 'Visits' })
	);
	assert.equal(count(archive, /<path\b/g), 4, 'one complete line path is rendered per series');
	assert.equal(count(archive, /class="line-dot"/g), 0);
	assert.ok(count(archive, /class="axis-label"/g) <= 10, 'daily x-axis labels are thinned');
	assert.match(archive, /2024-01-01/);
	assert.match(archive, /2025-12-31/);
	assert.equal(count(archive, /<th[^>]*scope="row"/g), 731, 'all dates remain in exact values');
	assert.match(archive, /7,403/, 'the last exact data point is retained');

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
	assert.equal(count(forecast, /class="[^"]*\bforecast-dot\b[^"]*"/g), 2);
	assert.match(forecast, /Forecast, Mar: 15/);
	assert.match(forecast, /Lower bound/);
	assert.match(forecast, /Upper bound/);
	assert.match(forecast, /18\.5/);

	// Singleton forecast periods need an explicit plotted mark; a one-point
	// dashed path is otherwise invisible even though exact values are present.
	const singlePointForecast = renderChart(generic('forecast', ['Jan', 'Feb', 'Mar'], [], {
		x_label: 'Month',
		payload: {
			type: 'forecast',
			observed: [10, 12, null],
			forecast: [null, null, 15],
			lower: [null, null, 13],
			upper: [null, null, 17],
			uncertainty_note: undefined
		}
	}));
	assert.equal(count(singlePointForecast, /class="[^"]*\bforecast-dot\b[^"]*"/g), 1);
	assert.match(singlePointForecast, /Forecast, Mar: 15/);
	const isolatedLinePoint = renderChart(generic('line', Array.from({ length: 100 }, (_, index) => `P${index + 1}`), [
		{ name: 'Observed', values: Array.from({ length: 100 }, (_, index) => index + 1) },
		{ name: 'Forecast', values: Array.from({ length: 100 }, (_, index) => index === 99 ? 42 : null) }
	]));
	assert.equal(count(isolatedLinePoint, /class="[^"]*\bline-dot\b[^"]*"/g), 1);
	assert.match(isolatedLinePoint, /Forecast, P100: 42/);

	const dailyScatter = renderChart(generic('scatter', [], [], {
		x_label: 'Temperature',
		y_label: 'Daily rentals',
		payload: {
			type: 'scatter',
			points: Array.from({ length: 731 }, (_, index) => ({ x: index / 1000, y: index * 2, label: String(index + 1) }))
		}
	}));
	assert.equal(count(dailyScatter, /class="[^\"]*\bscatter-dot\b[^\"]*"/g), 731);

	// Chart series use application tokens, with separate values in both themes.
	// This is a structural theme check; visual contrast still needs a human pass.
	const css = await readFile('src/app.css', 'utf8');
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
