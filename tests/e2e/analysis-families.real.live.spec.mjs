import assert from 'node:assert/strict';
import {
	access,
	appendFile,
	chmod,
	copyFile,
	mkdtemp,
	mkdir,
	readFile,
	rm,
	writeFile
} from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test, _electron as electron } from '@playwright/test';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const workspace = join(
	root,
	'bench',
	'fqa-bench',
	'suites',
	'uci-bike-sharing',
	'workspaces',
	'capital-bikeshare'
);
const runId = new Date().toISOString().replaceAll(':', '-').replaceAll('.', '-');
const journeyDefinitionVersion = 5;
const resultDir = join(root, 'test-results', 'e2e-analysis-families', runId);
const auditPath = join(resultDir, 'journey.jsonl');
const screenshotDir = join(resultDir, 'screenshots');
const records = [];

// Prompts are intentionally identical for both providers. They name an
// analysis goal and request a chart, but don't prescribe a chart type, SQL,
// filename, or answer. This is an operational journey, not a correctness
// benchmark: generated answers and chart choices are recorded, not graded.
const cases = [
	{
		id: 'time-series',
		family: 'time-series trend',
		prompt: 'How did rentals change month by month across the full two-year archive? Show the complete pattern in a chart.'
	},
	{
		id: 'composition',
		family: 'part-to-whole composition',
		prompt: 'For 2012, how were rentals split between casual and registered riders? Show each group’s share in a chart.'
	},
	{
		id: 'distribution',
		family: 'distribution',
		prompt: 'Across the full archive, what does the distribution of daily rental counts look like? Visualize the spread and unusually high or low days.'
	},
	{
		id: 'relationship',
		family: 'relationship between measures',
		prompt: 'Across the archive, do warmer days tend to have more rentals? Show the relationship between the temperature reading and daily rentals.'
	},
	{
		id: 'segmented-comparison',
		family: 'segmented comparison over time',
		prompt: 'How do average daily rentals on working and non-working days compare month by month across both years? Show both patterns together.'
	},
	{
		id: 'forecast',
		family: 'forecast',
		prompt: 'Pretend it is June 30, 2011. Using only observed monthly rental totals from January through June 2011, estimate July 2011 with their arithmetic mean. Plot the six observed months and July’s estimate together, clearly marking July as a forecast rather than an observation.'
	}
];

const providers = [
	{
		id: 'openai-luna',
		provider: 'openai',
		model: 'gpt-5.6-luna',
		credential: 'apikey:openai'
	},
	{
		id: 'ollama-gemma4-31b',
		provider: 'ollama-cloud',
		model: 'gemma4:31b',
		credential: 'apikey:ollama-cloud'
	}
];

function authPath() {
	if (process.env.FELLA_E2E_AUTH_FILE) return resolve(process.env.FELLA_E2E_AUTH_FILE);
	if (process.platform === 'win32') {
		return join(process.env.APPDATA || join(homedir(), 'AppData', 'Roaming'), 'dev.fella.app', 'auth.json');
	}
	if (process.platform === 'darwin') {
		return join(homedir(), 'Library', 'Application Support', 'dev.fella.app', 'auth.json');
	}
	return join(process.env.XDG_DATA_HOME || join(homedir(), '.local', 'share'), 'dev.fella.app', 'auth.json');
}

function sidecarPath() {
	const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
	const suffix = process.platform === 'win32' ? '.exe' : '';
	return join(root, 'electron', 'engine', `fella-engine-${arch}${suffix}`);
}

async function appendAudit(record) {
	records.push(record);
	await appendFile(auditPath, `${JSON.stringify(record)}\n`, 'utf8');
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

async function configureProvider(page, dataDir, provider) {
	const credentialPath = authPath();
	const auth = JSON.parse(await readFile(credentialPath, 'utf8'));
	assert.equal(typeof auth[provider.credential], 'string', `${provider.credential} is missing from the configured auth.json`);
	const privateAuthPath = join(dataDir, 'auth.json');
	await copyFile(credentialPath, privateAuthPath);
	if (process.platform !== 'win32') await chmod(privateAuthPath, 0o600);

	const settings = await page.evaluate((selection) => window.fella.invoke('set_settings', {
		settings: { provider: selection.provider, model: selection.model }
	}), provider);
	assert.equal(settings.provider, provider.provider);
	assert.equal(settings.model, provider.model);
	assert.equal(settings.has_credential, true);
	const health = await page.evaluate(() => window.fella.invoke('provider_health'));
	return {
		settings: {
			provider: settings.provider,
			model: settings.model,
			has_credential: settings.has_credential
		},
		provider_health: health
	};
}

async function mountWorkspace(page, app) {
	await app.evaluate(({ dialog }, folder) => {
		dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
	}, workspace);
	const addRepository = page.getByRole('button', { name: 'Add a repository' });
	if (await addRepository.isVisible().catch(() => false)) await addRepository.click();
	else await page.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('combobox', { name: /Ask about/ })).toBeVisible();
}

async function inspectChartCards(assistant, providerId, caseId) {
	const cards = assistant.locator('.chart-card');
	const charts = [];
	for (let index = 0; index < await cards.count(); index++) {
		const card = cards.nth(index);
		const title = (await card.locator('.chart-title').innerText().catch(() => '')).trim();
		const geometry = await card.evaluate((figure) => {
			const cardBounds = figure.getBoundingClientRect();
			const parentBounds = figure.parentElement?.getBoundingClientRect();
			const visual = figure.querySelector('.chart, .pie-layout, .boxplot-wrap, .heatmap-scroll');
			const visualBounds = visual?.getBoundingClientRect();
			const svg = figure.querySelector('svg[role="img"]');
			const viewBox = svg?.viewBox.baseVal;
			const axisLabels = svg ? [...svg.querySelectorAll('text.axis-label')] : [];
			const isJuly = (value) => /(?:\bjul(?:y)?\b|\b\d{4}-07\b)/i.test(value ?? '');
			const isForecastJuly = (value) => /\bforecast\b/i.test(value ?? '') && isJuly(value);
			const forecastMarkerTitles = [...figure.querySelectorAll('circle.line-dot title')]
				.map((title) => title.textContent?.trim() ?? '')
				.filter(isForecastJuly);
			const forecastLegend = [...figure.querySelectorAll('.chart-legend .legend-item')]
				.map((item) => item.textContent?.trim() ?? '');
			const forecastSeriesIndex = forecastLegend.findIndex((label) => /^forecast$/i.test(label));
			const julyForecastRows = [...figure.querySelectorAll('.row')].filter((row) => {
				const rowLabel = row.querySelector('.row-label')?.textContent?.trim() ?? '';
				if (!isJuly(rowLabel) || forecastSeriesIndex < 0) return false;
				const forecastTrack = row.querySelectorAll('.track-line')[forecastSeriesIndex];
				return Boolean(forecastTrack?.querySelector('.fill'));
			});
			const forecastMarks = forecastMarkerTitles.length + julyForecastRows.length;
			const outOfBoundsLabels = svg && viewBox
				? [...svg.querySelectorAll('text')].flatMap((label) => {
					try {
						// getBBox() is in the text node's local coordinates. Transform
						// its corners back into the SVG viewBox so rotated axis titles
						// are measured where they actually render.
						const bounds = label.getBBox();
						const labelToScreen = label.getScreenCTM();
						const screenToSvg = svg.getScreenCTM()?.inverse();
						if (!labelToScreen || !screenToSvg) throw new Error('SVG transform is unavailable');
						const corners = [
							[bounds.x, bounds.y],
							[bounds.x + bounds.width, bounds.y],
							[bounds.x, bounds.y + bounds.height],
							[bounds.x + bounds.width, bounds.y + bounds.height]
						].map(([x, y]) => new DOMPoint(x, y).matrixTransform(labelToScreen).matrixTransform(screenToSvg));
						const left = Math.min(...corners.map((point) => point.x));
						const top = Math.min(...corners.map((point) => point.y));
						const right = Math.max(...corners.map((point) => point.x));
						const bottom = Math.max(...corners.map((point) => point.y));
						const within = left >= -2 && top >= -2 && right <= viewBox.width + 2 && bottom <= viewBox.height + 2;
						return within ? [] : [{ text: label.textContent?.trim() ?? '', left, top, right, bottom }];
					} catch {
						return [{ text: label.textContent?.trim() ?? '', measurement_failed: true }];
					}
				})
				: [];
			let rendered_shape = 'unrecognized';
			if (figure.querySelector('.heatmap-scroll')) rendered_shape = 'heatmap';
			else if (figure.querySelector('.boxplot-wrap')) rendered_shape = 'box plot';
			else if (figure.querySelector('.pie-layout')) rendered_shape = 'pie/donut';
			else if (figure.querySelector('.scatter-dot')) rendered_shape = 'scatter';
			else if (figure.querySelector('.forecast-band')) rendered_shape = 'forecast line with interval';
			else if (figure.querySelector('.area-fill')) rendered_shape = 'area';
			else if (figure.querySelector('path.line, .line-dot')) rendered_shape = 'line';
			else if (figure.querySelector('.rows .row')) rendered_shape = 'bar/histogram';
			const marks = figure.querySelectorAll(
				'.rows .row, .pie-slice, .scatter-dot, .box-range, .outlier-dot, .heat-cell, .line-dot, .area-fill, .forecast-band'
			).length + [...figure.querySelectorAll('svg path.line')].filter((path) => (path.getAttribute('d') ?? '').length > 1).length;
			return {
				rendered_shape,
				card_width: figure.clientWidth,
				card_scroll_width: figure.scrollWidth,
				card_within_answer: Boolean(parentBounds && cardBounds.left >= parentBounds.left - 1 && cardBounds.right <= parentBounds.right + 1),
				visual_present: Boolean(visual),
				visual_within_card: Boolean(visualBounds && visualBounds.left >= cardBounds.left - 1 && visualBounds.right <= cardBounds.right + 1),
				visible_mark_count: marks,
				forecast_mark_count: forecastMarks,
				forecast_marker_titles: forecastMarkerTitles,
				july_forecast_bar_count: julyForecastRows.length,
				forecast_series_labels: forecastLegend,
				visible_period_labels: axisLabels.map((label) => label.textContent?.trim() ?? ''),
				visible_label_count: figure.querySelectorAll('.row-label, .axis-label, .pie-label, .box-label, .heatmap th').length,
				svg_text_contained: outOfBoundsLabels.length === 0,
				out_of_bounds_labels: outOfBoundsLabels
			};
		});

		const valuesDisclosure = card.locator('details.values');
		let values = { headings: [], row_count: 0, sample_rows: [] };
		if (await valuesDisclosure.count()) {
			const isOpen = await valuesDisclosure.evaluate((node) => node.open);
			if (!isOpen) await valuesDisclosure.locator('summary').click();
			values = await valuesDisclosure.evaluate((details) => {
				const headings = [...details.querySelectorAll('thead th')].map((cell) => cell.textContent?.trim() ?? '');
				const body = details.querySelector('tbody');
				if (!body) return { headings, row_count: 0, sample_rows: [] };
				const rows = [...body.querySelectorAll('tr')].map((row) =>
					[...row.querySelectorAll('th, td')].map((cell) => cell.textContent?.trim() ?? '')
				);
				return { headings, row_count: rows.length, sample_rows: rows.slice(0, 20) };
			}).catch(() => values);
			if (!isOpen) await valuesDisclosure.locator('summary').click();
		}

		const screenshots = {};
		for (const mode of ['light', 'dark']) {
			await assistant.page().evaluate((colorMode) => {
				document.documentElement.dataset.colorMode = colorMode;
			}, mode);
			await card.evaluate((figure) => figure.scrollIntoView({ block: 'start', inline: 'nearest', behavior: 'instant' }));
			const imagePath = join(screenshotDir, `${providerId}-${caseId}-chart-${index + 1}-${mode}.png`);
			await card.screenshot({ path: imagePath });
			screenshots[mode] = imagePath;
		}
		const textRows = await card.locator('.row-label, .pie-label, .box-label, .heatmap th, .axis-label').allTextContents();
		charts.push({
			title,
			geometry,
			values,
			text_labels: textRows.map((label) => label.trim()).filter(Boolean).slice(0, 80),
			screenshots
		});
	}
	await assistant.page().evaluate(() => { delete document.documentElement.dataset.colorMode; });
	return charts;
}

async function captureAnalysis(assistant) {
	const disclosure = assistant.getByRole('button', { name: 'Analysis details', exact: true });
	if (await disclosure.count() === 0) {
		return { available: false, text: '', evidence_steps: [], model_calls: [] };
	}
	if (await disclosure.getAttribute('aria-expanded') !== 'true') await disclosure.click();
	const body = assistant.locator('.evidence-body');
	await expect(body).toBeVisible();
	for (const toggle of await assistant.locator('.steps .detailtoggle').all()) {
		if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
	}
	const evidenceSteps = [];
	for (const step of await assistant.locator('.steps .step').all()) {
		evidenceSteps.push({
			label: (await step.locator('.line').innerText().catch(() => '')).trim(),
			failed: await step.evaluate((node) => node.classList.contains('failed')),
			language: (await step.locator('.language-badge').allTextContents()).map((value) => value.trim()),
			detail: (await step.innerText()).trim()
		});
	}
	let modelCalls = [];
	const timings = assistant.getByRole('button', { name: /show model timings/i });
	if (await timings.count()) {
		await timings.click();
		modelCalls = (await assistant.locator('.model-calls li').allTextContents()).map((value) => value.trim());
	}
	return { available: true, text: (await body.innerText()).trim(), evidence_steps: evidenceSteps, model_calls: modelCalls };
}

async function runTurn(page, provider, chartCase, rendererErrors) {
	const startedAt = new Date().toISOString();
	const start = Date.now();
	const beforeErrors = rendererErrors.length;
	const priorUserCount = await page.locator('.msg.user').count();
	const priorAssistantCount = await page.locator('.msg.assistant').count();
	const composer = page.getByRole('combobox', { name: /Ask about/ });
	let assistant = null;
	let responseText = '';
	let analysis = { available: false, text: '', evidence_steps: [], model_calls: [] };
	let charts = [];
	let clarification = null;
	let error = null;
	let submitted = false;
	let settled = false;
	try {
		await composer.fill(chartCase.prompt);
		await composer.press('Enter');
		await expect(page.locator('.msg.user')).toHaveCount(priorUserCount + 1, { timeout: 15_000 });
		const user = page.locator('.msg.user').last();
		await expect(user).toContainText(chartCase.prompt);
		submitted = true;
		assistant = page.locator('.msg.assistant').nth(priorAssistantCount);
		await expect.poll(async () => {
			if (await page.locator('.msg.assistant').count() <= priorAssistantCount) return false;
			return !(await assistant.locator('.text.rich.pending').count()) && !(await assistant.locator('.thinking').count());
		}, { timeout: 240_000, intervals: [500, 1000, 2000] }).toBe(true);
		settled = true;
		const richText = await assistant.locator('.text.rich').allTextContents();
		responseText = richText.join('\n\n').trim() || (await assistant.innerText()).trim();
		const clarificationField = page.locator('.dock .clarification-field');
		if (await clarificationField.count() && await clarificationField.isVisible()) {
			clarification = {
				question: (await clarificationField.locator('.clarification-question').innerText().catch(() => '')).trim(),
				options: (await clarificationField.locator('.clarification-options button').allTextContents()).map((option) => option.trim())
			};
		}
		analysis = await captureAnalysis(assistant);
		charts = await inspectChartCards(assistant, provider.id, chartCase.id);
	} catch (cause) {
		error = String(cause?.stack || cause);
		if (assistant && await assistant.count().catch(() => 0)) {
			responseText = (await assistant.locator('.text.rich').allTextContents().catch(() => [])).join('\n\n').trim();
			if (!responseText) responseText = await assistant.innerText().catch(() => '');
			charts = await inspectChartCards(assistant, provider.id, chartCase.id).catch(() => []);
		}
	}

	const checks = [
		{ criterion: 'the exact prompt appears as a submitted user turn', passed: submitted },
		{ criterion: 'an assistant turn settles without a stuck thinking state', passed: settled },
		{ criterion: 'at least one chart is rendered for the requested visualization', passed: charts.length > 0 },
		{ criterion: 'every rendered chart has a title and visible data marks', passed: charts.length > 0 && charts.every((chart) => chart.title && chart.geometry.visible_mark_count > 0) },
		{ criterion: 'the forecast value is visibly marked at July as a forecast', passed: chartCase.id !== 'forecast' || charts.some((chart) => chart.geometry.forecast_mark_count > 0) },
		{ criterion: 'chart visuals stay within their answer and card containers', passed: charts.length > 0 && charts.every((chart) => chart.geometry.visual_present && chart.geometry.card_within_answer && chart.geometry.visual_within_card && chart.geometry.card_scroll_width <= chart.geometry.card_width + 1) },
		{ criterion: 'SVG text labels stay within the chart viewBox when applicable', passed: charts.length > 0 && charts.every((chart) => chart.geometry.svg_text_contained) },
		{ criterion: 'no exact duplicate rendered charts appear in this answer', passed: new Set(charts.map((chart) => JSON.stringify({ title: chart.title, shape: chart.geometry.rendered_shape, labels: chart.text_labels, values: chart.values.sample_rows }))).size === charts.length },
		{ criterion: 'Analysis Details is available to inspect the run', passed: analysis.available },
		{ criterion: 'no uncaught renderer exception occurs during this turn', passed: rendererErrors.length === beforeErrors }
	];
	const duplicateCount = charts.length - new Set(charts.map((chart) => JSON.stringify({ title: chart.title, shape: chart.geometry.rendered_shape, labels: chart.text_labels, values: chart.values.sample_rows }))).size;
	const record = {
		record_type: 'analysis_family_turn',
		schema_version: 1,
		journey: provider.id,
		provider: provider.provider,
		model: provider.model,
		case_id: chartCase.id,
		analysis_family: chartCase.family,
		started_at: startedAt,
		elapsed_ms: Date.now() - start,
		input: { exact_prompt: chartCase.prompt },
		response: {
			text: responseText,
			clarification,
			analysis_details: analysis.text,
			evidence_steps: analysis.evidence_steps,
			evidence_step_count: analysis.evidence_steps.length,
			failed_evidence_step_count: analysis.evidence_steps.filter((step) => step.failed).length,
			model_call_timings: analysis.model_calls,
			chart_count: charts.length,
			duplicate_chart_count: duplicateCount,
			charts,
			renderer_errors: rendererErrors.slice(beforeErrors)
		},
		checks,
		result: error ? 'error' : checks.every((check) => check.passed) ? 'pass' : 'fail',
		...(error ? { operational_error: error } : {}),
		answer_correctness_scored: false
	};
	await appendAudit(record);
	return { record, clarification, unavailable: /can't reach|model service returned an error|error invoking remote method|no model chosen|invalid_api_key|authentication_error|model_not_found|404 not found/i.test(responseText) };
}

async function runJourney(provider) {
	await mkdir(resultDir, { recursive: true });
	await mkdir(screenshotDir, { recursive: true });
	const dataDir = await mkdtemp(join(tmpdir(), `fella-e2e-${provider.id}-`));
	let app;
	let stopReason = null;
	let setupError = null;
	const rendererErrors = [];
	const turnResults = [];
	const notRun = [];
	await appendAudit({
		record_type: 'journey_start',
		schema_version: 1,
		journey_definition_version: journeyDefinitionVersion,
		journey: provider.id,
		started_at: new Date().toISOString(),
		app: 'Electron + Rust engine + Svelte renderer',
		provider: provider.provider,
		model: provider.model,
		workspace: 'FQA-Bench UCI Bike Sharing / Capital Bikeshare (daily.csv, monthly-usage-from-hourly.csv, field-guide.md)',
		workspace_source_rows: { daily: 731, monthly: 24 },
		workspace_is_same_for_both_providers: true,
		case_order: cases.map(({ id, family, prompt }) => ({ id, family, prompt })),
		acceptance: [
			'each fixed prompt is submitted through the real Electron UI and produces a settled assistant turn',
			'each chart request renders at least one titled chart with visible data marks',
			'chart geometry remains within its UI containers; SVG text remains within the viewBox',
			'no exact duplicate charts or uncaught renderer exceptions',
			'Analysis Details and generated evidence/tool steps are inspectable',
			'answers, chart-type suitability, and numerical correctness are recorded but not graded'
		],
		grading_scope: 'real-provider harness/UI/render journey only; not answer-correctness scoring',
		credential_values_recorded: false
	});

	try {
		app = await launchApp(dataDir);
		const page = await app.firstWindow();
		page.on('pageerror', (error) => rendererErrors.push(String(error?.stack || error)));
		await page.waitForLoadState('domcontentloaded');
		const providerSetup = await configureProvider(page, dataDir, provider);
		await mountWorkspace(page, app);
		await appendAudit({ record_type: 'provider_setup', journey: provider.id, ...providerSetup });

		for (let index = 0; index < cases.length; index++) {
			const chartCase = cases[index];
			if (stopReason) {
				notRun.push({ case_id: chartCase.id, reason: stopReason });
				continue;
			}
			const result = await runTurn(page, provider, chartCase, rendererErrors);
			turnResults.push(result.record);
			if (!result.record.checks.find((check) => check.criterion === 'an assistant turn settles without a stuck thinking state')?.passed) {
				stopReason = 'assistant turn did not settle; later prompts were not submitted into a potentially busy conversation';
			} else if (result.unavailable) {
				stopReason = 'provider/model error surfaced in the answer; remaining prompts were not sent to avoid repeating a known connection or model failure';
			} else if (result.clarification) {
				stopReason = 'the model requested clarification; later prompts were not submitted over an unresolved clarification';
			}
		}
	} catch (cause) {
		setupError = String(cause?.stack || cause);
		await appendAudit({ record_type: 'journey_setup_failure', journey: provider.id, error: setupError });
	} finally {
		if (setupError) {
			for (const chartCase of cases) {
				if (!turnResults.some((turn) => turn.case_id === chartCase.id) && !notRun.some((item) => item.case_id === chartCase.id)) {
					notRun.push({ case_id: chartCase.id, reason: 'journey setup failed before the prompt was submitted' });
				}
			}
		}
		for (const item of notRun) {
			await appendAudit({ record_type: 'analysis_family_turn', journey: provider.id, case_id: item.case_id, result: 'not_run', reason: item.reason });
		}
		await appendAudit({
			record_type: 'journey_summary',
			journey: provider.id,
			provider: provider.provider,
			model: provider.model,
			finished_at: new Date().toISOString(),
			turns_passed: turnResults.filter((turn) => turn.result === 'pass').length,
			turns_failed: turnResults.filter((turn) => turn.result !== 'pass').length + (setupError ? 1 : 0),
			turns_not_run: notRun.length,
			evidence_steps_total: turnResults.reduce((sum, turn) => sum + turn.response.evidence_step_count, 0),
			failed_evidence_steps_total: turnResults.reduce((sum, turn) => sum + turn.response.failed_evidence_step_count, 0),
			charts_rendered_total: turnResults.reduce((sum, turn) => sum + turn.response.chart_count, 0),
			provider_setup_error: setupError,
			stop_reason: stopReason,
			renderer_errors: rendererErrors,
			audit_file: auditPath,
			screenshots: screenshotDir,
			answer_correctness_scored: false
		});
		try {
			await app?.close();
		} finally {
			await rm(dataDir, { recursive: true, force: true });
		}
	}
	const failures = turnResults.filter((turn) => turn.result !== 'pass');
	assert.equal(setupError, null, `${provider.id} journey setup failed: ${setupError}; audit: ${auditPath}`);
	assert.equal(notRun.length, 0, `${provider.id} journey left prompts unrun: ${JSON.stringify(notRun)}; audit: ${auditPath}`);
	assert.equal(failures.length, 0, `${provider.id} journey checks failed: ${JSON.stringify(failures.map((turn) => ({ case_id: turn.case_id, checks: turn.checks })), null, 2)}; audit: ${auditPath}`);
}

test.beforeAll(async () => {
	await mkdir(resultDir, { recursive: true });
	await mkdir(screenshotDir, { recursive: true });
	await writeFile(auditPath, '', 'utf8');
	const credentialFile = authPath();
	const auth = JSON.parse(await readFile(credentialFile, 'utf8'));
	for (const provider of providers) {
		assert.equal(typeof auth[provider.credential], 'string', `${provider.credential} is missing from the configured auth.json`);
	}
	await appendAudit({
		record_type: 'journey_run',
		schema_version: 1,
		journey_definition_version: journeyDefinitionVersion,
		run_id: runId,
		started_at: new Date().toISOString(),
		providers: providers.map(({ id, provider, model }) => ({ id, provider, model })),
		workspace: 'FQA-Bench UCI Bike Sharing / Capital Bikeshare',
		question_count_per_provider: cases.length,
		methodology_note: 'v5 corrects a v4 false positive: an endpoint marker is not enough; the visible July mark must be explicitly identified as a forecast. This is a locator correction only; prompts and intended behavior are unchanged. v3 added SVG line-mark accounting and a forecast-mark criterion.',
		credential_values_recorded: false,
		answer_correctness_scored: false,
		audit_file: auditPath
	});
});

test.afterAll(async () => {
	const turnRecords = records.filter((record) => record.record_type === 'analysis_family_turn' && record.result !== 'not_run');
	await appendAudit({
		record_type: 'journey_run_summary',
		run_id: runId,
		finished_at: new Date().toISOString(),
		turns_passed: turnRecords.filter((record) => record.result === 'pass').length,
		turns_failed: turnRecords.filter((record) => record.result !== 'pass').length,
		turns_not_run: records.filter((record) => record.record_type === 'analysis_family_turn' && record.result === 'not_run').length,
		provider_summaries: records.filter((record) => record.record_type === 'journey_summary'),
		answer_correctness_scored: false,
		audit_file: auditPath,
		screenshots: screenshotDir
	});
	console.log(`Real-model chart journey audit: ${auditPath}`);
	console.log(`Real-model chart journey screenshots: ${screenshotDir}`);
});

for (const provider of providers) {
	test(`${provider.id}: real-model journey across charted analysis families`, async () => {
		test.setTimeout(1_500_000);
		await runJourney(provider);
	});
}
