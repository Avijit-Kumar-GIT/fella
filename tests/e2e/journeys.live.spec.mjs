import assert from 'node:assert/strict';
import {
	access,
	appendFile,
	chmod,
	copyFile,
	mkdtemp,
	mkdir,
	readFile,
	rm
} from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test, _electron as electron } from '@playwright/test';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const bikeSuite = join(root, 'bench/fqa-bench/suites/uci-bike-sharing');
const housingSuite = join(root, 'bench/fqa-bench/suites/clarification-housing');
const auditId = new Date().toISOString().replaceAll(':', '-').replaceAll('.', '-');
const auditPath = process.env.FELLA_E2E_AUDIT_FILE
	? resolve(process.env.FELLA_E2E_AUDIT_FILE)
	: join(root, 'test-results', 'e2e-journeys', `${auditId}.jsonl`);
const screenshotDir = auditPath.replace(/\.jsonl$/i, '-screenshots');
const model = 'gpt-5.6-luna';
const records = [];

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

async function launchIsolatedApp(dataDir) {
	await access(sidecarPath()).catch(() => {
		throw new Error('Electron sidecar is missing; run pnpm electron:build first.');
	});
	return electron.launch({
		args: [join(root, 'electron', 'main.mjs'), `--user-data-dir=${join(dataDir, 'chromium')}`],
		cwd: root,
		env: { ...process.env, FELLA_DATA_DIR: dataDir, FELLA_ENGINE_PATH: sidecarPath() }
	});
}

async function configureProvider(page, dataDir) {
	const credentialFile = authPath();
	const auth = JSON.parse(await readFile(credentialFile, 'utf8'));
	assert.equal(typeof auth['apikey:openai'], 'string', `OpenAI credential missing from ${credentialFile}`);
	const privateAuthPath = join(dataDir, 'auth.json');
	await copyFile(credentialFile, privateAuthPath);
	if (process.platform !== 'win32') await chmod(privateAuthPath, 0o600);
	const settings = await page.evaluate(() => window.fella.invoke('set_settings', {
		settings: { provider: 'openai', model: 'gpt-5.6-luna' }
	}));
	assert.equal(settings.provider, 'openai');
	assert.equal(settings.model, model);
	assert.equal(settings.has_credential, true);
}

async function mount(page, app, folder) {
	await app.evaluate(({ dialog }, path) => {
		dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] });
	}, folder);
	const add = page.getByRole('button', { name: 'Add a repository' });
	if (await add.isVisible().catch(() => false)) await add.click();
	else await page.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('combobox', { name: /Ask about/ })).toBeVisible();
}

async function readJsonl(path) {
	return (await readFile(path, 'utf8')).trim().split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line));
}

async function loadTasks(suiteDir) {
	const tasks = await readJsonl(join(suiteDir, 'tasks.jsonl'));
	return new Map(tasks.map((task) => [task.id, task]));
}

async function expandAnalysisDetails(assistant) {
	const disclosure = assistant.getByRole('button', { name: 'Analysis details', exact: true });
	if (await disclosure.count() === 0) return { available: false, text: '', steps: [] };
	if (await disclosure.getAttribute('aria-expanded') !== 'true') await disclosure.click();
	const body = assistant.locator('.evidence-body');
	await expect(body).toBeVisible();
	for (const toggle of await assistant.locator('.steps .detailtoggle').all()) {
		if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
	}
	const steps = await assistant.locator('.steps .step').allTextContents();
	return { available: true, text: await body.innerText(), steps: steps.map((step) => step.trim()) };
}

async function readRenderedCharts(assistant, caseId) {
	const cards = assistant.locator('.chart-card');
	const chartCount = await cards.count();
	const charts = [];
	for (let index = 0; index < chartCount; index++) {
		const card = cards.nth(index);
		const title = (await card.locator('.chart-title').innerText()).trim();
		const geometry = await card.evaluate((figure) => {
			const frame = figure.getBoundingClientRect();
			const parent = figure.parentElement?.getBoundingClientRect();
			const visual = figure.querySelector('.chart, .pie-layout, .boxplot-wrap, .heatmap-scroll, [role="img"]');
			const visualBounds = visual?.getBoundingClientRect();
			const svg = figure.querySelector('svg[role="img"]');
			const viewBox = svg?.viewBox.baseVal;
			const outOfBoundsTexts = svg && viewBox ? [...svg.querySelectorAll('text')].flatMap((label) => {
				try {
					const box = label.getBBox();
					const contained = box.x >= -2 && box.y >= -2 && box.x + box.width <= viewBox.width + 2 && box.y + box.height <= viewBox.height + 2;
					return contained ? [] : [{ text: label.textContent?.trim() ?? '', x: box.x, y: box.y, right: box.x + box.width, bottom: box.y + box.height }];
				} catch { return [{ text: label.textContent?.trim() ?? '', measurementFailed: true }]; }
			}) : [];
			const svgTextContained = outOfBoundsTexts.length === 0;
			return {
				cardWidth: figure.clientWidth,
				cardScrollWidth: figure.scrollWidth,
				cardWithinAnswer: Boolean(parent && frame.left >= parent.left - 0.5 && frame.right <= parent.right + 0.5),
				visualWithinCard: Boolean(visualBounds && visualBounds.left >= frame.left - 0.5 && visualBounds.right <= frame.right + 0.5),
				svgTextContained,
				outOfBoundsTexts,
				visibleLabelCount: figure.querySelectorAll('.row-label, .axis-label, .pie-label, .box-label, .heatmap th').length
			};
		});
		const screenshots = {};
		for (const mode of ['light', 'dark']) {
			await assistant.page().evaluate((colorMode) => { document.documentElement.dataset.colorMode = colorMode; }, mode);
			const screenshotPath = join(screenshotDir, `${caseId}-${mode}.png`);
			await card.screenshot({ path: screenshotPath });
			screenshots[mode] = screenshotPath;
		}
		const values = card.locator('details.values');
		let rows = [];
		if (await values.count()) {
			const summary = values.locator('summary');
			if (!(await values.evaluate((node) => node.open))) await summary.click();
			rows = await values.locator('tbody tr').evaluateAll((tableRows) => tableRows.map((row) =>
				Array.from(row.querySelectorAll('th, td')).map((cell) => cell.textContent.trim())
			));
		}
		charts.push({ title, geometry, rows, screenshots });
	}
	await assistant.page().evaluate(() => { delete document.documentElement.dataset.colorMode; });
	return charts;
}

async function submitQuestion(page, task, {
	journey,
	prompt = task.request.prompt,
	clarificationReply = false,
	alreadySubmitted = false,
	priorUserCount,
	priorAssistantCount,
	rendererErrors = []
} = {}) {
	const startedAt = new Date().toISOString();
	const startTime = Date.now();
	const priorRendererErrorCount = rendererErrors.length;
	const beforeUserCount = priorUserCount ?? await page.locator('.msg.user').count();
	const beforeAssistantCount = priorAssistantCount ?? await page.locator('.msg.assistant').count();
	const composer = page.getByRole('combobox', { name: /Ask about/ });
	let assistant;
	let response = { text: '', analysis_details: '', analysis_details_available: false, steps: [], charts: [], clarification: null };
	let operationalError = null;
	try {
		if (!alreadySubmitted) {
			await composer.fill(prompt);
			await composer.press('Enter');
		}
		const userTurn = page.locator('.msg.user').last();
		await expect(page.locator('.msg.user')).toHaveCount(beforeUserCount + 1, { timeout: 15_000 });
		await expect(userTurn).toContainText(prompt);
		assistant = page.locator('.msg.assistant').last();
		await expect.poll(async () => {
			const count = await page.locator('.msg.assistant').count();
			if (count <= beforeAssistantCount) return false;
			const pending = await assistant.locator('.text.rich.pending').count();
			const thinking = await assistant.locator('.thinking').count();
			return pending === 0 && thinking === 0;
		}, { timeout: 120_000, intervals: [250, 500, 1000] }).toBe(true);
		const textLocator = assistant.locator('.text.rich');
		response.text = (await textLocator.count())
			? (await textLocator.allTextContents()).join('\n\n').trim()
			: await assistant.innerText();
		const clarification = page.locator('.dock .clarification-field');
		if (await clarification.count() && await clarification.isVisible()) {
			const question = clarification.locator('.clarification-question');
			response.clarification = {
				question: (await question.count()) ? (await question.innerText()).trim() : '',
				options: await clarification.locator('.clarification-options button').allTextContents()
			};
		}
		const details = await expandAnalysisDetails(assistant);
		response.analysis_details_available = details.available;
		response.analysis_details = details.text;
		response.steps = details.steps;
		response.charts = await readRenderedCharts(assistant, task.id);
	} catch (error) {
		operationalError = String(error?.stack || error);
		if (assistant && await assistant.count()) {
			const textParts = await assistant.locator('.text.rich').allTextContents().catch(() => []);
			response.text = textParts.join('\n\n').trim() || await assistant.innerText().catch(() => '');
			const clarification = page.locator('.dock .clarification-field');
			response.clarification = await clarification.count().catch(() => 0) && await clarification.isVisible().catch(() => false)
				? { question: await clarification.locator('.clarification-question').innerText().catch(() => ''), options: await clarification.locator('.clarification-options button').allTextContents().catch(() => []) }
				: null;
		}
	}

	const settledText = response.text.trim();
	const checks = [
		{ criterion: 'the exact prompt appears as a submitted user turn', passed: !operationalError },
		{ criterion: 'an assistant turn settles without a stuck thinking state', passed: !operationalError },
		{ criterion: 'the app renders answer text, a chart, or a typed clarification', passed: !!settledText || response.charts.length > 0 || !!response.clarification },
		{ criterion: 'Analysis Details is available for inspection', passed: response.analysis_details_available },
		{ criterion: 'no uncaught renderer exception occurs during this turn', passed: rendererErrors.length === priorRendererErrorCount }
	];
	if (task.id === 'bike-monthly-chart-2012') {
		const chart = response.charts[0];
		checks.push(
			{ criterion: 'exactly one chart is rendered for the single-series request', passed: response.charts.length === 1 },
			{ criterion: 'the chart has a title and inspectable exact-values rows', passed: !!chart?.title && chart.rows.length > 0 },
			{ criterion: 'the rendered chart and labels remain within their containers', passed: !!chart && chart.geometry.cardScrollWidth <= chart.geometry.cardWidth && chart.geometry.cardWithinAnswer && chart.geometry.visualWithinCard && chart.geometry.svgTextContained }
		);
	}
	if (task.id === 'bike-daily-hourly-month-reconcile') {
		checks.push({
			criterion: 'Analysis Details records both compared source files',
			passed: response.analysis_details.includes('daily.csv') && response.analysis_details.includes('monthly-usage-from-hourly.csv')
		});
	}
	if (task.id === 'housing-q1-clarification-and-resume' && !clarificationReply) {
		checks.push({
			criterion: 'the typed clarification replaces the chat input with choices and an Other response field',
			passed: !!response.clarification?.question && response.clarification.options.length > 0 && await page.locator('.dock .clarification-field').getByRole('textbox', { name: 'Other interpretation' }).count().catch(() => 0) === 1
		});
	}
	const passed = !operationalError && checks.every((check) => check.passed);
	const record = {
		record_type: 'journey_turn',
		schema_version: 1,
		journey,
		case_id: task.id,
		started_at: startedAt,
		elapsed_ms: Date.now() - startTime,
		task_metadata: {
			suite: task.suite,
			workspace: task.workspace?.id,
			domain: task.labels?.domain,
			primary_capability: task.labels?.primary_capability,
			supporting_capabilities: task.labels?.supporting_capabilities ?? [],
			analysis_families: task.labels?.analysis_families ?? [],
			interaction: task.labels?.interaction
		},
		input: { exact_prompt: prompt, prior_turns: task.request.prior_turns ?? [], clarification_reply: clarificationReply },
		response: { ...response, renderer_errors: rendererErrors.slice(priorRendererErrorCount) },
		workflow_checks: checks,
		result: passed ? 'pass' : operationalError ? 'error' : 'fail',
		...(operationalError ? { operational_error: operationalError } : {})
	};
	records.push(record);
	await appendFile(auditPath, `${JSON.stringify(record)}\n`, 'utf8');
	return { record, assistant };
}

async function markNotRun(task, journey, reason) {
	const record = {
		record_type: 'journey_turn', schema_version: 1, journey, case_id: task.id,
		input: { exact_prompt: task.request.prompt, prior_turns: task.request.prior_turns ?? [] },
		response: null, result: 'not_run', reason
	};
	records.push(record);
	await appendFile(auditPath, `${JSON.stringify(record)}\n`, 'utf8');
}

test.beforeAll(async () => {
	await mkdir(dirname(auditPath), { recursive: true });
	await mkdir(screenshotDir, { recursive: true });
	await appendFile(auditPath, `${JSON.stringify({
		record_type: 'journey_run', schema_version: 1, run_id: auditId,
		started_at: new Date().toISOString(), model, provider: 'openai',
		journey_only: true, answer_correctness_scored: false,
		credential: 'read from existing auth.json; never recorded', audit_file: auditPath
	})}\n`, 'utf8');
});

test.afterAll(async () => {
	const summary = {
		record_type: 'journey_summary', schema_version: 1, run_id: auditId,
		finished_at: new Date().toISOString(),
		workflow_pass: records.filter((record) => record.result === 'pass').length,
		workflow_fail: records.filter((record) => record.result === 'fail' || record.result === 'error').length,
		not_run: records.filter((record) => record.result === 'not_run').length,
		answer_correctness_scored: false,
		audit_file: auditPath,
		screenshots: screenshotDir
	};
	await appendFile(auditPath, `${JSON.stringify(summary)}\n`, 'utf8');
	console.log(`Journey audit: ${auditPath}`);
	console.log(`Journey screenshots: ${screenshotDir}`);
});

test.describe.configure({ mode: 'serial' });

test('complete analyst journey: inspect, compare, reconcile, visualize, forecast, and state limits', async () => {
	test.setTimeout(1_200_000);
	const tasks = await loadTasks(bikeSuite);
	const orderedIds = [
		'bike-daily-coverage',
		'bike-annual-total-2012',
		'bike-working-vs-nonworking-average',
		'bike-registered-share-2012',
		'bike-daily-hourly-month-reconcile',
		'bike-daily-count-integrity',
		'bike-highest-month-overall',
		'bike-monthly-chart-2012',
		'bike-followup-next-year',
		'bike-forecast-july-2011-mean-baseline',
		'bike-scenario-2012-ten-percent-lower',
		'bike-revenue-unavailable',
		'bike-hour-of-day-unavailable'
	];
	const dataDir = await mkdtemp(join(tmpdir(), 'fella-e2e-bike-journey-'));
	let app;
	let operationalFailure = null;
	const rendererErrors = [];
	try {
		app = await launchIsolatedApp(dataDir);
		const page = await app.firstWindow();
		page.on('pageerror', (error) => rendererErrors.push(String(error?.stack || error)));
		await page.waitForLoadState('domcontentloaded');
		await configureProvider(page, dataDir);
		await mount(page, app, join(bikeSuite, 'workspaces/capital-bikeshare'));

		for (const id of orderedIds) {
			const task = tasks.get(id);
			assert.ok(task, `FQA prompt must exist: ${id}`);
			if (operationalFailure) {
				await markNotRun(task, 'bike-sharing-analyst-session', operationalFailure);
				continue;
			}
			if (id === 'bike-followup-next-year') {
				const priorPrompt = task.request.prior_turns[0];
				const setupTask = { ...task, id: 'bike-followup-baseline-2011', request: { prompt: priorPrompt, prior_turns: [] } };
				const setup = await submitQuestion(page, setupTask, { journey: 'bike-sharing-analyst-session', rendererErrors });
				if (setup.record.result === 'error') operationalFailure = 'The baseline turn failed to complete; remaining questions were not submitted to a possibly busy conversation.';
			}
			if (operationalFailure) {
				await markNotRun(task, 'bike-sharing-analyst-session', operationalFailure);
				continue;
			}
			const result = await submitQuestion(page, task, { journey: 'bike-sharing-analyst-session', rendererErrors });
			if (result.record.result === 'error') operationalFailure = `${id} failed to complete; remaining questions were not submitted to a possibly busy conversation.`;
		}
	} finally {
		try { await app?.close(); } finally { await rm(dataDir, { recursive: true, force: true }); }
	}
	const failures = records.filter((record) => record.journey === 'bike-sharing-analyst-session' && record.result !== 'pass');
	assert.deepEqual(failures.map(({ case_id, result, workflow_checks }) => ({ case_id, result, workflow_checks })), [],
		'Judge the visible harness journey only; response correctness belongs to FQA-Bench, not this E2E test.');
	assert.deepEqual(rendererErrors, [], 'the complete journey must not produce uncaught Svelte/renderer errors');
});

test('complete clarification journey: ask, surface the question, resolve, and continue', async () => {
	test.setTimeout(360_000);
	const tasks = await loadTasks(housingSuite);
	const task = tasks.get('housing-q1-clarification-and-resume');
	assert.ok(task, 'the unchanged housing clarification prompt must exist');
	const dataDir = await mkdtemp(join(tmpdir(), 'fella-e2e-housing-journey-'));
	let app;
	const rendererErrors = [];
	try {
		app = await launchIsolatedApp(dataDir);
		const page = await app.firstWindow();
		page.on('pageerror', (error) => rendererErrors.push(String(error?.stack || error)));
		await page.waitForLoadState('domcontentloaded');
		await configureProvider(page, dataDir);
		await mount(page, app, join(housingSuite, 'workspaces/housing'));

		const initial = await submitQuestion(page, task, { journey: 'housing-clarification-session', rendererErrors });
		const clarification = page.locator('.dock .clarification-field');
		const initialPassed = initial.record.result === 'pass' && !!initial.record.response.clarification?.question;
		if (clarification && await clarification.count().catch(() => 0)) {
			await clarification.screenshot({ path: join(screenshotDir, 'housing-clarification.png') });
		}
		assert.ok(initialPassed && clarification && await clarification.count().catch(() => 0),
			`The in-composer clarification step is part of the journey contract; actual answer correctness is not graded. ${JSON.stringify(initial.record.workflow_checks)}`);

		const reply = task.request.clarification_reply;
		const priorUserCount = await page.locator('.msg.user').count();
		const priorAssistantCount = await page.locator('.msg.assistant').count();
		const replyInput = clarification.getByRole('textbox', { name: 'Other interpretation' });
		await replyInput.fill(reply);
		await clarification.getByRole('button', { name: 'Continue with this interpretation' }).click();
		await expect(page.locator('.msg.user').last()).toContainText(reply);

		const resolvedTask = { ...task, id: `${task.id}-resolved`, request: { prompt: reply, prior_turns: [task.request.prompt] } };
		const resumed = await submitQuestion(page, resolvedTask, {
			journey: 'housing-clarification-session',
			prompt: reply,
			clarificationReply: true,
			alreadySubmitted: true,
			priorUserCount,
			priorAssistantCount,
			rendererErrors
		});
		const lineageVisible = resumed.record.response.analysis_details.includes('This analysis continues from your clarification.');
		resumed.record.workflow_checks.push(
			{ criterion: 'clarification reply appears as a submitted user turn', passed: (await page.locator('.msg.user').last().innerText()).includes(reply) },
			{ criterion: 'resumed response exposes its clarification lineage in Analysis Details', passed: lineageVisible }
		);
		resumed.record.result = resumed.record.workflow_checks.every((check) => check.passed) ? 'pass' : 'fail';
		await appendFile(auditPath, `${JSON.stringify({ record_type: 'clarification_resume_checks', case_id: task.id, workflow_checks: resumed.record.workflow_checks.slice(-2), result: resumed.record.result })}\n`, 'utf8');
		assert.ok(resumed.record.workflow_checks.every((check) => check.passed),
			`The clarification response must resume and preserve lineage; response correctness is not graded. ${JSON.stringify(resumed.record.workflow_checks)}`);
		assert.deepEqual(rendererErrors, [], 'clarification and resume must not produce uncaught renderer errors');
	} finally {
		try { await app?.close(); } finally { await rm(dataDir, { recursive: true, force: true }); }
	}
});
