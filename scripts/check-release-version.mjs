import { readFileSync } from 'node:fs';

const packageJson = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
const cargoToml = readFileSync(new URL('../backend/Cargo.toml', import.meta.url), 'utf8');
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const tag = process.argv[2] ?? process.env.GITHUB_REF_NAME ?? '';
const match = /^v(\d+\.\d+\.\d+)(?:-[0-9A-Za-z.-]+)?$/.exec(tag);

if (!match) {
	console.error(`Release tag must be a semantic version such as v${packageJson.version} or v${packageJson.version}-rc.1; received ${JSON.stringify(tag)}.`);
	process.exit(1);
}

if (!cargoVersion || cargoVersion !== packageJson.version) {
	console.error(`package.json (${packageJson.version}) and backend/Cargo.toml (${cargoVersion ?? 'missing'}) versions must match.`);
	process.exit(1);
}

if (match[1] !== packageJson.version) {
	console.error(`Tag ${tag} does not match package version ${packageJson.version}.`);
	process.exit(1);
}

console.log(`Release tag ${tag} matches Electron ${packageJson.version} and Rust ${cargoVersion}.`);
