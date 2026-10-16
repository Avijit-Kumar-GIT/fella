// A diagnostic runner, not an analytics benchmark or a release gate.
// Usage: node scripts/profile-desktop.mjs --output test-results/performance/<run>
//   [--executable dist-electron/linux-unpacked/fella] [--live]
//   [--auth-file <existing auth.json>] [--startup-runs 3] [--startup-only]
import { _electron as electron, expect } from '@playwright/test';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { parseArgs } from 'node:util';
import { appendFile, chmod, copyFile, mkdir, readFile, readdir, stat, unlink, writeFile } from 'node:fs/promises';
import { cpus, homedir, platform, release, totalmem } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const run = promisify(execFile);
const { values: options } = parseArgs({ options: {
	output: { type: 'string' }, executable: { type: 'string' }, live: { type: 'boolean' },
	'auth-file': { type: 'string' }, 'startup-runs': { type: 'string', default: '3' },
	'startup-only': { type: 'boolean' }, 'dev-url': { type: 'string' }, 'returning-profile': { type: 'string' }
} });
const output = resolve(options.output ?? join(root, 'test-results/performance', new Date().toISOString().replaceAll(':', '-')));
const executable = options.executable ? resolve(options.executable) : undefined;
const startupRuns = Number(options['startup-runs']);
if (!Number.isInteger(startupRuns) || startupRuns < 1 || startupRuns > 10) throw new Error('startup-runs must be 1–10');
if (options['returning-profile'] && !options['startup-only']) throw new Error('returning-profile requires startup-only');
const authFile = options['auth-file'] ?? join(homedir(), '.local/share/dev.fella.app/auth.json');
const now = () => performance.now();
const round = (value) => Math.round(value * 10) / 10;
const records = [];
await mkdir(join(output, 'screenshots'), { recursive: true });

async function record(kind, data) {
	const row = { at: new Date().toISOString(), kind, ...data };
	records.push(row);
	await appendFile(join(output, 'measurements.jsonl'), `${JSON.stringify(row)}\n`);
	console.log(JSON.stringify({ kind, name: data.name ?? data.case_id ?? data.label, status: data.status, elapsed_ms: data.elapsed_ms }));
}

async function measure(name, action, metadata = {}) {
	const start = now();
	try {
		const result = await action();
		await record('operation', { name, ...metadata, status: 'passed', elapsed_ms: round(now() - start), result });
		return result;
	} catch (error) {
		await record('operation', { name, ...metadata, status: 'failed', elapsed_ms: round(now() - start), error: String(error.stack ?? error) });
		return null;
	}
}

async function treeMemory(pid) {
	if (platform() !== 'linux') return { available: false, reason: 'Full process-tree sampling currently uses Linux /proc.' };
	const entries = (await readdir('/proc')).filter((entry) => /^\d+$/.test(entry));
	const snapshot = (await Promise.all(entries.map(async (entry) => {
		try {
			const text = await readFile(`/proc/${entry}/status`, 'utf8');
			return { pid: Number(entry), parent: Number(text.match(/^PPid:\s+(\d+)/m)?.[1]), name: text.match(/^Name:\s+(.+)/m)?.[1], rss_kib: Number(text.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0), state: text.match(/^State:\s+(.+)/m)?.[1] };
		} catch { return null; }
	}))).filter(Boolean);
	const selected = new Set([pid]);
	for (let changed = true; changed;) {
		changed = false;
		for (const row of snapshot) if (selected.has(row.parent) && !selected.has(row.pid)) { selected.add(row.pid); changed = true; }
	}
	const processes = await Promise.all(snapshot.filter((row) => selected.has(row.pid)).map(async (row) => {
		try {
			const rollup = await readFile(`/proc/${row.pid}/smaps_rollup`, 'utf8');
			return { ...row, pss_kib: Number(rollup.match(/^Pss:\s+(\d+)/m)?.[1] ?? 0) };
		} catch { return row; }
	}));
	return { available: true, process_count: processes.length, rss_mib: round(processes.reduce((sum, item) => sum + item.rss_kib, 0) / 1024), pss_mib: round(processes.reduce((sum, item) => sum + (item.pss_kib ?? 0), 0) / 1024), processes };
}

async function launch(profile, label) {
	await mkdir(profile, { recursive: true, mode: 0o700 });
	const started = now();
	const app = await electron.launch({
		...(executable ? { executablePath: executable } : {}),
		args: [...(executable ? [] : [join(root, 'electron/main.mjs')]), `--user-data-dir=${join(profile, 'chromium')}`],
		cwd: executable ? dirname(executable) : root,
		env: { ...process.env, FELLA_DATA_DIR: profile, FELLA_ELECTRON_URL: options['dev-url'] ?? '', FELLA_ENGINE_PATH: executable ? '' : join(root, 'electron/engine', `fella-engine-${process.arch}${platform() === 'win32' ? '.exe' : ''}`) },
		timeout: 30_000
	});
	const debugReady = now();
	const page = await app.firstWindow({ timeout: 30_000 });
	const rendererErrors = [];
	page.on('pageerror', (error) => rendererErrors.push(error.message));
	const processLog = [];
	app.process().stderr?.on('data', (chunk) => processLog.push(chunk.toString()));
	await page.getByRole('combobox', { name: /^Ask a question$|^Ask about / }).first().waitFor({ state: 'visible', timeout: 30_000 });
	const composerReady = now();
	const pong = await page.evaluate(() => window.fella.invoke('ping'));
	if (pong !== 'pong') throw new Error('Engine ping did not return pong');
	const ready = now();
	const navigation = await page.evaluate(async () => {
		await new Promise((resolveFrame) => requestAnimationFrame(() => requestAnimationFrame(resolveFrame)));
		const nav = performance.getEntriesByType('navigation')[0];
		return { navigation: nav?.toJSON(), paint: performance.getEntriesByType('paint').map((entry) => entry.toJSON()), resources: performance.getEntriesByType('resource').map((entry) => ({ name: entry.name.replace(/.*\//, ''), duration: entry.duration, decoded_bytes: entry.decodedBodySize, encoded_bytes: entry.encodedBodySize })) };
	});
	await record('startup', { label, profile, mode: options['dev-url'] ? 'vite-development' : executable ? 'packaged' : 'production-assets-source-launch', status: 'passed', elapsed_ms: round(ready - started), launch_to_debug_ready_ms: round(debugReady - started), launch_to_composer_ms: round(composerReady - started), ping_ms: round(ready - composerReady), ...navigation, versions: await app.evaluate(({ app: instance }) => ({ packaged: instance.isPackaged, ...process.versions })) });
	return { app, page, rendererErrors, processLog, profile, label };
}

async function close(instance) {
	if (!instance) return;
	const { app, rendererErrors, processLog, profile, label } = instance;
	const memory = await treeMemory(app.process().pid);
	const started = now();
	await app.close();
	await new Promise((resolveWait) => setTimeout(resolveWait, 350));
	const survivors = [];
	for (const child of memory.processes ?? []) {
		try { const status = await readFile(`/proc/${child.pid}/status`, 'utf8'); if (!/^State:\s+Z/m.test(status)) survivors.push(child.pid); } catch { /* Process exited. */ }
	}
	await record('shutdown', { label, elapsed_ms: round(now() - started), renderer_errors: rendererErrors, surviving_owned_pids: survivors, status: survivors.length || rendererErrors.length ? 'failed' : 'passed' });
	await writeFile(join(profile, 'stderr.log'), processLog.join(''));
	await unlink(join(profile, 'auth.json')).catch(() => {});
}

function csv(text) {
	const rows = [];
	let row = [], field = '', quoted = false;
	for (let index = 0; index < text.length; index++) {
		const character = text[index];
		if (character === '"') { if (quoted && text[index + 1] === '"') { field += '"'; index++; } else quoted = !quoted; }
		else if (character === ',' && !quoted) { row.push(field); field = ''; }
		else if (character === '\n' && !quoted) { row.push(field.replace(/\r$/, '')); rows.push(row); row = []; field = ''; }
		else field += character;
	}
	if (field || row.length) { row.push(field); rows.push(row); }
	const header = rows.shift();
	return rows.map((values) => Object.fromEntries(header.map((key, index) => [key, values[index]])));
}

async function prepareFixtures() {
	const directory = join(output, 'workspaces');
	await mkdir(directory, { recursive: true });
	const northstar = join(directory, 'northstar');
	await run(process.execPath, [join(root, 'scripts/generate-analyst-sample.mjs'), northstar]);
	const housing = join(root, 'bench/fqa-bench/suites/clarification-housing/workspaces/housing');
	const large = join(directory, 'million-rows');
	await mkdir(large, { recursive: true });
	for (let source = 0; source < 500; source++) {
		const rows = ['id,month,region,amount'];
		for (let row = 0; row < 2000; row++) rows.push(`${source * 2000 + row},2025-${String(row % 12 + 1).padStart(2, '0')},${['North', 'South'][row % 2]},1.25`);
		await writeFile(join(large, `part-${String(source).padStart(3, '0')}.csv`), `${rows.join('\n')}\n`);
	}
	for (let index = 0; index < 20; index++) await writeFile(join(large, `note-${index}.md`), '# Fictional performance fixture\nAmounts are USD. Every CSV contains 2,000 rows.\n');
	const readCsv = async (name) => csv(await readFile(join(northstar, name), 'utf8'));
	const orders = new Map((await readCsv('commerce/orders/orders_2025.csv')).map((order) => [order.order_id, order]));
	const quarters = { Q3: { merchandise: 0, refunds: 0 }, Q4: { merchandise: 0, refunds: 0 } };
	const quarter = (date) => date >= '2025-07-01' && date < '2025-10-01' ? 'Q3' : date >= '2025-10-01' && date < '2026-01-01' ? 'Q4' : null;
	for (const line of await readCsv('commerce/order_lines.csv')) {
		const order = orders.get(line.order_id), key = quarter(order?.order_date ?? '');
		if (key && order.status.toLowerCase() !== 'cancelled') quarters[key].merchandise += Number(line.quantity) * Number(line.unit_price_usd) - Number(line.line_discount_usd);
	}
	const refunds = new Map((await readCsv('finance/refund_transactions.csv')).map((refund) => [refund.refund_id, refund]));
	for (const refund of refunds.values()) { const key = quarter(refund.refund_date); if (key && refund.status === 'posted') quarters[key].refunds += Number(refund.amount_usd); }
	const gold = { Q3: Number((quarters.Q3.merchandise - quarters.Q3.refunds).toFixed(2)), Q4: Number((quarters.Q4.merchandise - quarters.Q4.refunds).toFixed(2)) };
	gold.change = Number((gold.Q4 - gold.Q3).toFixed(2));
	await record('fixture_oracle', { quarters, gold, large_expected_rows: 1_000_000, large_expected_table_sources: 500, large_expected_documents: 20, large_expected_amount_total: 1_250_000 });
	return { housing, northstar, large, gold };
}

const catalogSummary = (catalog) => ({ workspace: catalog.workspace, revision: catalog.revision, sources: catalog.sources?.length, tables: catalog.sources?.filter((source) => source.view).length, rows: catalog.sources?.reduce((sum, source) => sum + (source.row_count ?? 0), 0), skipped: catalog.skipped, bytes: catalog.sources?.reduce((sum, source) => sum + (source.size_bytes ?? 0), 0) });

async function mount(instance, path, label) {
	const { page, app } = instance;
	await app.evaluate(({ dialog }, selected) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [selected] }); }, path);
	return measure('ui-mount', async () => {
		await page.getByRole('button', { name: 'Add repository', exact: true }).click();
		await page.getByRole('combobox', { name: `Ask about ${basename(path)}`, exact: true }).waitFor({ state: 'visible', timeout: 120_000 });
		return catalogSummary(await page.evaluate((workspaceId) => window.fella.invoke('get_catalog', { workspaceId }), path));
	}, { label });
}

async function rpcMeasurements(instance, path, label) {
	const { page } = instance;
	for (let repetition = 1; repetition <= 3; repetition++) await measure('repeat-open-workspace', () => page.evaluate(async (path) => {
		const progress = [], started = performance.now();
		const catalog = await window.fella.openWorkspace(path, (event) => progress.push({ at_ms: performance.now() - started, ...event }));
		return { source_count: catalog.sources.length, row_count: catalog.sources.reduce((sum, item) => sum + (item.row_count ?? 0), 0), progress };
	}, path), { label, repetition });
	for (const method of ['get_catalog', 'get_workspace_model', 'conversations_list', 'investigations_list']) {
		for (let repetition = 1; repetition <= 5; repetition++) await measure(method, () => page.evaluate(async ({ method, path }) => {
			const value = await window.fella.invoke(method, { workspaceId: path });
			return { serialized_bytes: JSON.stringify(value).length };
		}, { method, path }), { label, repetition });
	}
}

async function interactions(instance) {
	const { page } = instance;
	await measure('tools-open', async () => { await page.getByRole('button', { name: 'Open workspace tools', exact: true }).click(); await page.getByTestId('workspace-tool-chooser').waitFor({ state: 'visible' }); });
	await measure('investigation-create', async () => { await page.getByTestId('workspace-tool-investigations').click(); await page.getByTestId('workspace-tool-new-investigation').click(); await page.getByRole('tablist', { name: 'Investigation sections' }).waitFor({ state: 'visible' }); });
	for (const section of ['Data', 'Questions', 'Analysis', 'Results', 'Share', 'Brief']) await measure(`investigation-section-${section.toLowerCase()}`, async () => {
		const tab = page.getByRole('tablist', { name: 'Investigation sections' }).getByRole('tab', { name: section, exact: true });
		await tab.click(); await expect(tab).toHaveAttribute('aria-selected', 'true');
		await page.locator(`#${await tab.getAttribute('aria-controls')}`).waitFor({ state: 'visible' });
	});
	await measure('data-context-open', async () => { await page.getByTestId('workspace-tool-add').click(); await page.getByTestId('workspace-tool-context').click(); await page.getByRole('heading', { name: 'Data & context', exact: true }).waitFor({ state: 'visible' }); await expect(page.locator('.context-page .loading, .context-page .knowledge-loading')).toHaveCount(0); });
	await measure('shell-open', async () => { await page.getByTestId('workspace-tool-add').click(); await page.getByTestId('workspace-tool-terminal').click(); await page.locator('.terminal-surface .xterm-rows').waitFor({ state: 'visible' }); await expect.poll(() => page.locator('.terminal-surface .xterm-rows').innerText(), { timeout: 15_000 }).not.toBe(''); });
	await page.screenshot({ path: join(output, 'screenshots', 'workspace-tools.png') });
	await measure('tools-close', async () => { await page.getByRole('button', { name: 'Close workspace tools', exact: true }).click(); await page.locator('.workspace-tools-pane').waitFor({ state: 'hidden' }); });
	await record('memory', { label: 'loaded-with-shell', ...await treeMemory(instance.app.process().pid) });
}

const tasks = [
	{ id: 'general', prompt: 'What is 17 multiplied by 23? Reply with just the number.', expected: [391] },
	{ id: 'housing-total', prompt: 'How much did rent and utilities total in Q1 2024? Exclude repairs and maintenance.', expected: [4647] },
	{ id: 'housing-chart', prompt: 'Show monthly rent plus utilities totals for January, February, and March 2024 in a chart. Exclude repairs and maintenance.', expected: [1545, 1542, 1560] },
	{ id: 'northstar', prompt: 'What was total net merchandise revenue for Northstar Outfitters in Q3 and Q4 2025, and how much did it change? Use the current metric policy: merchandise by order date, posted refunds by refund date; deduplicate order and refund IDs, exclude cancelled orders and pending refunds, include posted orphan refund adjustments in the refund period, and exclude tax and shipping.' }
];

async function modelTurn(instance, task, route, fixtures) {
	const { page } = instance;
	await page.evaluate(() => {
		const baseUsers = document.querySelectorAll('.msg.user').length, baseAnswers = document.querySelectorAll('.msg.assistant').length;
		const timing = window.__fellaPerformanceTurn = { started: performance.now(), user_ms: null, first_text_ms: null, complete_ms: null };
		const observer = new MutationObserver(() => {
			const elapsed = performance.now() - timing.started;
			if (document.querySelectorAll('.msg.user').length > baseUsers && timing.user_ms === null) timing.user_ms = elapsed;
			const answers = document.querySelectorAll('.msg.assistant');
			if (answers.length <= baseAnswers) return;
			const answer = answers[answers.length - 1];
			if (answer.querySelector('.text.rich')?.textContent.trim() && timing.first_text_ms === null) timing.first_text_ms = elapsed;
			if (!answer.querySelector('.pending, .thinking') && (answer.textContent.trim() || document.querySelector('.clarification-field'))) { timing.complete_ms = elapsed; observer.disconnect(); }
		});
		observer.observe(document.body, { childList: true, characterData: true, subtree: true });
	});
	const started = now();
	try {
		const composer = page.getByRole('combobox', { name: /^Ask a question$|^Ask about / }).first();
		await composer.fill(task.prompt);
		await composer.press('Enter');
		await expect.poll(() => page.evaluate(() => window.__fellaPerformanceTurn.complete_ms), { timeout: 180_000, intervals: [100, 250, 500] }).not.toBeNull();
		const timing = await page.evaluate(() => window.__fellaPerformanceTurn);
		const assistant = page.locator('.msg.assistant').last();
		const text = (await assistant.innerText()).trim();
		const chartCount = await assistant.locator('.chart-card').count();
		let saved;
		await expect.poll(async () => {
			saved = await page.evaluate((prompt) => {
				for (const key of Object.keys(localStorage).filter((key) => key.startsWith('fella:conversation:'))) {
					const conversation = JSON.parse(localStorage.getItem(key));
					const index = conversation.messages?.findLastIndex((message) => message.role === 'user' && message.text === prompt);
					if (index >= 0) { const response = conversation.messages.slice(index + 1).find((message) => message.role === 'assistant' && message.answer); if (response) return { id: conversation.id, response }; }
				}
				return null;
			}, task.prompt);
			return Boolean(saved);
		}, { timeout: 10_000 }).toBe(true);
		const answer = saved.response.answer;
		const numbers = (answer.text ?? saved.response.text ?? '').match(/-?\d[\d,]*(?:\.\d+)?/g)?.map((value) => Number(value.replaceAll(',', ''))) ?? [];
		const chartNumbers = (answer.evidence ?? []).filter((item) => item.chart).flatMap((item) => item.chart.series?.flatMap((series) => series.values ?? []) ?? []);
		const expected = task.id === 'northstar' ? [fixtures.gold.Q3, fixtures.gold.Q4, fixtures.gold.change] : task.expected;
		const numericScreen = expected.every((gold) => (task.id === 'housing-chart' ? chartNumbers : numbers).some((actual) => Math.abs(actual - gold) <= 0.02));
		const checks = { persisted_answer: true, numerical_screen: numericScreen, ...(task.id === 'housing-chart' ? { chart_rendered: chartCount > 0 } : {}), ...(task.id !== 'general' ? { evidence_present: Boolean(answer.evidence?.length) } : {}) };
		const failed = Object.values(checks).some((value) => !value);
		await record('model_turn', { case_id: task.id, route, status: failed ? 'failed' : 'passed', elapsed_ms: round(now() - started), prompt: task.prompt, expected, checks, timing, chart_count: chartCount, answer, rendered_text: text, conversation_id: saved.id });
		await assistant.screenshot({ path: join(output, 'screenshots', `${route.provider}-${task.id}.png`) });
		return answer.status !== 'failed' && !/invalid_api_key|authentication_error|model_not_found|model service returned|can't reach|could not reach|401 Unauthorized|404 Not Found/i.test(text);
	} catch (error) {
		await record('model_turn', { case_id: task.id, route, status: 'failed', elapsed_ms: round(now() - started), error: String(error.stack ?? error), timing: await page.evaluate(() => window.__fellaPerformanceTurn), rendered_text: await page.locator('.msg.assistant').last().innerText().catch(() => '') });
		await page.screenshot({ path: join(output, 'screenshots', `${route.provider}-${task.id}-failure.png`) }).catch(() => {});
		return false;
	}
}

const git = (await run('git', ['rev-parse', 'HEAD'], { cwd: root })).stdout.trim();
await record('manifest', { protocol: 'desktop-performance-v1.1', git_revision: git, harness_sha256: createHash('sha256').update(await readFile(fileURLToPath(import.meta.url))).digest('hex'), os: platform(), release: release(), cpu: cpus()[0]?.model, logical_cpus: cpus().length, total_memory_bytes: totalmem(), node: process.version, executable: executable ?? 'node_modules/electron', output, live: Boolean(options.live), startup_runs: startupRuns, cold_cache: false, returning_profile: options['returning-profile'] ?? null });

for (let repetition = 1; repetition <= startupRuns; repetition++) {
	let instance;
	const profile = options['returning-profile'] ? resolve(options['returning-profile']) : join(output, 'profiles', `fresh-${repetition}`);
	const label = options['returning-profile'] ? `returning-authenticated-${repetition}` : `fresh-${repetition}`;
	if (options['returning-profile'] && options['auth-file']) { await copyFile(authFile, join(profile, 'auth.json')); await chmod(join(profile, 'auth.json'), 0o600); }
	await measure(options['returning-profile'] ? 'returning-authenticated-startup' : 'fresh-startup', async () => { instance = await launch(profile, label); await record('memory', { label, ...await treeMemory(instance.app.process().pid) }); }, { repetition });
	await close(instance);
}

if (!options['startup-only']) {
	const fixtures = await prepareFixtures();
	const routes = options.live ? [{ provider: 'ollama-cloud', model: 'gemma4:31b' }, { provider: 'openai', model: 'gpt-5.6-luna' }] : [{ provider: 'none', model: 'none' }];
	for (const route of routes) {
		const profile = join(output, 'profiles', route.provider);
		await mkdir(profile, { recursive: true, mode: 0o700 });
		if (options.live) { await copyFile(authFile, join(profile, 'auth.json')); await chmod(join(profile, 'auth.json'), 0o600); }
		let instance;
		try {
			instance = await launch(profile, `journey-${route.provider}`);
			if (options.live) {
				const settings = await instance.page.evaluate((route) => window.fella.invoke('set_settings', { settings: route }), route);
				await record('provider', { route, has_credential: settings.has_credential });
				await instance.page.reload();
				await instance.page.getByRole('combobox', { name: 'Ask a question', exact: true }).waitFor();
			}
			let available = !options.live || await modelTurn(instance, tasks[0], route, fixtures);
			await mount(instance, fixtures.housing, 'housing');
			if (options.live && available) available = await modelTurn(instance, tasks[1], route, fixtures);
			if (options.live && available) available = await modelTurn(instance, tasks[2], route, fixtures);
			await mount(instance, fixtures.northstar, 'northstar');
			if (options.live && available) available = await modelTurn(instance, tasks[3], route, fixtures);
			if (route === routes[0]) {
				await rpcMeasurements(instance, fixtures.northstar, 'northstar');
				await interactions(instance);
				await mount(instance, fixtures.large, 'million-rows');
				await rpcMeasurements(instance, fixtures.large, 'million-rows');
				await record('memory', { label: 'million-rows-loaded', ...await treeMemory(instance.app.process().pid) });
			}
			await record('history', { label: route.provider, conversations: await instance.page.evaluate(() => window.fella.invoke('conversations_list')), run_log: await instance.page.evaluate(() => window.fella.invoke('run_log_recent', { limit: 20 })) });
		} catch (error) {
			await record('journey_failure', { label: route.provider, status: 'failed', error: String(error.stack ?? error) });
		} finally { await close(instance); await unlink(join(profile, 'auth.json')).catch(() => {}); }
		if (route === routes[0]) for (let repetition = 1; repetition <= startupRuns; repetition++) {
			let returning;
			await measure('returning-startup', async () => { returning = await launch(profile, `returning-${repetition}`); await record('memory', { label: `returning-${repetition}`, ...await treeMemory(returning.app.process().pid) }); }, { repetition });
			await close(returning);
		}
	}
}

await writeFile(join(output, 'summary.json'), JSON.stringify({ protocol: 'desktop-performance-v1', records }, null, 2));
console.log(`Performance evidence: ${output}`);
process.exitCode = records.some((row) => row.status === 'failed') ? 1 : 0;
