import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';

import { assetCandidates } from '../electron/update.mjs';
import { checkReleaseArtifacts, expectedReleaseArtifacts } from './check-release-artifacts.mjs';

const directories = [];
const packageManifest = JSON.parse(
	await readFile(new URL('../package.json', import.meta.url), 'utf8')
);

async function fixture() {
	const directory = await mkdtemp(join(tmpdir(), 'fella-release-artifacts-'));
	directories.push(directory);
	return directory;
}

after(async () => {
	await Promise.all(directories.map((directory) => rm(directory, { recursive: true, force: true })));
});

test('release artifact names match the updater platform contract', () => {
	assert.deepEqual(expectedReleaseArtifacts('v0.3.0', 'mac'), [
		'Fella_0.3.0_universal.dmg',
		'Fella_0.3.0_universal.zip'
	]);
	assert.deepEqual(expectedReleaseArtifacts('0.3.0', 'win'), [
		'Fella_0.3.0_x64.exe',
		'Fella_0.3.0_x64.msi'
	]);
	assert.deepEqual(expectedReleaseArtifacts('0.3.0', 'linux'), [
		'Fella_0.3.0_x64.AppImage',
		'Fella_0.3.0_x64.deb'
	]);
	assert.deepEqual(expectedReleaseArtifacts('0.3.0', 'mac'), assetCandidates('0.3.0', 'darwin'));
	assert.deepEqual(expectedReleaseArtifacts('0.3.0', 'win'), assetCandidates('0.3.0', 'win32'));
	assert.deepEqual(expectedReleaseArtifacts('0.3.0', 'linux'), assetCandidates('0.3.0', 'linux'));
	assert.equal(expectedReleaseArtifacts('0.3.0-rc.1').length, 6);
});

test('Linux package metadata emits the names consumed by the updater', () => {
	assert.equal(packageManifest.build.linux.artifactName, 'Fella_${version}_x64.${ext}');
	assert.equal(packageManifest.build.linux.maintainer, packageManifest.author);
	assert.equal(packageManifest.build.linux.syncDesktopName, true);
	assert.equal(packageManifest.desktopName, packageManifest.build.productName);
});

test('rejects malformed tags and unknown platforms', () => {
	assert.throws(() => expectedReleaseArtifacts('0.3'), /invalid release tag/);
	assert.throws(() => expectedReleaseArtifacts('0.3.0', 'android'), /unknown release platform/);
});

test('platform validation fails on a missing or empty installer', async () => {
	const directory = await fixture();
	await writeFile(join(directory, 'Fella_0.3.0_x64.exe'), 'installer');
	await assert.rejects(checkReleaseArtifacts('v0.3.0', directory, 'win'), /missing Fella_0.3.0_x64.msi/);
	await writeFile(join(directory, 'Fella_0.3.0_x64.msi'), '');
	await assert.rejects(checkReleaseArtifacts('v0.3.0', directory, 'win'), /not a non-empty file/);
});

test('validates all six non-empty platform artifacts before draft creation', async () => {
	const directory = await fixture();
	for (const name of expectedReleaseArtifacts('v0.3.0')) {
		await writeFile(join(directory, name), 'installer');
	}
	assert.equal((await checkReleaseArtifacts('v0.3.0', directory)).length, 6);
});
