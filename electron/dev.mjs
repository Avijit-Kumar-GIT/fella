import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const rootKey = createHash('sha256')
	.update(root.replaceAll('\\', '/').toLowerCase())
	.digest('hex')
	.slice(0, 16);
const windows = process.platform === 'win32';
const pnpm = windows ? 'pnpm.cmd' : 'pnpm';
const firstPort = 1420;
const lastPort = 1440;
let vite = null;
let electron = null;
let ownsVite = false;
let stopping = false;

const pnpmOptions = (env) => ({
	cwd: root,
	env,
	stdio: 'inherit',
	...(windows ? { shell: true } : {})
});

async function isOurViteServer(url) {
	try {
		const response = await fetch(url, { method: 'HEAD', signal: AbortSignal.timeout(700) });
		return response.headers.get('x-fella-dev-root') === rootKey;
	} catch {
		return false;
	}
}

function portIsAvailable(port) {
	return new Promise((resolvePort, reject) => {
		const probe = createServer();
		probe.once('error', (error) => {
			if (error.code === 'EADDRINUSE') resolvePort(false);
			else reject(error);
		});
		probe.listen({ host: '127.0.0.1', port, exclusive: true }, () => {
			probe.close((error) => error ? reject(error) : resolvePort(true));
		});
	});
}

async function selectDevEndpoint() {
	const requestedUrl = process.env.FELLA_ELECTRON_URL;
	if (requestedUrl) {
		const url = new URL(requestedUrl);
		const port = Number(url.port || (url.protocol === 'https:' ? 443 : firstPort));
		if (await isOurViteServer(url)) return { url: url.toString(), port, owned: false };
		if (!await portIsAvailable(port)) {
			throw new Error(`Port ${port} is occupied by a different server. Stop it or unset FELLA_ELECTRON_URL.`);
		}
		return { url: url.toString(), port, owned: true };
	}

	for (let port = firstPort; port <= lastPort; port++) {
		const url = `http://127.0.0.1:${port}/`;
		if (await isOurViteServer(url)) return { url, port, owned: false };
		if (await portIsAvailable(port)) return { url, port, owned: true };
	}
	throw new Error(`No Fella dev-server port is available between ${firstPort} and ${lastPort}.`);
}

function stop(child) {
	if (!child || child.exitCode !== null) return;
	if (process.platform === 'win32') {
		spawn('taskkill.exe', ['/pid', String(child.pid), '/t', '/f'], { stdio: 'ignore' });
	} else {
		child.kill('SIGTERM');
	}
}

function waitForSpawn(child) {
	return new Promise((resolveSpawn, rejectSpawn) => {
		const onSpawn = () => {
			child.off('error', onError);
			resolveSpawn();
		};
		const onError = (error) => {
			child.off('spawn', onSpawn);
			rejectSpawn(error);
		};
		child.once('spawn', onSpawn);
		child.once('error', onError);
	});
}

async function waitForVite(url, timeoutMs = 30000) {
	const deadline = Date.now() + timeoutMs;
	let lastError;
	while (Date.now() < deadline) {
		if (vite.exitCode !== null) {
			throw new Error(`Vite exited before becoming ready (code ${vite.exitCode ?? 'unknown'})`);
		}
		try {
			const response = await fetch(url, { method: 'HEAD' });
			if (response.headers.get('x-fella-dev-root') === rootKey) return;
			lastError = new Error('the response is not from this Fella checkout');
		} catch (error) {
			lastError = error;
		}
		await new Promise((resolvePromise) => setTimeout(resolvePromise, 200));
	}
	throw new Error(`Vite did not become ready at ${url}: ${lastError ?? 'timed out'}`);
}

async function main() {
	try {
		const endpoint = await selectDevEndpoint();
		const env = {
			...process.env,
			FELLA_ELECTRON_URL: endpoint.url,
			FELLA_DEV_ROOT_KEY: rootKey,
			FELLA_VITE_PORT: String(endpoint.port)
		};
		if (endpoint.owned) {
			vite = spawn(pnpm, ['dev'], pnpmOptions(env));
			ownsVite = true;
			await waitForSpawn(vite);
			await waitForVite(endpoint.url);
		} else {
			console.info(`Reusing this checkout's Vite server at ${endpoint.url}`);
		}

		if (stopping) return;
		electron = spawn(pnpm, ['exec', 'electron', 'electron/main.mjs'], pnpmOptions(env));
		await waitForSpawn(electron);
		const exitCode = await new Promise((resolveExit, reject) => {
			electron.once('error', reject);
			electron.once('exit', (code) => resolveExit(code ?? 1));
		});
		electron = null;
		stopping = true;
		if (ownsVite) stop(vite);
		if (process.exitCode == null) process.exitCode = exitCode;
	} catch (error) {
		console.error(`Electron dev server failed: ${error instanceof Error ? error.message : error}`);
		stopping = true;
		stop(electron);
		if (ownsVite) stop(vite);
		process.exitCode = 1;
	}
}

for (const signal of ['SIGINT', 'SIGTERM']) {
	process.on(signal, () => {
		if (stopping) return;
		stopping = true;
		stop(electron);
		if (ownsVite) stop(vite);
		process.exitCode = 130;
	});
}

void main();
