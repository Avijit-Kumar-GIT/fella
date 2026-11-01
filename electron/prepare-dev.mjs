import { spawn } from 'node:child_process';
import { stat } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
function run(command, args, capture = false) {
	return new Promise((resolveRun, reject) => {
		const child = spawn(command, args, { cwd: root, windowsHide: true, stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit' });
		let output = '';
		if (capture) child.stdout.on('data', chunk => { output += chunk; });
		child.once('error', reject);
		child.once('exit', code => code === 0 ? resolveRun(output) : reject(new Error(`${command} exited with code ${code}`)));
	});
}

/** Prepare only what Vite/Electron development actually consumes. Cargo owns
 * native incrementality; production frontend builds and release LTO stay in
 * electron:build/package. An explicit engine path is an intentional override. */
export async function prepareDev() {
	await run(process.execPath, ['scripts/generate-presentation-catalog.mjs']);
	await run(process.execPath, ['scripts/build-presentation-contract.mjs']);
	let engine = process.env.FELLA_ENGINE_PATH?.trim();
	if (engine) engine = resolve(root, engine);
	else {
		await run('cargo', ['build', '--locked', '--manifest-path', 'backend/Cargo.toml', '--bin', 'fella']);
		const metadata = JSON.parse(await run('cargo', ['metadata', '--locked', '--no-deps', '--format-version', '1', '--manifest-path', 'backend/Cargo.toml'], true));
		engine = join(metadata.target_directory, 'debug', process.platform === 'win32' ? 'fella.exe' : 'fella');
	}
	if (!(await stat(engine)).isFile()) throw new Error(`Development engine is not a file: ${engine}`);
	console.info(`Development engine: ${engine}`);
	return engine;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	prepareDev().catch(error => { console.error(error.message); process.exitCode = 1; });
}
