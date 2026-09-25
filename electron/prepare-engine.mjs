import { copyFileSync, chmodSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const suffix = process.platform === 'win32' ? '.exe' : '';
const candidates = [
	join(root, 'src-tauri', 'target', 'release', `fella${suffix}`),
	join(root, 'src-tauri', 'target', 'debug', `fella${suffix}`)
];
const source = candidates.find((path) => existsSync(path));
if (!source) {
	throw new Error(`Rust engine not found. Build it first: ${candidates.join(' or ')}`);
}

const destinationDir = join(root, 'electron', 'engine');
const destination = join(destinationDir, `fella-engine${suffix}`);
mkdirSync(destinationDir, { recursive: true });
for (const entry of [
	join(destinationDir, 'fella-engine'),
	join(destinationDir, 'fella-engine.exe'),
	join(destinationDir, 'fella'),
	join(destinationDir, 'fella.exe')
]) {
	if (entry !== destination && existsSync(entry)) rmSync(entry);
}
copyFileSync(source, destination);
if (process.platform !== 'win32') chmodSync(destination, 0o755);
console.log(`prepared Electron engine: ${destination}`);
