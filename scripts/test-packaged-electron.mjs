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
const terminalCommand = process.platform === 'win32'
	? 'echo __FELLA_PTY_OK__ & cd\r'
	: "printf '\\n__FELLA_PTY_OK__\\n'; pwd\r";
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
	const terminalOutput = await page.evaluate(({ workspacePath, command }) => new Promise((resolve, reject) => {
		let terminalId;
		let output = '';
		const timeout = setTimeout(() => {
			if (terminalId) window.fella.closeTerminal(terminalId);
			reject(new Error('Embedded terminal did not return its command output.'));
		}, 10_000);
		const finish = () => {
			clearTimeout(timeout);
			if (terminalId) window.fella.closeTerminal(terminalId);
			resolve(output);
		};
		window.fella.startTerminal(workspacePath, 80, 24, (data) => {
			output += data;
			const markerCount = output.split('__FELLA_PTY_OK__').length - 1;
			if (markerCount >= 2 && output.includes(workspacePath)) finish();
		}, ({ exitCode, signal }) => {
			clearTimeout(timeout);
			reject(new Error(`Embedded terminal exited early (${exitCode}, signal ${signal}).`));
		}).then(({ terminalId: startedTerminalId }) => {
			terminalId = startedTerminalId;
			window.fella.writeTerminal(terminalId, command);
		}).catch((error) => {
			clearTimeout(timeout);
			reject(error);
		});
	}), { workspacePath: fixture, command: terminalCommand });
	assert.ok(terminalOutput.includes('__FELLA_PTY_OK__'));
	assert.ok(terminalOutput.includes(fixture));
	console.log('Packaged Electron UI, Rust sidecar, and embedded terminal smoke passed.');
} finally {
	try {
		await app?.close();
	} finally {
		await rm(dataDir, { recursive: true, force: true });
	}
}
