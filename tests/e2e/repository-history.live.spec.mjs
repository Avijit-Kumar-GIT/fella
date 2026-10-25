import assert from 'node:assert/strict';
import { access, mkdir, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, test, _electron as electron } from '@playwright/test';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function enginePath() {
	const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
	const suffix = process.platform === 'win32' ? '.exe' : '';
	return join(root, 'electron', 'engine', `fella-engine-${arch}${suffix}`);
}

async function launchApp(dataDir) {
	const engine = enginePath();
	await access(engine).catch(() => {
		throw new Error('Electron sidecar is missing; run pnpm electron:build first.');
	});
	return electron.launch({
		args: [join(root, 'electron', 'main.mjs'), `--user-data-dir=${join(dataDir, 'chromium')}`],
		cwd: root,
		env: {
			...process.env,
			FELLA_DATA_DIR: dataDir,
			FELLA_ENGINE_PATH: engine,
			FELLA_ELECTRON_URL: ''
		}
	});
}

test('missing repository opens as history-only and returns to live after a successful retry', async () => {
	const dataDir = await mkdtemp(join(tmpdir(), 'fella-repository-history-'));
	const workspace = join(dataDir, 'reconnect-me');
	let app;
	try {
		app = await launchApp(dataDir);
		const page = await app.firstWindow();
		await page.waitForLoadState('domcontentloaded');
		await page.locator('.sidebar').waitFor();

		const conversationId = 'history-only-mount-retry';
		const savedAt = Date.now();
		const archive = {
			id: conversationId,
			saved_at_ms: savedAt,
			workspace,
			messages: [
				{ id: 'user-question', role: 'user', text: 'What does the archived report say?', ts: savedAt },
				{ id: 'saved-answer', role: 'assistant', text: 'The saved report answer remains readable.', ts: savedAt + 1 }
			]
		};
		await page.evaluate(
			({ id, body }) => window.fella.invoke('archive_conversation', { id, body }),
			{ id: conversationId, body: JSON.stringify(archive) }
		);
		await page.reload();
		await page.waitForLoadState('domcontentloaded');

		const repository = page.locator('.repository-row').filter({ hasText: 'reconnect-me' });
		await expect(repository).toBeVisible();
		await repository.click();
		await expect(repository).toHaveAttribute('aria-expanded', 'true');
		await expect(page.getByRole('status', { name: /History only/i })).toBeVisible();
		await expect(page.getByRole('log')).not.toContainText(/That doesn't look like a folder/);

		await page.getByRole('button', { name: /What does the archived report say/ }).click();
		await expect(page.getByText('The saved report answer remains readable.')).toBeVisible();
		await expect(page.getByText(/This conversation is history only/)).toBeVisible();
		const composer = page.locator('.dock textarea').last();
		await expect(composer).toBeDisabled();

		await mkdir(workspace);
		await page.getByRole('button', { name: 'Try to reopen reconnect-me' }).click();
		await expect(page.locator('.repository-history-state')).toHaveCount(0);
		await expect(composer).toBeEnabled();
		await expect(page.getByRole('button', { name: 'Open reconnect-me workspace' })).toBeVisible();

		// The first repository remains expanded. Mount another repository, then
		// select the first row again: row selection must mount it even when the
		// click also collapses its conversation list.
		const alternate = join(dataDir, 'alternate-workspace');
		await mkdir(alternate);
		await app.evaluate(({ dialog }, folder) => {
			dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
		}, alternate);
		await page.getByRole('button', { name: 'Add repository' }).click();
		await expect(page.getByRole('combobox', { name: /Ask about/ })).toBeVisible();
		const alternateCatalog = await page.evaluate(() => window.fella.invoke('get_catalog'));
		assert.equal(alternateCatalog.workspace, alternate);

		await repository.click();
		const selectedCatalog = await page.evaluate(() => window.fella.invoke('get_catalog'));
		assert.equal(selectedCatalog.workspace, workspace);
		await expect(page.getByRole('combobox', { name: /Ask about reconnect-me/ })).toBeVisible();
	} finally {
		try {
			await app?.close();
		} finally {
			await rm(dataDir, { recursive: true, force: true });
		}
	}
});
