import assert from 'node:assert/strict';
import {
	access,
	appendFile,
	mkdtemp,
	mkdir,
	rm,
	writeFile
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect as baseExpect, test, _electron as electron } from '@playwright/test';
import { createServer } from 'node:http';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const expect = baseExpect.configure({ timeout: 15_000 });
const runId = new Date().toISOString().replaceAll(':', '-').replaceAll('.', '-');
const resultDir = join(root, 'test-results', 'e2e-chart-families', runId);
const auditPath = join(resultDir, 'journey.jsonl');
const screenshotDir = join(resultDir, 'screenshots');

const months = [
	'2025-01', '2025-02', '2025-03', '2025-04', '2025-05', '2025-06',
	'2025-07', '2025-08', '2025-09', '2025-10', '2025-11', '2025-12'
];
const channels = ['Direct', 'Search', 'Partner'];
const categories = ['Books', 'Travel', 'Grocery'];
const weekdays = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday'];
const dayparts = ['Morning', 'Afternoon', 'Evening'];

// Synthetic but varied data gives the visual families enough marks to expose
// empty-series, clipping, and rendering issues without asserting answer values.
const observationRows = months.flatMap((period, monthIndex) =>
	channels.map((channel, channelIndex) => {
		const offset = monthIndex * channels.length + channelIndex;
		const cohort = offset % 2 === 0 ? 'Control' : 'Treatment';
		const category = categories[(monthIndex + channelIndex) % categories.length];
		const categoryMultiplier = category === 'Travel' ? 2.2 : category === 'Grocery' ? 0.65 : 1;
		const amount = Math.round((95 + monthIndex * 13 + channelIndex * 19 + ((monthIndex + channelIndex) % 4) * 7) * categoryMultiplier);
		const visits = 80 + monthIndex * 9 + channelIndex * 17 + ((monthIndex * 2 + channelIndex) % 5) * 11;
		const studyHours = 1 + ((offset * 7) % 29) / 4;
		const score = 58 + ((offset * 17 + monthIndex * 3) % 41);
		const reading = 2.1 + ((offset * 17 + channelIndex * 7) % 37) / 10;
		const weekday = weekdays[(monthIndex + channelIndex * 2) % weekdays.length];
		const daypart = dayparts[(monthIndex + channelIndex) % dayparts.length];
		return [
			`R${String(offset + 1).padStart(2, '0')}`,
			category,
			period,
			amount,
			channel,
			visits,
			studyHours.toFixed(2),
			score,
			cohort,
			reading.toFixed(1),
			weekday,
			daypart
		];
	})
);

const observationCsv = [
	'id,category,period,amount,channel,visits,study_hours,score,cohort,reading,weekday,daypart',
	...observationRows.map((row) => row.join(','))
].join('\n') + '\n';

const projectionRows = [
	...months.map((period, index) => `${period},${100 + index * 6 + (index % 3) * 3},,,,`),
	'2026-01,,176,161,191',
	'2026-02,,182,165,199',
	'2026-03,,188,169,207'
].join('\n');

const chartCases = [
	{
		id: 'bar',
		kind: 'bar',
		sourceFile: 'chart_observations.csv',
		question: 'Compare total sales across product categories as a horizontal bar chart.',
		args: {
			kind: 'bar', title: 'Sales by category',
			sql: 'SELECT category, SUM(amount) AS total_sales FROM chart_observations GROUP BY category ORDER BY category',
			x_field: 'category', value_field: 'total_sales', unit: '$', aggregation: 'sum', missing_treatment: 'reject'
		},
		markers: [{ selector: '.rows .row', min: 3 }, { selector: '.row-label', min: 3 }]
	},
	{
		id: 'line',
		kind: 'line',
		sourceFile: 'chart_observations.csv',
		question: 'Show the monthly trend in visits as a line chart.',
		args: {
			kind: 'line', title: 'Monthly visits',
			sql: 'SELECT period, SUM(visits) AS total_visits FROM chart_observations GROUP BY period ORDER BY period',
			x_field: 'period', value_field: 'total_visits', unit: 'visits', aggregation: 'sum', missing_treatment: 'reject'
		},
		markers: [{ selector: 'svg[role="img"] path.line:not(.dashed)', min: 1 }, { selector: 'svg[role="img"] .line-dot', min: 10 }, { selector: 'svg[role="img"] .axis-label', min: 4 }]
	},
	{
		id: 'pie',
		kind: 'pie',
		sourceFile: 'chart_observations.csv',
		question: 'Show the sales mix across product categories as a pie chart.',
		args: {
			kind: 'pie', title: 'Sales mix',
			sql: 'SELECT category, SUM(amount) AS total_sales FROM chart_observations GROUP BY category ORDER BY category',
			x_field: 'category', value_field: 'total_sales', unit: '$', part_to_whole: true,
			denominator: 'All recorded sales in chart_observations across the full period', aggregation: 'sum', missing_treatment: 'reject'
		},
		markers: [{ selector: '.pie-slice', min: 3 }, { selector: '.pie-legend-row', min: 3 }],
		pieArcCommands: 1
	},
	{
		id: 'donut',
		kind: 'donut',
		sourceFile: 'chart_observations.csv',
		question: 'Show the sales mix across product categories as a donut chart.',
		args: {
			kind: 'donut', title: 'Sales mix by category',
			sql: 'SELECT category, SUM(amount) AS total_sales FROM chart_observations GROUP BY category ORDER BY category',
			x_field: 'category', value_field: 'total_sales', unit: '$', part_to_whole: true,
			denominator: 'All recorded sales in chart_observations across the full period', aggregation: 'sum', missing_treatment: 'reject'
		},
		markers: [{ selector: '.pie-slice', min: 3 }, { selector: '.pie-legend-row', min: 3 }],
		pieArcCommands: 2
	},
	{
		id: 'scatter',
		kind: 'scatter',
		sourceFile: 'chart_observations.csv',
		question: 'Plot study hours against score for each observation as a scatter plot.',
		args: {
			kind: 'scatter', title: 'Study hours and score',
			sql: 'SELECT id, study_hours, score FROM chart_observations ORDER BY id',
			x_field: 'study_hours', y_field: 'score', label_field: 'id', missing_treatment: 'reject'
		},
		markers: [{ selector: 'svg[role="img"] .scatter-dot', min: 36 }, { selector: 'svg[role="img"] .axis-title', min: 2 }]
	},
	{
		id: 'histogram',
		kind: 'histogram',
		sourceFile: 'chart_observations.csv',
		question: 'Show the distribution of reading observations as a histogram.',
		args: {
			kind: 'histogram', title: 'Reading distribution',
			sql: 'SELECT reading FROM chart_observations ORDER BY reading',
			x_field: 'reading', bin_count: 6, missing_treatment: 'reject'
		},
		markers: [{ selector: '.rows .row', exact: 6 }, { selector: '.row-label', exact: 6 }]
	},
	{
		id: 'box-plot',
		kind: 'box_plot',
		sourceFile: 'chart_observations.csv',
		question: 'Compare score distributions between cohorts with a box plot.',
		args: {
			kind: 'box_plot', title: 'Scores by cohort',
			sql: 'SELECT cohort, score FROM chart_observations ORDER BY cohort, score',
			group_field: 'cohort', value_field: 'score', missing_treatment: 'reject'
		},
		markers: [{ selector: '.boxplot-wrap .box-range', min: 2 }, { selector: '.boxplot-wrap .box-label', min: 2 }]
	},
	{
		id: 'area',
		kind: 'area',
		sourceFile: 'chart_observations.csv',
		question: 'Plot total sales by month as an area chart.',
		args: {
			kind: 'area', title: 'Monthly sales',
			sql: 'SELECT period, SUM(amount) AS total_sales FROM chart_observations GROUP BY period ORDER BY period',
			x_field: 'period', value_field: 'total_sales', unit: '$', aggregation: 'sum', missing_treatment: 'reject'
		},
		markers: [{ selector: 'svg[role="img"] .area-fill', min: 1 }, { selector: 'svg[role="img"] path.line', min: 1 }]
	},
	{
		id: 'stacked-area',
		kind: 'stacked_area',
		sourceFile: 'chart_observations.csv',
		question: 'Compare monthly visits by channel as a stacked area chart.',
		args: {
			kind: 'stacked_area', title: 'Monthly visits by channel',
			sql: 'SELECT period, channel, SUM(visits) AS total_visits FROM chart_observations GROUP BY period, channel ORDER BY period, channel',
			x_field: 'period', group_field: 'channel', value_field: 'total_visits', unit: 'visits', aggregation: 'sum', missing_treatment: 'reject'
		},
		markers: [{ selector: 'svg[role="img"] .area-fill', min: 3 }, { selector: '.chart-legend .legend-item', min: 3 }]
	},
	{
		id: 'heatmap',
		kind: 'heatmap',
		sourceFile: 'chart_observations.csv',
		question: 'Show visits by weekday and daypart in a heatmap.',
		args: {
			kind: 'heatmap', title: 'Visits by weekday and daypart',
			sql: 'SELECT weekday, daypart, SUM(visits) AS total_visits FROM chart_observations GROUP BY weekday, daypart ORDER BY weekday, daypart',
			x_field: 'weekday', group_field: 'daypart', value_field: 'total_visits', unit: 'visits', aggregation: 'sum', missing_treatment: 'reject'
		},
		markers: [{ selector: '.heatmap tbody tr', min: 3 }, { selector: '.heatmap thead th', min: 4 }, { selector: '.heat-cell', min: 9 }]
	},
	{
		id: 'forecast',
		kind: 'forecast',
		sourceFile: 'projection.csv',
		question: 'Show observed and projected monthly visits with the supplied uncertainty interval as a forecast chart.',
		args: {
			kind: 'forecast', title: 'Observed and projected visits',
			sql: 'SELECT period, observed, forecast, lower, upper FROM projection ORDER BY period',
			x_field: 'period', series_fields: ['observed', 'forecast', 'lower', 'upper'], unit: 'visits', missing_treatment: 'gap'
		},
		markers: [
			{ selector: 'svg[role="img"] path.line:not(.dashed)', min: 1 },
			{ selector: 'svg[role="img"] path.line.dashed', min: 1 },
			{ selector: 'svg[role="img"] .forecast-dot', min: 3 },
			{ selector: 'svg[role="img"] .forecast-band', min: 1 },
			{ selector: '.chart-legend .forecast-swatch', min: 1 }
		]
	}
];

function sidecarPath() {
	const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
	const suffix = process.platform === 'win32' ? '.exe' : '';
	return join(root, 'electron', 'engine', `fella-engine-${arch}${suffix}`);
}

async function launchApp(dataDir) {
	await access(sidecarPath()).catch(() => {
		throw new Error('Electron sidecar is missing; run pnpm electron:build first.');
	});
	return electron.launch({
		args: [join(root, 'electron', 'main.mjs'), `--user-data-dir=${join(dataDir, 'chromium')}`],
		cwd: root,
		env: { ...process.env, FELLA_DATA_DIR: dataDir, FELLA_ENGINE_PATH: sidecarPath() }
	});
}

async function mountWorkspace(app, page, workspace) {
	await app.evaluate(({ dialog }, folder) => {
		dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
	}, workspace);
	const addRepository = page.getByRole('button', { name: 'Add a repository' });
	if (await addRepository.isVisible().catch(() => false)) await addRepository.click();
	else await page.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('combobox', { name: /Ask about/ })).toBeVisible();
}

function sse(response, value) {
	response.write(`data: ${JSON.stringify(value)}\n\n`);
}

function contentText(content) {
	if (typeof content === 'string') return content;
	if (!Array.isArray(content)) return '';
	return content.map((part) => typeof part?.text === 'string' ? part.text : '').join('\n');
}

function caseForRequest(body) {
	const messages = Array.isArray(body?.messages) ? body.messages : [];
	for (let index = messages.length - 1; index >= 0; index--) {
		if (messages[index]?.role !== 'user') continue;
		const text = contentText(messages[index].content);
		const match = chartCases.find((chartCase) => text.includes(chartCase.question));
		if (match) return { chartCase: match, userMessageIndex: index };
	}
	return null;
}

async function recordAudit(record) {
	await appendFile(auditPath, `${JSON.stringify(record)}\n`, 'utf8');
}

test('chart journey: all concrete chart families render through one Electron conversation', async () => {
	test.setTimeout(360_000);
	const dataDir = await mkdtemp(join(tmpdir(), 'fella-electron-chart-families-'));
	const workspace = join(dataDir, 'workspace');
	await mkdir(workspace);
	await mkdir(screenshotDir, { recursive: true });
	await writeFile(join(workspace, 'chart_observations.csv'), observationCsv, 'utf8');
	await writeFile(join(workspace, 'projection.csv'), `period,observed,forecast,lower,upper\n${projectionRows}\n`, 'utf8');
	await writeFile(auditPath, '', 'utf8');
	await recordAudit({
		record_type: 'journey_start',
		journey: 'chart-family-sweep',
		started_at: new Date().toISOString(),
		app: 'Electron + Rust sidecar + Svelte renderer',
		model: 'local OpenAI-wire mock',
		workspace_files: ['chart_observations.csv (36 rows)', 'projection.csv (15 rows)'],
		families: chartCases.map(({ id, kind, sourceFile, question, args, markers }) => ({ id, kind, source_file: sourceFile, question, args, markers })),
		acceptance: [
			'one submitted user turn and one settled assistant turn for every prompt',
			'exactly one visible chart card in each assistant turn',
			'family-specific non-empty marks and accessible chart visual',
			'non-empty exact-values disclosure',
			'inspectable source filename and exact SQL in Analysis Details',
			'chart card and visual remain within their containers; SVG text remains in its viewBox',
			'no uncaught renderer exceptions'
		],
		grading_scope: 'harness/render integration only; no semantic chart choice or numerical answer correctness grading'
	});

	const requestLog = [];
	const caseRequestCounts = Object.fromEntries(chartCases.map(({ id }) => [id, 0]));
	const server = createServer(async (request, response) => {
		const chunks = [];
		for await (const chunk of request) chunks.push(chunk);
		if (request.method !== 'POST' || request.url !== '/v1/chat/completions') {
			response.writeHead(404).end();
			return;
		}
		let body;
		try { body = JSON.parse(Buffer.concat(chunks).toString('utf8')); } catch { body = null; }
		const found = caseForRequest(body);
		if (!found) {
			response.writeHead(400).end('Request did not include one of the declared chart journey prompts.');
			return;
		}
		const { chartCase, userMessageIndex } = found;
		const messages = body.messages;
		const priorChartCall = messages.slice(userMessageIndex + 1).some((message) =>
			message?.role === 'assistant' && Array.isArray(message.tool_calls) &&
			message.tool_calls.some((call) => call?.function?.name === 'make_chart')
		);
		const phase = priorChartCall ? 'final' : 'chart_tool';
		caseRequestCounts[chartCase.id] += 1;
		const priorToolResult = messages.slice(userMessageIndex + 1).findLast((message) => message?.role === 'tool');
		const toolResultFailed = /could not|couldn't|didn't work|failed|error/i.test(contentText(priorToolResult?.content));
		requestLog.push({
			case_id: chartCase.id,
			phase,
			model: body?.model,
			request_number_for_case: caseRequestCounts[chartCase.id],
			streaming: body?.stream === true,
			prior_tool_result_failed: phase === 'final' ? toolResultFailed : null
		});

		response.writeHead(200, {
			'content-type': 'text/event-stream; charset=utf-8',
			'cache-control': 'no-cache',
			connection: 'keep-alive'
		});
		response.flushHeaders();
		if (phase === 'chart_tool' && caseRequestCounts[chartCase.id] === 1) {
			sse(response, {
				choices: [{
					index: 0,
					delta: {
						tool_calls: [{
							index: 0,
							id: `call_chart_${chartCase.id}`,
							type: 'function',
							function: { name: 'make_chart', arguments: JSON.stringify(chartCase.args) }
						}]
					},
					finish_reason: null
				}]
			});
			sse(response, { choices: [{ index: 0, delta: {}, finish_reason: 'tool_calls' }] });
			response.end('data: [DONE]\n\n');
			return;
		}

		sse(response, {
			choices: [{
				index: 0,
				delta: {
					content: toolResultFailed
						? 'The chart request could not be completed; Analysis Details includes the tool response.'
						: `The ${chartCase.kind.replaceAll('_', ' ')} chart is shown above.`
				},
				finish_reason: null
			}]
		});
		sse(response, { choices: [{ index: 0, delta: {}, finish_reason: 'stop' }] });
		response.end('data: [DONE]\n\n');
	});
	await new Promise((done) => server.listen(0, '127.0.0.1', done));
	const address = server.address();
	assert.ok(address && typeof address === 'object');

	const rendererErrors = [];
	const failures = [];
	const passedCaseIds = [];
	const recordedCaseIds = new Set();
	let app;
	let page;
	let stoppedEarly = false;
	let setupFailed = false;
	try {
		app = await launchApp(dataDir);
		page = await app.firstWindow();
		page.on('pageerror', (error) => rendererErrors.push(String(error?.stack || error)));
		await page.waitForLoadState('domcontentloaded');
		await page.setViewportSize({ width: 1280, height: 900 });
		await page.evaluate(async ({ baseUrl }) => {
			await window.fella.invoke('set_api_key', { provider: 'custom', key: 'local-chart-journey' });
			await window.fella.invoke('set_settings', {
				settings: { provider: 'custom', base_url: baseUrl, model: 'local-chart-journey' }
			});
		}, { baseUrl: `http://127.0.0.1:${address.port}/v1` });
		await mountWorkspace(app, page, workspace);

		for (let index = 0; index < chartCases.length; index++) {
			const chartCase = chartCases[index];
			const record = {
				record_type: 'chart_turn',
				sequence: index + 1,
				case_id: chartCase.id,
				kind: chartCase.kind,
				question: chartCase.question,
				status: 'started',
				started_at: new Date().toISOString(),
				tool_arguments: chartCase.args,
				checks: []
			};
			let turnReady = false;
			try {
				const composer = page.getByRole('combobox', { name: /Ask about/ });
				await expect(composer).toBeVisible();
				const previousUserCount = await page.locator('.msg.user').count();
				const previousAssistantCount = await page.locator('.msg.assistant').count();
				await composer.fill(chartCase.question);
				await composer.press('Enter');
				await expect(page.locator('.msg.user')).toHaveCount(previousUserCount + 1, { timeout: 20_000 });
				await expect(page.locator('.msg.user').last()).toContainText(chartCase.question);
				const assistant = page.locator('.msg.assistant').last();
				await expect.poll(async () => {
					if (await page.locator('.msg.assistant').count() !== previousAssistantCount + 1) return false;
					return (await assistant.locator('.text.rich.pending').count()) === 0 &&
						(await assistant.locator('.thinking').count()) === 0;
				}, { timeout: 45_000, intervals: [100, 250, 500] }).toBe(true);
				turnReady = true;
				record.assistant_text = (await assistant.locator('.text.rich').allTextContents()).join('\n').trim();
				record.checks.push({ criterion: 'submitted user turn and assistant response settle in the same conversation', passed: true });

			const card = assistant.locator('.chart-card');
			const chartCount = await card.count();
				record.rendered_chart_count = chartCount;
				assert.equal(chartCount, 1, `${chartCase.id}: expected exactly one visible chart card in its assistant turn`);
				record.checks.push({ criterion: 'exactly one chart card is rendered for this turn', passed: true });
				await expect(card).toBeVisible();
				const title = (await card.locator('.chart-title').innerText()).trim();
				record.rendered_title = title;
				assert.equal(title, chartCase.args.title, `${chartCase.id}: chart title should render`);
				await expect(card.locator('.chart-title')).toBeVisible();
				await expect(card.locator('[role="img"]')).toBeVisible();
				record.checks.push({ criterion: 'chart title and accessible visual render', passed: true });

				const markerResults = [];
				for (const marker of chartCase.markers) {
					const count = await card.locator(marker.selector).count();
					const passed = marker.exact === undefined ? count >= marker.min : count === marker.exact;
					markerResults.push({ selector: marker.selector, count, minimum: marker.min, exact: marker.exact, passed });
					assert.ok(passed, `${chartCase.id}: ${marker.selector} expected ${marker.exact ?? `at least ${marker.min}`}; found ${count}`);
				}
				if (chartCase.pieArcCommands !== undefined) {
					const arcCounts = await card.locator('.pie-slice').evaluateAll((paths) =>
						paths.map((path) => (path.getAttribute('d')?.match(/A/g) ?? []).length)
					);
					const passed = arcCounts.length >= 3 && arcCounts.every((count) => count === chartCase.pieArcCommands);
					markerResults.push({ selector: '.pie-slice SVG arc commands', counts: arcCounts, expectedEach: chartCase.pieArcCommands, passed });
					assert.ok(passed, `${chartCase.id}: pie slices should have ${chartCase.pieArcCommands} arc command(s), got ${arcCounts}`);
				}
				record.family_markers = markerResults;
				record.checks.push({ criterion: `non-empty ${chartCase.kind} family marks render`, passed: true });

				const geometry = await card.evaluate((figure) => {
					const cardRect = figure.getBoundingClientRect();
					const answerRect = figure.parentElement?.getBoundingClientRect();
					const visual = figure.querySelector('.chart, .pie-layout, .boxplot-wrap, .heatmap-scroll');
					const visualRect = visual?.getBoundingClientRect();
					const outOfBoundsSvgText = [];
					for (const svg of figure.querySelectorAll('svg[role="img"]')) {
						const viewBox = svg.viewBox.baseVal;
						const svgRect = svg.getBoundingClientRect();
						for (const label of svg.querySelectorAll('text')) {
							const bounds = label.getBoundingClientRect();
							const left = (bounds.left - svgRect.left) * viewBox.width / svgRect.width;
							const right = (bounds.right - svgRect.left) * viewBox.width / svgRect.width;
							const top = (bounds.top - svgRect.top) * viewBox.height / svgRect.height;
							const bottom = (bounds.bottom - svgRect.top) * viewBox.height / svgRect.height;
							if (left < -2 || top < -2 || right > viewBox.width + 2 || bottom > viewBox.height + 2) {
								outOfBoundsSvgText.push({ text: label.textContent?.trim() ?? '', left, top, right, bottom, width: viewBox.width, height: viewBox.height });
							}
						}
					}
					return {
						card: { width: cardRect.width, height: cardRect.height, scrollWidth: figure.scrollWidth, clientWidth: figure.clientWidth },
						cardWithinAnswer: Boolean(answerRect && cardRect.left >= answerRect.left - 1 && cardRect.right <= answerRect.right + 1),
						visualWithinCard: Boolean(visualRect && visualRect.left >= cardRect.left - 1 && visualRect.right <= cardRect.right + 1 && visualRect.top >= cardRect.top - 1 && visualRect.bottom <= cardRect.bottom + 1),
						outOfBoundsSvgText
					};
				});
				record.geometry = geometry;
				assert.ok(geometry.card.width > 0 && geometry.card.height > 0, `${chartCase.id}: chart card must have visible dimensions`);
				assert.ok(geometry.card.scrollWidth <= geometry.card.clientWidth + 1, `${chartCase.id}: chart content overflows horizontally: ${JSON.stringify(geometry.card)}`);
				assert.ok(geometry.cardWithinAnswer, `${chartCase.id}: chart card escapes its answer container`);
				assert.ok(geometry.visualWithinCard, `${chartCase.id}: chart visual escapes its card`);
				assert.deepEqual(geometry.outOfBoundsSvgText, [], `${chartCase.id}: SVG axis/label text extends beyond its viewBox`);
				record.checks.push({ criterion: 'chart card, plot, and SVG text remain inside their containers', passed: true });
				if (chartCase.id === 'heatmap') {
					const cornerHeader = card.locator('.heatmap thead th:first-child');
					const header = await cornerHeader.evaluate((element) => ({
						text: element.textContent?.trim() ?? '',
						title: element.getAttribute('title') ?? '',
						clientWidth: element.clientWidth,
						scrollWidth: element.scrollWidth
					}));
					record.heatmap_corner_header = header;
					assert.equal(header.text, 'daypart / weekday', 'heatmap should name both dimensions in its corner header');
					assert.equal(header.title, header.text, 'the full heatmap dimension label should remain available as a tooltip');
					assert.ok(header.scrollWidth <= header.clientWidth + 1, `heatmap corner header is visually clipped: ${JSON.stringify(header)}`);
					record.checks.push({ criterion: 'heatmap corner label names both dimensions without clipping', passed: true });
				}

				const values = card.locator('details.values');
				await expect(values).toBeVisible();
				if (!(await values.evaluate((node) => node.open))) await values.locator('summary').click();
				const valueRows = await values.locator('tbody tr').evaluateAll((rows) => rows.map((row) =>
					Array.from(row.querySelectorAll('th, td')).map((cell) => cell.textContent?.trim() ?? '')
				));
				assert.ok(valueRows.length > 0, `${chartCase.id}: exact-values disclosure should contain data rows`);
				assert.ok(valueRows.every((row) => row.length >= 2 && row.every((cell) => cell.length > 0)), `${chartCase.id}: exact-values table should have populated cells`);
				record.exact_values_rows = valueRows.length;
				record.checks.push({ criterion: 'exact-values disclosure contains populated rows', passed: true });
				await values.locator('summary').click();

				// Capture the compact card state in both modes. Keeping the exact-value
				// table closed makes these useful as visual artifacts rather than tall
				// transcript crops; the table itself was already inspected above.
				record.screenshots = {};
				for (const mode of ['light', 'dark']) {
					await page.evaluate((colorMode) => { document.documentElement.dataset.colorMode = colorMode; }, mode);
					const path = join(screenshotDir, `${String(index + 1).padStart(2, '0')}-${chartCase.id}-${mode}.png`);
					await card.screenshot({ path });
					record.screenshots[mode] = path;
				}
				await page.evaluate(() => { delete document.documentElement.dataset.colorMode; });

				const detailsButton = assistant.getByRole('button', { name: 'Analysis details', exact: true });
				await expect(detailsButton).toBeVisible();
				if (await detailsButton.getAttribute('aria-expanded') !== 'true') await detailsButton.click();
				await expect(assistant.locator('.evidence-body')).toBeVisible();
				record.recorded_source_file = chartCase.sourceFile;
				await expect(assistant.locator('.evidence-body')).toContainText(chartCase.sourceFile);
				const queryToggle = assistant.getByRole('button', { name: /show the query/i });
				await expect(queryToggle).toBeVisible();
				await queryToggle.click();
				await expect(assistant.locator('.evidence-body')).toContainText(chartCase.args.sql);
				record.source_and_query_visible = true;
				record.checks.push({ criterion: 'Analysis Details exposes the input source and exact chart SQL', passed: true });

				record.renderer_errors = [...rendererErrors];
				assert.equal(rendererErrors.length, 0, `uncaught renderer errors: ${rendererErrors.join('\n')}`);
				record.checks.push({ criterion: 'no uncaught renderer exception has occurred', passed: true });
				record.status = 'pass';
				record.completed_at = new Date().toISOString();
				passedCaseIds.push(chartCase.id);
			} catch (error) {
				record.status = 'fail';
				record.error = String(error?.stack || error);
				record.renderer_errors = [...rendererErrors];
				try {
					const assistant = page.locator('.msg.assistant').last();
					record.assistant_dom_text = (await assistant.innerText()).slice(0, 8000);
					const detailsButton = assistant.getByRole('button', { name: 'Analysis details', exact: true });
					if (await detailsButton.count() && await detailsButton.getAttribute('aria-expanded') !== 'true') await detailsButton.click();
					const evidenceBody = assistant.locator('.evidence-body');
					if (await evidenceBody.count()) {
						for (const toggle of await assistant.locator('.steps .detailtoggle').all()) {
							if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
						}
						record.analysis_details = (await evidenceBody.innerText()).slice(0, 12_000);
						record.evidence_steps = await assistant.locator('.steps .step').allTextContents();
					}
				} catch (captureError) {
					record.failure_capture_error = String(captureError?.stack || captureError);
				}
				failures.push({ case_id: chartCase.id, error: record.error });
				if (!turnReady) {
					stoppedEarly = true;
					record.stop_reason = 'assistant turn did not settle; remaining prompts were not submitted into a potentially busy conversation';
				}
			} finally {
				await recordAudit(record);
				recordedCaseIds.add(chartCase.id);
			}
			if (stoppedEarly) break;
		}
	} catch (error) {
		setupFailed = true;
		failures.push({ case_id: 'journey_setup', error: String(error?.stack || error) });
		await recordAudit({ record_type: 'journey_setup_failure', error: String(error?.stack || error), renderer_errors: rendererErrors });
	} finally {
		for (const chartCase of chartCases) {
			if (recordedCaseIds.has(chartCase.id)) continue;
			const reason = setupFailed
				? 'journey setup failed before this chart turn was recorded'
				: stoppedEarly
					? 'journey stopped because an earlier assistant turn did not settle'
					: 'journey exited before this chart turn was submitted';
			await recordAudit({ record_type: 'chart_turn', case_id: chartCase.id, kind: chartCase.kind, question: chartCase.question, status: 'not_run', reason });
		}
		await recordAudit({
			record_type: 'journey_summary',
			completed_at: new Date().toISOString(),
			status: failures.length === 0 && !stoppedEarly ? 'pass' : 'fail',
			passed_families: passedCaseIds.length,
			passed_case_ids: passedCaseIds,
			requested_families: chartCases.length,
			request_counts: caseRequestCounts,
			model_requests: requestLog,
			failures,
			renderer_errors: rendererErrors,
			screenshot_directory: screenshotDir
		});
		try {
			await app?.close();
		} finally {
			await new Promise((done) => server.close(done));
			await rm(dataDir, { recursive: true, force: true });
		}
	}

	assert.equal(failures.length, 0, `chart journey failures: ${JSON.stringify(failures, null, 2)}; audit: ${auditPath}`);
	assert.equal(stoppedEarly, false, `chart journey stopped early; audit: ${auditPath}`);
	assert.deepEqual(
		Object.fromEntries(Object.entries(caseRequestCounts).sort()),
		Object.fromEntries(chartCases.map(({ id }) => [id, 2]).sort(([left], [right]) => left.localeCompare(right))),
		`each prompt must produce exactly one make_chart request and one final-answer request; audit: ${auditPath}`
	);
});
