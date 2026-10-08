import { spawn } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const windows = process.platform === 'win32';
const pnpm = windows ? 'pnpm.cmd' : 'pnpm';
const devUrl = process.env.FELLA_ELECTRON_URL || 'http://localhost:1420';
const env = { ...process.env, FELLA_ELECTRON_URL: devUrl };
const pnpmOptions = {
	cwd: root,
	env,
	stdio: 'inherit',
	...(windows ? { shell: true } : {})
};

const vite = spawn(pnpm, ['dev'], pnpmOptions);

let stopping = false;

function stop(child) {
	if (!child || child.exitCode !== null) return;
	if (process.platform === 'win32') {
		spawn('taskkill.exe', ['/pid', String(child.pid), '/t', '/f'], { stdio: 'ignore' });
	} else {
		child.kill('SIGTERM');
	}
}

async function waitForVite(timeoutMs = 30000) {
	const deadline = Date.now() + timeoutMs;
	let lastError;
	while (Date.now() < deadline) {
		if (vite.exitCode !== null) {
			throw new Error(`Vite exited before becoming ready (code ${vite.exitCode ?? 'unknown'})`);
		}
		try {
			const response = await fetch(devUrl, { method: 'HEAD' });
			if (response.ok || response.status < 500) return;
			lastError = new Error(`HTTP ${response.status}`);
		} catch (error) {
			lastError = error;
		}
		await new Promise((resolvePromise) => setTimeout(resolvePromise, 200));
	}
	throw new Error(`Vite did not become ready at ${devUrl}: ${lastError ?? 'timed out'}`);
}

async function main() {
	try {
		await waitForVite();
		const electron = spawn(pnpm, ['exec', 'electron', 'electron/main.mjs'], pnpmOptions);

		const exitCode = await new Promise((resolveExit) => {
			electron.once('exit', (code) => resolveExit(code ?? 1));
		});
		stopping = true;
		stop(vite);
		process.exitCode = exitCode;
	} catch (error) {
		console.error(`Electron dev server failed: ${error instanceof Error ? error.message : error}`);
		stopping = true;
		stop(vite);
		process.exitCode = 1;
	}
}

for (const signal of ['SIGINT', 'SIGTERM']) {
	process.on(signal, () => {
		if (stopping) return;
		stopping = true;
		stop(vite);
		process.exitCode = 130;
	});
}

void main();
