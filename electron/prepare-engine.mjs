import { copyFileSync, chmodSync, existsSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const requestedTarget = process.argv.find((arg) => arg.startsWith('--target='))?.slice('--target='.length);
const target = requestedTarget || null;
const arch = target === 'aarch64-apple-darwin'
	? 'arm64'
	: target === 'x86_64-apple-darwin'
		? 'x64'
		: process.arch;
const suffix = process.platform === 'win32' ? '.exe' : '';
const targetDir = target ? join(root, 'backend', 'target', target) : join(root, 'backend', 'target');
const candidates = [
	join(targetDir, 'release', `fella${suffix}`),
	join(targetDir, 'debug', `fella${suffix}`)
];
const source = candidates.find((path) => existsSync(path));
if (!source) {
	throw new Error(`Rust engine not found. Build it first: ${candidates.join(' or ')}`);
}

const destinationDir = join(root, 'electron', 'engine');
const destination = join(destinationDir, `fella-engine-${arch}${suffix}`);
mkdirSync(destinationDir, { recursive: true });
copyFileSync(source, destination);
if (process.platform !== 'win32') chmodSync(destination, 0o755);
console.log(`prepared Electron engine: ${destination}`);
