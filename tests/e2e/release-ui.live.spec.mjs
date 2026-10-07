import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import {
	access,
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
const bikeFixture = join(
	root,
	'bench',
	'fqa-bench',
	'suites',
	'uci-bike-sharing',
	'workspaces',
	'capital-bikeshare'
);

function defaultAuthPath() {
	if (process.env.FELLA_E2E_AUTH_FILE) return resolve(process.env.FELLA_E2E_AUTH_FILE);
	if (process.platform === 'win32') {
		return join(process.env.APPDATA || join(homedir(), 'AppData', 'Roaming'), 'dev.fella.app', 'auth.json');
	}
	if (process.platform === 'darwin') {
		return join(homedir(), 'Library', 'Application Support', 'dev.fella.app', 'auth.json');
	}
	const dataHome = process.env.XDG_DATA_HOME || join(homedir(), '.local', 'share');
	return join(dataHome, 'dev.fella.app', 'auth.json');
}

function enginePath() {
	const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
	const suffix = process.platform === 'win32' ? '.exe' : '';
	return join(root, 'electron', 'engine', `fella-engine-${arch}${suffix}`);
}

async function launchApp(dataDir) {
	await access(enginePath()).catch(() => {
		throw new Error('Electron sidecar is missing; run pnpm electron:build first.');
	});
	return electron.launch({
		args: [join(root, 'electron', 'main.mjs'), `--user-data-dir=${join(dataDir, 'chromium')}`],
		cwd: root,
		env: { ...process.env, FELLA_DATA_DIR: dataDir, FELLA_ENGINE_PATH: enginePath() }
	});
}

async function mountWorkspace(app, page, path) {
	await app.evaluate(({ dialog }, folder) => {
		dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
	}, path);
	const addRepository = page.getByRole('button', { name: 'Add a repository' });
	if (await addRepository.isVisible().catch(() => false)) await addRepository.click();
	else await page.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('combobox', { name: /Ask about/ })).toBeVisible();
}

async function configureOpenAi(page, dataDir) {
	const authPath = defaultAuthPath();
	const auth = JSON.parse(await readFile(authPath, 'utf8'));
	assert.equal(typeof auth['apikey:openai'], 'string', 'OpenAI credential missing from existing auth.json');
	await copyFile(authPath, join(dataDir, 'auth.json'));
	if (process.platform !== 'win32') await chmod(join(dataDir, 'auth.json'), 0o600);
	const configured = await page.evaluate(() => window.fella.invoke('set_settings', {
		settings: { provider: 'openai', model: 'gpt-5.6-luna' }
	}));
	assert.equal(configured.provider, 'openai');
	assert.equal(configured.model, 'gpt-5.6-luna');
	assert.equal(configured.has_credential, true);
}

function sse(response, value) {
	response.write(`data: ${JSON.stringify(value)}\n\n`);
}

test('G3: Stop retains completed SQL evidence and prevents later streamed text', async () => {
	const dataDir = await mkdtemp(join(tmpdir(), 'fella-electron-stop-'));
	const workspace = join(dataDir, 'workspace');
	await mkdir(workspace);
	await writeFile(join(workspace, 'sales.csv'), 'amount\n10\n15\n', 'utf8');

	let requestCount = 0;
	let slowResponseClosed = false;
	const server = createServer((request, response) => {
		if (request.method !== 'POST' || request.url !== '/v1/chat/completions') {
			response.writeHead(404).end();
			return;
		}
		requestCount += 1;
		response.writeHead(200, {
			'content-type': 'text/event-stream; charset=utf-8',
			'cache-control': 'no-cache',
			connection: 'keep-alive'
		});
		response.flushHeaders();
		if (requestCount === 1) {
			sse(response, {
				choices: [{
					index: 0,
					delta: {
						tool_calls: [{
							index: 0,
							id: 'call_total',
							type: 'function',
							function: {
								name: 'run_sql',
								arguments: '{"sql":"SELECT sum(amount) AS total FROM sales"}'
							}
						}]
					},
					finish_reason: null
				}]
			});
			sse(response, { choices: [{ index: 0, delta: {}, finish_reason: 'tool_calls' }] });
			response.end('data: [DONE]\n\n');
			return;
		}

		sse(response, { choices: [{ index: 0, delta: { content: 'The total is ' }, finish_reason: null }] });
		response.on('close', () => { slowResponseClosed = true; });
		setTimeout(() => {
			if (!response.destroyed) {
				sse(response, { choices: [{ index: 0, delta: { content: 'LATE_TOKEN_SHOULD_NOT_APPEAR' }, finish_reason: null }] });
				sse(response, { choices: [{ index: 0, delta: {}, finish_reason: 'stop' }] });
				response.end('data: [DONE]\n\n');
			}
		}, 2800);
	});
	await new Promise((done) => server.listen(0, '127.0.0.1', done));
	const address = server.address();
	assert.ok(address && typeof address === 'object');

	let app;
	try {
		app = await launchApp(dataDir);
		const page = await app.firstWindow();
		await page.waitForLoadState('domcontentloaded');
		await page.evaluate(async ({ baseUrl }) => {
			await window.fella.invoke('set_api_key', { provider: 'custom', key: 'local-release-ui-test' });
			await window.fella.invoke('set_settings', {
				settings: { provider: 'custom', base_url: baseUrl, model: 'local-release-ui-test' }
			});
		}, { baseUrl: `http://127.0.0.1:${address.port}/v1` });

		await mountWorkspace(app, page, workspace);
		const composer = page.getByRole('combobox', { name: /Ask about/ });
		await composer.fill('What is the total sales amount?');
		await composer.press('Enter');
		const assistant = page.locator('.msg.assistant').last();
		await expect(assistant.locator('.text.rich')).toContainText('The total is', { timeout: 20_000 });
		await expect(page.getByRole('button', { name: 'Stop' })).toBeVisible();
		await page.getByRole('button', { name: 'Stop' }).click();
		await expect(assistant.locator('.text.rich')).toContainText('Stopped.', { timeout: 10_000 });
		await expect(assistant.locator('.text.rich')).not.toContainText('LATE_TOKEN_SHOULD_NOT_APPEAR');
		await expect.poll(() => slowResponseClosed, { timeout: 5_000 }).toBe(true);

		const details = assistant.getByRole('button', { name: 'Analysis details' });
		await expect(details).toBeVisible();
		await details.click();
		await expect(assistant).toContainText('sales.csv');
		const queryToggle = assistant.getByRole('button', { name: 'show the query' });
		await expect(queryToggle).toBeVisible();
		await queryToggle.click();
		await expect(assistant).toContainText('SELECT sum(amount) AS total FROM sales');
		await expect(assistant).toContainText('25');
		await page.waitForTimeout(3200);
		await expect(assistant.locator('.text.rich')).not.toContainText('LATE_TOKEN_SHOULD_NOT_APPEAR');
		assert.equal(requestCount, 2, 'expected one tool-selection call and one interrupted final-answer call');
	} finally {
		try {
			await app?.close();
		} finally {
			await new Promise((done) => server.close(done));
			await rm(dataDir, { recursive: true, force: true });
		}
	}
});

test('G5: typed clarification replaces the composer, accepts a choice and Other, then resumes the same chat', async () => {
	const dataDir = await mkdtemp(join(tmpdir(), 'fella-electron-clarification-composer-'));
	const workspace = join(dataDir, 'workspace');
	await mkdir(workspace);
	await writeFile(join(workspace, 'ledger.csv'), 'date,category,amount\n2024-01-03,rent,1200\n', 'utf8');

	let requestCount = 0;
	const requestLog = [];
	const server = createServer(async (request, response) => {
		const chunks = [];
		for await (const chunk of request) chunks.push(chunk);
		if (request.method !== 'POST' || request.url !== '/v1/chat/completions') {
			response.writeHead(404).end();
			return;
		}
		requestCount += 1;
		let body;
		try { body = JSON.parse(Buffer.concat(chunks).toString('utf8')); } catch { body = null; }
		requestLog.push({
			index: requestCount,
			messages: Array.isArray(body?.messages) ? body.messages.map(({ role, content, tool_calls }) => ({ role, content, tool_calls })) : []
		});
		response.writeHead(200, {
			'content-type': 'text/event-stream; charset=utf-8',
			'cache-control': 'no-cache',
			connection: 'keep-alive'
		});
		response.flushHeaders();
		if (requestCount === 1 || requestCount === 4) {
			const question = requestCount === 1
				? 'Which housing costs should be included?'
				: 'Should the summary include utilities?';
			const options = requestCount === 1
				? ['Rent only', 'Rent and utilities']
				: ['Include utilities', 'Exclude utilities'];
			sse(response, {
				choices: [{
					index: 0,
					delta: {
						tool_calls: [{
							index: 0,
							id: `call_clarify_${requestCount}`,
							type: 'function',
							function: {
								name: '__request_clarification',
								arguments: JSON.stringify({ question, options, reason: 'The scope changes the aggregate.' })
							}
						}]
					},
					finish_reason: null
				}]
			});
			sse(response, { choices: [{ index: 0, delta: {}, finish_reason: 'tool_calls' }] });
		} else {
			const text = requestCount === 2 || requestCount === 5
				? 'I am holding the calculation until you choose a scope.'
				: 'Thanks, I will continue with that scope.';
			sse(response, { choices: [{ index: 0, delta: { content: text }, finish_reason: null }] });
			sse(response, { choices: [{ index: 0, delta: {}, finish_reason: 'stop' }] });
		}
		response.end('data: [DONE]\n\n');
	});
	await new Promise((done) => server.listen(0, '127.0.0.1', done));
	const address = server.address();
	assert.ok(address && typeof address === 'object');

	let app;
	try {
		app = await launchApp(dataDir);
		const page = await app.firstWindow();
		await page.waitForLoadState('domcontentloaded');
		await page.evaluate(async ({ baseUrl }) => {
			await window.fella.invoke('set_api_key', { provider: 'custom', key: 'local-clarification-ui-test' });
			await window.fella.invoke('set_settings', {
				settings: { provider: 'custom', base_url: baseUrl, model: 'local-clarification-ui-test' }
			});
		}, { baseUrl: `http://127.0.0.1:${address.port}/v1` });
		await mountWorkspace(app, page, workspace);

		const composer = page.getByRole('combobox', { name: /Ask about/ });
		await composer.fill('How much did I spend on housing?');
		await composer.press('Enter');
		const clarification = page.locator('.dock .clarification-field');
		await expect(clarification).toBeVisible({ timeout: 30_000 });
		await expect(page.getByRole('combobox', { name: /Ask about/ })).toHaveCount(0);
		await expect(clarification.locator('.clarification-options button')).toHaveCount(2);
		await expect(clarification.getByRole('textbox', { name: 'Other interpretation' })).toBeVisible();
		const clarificationScreenshot = test.info().outputPath('clarification-composer.png');
		await mkdir(dirname(clarificationScreenshot), { recursive: true });
		await clarification.screenshot({ path: clarificationScreenshot });

		const assistantCountBeforeChoice = await page.locator('.msg.assistant').count();
		await clarification.getByRole('button', { name: /Rent and utilities/ }).click();
		await expect(page.locator('.msg.user').last()).toContainText('Rent and utilities');
		await expect.poll(() => page.locator('.msg.assistant').count(), {
			timeout: 30_000,
			message: `selecting a clarification choice must start a same-conversation continuation; model requests: ${JSON.stringify(requestLog)}`
		}).toBe(assistantCountBeforeChoice + 1);
		await expect(page.locator('.msg.assistant').last().locator('.text.rich')).toContainText(
			'Thanks, I will continue with that scope.',
			{ timeout: 30_000 }
		);

		await composer.fill('Can you also summarize the housing costs?');
		await composer.press('Enter');
		await expect(clarification).toBeVisible({ timeout: 30_000 });
		const other = clarification.getByRole('textbox', { name: 'Other interpretation' });
		await other.fill('Rent only, excluding utilities and repairs.');
		await clarification.getByRole('button', { name: 'Continue with this interpretation' }).click();
		await expect(page.locator('.msg.user').last()).toContainText('Rent only, excluding utilities and repairs.');
		await expect(page.locator('.msg.assistant').last().locator('.text.rich')).toContainText(
			'Thanks, I will continue with that scope.',
			{ timeout: 30_000 }
		);
		assert.ok(requestCount >= 6, `expected clarification and resumed-analysis requests; observed ${requestCount}: ${JSON.stringify(requestLog)}`);
		assert.equal(await page.locator('.msg.user').count(), 4, 'both clarifications should resume as user turns in the existing chat');
		assert.deepEqual(await page.locator('.msg.assistant').count(), 4);
	} finally {
		try { await app?.close(); } finally {
			await new Promise((done) => server.close(done));
			await rm(dataDir, { recursive: true, force: true });
		}
	}
});

test('G8: the mounted-workspace composer remains visible at the reported and compact sizes', async () => {
	const dataDir = await mkdtemp(join(tmpdir(), 'fella-electron-composer-layout-'));
	const workspace = join(dataDir, 'workspace');
	await mkdir(workspace);
	await writeFile(join(workspace, 'records.csv'), 'date,amount\n2026-01-01,12\n2026-01-02,19\n', 'utf8');
	let app;
	try {
		app = await launchApp(dataDir);
		const page = await app.firstWindow();
		await page.waitForLoadState('domcontentloaded');
		await mountWorkspace(app, page, workspace);

		const composer = page.locator('.dock .wrap');
		await expect(composer).toBeVisible();
		for (const viewport of [
			{ width: 1288, height: 832 },
			{ width: 1024, height: 640 }
		]) {
			await page.setViewportSize(viewport);
			await page.waitForFunction(() => {
				const dock = document.querySelector('.dock');
				return !!dock && window.innerHeight > 0;
			});
			const layout = await page.evaluate(() => {
				const rect = (selector) => {
					const element = document.querySelector(selector);
					if (!element) return null;
					const bounds = element.getBoundingClientRect();
					return {
						top: bounds.top,
						bottom: bounds.bottom,
						left: bounds.left,
						right: bounds.right,
						width: bounds.width,
						height: bounds.height
					};
				};
				return {
					viewport: { width: window.innerWidth, height: window.innerHeight },
					root: {
						clientHeight: document.documentElement.clientHeight,
						scrollHeight: document.documentElement.scrollHeight
					},
					dock: rect('.dock'),
					composer: rect('.dock .wrap'),
					field: rect('.dock .field'),
					context: rect('.dock .context-row'),
					contextButton: rect('.dock .context-add'),
					question: rect('.dock textarea'),
					controls: rect('.dock .bottom-row'),
					modeControl: rect('.dock .mode-trigger'),
					modelControl: rect('.dock .model-trigger')
				};
			});
			assert.equal(layout.viewport.width, viewport.width, 'renderer width should match the requested test viewport');
			assert.equal(layout.viewport.height, viewport.height, 'renderer height should match the requested test viewport');
			for (const name of [
				'dock', 'composer', 'field', 'context', 'contextButton', 'question',
				'controls', 'modeControl', 'modelControl'
			]) {
				const bounds = layout[name];
				assert.ok(bounds, `${name} should exist at ${viewport.width}x${viewport.height}`);
				assert.ok(bounds.width > 0 && bounds.height > 0,
					`${name} should have visible dimensions at ${viewport.width}x${viewport.height}`);
				assert.ok(bounds.top >= 0 && bounds.bottom <= layout.viewport.height,
					`${name} should remain fully inside the viewport at ${viewport.width}x${viewport.height}: ${JSON.stringify(layout)}`);
			}
			assert.ok(layout.root.scrollHeight <= layout.root.clientHeight,
				`the renderer document should not extend below its viewport at ${viewport.width}x${viewport.height}: ${JSON.stringify(layout)}`);
		}
	} finally {
		try {
			await app?.close();
		} finally {
			await rm(dataDir, { recursive: true, force: true });
		}
	}
});

test('G7 chart: live chart completes and renders inspectable, contained visuals', async () => {
	test.setTimeout(240_000);
	const dataDir = await mkdtemp(join(tmpdir(), 'fella-electron-chart-'));
	let capturesDir;
	let app;
	try {
		app = await launchApp(dataDir);
		const page = await app.firstWindow();
		await page.waitForLoadState('domcontentloaded');
		await page.setViewportSize({ width: 1024, height: 900 });
		await configureOpenAi(page, dataDir);
		await mountWorkspace(app, page, bikeFixture);

		const chartQuestion = 'Chart total rentals by month for 2012. Use month names on the horizontal axis and keep them in calendar order.';
		const composer = page.getByRole('combobox', { name: /Ask about/ });
		await composer.fill(chartQuestion);
		await composer.press('Enter');
		const chartAnswer = page.locator('.msg.assistant').last();
		const chart = chartAnswer.locator('.chart-card');
		await expect(chart).toBeVisible({ timeout: 120_000 });
		await expect(chartAnswer.locator('.text.rich.pending')).toHaveCount(0, { timeout: 120_000 });
		await expect(chartAnswer.locator('.thinking')).toHaveCount(0);
		await expect(chartAnswer.locator('.chart-card')).toHaveCount(1);
		await expect(chart.locator('[role="img"]')).toBeVisible();
		const chartTitle = await chart.locator('.chart-title').innerText();
		assert.ok(chartTitle.trim(), 'chart should have a visible title');
		const chartGeometry = await chart.evaluate((figure) => {
			const frame = figure.getBoundingClientRect();
			const parent = figure.parentElement?.getBoundingClientRect();
			const visual = figure.querySelector('.chart, .pie-layout, .boxplot-wrap, .heatmap-scroll');
			const visualBounds = visual?.getBoundingClientRect();
			const svg = figure.querySelector('svg[role="img"]');
			const viewBox = svg?.viewBox.baseVal;
			const axisTextWithinSvg = !svg || [...svg.querySelectorAll('text.axis-label, text.y-axis-label, text.axis-title')].every((label) => {
				try {
					const box = label.getBBox();
					const tolerance = 2;
					return box.x >= -tolerance && box.y >= -tolerance &&
						box.x + box.width <= viewBox.width + tolerance &&
						box.y + box.height <= viewBox.height + tolerance;
				} catch {
					return false;
				}
			});
			const visibleLabels = figure.querySelectorAll(
				'.row-label, .axis-label, .pie-label, .box-label, .heatmap th'
			).length;
			return {
				cardWidth: figure.clientWidth,
				cardScrollWidth: figure.scrollWidth,
				cardWithinAnswer: Boolean(parent && frame.left >= parent.left - 0.5 && frame.right <= parent.right + 0.5),
				visualWithinCard: Boolean(visualBounds && visualBounds.left >= frame.left - 0.5 && visualBounds.right <= frame.right + 0.5),
				axisTextWithinSvg,
				visibleLabels
			};
		});
		assert.ok(chartGeometry.cardScrollWidth <= chartGeometry.cardWidth,
			`chart content should not overflow its card: ${JSON.stringify(chartGeometry)}`);
		assert.ok(chartGeometry.cardWithinAnswer,
			`the chart card should stay within the answer column: ${JSON.stringify(chartGeometry)}`);
		assert.ok(chartGeometry.visualWithinCard,
			`the rendered chart family should remain inside its card: ${JSON.stringify(chartGeometry)}`);
		assert.ok(chartGeometry.axisTextWithinSvg,
			`SVG axis titles and tick labels should remain inside their viewBox: ${JSON.stringify(chartGeometry)}`);
		assert.ok(chartGeometry.visibleLabels > 0,
			`the chosen chart family should expose category or period labels: ${JSON.stringify(chartGeometry)}`);

		const exactValues = chart.locator('details.values');
		await expect(exactValues).toBeVisible();
		await exactValues.locator('summary').click();
		await expect(exactValues.locator('tbody tr').first()).toBeVisible();
		const chartRows = await exactValues.locator('tbody tr').evaluateAll((rows) => rows.map((row) =>
			Array.from(row.querySelectorAll('th,td')).map((cell) => cell.textContent.trim())
		));
		assert.ok(chartRows.length > 0, 'the chart should expose inspectable values');
		assert.ok(chartRows.every((row) => row.length >= 2 && row.every((cell) => cell.length > 0)),
			`the values table should contain readable labels and cells: ${JSON.stringify(chartRows.slice(0, 3))}`);
		const tickLabels = await chart.locator('.y-axis-label').allTextContents();
		assert.ok(tickLabels.every((label) => !label.includes('rentals')),
			'long units should not be repeated on compact chart-axis ticks');
		const periodTicks = await chart.locator('.axis-label').allTextContents();
		assert.ok(periodTicks.every((label) => !/^2012-\d{2}$/.test(label)),
			'when the chart uses a time axis, month labels should be human-readable');

		capturesDir = await mkdtemp(join(tmpdir(), 'fella-v030-g7-captures-'));
		await exactValues.locator('summary').click();
		await page.evaluate(() => { document.documentElement.dataset.colorMode = 'dark'; });
		await chart.screenshot({ path: join(capturesDir, 'chart-dark.png') });
		await page.evaluate(() => { document.documentElement.dataset.colorMode = 'light'; });
		await chart.screenshot({ path: join(capturesDir, 'chart-light.png') });

	} finally {
		if (capturesDir) console.log(`G7 chart visual captures: ${capturesDir}`);
		try {
			await app?.close();
		} finally {
			await rm(dataDir, { recursive: true, force: true });
		}
	}
});

test('G7 forecast: live forecast completes and exposes its method and source details', async () => {
	test.setTimeout(240_000);
	const dataDir = await mkdtemp(join(tmpdir(), 'fella-electron-forecast-'));
	let app;
	let capturesDir;
	try {
		app = await launchApp(dataDir);
		const page = await app.firstWindow();
		await page.waitForLoadState('domcontentloaded');
		await page.setViewportSize({ width: 1024, height: 900 });
		await configureOpenAi(page, dataDir);
		await mountWorkspace(app, page, bikeFixture);

		const forecastQuestion = 'For a backtest, pretend you are at the end of June 2011. Using only the monthly rental totals from January through June 2011, estimate July 2011 with their arithmetic mean. What forecast value would you record? Label it as a forecast, not an observed total.';
		const composer = page.getByRole('combobox', { name: /Ask about/ });
		await composer.fill(forecastQuestion);
		await composer.press('Enter');
		const forecastAnswer = page.locator('.msg.assistant').last();
		const answerText = forecastAnswer.locator('.text.rich');
		await expect(answerText).not.toHaveClass(/pending/, { timeout: 120_000 });
		await expect(forecastAnswer.locator('.thinking')).toHaveCount(0);
		assert.ok((await answerText.innerText()).trim(), 'the forecast request should produce a settled response');

		const analysisDetails = forecastAnswer.getByRole('button', { name: 'Analysis details' });
		await expect(analysisDetails).toBeVisible();
		await analysisDetails.click();
		await expect(analysisDetails).toHaveAttribute('aria-expanded', 'true');
		const methodToggle = forecastAnswer.getByRole('button', { name: 'show forecast method' });
		await expect(methodToggle).toBeVisible();
		const forecastStep = forecastAnswer.locator('.steps > .step:has(.language-badge)').first();
		const sourceLine = forecastStep.locator('.source-line').first();
		await expect(sourceLine).toBeVisible();
		await expect(sourceLine).toContainText(/^from .+\(.+\)/);
		await methodToggle.click();
		await expect(forecastStep.locator('[aria-label="Forecast method"]')).toBeVisible();

		const inputRows = await forecastStep.locator('.detail table tbody tr').evaluateAll((rows) => rows.map((row) =>
			Array.from(row.querySelectorAll('th,td')).map((cell) => cell.textContent.trim())
		));
		assert.ok(inputRows.length > 0, 'the forecast step should expose inspectable source rows');
		assert.ok(inputRows.every((row) => row.length > 0 && row.some((cell) => cell.length > 0)),
			'the forecast source table should render non-empty rows');

		capturesDir = await mkdtemp(join(tmpdir(), 'fella-v030-g7-forecast-captures-'));
		await page.evaluate(() => { document.documentElement.dataset.colorMode = 'dark'; });
		await forecastAnswer.screenshot({ path: join(capturesDir, 'forecast-details-dark.png') });
		await page.evaluate(() => { document.documentElement.dataset.colorMode = 'light'; });
		await forecastAnswer.screenshot({ path: join(capturesDir, 'forecast-details-light.png') });
	} finally {
		if (capturesDir) console.log(`G7 forecast visual captures: ${capturesDir}`);
		try {
			await app?.close();
		} finally {
			await rm(dataDir, { recursive: true, force: true });
		}
	}
});
