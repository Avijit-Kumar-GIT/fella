// Supplemental, read-only navigation/query probes against a profile produced
// by profile-desktop.mjs. No model requests are submitted.
import { _electron as electron, expect } from '@playwright/test';
import { appendFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';

const { values: options } = parseArgs({ options: { profile: { type: 'string' }, executable: { type: 'string' }, output: { type: 'string' }, 'windows-workspace': { type: 'string' }, 'switch-only': { type: 'boolean' } } });
if (!options.profile || !options.executable || !options.output) throw new Error('Required: --profile <profile> --executable <Fella> --output <directory>');
const profile = resolve(options.profile), executable = resolve(options.executable), output = resolve(options.output);
const fixtureRoot = join(dirname(dirname(profile)), 'workspaces');
await mkdir(output, { recursive: true });
const rows = [];
const failedNames = new Set();
async function measure(name, operation, repetition) {
	const started = performance.now();
	let row;
	try { row = failedNames.has(name) ? { name, repetition, status: 'not_run', reason: 'The same probe already failed; repeated identical attempts are not informative.' } : { name, repetition, status: 'passed', result: await operation() }; }
	catch (error) { row = { name, repetition, status: 'failed', error: String(error.stack ?? error) }; }
	if (row.status === 'failed') failedNames.add(name);
	row.elapsed_ms = Math.round((performance.now() - started) * 10) / 10;
	rows.push(row);
	await appendFile(join(output, 'navigation.jsonl'), `${JSON.stringify(row)}\n`);
	console.log(JSON.stringify({ name, repetition, status: row.status, elapsed_ms: row.elapsed_ms }));
}

let app;
try {
	app = await electron.launch({ executablePath: executable, args: [`--user-data-dir=${join(profile, 'chromium')}`], cwd: dirname(executable), env: { ...process.env, FELLA_DATA_DIR: profile, FELLA_ELECTRON_URL: '', FELLA_ENGINE_PATH: '' }, timeout: 30_000 });
	const page = await app.firstWindow();
	page.setDefaultTimeout(10_000);
	await measure('early-startup-observation', async () => {
		await page.waitForLoadState('domcontentloaded');
		await page.waitForTimeout(500);
		const observation = await page.evaluate(() => ({ body: document.body.innerText, composers: [...document.querySelectorAll('[role="combobox"]')].map((element) => ({ label: element.getAttribute('aria-label'), visible: Boolean(element.getClientRects().length) })), paint: performance.getEntriesByType('paint').map((entry) => entry.toJSON()) }));
		await page.screenshot({ path: join(output, 'returning-startup-before-workspaces-ready.png') });
		return observation;
	});
	await page.getByRole('combobox', { name: /^Ask about / }).first().waitFor({ state: 'visible', timeout: 30_000 });
	for (let repetition = 1; repetition <= 5; repetition++) for (const workspace of ['housing', 'northstar', 'million-rows']) {
		await measure(`switch-workspace-${workspace}`, async () => {
			await page.getByRole('button', { name: `Open workspace ${workspace}`, exact: true }).click();
			await page.getByRole('combobox', { name: `Ask about ${workspace}`, exact: true }).waitFor({ state: 'visible' });
			return { scope: await page.locator('.workspace-identity').innerText() };
		}, repetition);
	}
	if (!options['switch-only']) {
	await measure('million-row-sql-sum', async () => page.evaluate(async (workspaceId) => {
		const catalog = await window.fella.invoke('get_catalog', { workspaceId });
		const sql = `SELECT SUM(total) AS total FROM (${catalog.sources.filter((source) => source.view).map((source) => `SELECT SUM(amount) AS total FROM "${source.view.replaceAll('"', '""')}"`).join(' UNION ALL ')})`;
		const result = await window.fella.invoke('run_sql_direct', { workspaceId, sql });
		if (result.rows?.[0]?.[0] !== 1250000) throw new Error(`Expected 1,250,000 from independent fixture construction: ${JSON.stringify(result)}`);
		return result;
	}, join(fixtureRoot, 'million-rows')));
	await measure('million-row-reindex', async () => page.evaluate(async (workspaceId) => {
		const catalog = await window.fella.invoke('reindex', { workspaceId });
		const rowCount = catalog.sources.reduce((sum, source) => sum + (source.row_count ?? 0), 0);
		if (rowCount !== 1000000) throw new Error(`Reindex row count: ${rowCount}`);
		return { source_count: catalog.sources.length, rows: rowCount };
	}, join(fixtureRoot, 'million-rows')));
	await page.getByRole('button', { name: 'Open workspace housing', exact: true }).click();
	const expandHousing = page.getByRole('button', { name: 'Expand housing', exact: true });
	if (await expandHousing.isVisible()) await expandHousing.click();
	await measure('open-saved-housing-conversation', async () => {
		await page.getByRole('button', { name: /^Open conversation: How much did rent and utilities/ }).click();
		await expect(page.getByRole('log', { name: 'conversation' })).toContainText('4,647');
		return { messages: await page.locator('.msg').count() };
	});
	await page.getByRole('button', { name: 'Open workspace northstar', exact: true }).click();
	const expandNorthstar = page.getByRole('button', { name: 'Expand northstar', exact: true });
	if (await expandNorthstar.isVisible()) await expandNorthstar.click();
	await measure('open-saved-northstar-conversation', async () => {
		await page.getByRole('button', { name: /^Open conversation: What was total net merchandise revenue/ }).click();
		await expect(page.getByRole('log', { name: 'conversation' })).toContainText('45,714.32');
	});
	if (options['windows-workspace']) await measure('mount-windows-files-through-wsl', async () => {
		await app.evaluate(({ dialog }, path) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] }); }, options['windows-workspace']);
		await page.getByRole('button', { name: 'Add repository', exact: true }).click();
		await page.getByRole('combobox', { name: 'Ask about Fella Analyst Sample Workspace', exact: true }).waitFor({ state: 'visible' });
		const catalog = await page.evaluate((workspaceId) => window.fella.invoke('get_catalog', { workspaceId }), options['windows-workspace']);
		return { sources: catalog.sources.length, rows: catalog.sources.reduce((sum, source) => sum + (source.row_count ?? 0), 0) };
	});
	}
	await page.screenshot({ path: join(output, 'navigation-finished.png') });
} finally {
	await app?.close();
	await writeFile(join(output, 'summary.json'), JSON.stringify({ protocol: 'desktop-performance-navigation-v1', profile, rows }, null, 2));
}
process.exitCode = rows.some((row) => row.status === 'failed') ? 1 : 0;
