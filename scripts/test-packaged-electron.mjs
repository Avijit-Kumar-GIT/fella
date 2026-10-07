import assert from 'node:assert/strict';
import { access, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { _electron as electron } from '@playwright/test';

const executableArgument = process.argv.slice(2).find((argument) => argument !== '--');
const executable = executableArgument ? resolve(executableArgument) : null;
if (!executable) {
	throw new Error('Usage: pnpm run test:electron:packaged -- <path-to-packaged-Fella-executable>');
}
await access(executable);

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const fixture = join(
	root,
	'bench',
	'fqa-bench',
	'suites',
	'clarification-housing',
	'workspaces',
	'housing'
);
await access(fixture);

const dataDir = await mkdtemp(join(tmpdir(), 'fella-packaged-smoke-'));
let app;
try {
	app = await electron.launch({
		executablePath: executable,
		args: [`--user-data-dir=${join(dataDir, 'chromium')}`],
		cwd: dirname(executable),
		env: {
			...process.env,
			FELLA_DATA_DIR: dataDir
		}
	});
	const page = await app.firstWindow();
	await page.waitForLoadState('domcontentloaded');
	assert.equal(await app.evaluate(({ app: electronApp }) => electronApp.isPackaged), true);
	await page.getByRole('combobox', { name: 'Ask a question' }).waitFor({ state: 'visible' });
	assert.equal(await page.evaluate(() => window.fella.invoke('ping')), 'pong');
	console.log('Packaged Electron UI and bundled Rust sidecar smoke passed.');
} finally {
	try {
		await app?.close();
	} finally {
		await rm(dataDir, { recursive: true, force: true });
	}
}
