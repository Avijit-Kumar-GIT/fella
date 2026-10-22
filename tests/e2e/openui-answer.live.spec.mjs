import assert from 'node:assert/strict';
import { access, chmod, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { expect, test, _electron as electron } from '@playwright/test';

const root = resolve('.');

function authPath() {
	if (process.env.FELLA_E2E_AUTH_FILE) return resolve(process.env.FELLA_E2E_AUTH_FILE);
	if (process.platform === 'win32') return join(process.env.APPDATA || join(homedir(), 'AppData', 'Roaming'), 'dev.fella.app', 'auth.json');
	if (process.platform === 'darwin') return join(homedir(), 'Library', 'Application Support', 'dev.fella.app', 'auth.json');
	return join(process.env.XDG_DATA_HOME || join(homedir(), '.local', 'share'), 'dev.fella.app', 'auth.json');
}

function enginePath() {
	const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
	return join(root, 'electron', 'engine', `fella-engine-${arch}${process.platform === 'win32' ? '.exe' : ''}`);
}

async function invoke(page, command, args = {}) {
	return page.evaluate(({ command, args }) => window.fella.invoke(command, args), { command, args });
}

test('real model OpenUI answer validates and renders as the primary workspace answer', async () => {
	test.setTimeout(300_000);
	const credentialPath = authPath();
	const auth = JSON.parse(await readFile(credentialPath, 'utf8'));
	assert.equal(typeof auth['apikey:openai'], 'string', `OpenAI credential missing from ${credentialPath}`);
	await access(enginePath()).catch(() => { throw new Error('Electron sidecar is missing; run pnpm electron:build first.'); });

	const profile = await mkdtemp(join(tmpdir(), 'fella-openui-model-'));
	const workspace = join(profile, 'quarterly-sales');
	await writeFile(join(profile, 'auth.json'), await readFile(credentialPath));
	if (process.platform !== 'win32') await chmod(join(profile, 'auth.json'), 0o600);
	await mkdir(workspace);
	await writeFile(join(workspace, 'sales.csv'), [
		'quarter,region,revenue,orders',
		'Q1,North,100,10',
		'Q1,South,40,8',
		'Q2,North,120,12',
		'Q2,South,55,11',
		'Q3,North,90,9',
		'Q3,South,75,15',
		'Q4,North,80,8',
		'Q4,South,110,22'
	].join('\n'));

	let app;
	try {
		app = await electron.launch({
			args: [join(root, 'electron', 'main.mjs'), `--user-data-dir=${join(profile, 'chromium')}`],
			cwd: root,
			env: { ...process.env, FELLA_DATA_DIR: profile, FELLA_ENGINE_PATH: enginePath() }
		});
		const page = await app.firstWindow();
		await page.waitForLoadState('domcontentloaded');
		const settings = await invoke(page, 'set_settings', { settings: { provider: 'openai', model: 'gpt-5.6-luna' } });
		assert.equal(settings.has_credential, true);
		await app.evaluate(({ dialog }, path) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] }); }, workspace);
		await page.getByRole('button', { name: /Switch workspace\. Current:/ }).click();
		await page.getByRole('menuitem', { name: 'Add workspace', exact: true }).click();

		const composer = page.getByRole('combobox', { name: /Ask about/ });
		await expect(composer).toBeVisible();
		const question = 'How did revenue change quarter by quarter for North and South, and how do orders compare? Use a visualization that makes both regional trends easy to compare, with a short explanation grounded in sales.csv.';
		const priorAnswers = await page.locator('.msg.assistant').count();
		await composer.fill(question);
		await composer.press('Enter');
		await expect(page.locator('.msg.user').last()).toContainText(question);
		const answer = page.locator('.msg.assistant').nth(priorAnswers);
		await expect.poll(async () => {
			if (await page.locator('.msg.assistant').count() <= priorAnswers) return false;
			return !(await answer.locator('.text.rich.pending').count()) && !(await answer.locator('.thinking').count());
		}, { timeout: 240_000, intervals: [500, 1000, 2000] }).toBe(true);

		const turnId = await answer.getAttribute('data-analysis-turn-id');
		assert.ok(turnId, 'the real-model response should link its canonical analysis turn');
		const turn = await invoke(page, 'analysis_turn_load', { turnId });
		const program = turn.result?.presentation_program;
		console.log(`REAL_MODEL_OPENUI_PROGRAM\n${program ?? '(missing)'}\nEND_REAL_MODEL_OPENUI_PROGRAM`);
		assert.equal(typeof program, 'string', 'the real model must return the same-turn OpenUI program');
		assert.match(program, /^root\s*=\s*Document\(/, 'the real model program must use the required OpenUI root');

		await expect(answer.getByRole('button', { name: 'Save to Studio' })).toBeVisible({ timeout: 30_000 });
		await expect(answer.locator('.view-note')).toHaveCount(0);
		const artifacts = await invoke(page, 'artifact_list', { workspaceId: workspace });
		assert.ok(artifacts.some((artifact) => artifact.revisions.some((revision) => revision.run_id === turnId)), 'the validated same-turn answer should be persisted as an artifact');
	} finally {
		try { await app?.close(); }
		finally { await rm(profile, { recursive: true, force: true }); }
	}
});
