import assert from 'node:assert/strict';
import { copyFile, mkdtemp, readFile, rm, chmod, access } from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test, _electron as electron } from '@playwright/test';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const fixture = join(
	root,
	'bench',
	'fqa-bench',
	'suites',
	'clarification-housing',
	'workspaces',
	'housing'
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

test('G5: real OpenAI clarification is submitted and resumes the same analysis', async () => {
	test.setTimeout(300_000);
	const authPath = defaultAuthPath();
	const authText = await readFile(authPath, 'utf8');
	const auth = JSON.parse(authText);
	assert.equal(
		typeof auth['apikey:openai'],
		'string',
		`No OpenAI key was found in ${authPath}; set FELLA_E2E_AUTH_FILE to the existing auth.json`
	);
	await access(enginePath()).catch(() => {
		throw new Error('Electron sidecar is missing; run pnpm electron:build first.');
	});

	const dataDir = await mkdtemp(join(tmpdir(), 'fella-electron-live-'));
	const temporaryAuth = join(dataDir, 'auth.json');
	await copyFile(authPath, temporaryAuth);
	if (process.platform !== 'win32') await chmod(temporaryAuth, 0o600);

	let app;
	try {
		app = await electron.launch({
			args: [
				join(root, 'electron', 'main.mjs'),
				`--user-data-dir=${join(dataDir, 'chromium')}`
			],
			cwd: root,
			env: {
				...process.env,
				FELLA_DATA_DIR: dataDir,
				FELLA_ENGINE_PATH: enginePath()
			}
		});
		const page = await app.firstWindow();
		await page.waitForLoadState('domcontentloaded');

		// Isolate the test from native folder-picker UI while retaining the real
		// application click path and workspace mount implementation.
		await app.evaluate(({ dialog }, path) => {
			dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] });
		}, fixture);

		// Point this disposable profile at the user's actual OpenAI credential.
		const settings = await page.evaluate(() =>
			window.fella.invoke('set_settings', {
				settings: { provider: 'openai', model: 'gpt-5.6-luna' }
			})
		);
		assert.equal(settings.provider, 'openai');
		assert.equal(settings.model, 'gpt-5.6-luna');
		assert.equal(settings.has_credential, true);

		await page.getByRole('button', { name: 'Add a repository' }).click();
		const composer = page.getByRole('combobox', { name: 'Ask about housing' });
		await expect(composer).toBeVisible();
		await composer.fill('What was my housing spending in Q1 2024?');
		await composer.press('Enter');

		const clarification = page.locator('.dock .clarification-field');
		await expect(clarification).toBeVisible({ timeout: 120_000 });
		await expect(clarification.locator('.clarification-question')).not.toBeEmpty();
		await expect(clarification.locator('.clarification-options button').first()).toBeVisible();
		await expect(clarification.getByRole('textbox', { name: 'Other interpretation' })).toBeVisible();

		const reply = clarification.getByRole('textbox', { name: 'Other interpretation' });
		await reply.fill('Count rent and utilities, but leave out repairs and maintenance.');
		const continueButton = clarification.getByRole('button', { name: 'Continue with this interpretation' });
		await expect(continueButton).toBeEnabled();
		await continueButton.click();

		// Do not infer submission from an input value: require the new user turn
		// to appear in the rendered transcript before assessing the continuation.
		await expect(page.locator('.msg.user').last()).toContainText(
			'Count rent and utilities, but leave out repairs and maintenance.'
		);
		const finalAnswer = page.locator('.msg.assistant').last();
		const finalText = finalAnswer.locator('.text.rich');
		await expect(finalText).not.toHaveClass(/pending/, { timeout: 120_000 });
		await expect(finalAnswer.locator('.thinking')).toHaveCount(0);
		assert.ok((await finalText.innerText()).trim(), 'the resumed turn should produce a settled response');

		const analysisDetails = finalAnswer.getByRole('button', { name: 'Analysis details' });
		await expect(analysisDetails).toBeVisible();
		await analysisDetails.click();
		await expect(analysisDetails).toHaveAttribute('aria-expanded', 'true');
		await expect(finalAnswer).toContainText('This analysis continues from your clarification.');
		await expect(finalAnswer).toContainText('ledger.csv');
	} finally {
		try {
			await app?.close();
		} finally {
			await rm(dataDir, { recursive: true, force: true });
		}
	}
});
