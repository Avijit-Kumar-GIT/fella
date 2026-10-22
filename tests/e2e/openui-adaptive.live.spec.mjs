import assert from 'node:assert/strict';
import { access, chmod, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { createParser } from '@openuidev/lang-core';
import '../../src/lib/presentation/functions.ts';
import { expect, test, _electron as electron } from '@playwright/test';

// Frozen before execution: docs/testing/OPENUI-ADAPTIVE-v1.md (contract v1.1). These are
// independent source/answer oracles, not values derived from candidate output.
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const provider = process.env.FELLA_OPENUI_LIVE_PROVIDER || 'ollama-cloud';
const model = process.env.FELLA_OPENUI_LIVE_MODEL || 'gemma4:31b';
const categories = ['Apparel', 'Footwear', 'Equipment'];
const categoryTotals = { Apparel: [300, 600], Footwear: [600, 360], Equipment: [200, 450] };
const categoryChanges = { Apparel: 300, Footwear: -240, Equipment: 250 };
const monthlyTotals = [350, 360, 390, 410, 460, 540];
const exactRecords = [
	['R002', '2025-08-01', 'Apparel', 'North', '120', '12'],
	['R009', '2025-09-01', 'Footwear', 'South', '220', '22'],
	['R017', '2025-11-01', 'Equipment', 'West', '140', '14']
];
const sales = [
	'record_id,order_date,quarter,category,region,net_revenue,orders',
	'R001,2025-07-01,Q3,Apparel,North,100,10',
	'R002,2025-08-01,Q3,Apparel,North,120,12',
	'R003,2025-09-01,Q3,Apparel,North,80,10',
	'R004,2025-10-01,Q4,Apparel,North,160,16',
	'R005,2025-11-01,Q4,Apparel,North,200,20',
	'R006,2025-12-01,Q4,Apparel,North,240,24',
	'R007,2025-07-01,Q3,Footwear,South,200,20',
	'R008,2025-08-01,Q3,Footwear,South,180,18',
	'R009,2025-09-01,Q3,Footwear,South,220,22',
	'R010,2025-10-01,Q4,Footwear,South,150,15',
	'R011,2025-11-01,Q4,Footwear,South,120,12',
	'R012,2025-12-01,Q4,Footwear,South,90,9',
	'R013,2025-07-01,Q3,Equipment,West,50,10',
	'R014,2025-08-01,Q3,Equipment,West,60,8',
	'R015,2025-09-01,Q3,Equipment,West,90,12',
	'R016,2025-10-01,Q4,Equipment,West,100,10',
	'R017,2025-11-01,Q4,Equipment,West,140,14',
	'R018,2025-12-01,Q4,Equipment,West,210,21'
].join('\n') + '\n';

const cases = [
	{ id: 'A1-category-changes', kind: 'category', question: 'Which categories drove the change in net revenue between Q3 and Q4 2025, and how do their contributions rank? Compare the category totals in sales.csv; net_revenue is USD.' },
	{ id: 'A2-monthly-trend', kind: 'trend', question: 'How did total net revenue change month by month from July through December 2025 across all categories in sales.csv? net_revenue is USD.' },
	{ id: 'A3-scalar', kind: 'scalar', question: 'What was the total net revenue in Q4 2025 across all categories in sales.csv? net_revenue is USD.' },
	{ id: 'A4-exact-records', kind: 'records', question: 'What are the record_id, order_date, category, region, net_revenue, and orders for records R002, R009, and R017 in sales.csv?' }
];

test.describe.configure({ mode: 'default', retries: 0, timeout: 420_000 });

function authPath() {
	if (process.env.FELLA_E2E_AUTH_FILE) return resolve(process.env.FELLA_E2E_AUTH_FILE);
	if (process.platform === 'win32') return join(process.env.APPDATA || join(homedir(), 'AppData', 'Roaming'), 'dev.fella.app', 'auth.json');
	if (process.platform === 'darwin') return join(homedir(), 'Library', 'Application Support', 'dev.fella.app', 'auth.json');
	return join(process.env.XDG_DATA_HOME || join(homedir(), '.local', 'share'), 'dev.fella.app', 'auth.json');
}

function enginePath() {
	if (process.env.FELLA_ENGINE_PATH) return resolve(process.env.FELLA_ENGINE_PATH);
	const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
	return join(root, 'electron', 'engine', `fella-engine-${arch}${process.platform === 'win32' ? '.exe' : ''}`);
}

async function invoke(page, command, args = {}) {
	return page.evaluate(({ command, args }) => window.fella.invoke(command, args), { command, args });
}

function number(value) {
	if (typeof value === 'number') return Number.isFinite(value) ? value : NaN;
	if (typeof value !== 'string' || !/^[-+]?\d[\d,]*(?:\.\d+)?$/.test(value.trim())) return NaN;
	return Number(value.replaceAll(',', ''));
}

function includesValue(values, expected) { return values.some(value => number(value) === expected); }
function month(value) {
	const text = String(value).trim();
	const iso = text.match(/^2025[-/](0[7-9]|1[0-2])(?:[-/ T]|$)/);
	if (iso) return Number(iso[1]) - 7;
	const names = ['jul', 'aug', 'sep', 'oct', 'nov', 'dec'];
	const named = names.indexOf(text.slice(0, 3).toLowerCase());
	if (named >= 0) return named;
	if (/^(?:0?[7-9]|1[0-2])$/.test(text)) return Number(text) - 7;
	return -1;
}

function assertCanonical(task, results) {
	assert.ok(results.length, 'the answer must have canonical accepted result bindings');
	const complete = results.filter(result => result.complete && result.rows.length === result.row_count);
	assert.ok(complete.length, 'these fully specified totals/records require complete saved results');
	const rows = complete.flatMap(result => result.exact_rows);
	if (task.kind === 'category') {
		for (const [category, amounts] of Object.entries(categoryTotals)) {
			const values = rows.filter(row => row.some(value => String(value).toLowerCase() === category.toLowerCase())).flat();
			for (const expected of amounts) assert.ok(includesValue(values, expected), `${category} canonical Q3/Q4 totals must include ${expected}`);
		}
	} else if (task.kind === 'trend') {
		for (let index = 0; index < monthlyTotals.length; index++) {
			assert.ok(rows.some(row => row.some(value => month(value) === index) && includesValue(row, monthlyTotals[index])), `canonical month ${index + 7} total must be ${monthlyTotals[index]}`);
		}
	} else if (task.kind === 'scalar') {
		assert.ok(rows.some(row => includesValue(row, 1410)), 'canonical Q4 net revenue must be exactly 1410 USD');
	} else {
		for (const expected of exactRecords) {
			assert.ok(rows.some(row => expected.every(value => row.some(cell => cell === value))), `canonical values for ${expected[0]} must include all requested fields`);
		}
	}
}

function walkTree(node, callback, ancestors = []) {
	if (!node || typeof node !== 'object') return;
	if (Array.isArray(node)) { for (const child of node) walkTree(child, callback, ancestors); return; }
	if (node.type === 'element') {
		callback(node, ancestors);
		const children = node.typeName === 'Tabs' ? [node.props.children?.[0]] : node.props.children;
		walkTree(children, callback, [...ancestors, node.typeName]);
	}
}

function rowReferences(value, ids = new Set()) {
	if (!value || typeof value !== 'object') return ids;
	if (value.k === 'Comp' && value.name === 'Rows' && value.args?.[0]?.k === 'Str') ids.add(value.args[0].v);
	for (const child of Object.values(value)) rowReferences(child, ids);
	return ids;
}

function fieldsIn(value, fields = new Set()) {
	if (!value || typeof value !== 'object') return fields;
	if (typeof value.field === 'string') fields.add(value.field);
	for (const child of Object.values(value)) fieldsIn(child, fields);
	return fields;
}

function relevantChart(task, chart, results) {
	const result = results.find(item => item.id === chart.props.resultId && item.complete);
	if (!result) return false;
	const mapped = [...fieldsIn(chart.props.spec)].map(field => result.columns.indexOf(field)).filter(index => index >= 0);
	const numeric = mapped.filter(index => result.rows.some(row => typeof row[index] === 'number'));
	if (!numeric.length) return false;
	if (task.kind === 'category') {
		const categoryColumn = mapped.find(index => categories.every(category => result.rows.some(row => row[index] === category)));
		if (categoryColumn === undefined) return false;
		const values = category => result.rows.filter(row => row[categoryColumn] === category).flatMap(row => numeric.map(index => row[index]));
		const changes = categories.every(category => includesValue(values(category), categoryChanges[category]));
		const beforeAfter = categories.every(category => categoryTotals[category].every(amount => includesValue(values(category), amount)));
		// Long form must map the quarter as well; a category-only annual sum
		// does not explain Q3 -> Q4 change. Wide before/after measures need two fields.
		const periodMapped = mapped.some(index => ['Q3', 'Q4'].every(period => result.rows.some(row => String(row[index]).toUpperCase().includes(period))));
		return changes || beforeAfter && (numeric.length >= 2 || periodMapped);
	}
	const dateColumn = mapped.find(index => monthlyTotals.every((_, wanted) => result.rows.some(row => month(row[index]) === wanted)));
	return dateColumn !== undefined && monthlyTotals.every((expected, wanted) => result.rows.some(row => month(row[dateColumn]) === wanted && numeric.some(index => number(row[index]) === expected)));
}

async function chartGeometry(presentation) {
	return presentation.locator('.chart-target').evaluateAll(targets => targets.map(target => {
		const visible = element => {
			const box = element.getBoundingClientRect();
			return box.width > 0 && box.height > 0 && getComputedStyle(element).visibility !== 'hidden' && getComputedStyle(element).display !== 'none';
		};
		return [...target.querySelectorAll('canvas, svg')].map(element => {
			const box = element.getBoundingClientRect();
			let graphical = false;
			if (element.tagName.toLowerCase() === 'canvas') {
				try {
					const context = element.getContext('2d');
					const pixels = context?.getImageData(0, 0, element.width, element.height).data;
					graphical = !!pixels?.some((value, index) => index % 4 === 3 && value > 0);
				} catch { graphical = false; }
			} else graphical = !!element.querySelector('path, rect, circle, line, polygon, polyline');
			return { visible: visible(element), width: box.width, height: box.height, intrinsicWidth: element.width?.baseVal?.value ?? element.width, intrinsicHeight: element.height?.baseVal?.value ?? element.height, graphical };
		});
	}).flat());
}

async function assertGeneratedLayouts(presentation, rootNode) {
	const layouts = [];
	walkTree(rootNode, node => { if (['Row', 'Grid', 'Columns', 'Tabs'].includes(node.typeName)) layouts.push(node); });
	const indices = {};
	for (const layout of layouts) {
		const selector = { Row: '.row', Grid: '.grid', Columns: '.columns', Tabs: '.tabs' }[layout.typeName];
		const index = indices[layout.typeName] ?? 0; indices[layout.typeName] = index + 1;
		const host = presentation.locator(selector).nth(index);
		await expect(host, `generated ${layout.typeName} must exist as a real layout`).toBeVisible();
		const leaves = [];
		walkTree(layout, node => { if (['Metric', 'Chart', 'Table'].includes(node.typeName)) leaves.push(node.typeName); });
		for (const [type, selector] of [['Metric', 'dl'], ['Chart', '.chart-target'], ['Table', 'table:visible']]) {
			const expected = leaves.filter(leaf => leaf === type).length;
			if (expected) assert.ok(await host.locator(selector).count() >= expected, `generated ${layout.typeName} must render all ${expected} nested ${type} children, not merely its heading`);
		}
	}
	return layouts.map(layout => layout.typeName);
}

async function gradePresentation(task, presentation, source, results, schema, observations) {
	const parsed = createParser(schema).parse(source);
	observations.parser = parsed.meta;
	assert.ok(parsed.root, 'persisted source must parse into an actual OpenUI tree');
	assert.equal(parsed.meta.incomplete, false);
	assert.deepEqual(parsed.meta.unresolved, []);
	assert.deepEqual(parsed.meta.errors, []);
	const components = [];
	walkTree(parsed.root, (node, ancestors) => components.push({ ...node, ancestors }));
	observations.components = components;
	for (const node of components.filter(node => ['Chart', 'Table', 'Metric'].includes(node.typeName))) {
		if (typeof node.props.resultId === 'string') assert.ok(results.some(result => result.id === node.props.resultId), 'generated data components must bind accepted saved results');
	}
	const nestedLayouts = await assertGeneratedLayouts(presentation, parsed.root);
	if (task.kind === 'category' || task.kind === 'trend') {
		assert.ok(components.some(node => node.typeName === 'Chart' && relevantChart(task, node, results)), 'a task-relevant OpenUI chart must map the requested category/time and revenue data, not unrelated fields');
		await expect.poll(async () => (await chartGeometry(presentation)).some(chart => chart.visible && chart.width > 0 && chart.height > 0 && chart.intrinsicWidth > 0 && chart.intrinsicHeight > 0 && chart.graphical), { timeout: 30_000, intervals: [250, 500, 1000] }).toBe(true);
		await presentation.locator('.chart-target').filter({ visible: true }).first().scrollIntoViewIfNeeded();
	} else if (task.kind === 'scalar') {
		const boundMetric = components.some(node => {
			if (node.typeName !== 'Metric') return false;
			const result = results.find(result => result.id === node.props.resultId);
			const column = result?.columns.indexOf(node.props.field) ?? -1;
			return result && column >= 0 && number(result.rows[node.props.row]?.[column]) === 1410
				|| [...rowReferences(node.props.value)].some(id => results.some(result => result.id === id && result.rows.some(row => includesValue(row, 1410))));
		});
		const boundText = components.some(node => ['Text', 'Callout'].includes(node.typeName) && [...rowReferences(node.props.content)].some(id => results.some(result => result.id === id && result.rows.some(row => includesValue(row, 1410)))));
		const metricValues = await presentation.locator('dl:visible dd').allInnerTexts();
		const paragraphs = await presentation.locator('p:visible').allInnerTexts();
		const hasTotal = text => (text.match(/[-+]?\d[\d,]*(?:\.\d+)?/g) ?? []).some(value => number(value) === 1410);
		assert.ok(boundMetric && metricValues.some(hasTotal) || boundText && paragraphs.some(text => text.length >= 20 && /revenue|total|Q4|quarter/i.test(text) && hasTotal(text)), 'the scalar answer needs a rendered bound Metric or substantive bound explanation, not a table-only response');
	} else {
		assert.ok(components.some(node => node.typeName === 'Table'), 'record retrieval must actually use an OpenUI Table');
		const tables = presentation.locator('table:visible');
		assert.ok(await tables.count() > 0, 'record retrieval needs visible rendered rows');
		const rows = await tables.locator('tbody tr').evaluateAll(rows => rows.map(row => [...row.querySelectorAll('td')].map(cell => cell.innerText.trim())));
		for (const expected of exactRecords) {
			assert.ok(rows.some(row => expected.every(value => row.some(cell => cell === value || Number.isFinite(number(value)) && (cell.match(/[-+]?\d[\d,]*(?:\.\d+)?/g) ?? []).some(token => number(token) === number(value))))), `visible table must include all requested fields for ${expected[0]}`);
		}
		assert.deepEqual([...new Set(rows.flatMap(row => row.flatMap(cell => cell.match(/\bR\d{3}\b/g) ?? [])))].sort(), exactRecords.map(row => row[0]).sort(), 'visible record tables must not silently show unrelated records');
	}
	return { componentNames: components.map(node => node.typeName), nestedLayouts, charts: await chartGeometry(presentation) };
}

for (const task of cases) test(`${task.id} ${provider}/${model} chooses an appropriate real OpenUI answer`, async ({}, testInfo) => {
	if (task.kind === 'trend') test.setTimeout(780_000);
	const timings = {};
	const started = performance.now();
	const runtimeErrors = [];
	const diagnostics = {};
	let app, page, profile, workspace, secret = '', completed = false, answer;
	const redact = text => String(text).replaceAll(secret || '\0UNSET_SECRET', '[REDACTED]').replace(/(Bearer\s+)[\w.+/=-]+/gi, '$1[REDACTED]');
	const output = async (name, value, contentType = 'application/json') => {
		const body = redact(typeof value === 'string' ? value : JSON.stringify(value, null, 2));
		if (profile) { await mkdir(join(profile, 'evidence'), { recursive: true }); await writeFile(join(profile, 'evidence', name), body); }
		const path = testInfo.outputPath(name);
		await mkdir(dirname(path), { recursive: true });
		await writeFile(path, body);
		await testInfo.attach(name, { path, contentType });
	};
	try {
		let auth;
		try { auth = JSON.parse(await readFile(authPath(), 'utf8')); }
		catch { throw new Error('The configured auth.json could not be read or decoded. Credential contents are intentionally omitted.'); }
		const credentialKey = `apikey:${provider}`;
		assert.equal(typeof auth[credentialKey], 'string', `a stored ${provider} credential is required; configure FELLA_E2E_AUTH_FILE if needed`);
		secret = auth[credentialKey];
		assert.ok(secret.trim(), 'the selected provider credential must not be empty');
		await access(enginePath()); await access(join(root, 'build', 'index.html'));
		profile = await mkdtemp(join(tmpdir(), `fella-openui-adaptive-v1-${task.id}-`));
		workspace = join(profile, 'adaptive-sales'); await mkdir(workspace);
		await writeFile(join(workspace, 'sales.csv'), sales);
		await writeFile(join(profile, 'auth.json'), JSON.stringify({ [credentialKey]: secret }), { mode: 0o600 });
		if (process.platform !== 'win32') await chmod(join(profile, 'auth.json'), 0o600);
		const launchStarted = performance.now();
		app = await electron.launch({ args: [join(root, 'electron', 'main.mjs'), `--user-data-dir=${join(profile, 'chromium')}`], cwd: root,
			env: { ...process.env, OPENAI_API_KEY: '', ANTHROPIC_API_KEY: '', OLLAMA_API_KEY: '', FELLA_DATA_DIR: profile, FELLA_ENGINE_PATH: enginePath(), FELLA_ELECTRON_URL: '' } });
		app.process().stderr?.on('data', chunk => runtimeErrors.push({ kind: 'process-stderr', message: redact(chunk.toString()) }));
		page = await app.firstWindow();
		page.on('pageerror', error => runtimeErrors.push({ kind: 'pageerror', message: redact(error.stack || error.message) }));
		page.on('console', message => { if (message.type() === 'error') runtimeErrors.push({ kind: 'console-error', message: redact(message.text()) }); });
		await page.setViewportSize({ width: 1440, height: 1000 });
		await page.waitForLoadState('domcontentloaded');
		await page.getByRole('combobox', { name: /^(Ask a question|Ask about )/ }).waitFor({ timeout: 30_000 });
		timings.desktop_ready_ms = performance.now() - launchStarted;
		const settings = await invoke(page, 'set_settings', { settings: { provider, model } });
		assert.equal(settings.provider, provider); assert.equal(settings.model, model); assert.equal(settings.has_credential, true);
		await app.evaluate(({ dialog }, folder) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] }); }, workspace);
		const mountStarted = performance.now();
		await page.getByRole('button', { name: /^Switch workspace\. Current:/ }).click();
		await page.getByRole('menuitem', { name: 'Add workspace', exact: true }).click();
		const composer = page.getByRole('combobox', { name: /Ask about/ });
		await expect(composer).toBeVisible({ timeout: 30_000 });
		timings.workspace_ready_ms = performance.now() - mountStarted;
		const documentMarker = await page.evaluate(() => {
			const marker = crypto.randomUUID();
			Object.defineProperty(document, '__fellaAdaptiveDocumentMarker', { value: marker, configurable: false });
			return marker;
		});
		diagnostics.documentMarker = documentMarker;
		const journey = task.kind === 'trend' ? [task, { ...cases.find(candidate => candidate.kind === 'scalar'), id: 'A2-follow-up-A3-scalar' }] : [task];
		let originalPresentation, originalAnswer, originalRun, originalPath, originalBytes;
		for (const [stepIndex, currentTask] of journey.entries()) {
		const stepDiagnostics = stepIndex === 0 ? diagnostics : (diagnostics.followUp = {});
		const stepTimings = stepIndex === 0 ? timings : (timings.follow_up = {});
		const count = await page.locator('.msg.assistant').count();
		const askStarted = performance.now();
		await composer.fill(currentTask.question); await composer.press('Enter');
		await expect(page.locator('.msg.user').last()).toContainText(currentTask.question);
		answer = page.locator('.msg.assistant').nth(count);
		await expect.poll(async () => {
			if (await page.locator('.msg.assistant').count() <= count) return false;
			return !!await answer.getAttribute('data-analysis-turn-id') && !(await answer.locator('.thinking, .text.rich.pending, .assistant-heading .status').count());
		}, { timeout: 300_000, intervals: [500, 1000, 2000] }).toBe(true);
		stepTimings.analysis_complete_ms = performance.now() - askStarted;
		const turnId = await answer.getAttribute('data-analysis-turn-id');
		assert.match(turnId, /^[A-Za-z0-9_-]+$/);
		const run = await invoke(page, 'analysis_turn_load', { turnId });
		stepDiagnostics.run = run;
		await output(`${stepIndex ? 'follow-up-' : ''}same-turn.openui`, run.result?.presentation_program ?? '(missing)', 'text/plain');
		assert.equal(run.question, currentTask.question, 'canonical original request must remain unchanged');
		assert.equal(typeof run.result?.presentation_program, 'string', 'a real same-turn OpenUI answer is required');
		await expect(answer.getByRole('button', { name: /^(Save to Studio|Open in Studio)$/ })).toBeVisible({ timeout: 30_000 });
		const artifacts = await invoke(page, 'artifact_list', { workspaceId: workspace });
		const artifact = artifacts.find(record => record.revisions.some(revision => revision.run_id === turnId));
		assert.ok(artifact, 'the real OpenUI answer must persist as an artifact');
		const revision = artifact.revisions.find(revision => revision.run_id === turnId);
		const resolved = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id, revision: revision.revision });
		stepDiagnostics.resolved = resolved;
		await output(`${stepIndex ? 'follow-up-' : ''}persisted.openui`, resolved.revision.program, 'text/plain');
		const canonicalPath = join(profile, 'analysis', 'turns', `${turnId}.json`);
		const canonicalBytes = await readFile(canonicalPath, 'utf8');
		assert.deepEqual(JSON.parse(canonicalBytes), run, 'the canonical run returned to the UI must be stored durably');
		assert.equal(resolved.run.result.presentation_program, run.result.presentation_program);
		assertCanonical(currentTask, resolved.results);
		const presentation = answer.locator('.analytical-presentation');
		await expect(presentation).toBeVisible({ timeout: 30_000 });
		stepDiagnostics.documentMarkerAfterRender = await page.evaluate(() => document.__fellaAdaptiveDocumentMarker ?? null);
		assert.equal(stepDiagnostics.documentMarkerAfterRender, documentMarker, 'the OpenUI answer must render in the original Document without a page/window reload');
		await expect(answer.locator('.view-note, .fallback, .fallback-result')).toHaveCount(0);
		await expect(presentation.getByText(/Visualization unavailable|This presentation could not be displayed/i)).toHaveCount(0);
		const { schema } = JSON.parse(await readFile(join(root, 'backend', 'resources', 'presentation-library.json'), 'utf8'));
		stepDiagnostics.observations = {};
		Object.assign(stepDiagnostics.observations, await gradePresentation(currentTask, presentation, resolved.revision.program, resolved.results, schema, stepDiagnostics.observations));
		stepTimings.interactive_answer_ms = performance.now() - askStarted;
		assert.equal(await readFile(canonicalPath, 'utf8'), canonicalBytes, 'presentation must not rewrite canonical analysis');
		assert.deepEqual((await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id, revision: revision.revision })).results, resolved.results, 'exact result bindings must persist after rendering');
		if (stepIndex === 0 && task.kind === 'trend') {
			originalPresentation = await presentation.elementHandle();
			originalAnswer = answer;
			originalRun = run;
			originalPath = canonicalPath;
			originalBytes = canonicalBytes;
			const image = await answer.screenshot({ timeout: 10_000 });
			await testInfo.attach('original-trend-answer.png', { body: image, contentType: 'image/png' });
			await writeFile(join(profile, 'evidence', 'original-trend-answer.png'), image);
		} else if (stepIndex > 0) {
			assert.notEqual(run.id, originalRun.id, 'the follow-up must produce its own analytical turn');
			assert.equal(run.conversation_id, originalRun.conversation_id, 'the follow-up must stay in the original chat');
			await expect(page.locator('.msg.assistant')).toHaveCount(count + 1);
			assert.equal(await originalPresentation.evaluate(element => element.isConnected && element.ownerDocument === document), true, 'the first answer must not be remounted to make the second answer render');
			await expect(originalAnswer.locator('.analytical-presentation')).toBeVisible();
			stepDiagnostics.retainedOriginalCharts = await chartGeometry(originalAnswer.locator('.analytical-presentation'));
			assert.ok(stepDiagnostics.retainedOriginalCharts.some(chart => chart.visible && chart.width > 0 && chart.height > 0 && chart.intrinsicWidth > 0 && chart.intrinsicHeight > 0 && chart.graphical), 'the original trend chart must remain rendered after the scalar follow-up');
			assert.equal(await readFile(originalPath, 'utf8'), originalBytes, 'the follow-up must not change the original canonical trend answer');
			assert.equal(await page.evaluate(() => document.__fellaAdaptiveDocumentMarker ?? null), documentMarker, 'both consecutive answers must use the original browser Document');
		}
		}
		const fatal = runtimeErrors.filter(error => error.kind === 'pageerror' || error.kind === 'console-error' && /openui|presentation|vega|chart compilation|rendering failed|uncaught/i.test(error.message));
		assert.deepEqual(fatal, [], 'uncaught renderer or presentation/chart runtime errors must not be hidden by a successful DOM check');
		completed = true;
	} finally {
		const cleanupProblems = [];
		if (page && !page.isClosed()) {
			try {
				await page.screenshot({ path: testInfo.outputPath('desktop.png'), fullPage: true, timeout: 10_000 });
				await testInfo.attach('desktop.png', { path: testInfo.outputPath('desktop.png'), contentType: 'image/png' });
				if (profile) { await mkdir(join(profile, 'evidence'), { recursive: true }); await writeFile(join(profile, 'evidence', 'desktop.png'), await readFile(testInfo.outputPath('desktop.png'))); }
			} catch (error) { cleanupProblems.push(`desktop screenshot: ${redact(error.message)}`); }
			try {
				if (answer && await answer.isVisible()) {
					const image = await answer.screenshot({ timeout: 10_000 });
					await testInfo.attach('answer.png', { body: image, contentType: 'image/png' });
					if (profile) await writeFile(join(profile, 'evidence', 'answer.png'), image);
				}
				await output('visible-desktop.txt', await page.locator('body').innerText(), 'text/plain');
			} catch (error) { cleanupProblems.push(`answer/DOM capture: ${redact(error.message)}`); }
		}
		try { await app?.close(); } catch (error) { cleanupProblems.push(`desktop close: ${redact(error.message)}`); }
		if (profile) {
			await rm(join(profile, 'auth.json'), { force: true });
			await rm(join(profile, 'auth.json.tmp'), { force: true });
		}
		timings.total_ms = performance.now() - started;
		let candidateRevision = 'unavailable', candidateWorktree = 'unavailable';
		try { candidateRevision = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(); } catch { /* diagnostic only */ }
		try { candidateWorktree = execFileSync('git', ['status', '--short'], { cwd: root, encoding: 'utf8' }).trim(); } catch { /* diagnostic only */ }
		await output('runtime-errors.json', runtimeErrors);
		await output('canonical-output.json', diagnostics);
		await output('acceptance.json', { contract: 'OPENUI-ADAPTIVE-v1.1', task, provider, model, candidateRevision, candidateWorktree, platform: process.platform, timings, accepted: completed, profileRetained: !completed, profile: !completed ? profile ?? null : null, cleanupProblems, oracle: { categoryTotals, categoryChanges, monthlyTotals, scalarQ4: 1410, exactRecords } });
		if (completed && profile) await rm(profile, { recursive: true, force: true });
	}
});
