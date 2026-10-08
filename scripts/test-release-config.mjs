import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

const packageManifest = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
const releaseWorkflow = await readFile(new URL('../.github/workflows/release.yml', import.meta.url), 'utf8');

test('universal macOS packaging preserves both Rust sidecar slices as resources', () => {
	const mac = packageManifest.build.mac;
	assert.equal(mac.x64ArchFiles, '**/fella-engine-*');
	assert.ok(packageManifest.build.extraResources.some((resource) =>
		resource.filter?.includes('fella-engine-*')
	));
});

test('platform artifact validation expands the release tag on every runner', () => {
	const step = releaseWorkflow.match(
		/      - name: Validate platform installers\n([\s\S]*?)(?=\n      - name: Preserve platform installers)/
	)?.[1];
	assert.ok(step, 'release workflow must contain the platform installer validation step');
	assert.match(step, /^        shell: bash$/m);
	assert.match(step, /check-release-artifacts\.mjs "\$\{GITHUB_REF_NAME\}"/);
});
