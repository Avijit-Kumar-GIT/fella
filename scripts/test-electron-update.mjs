import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';

import {
	assetCandidates,
	checkAndApply,
	checksumFor,
	isNewer,
	versionTuple,
	windowsUpdateScript
} from '../electron/update.mjs';

test('update comparison accepts plain semantic versions and rejects malformed tags', () => {
	assert.deepEqual(versionTuple('v0.3.1'), [0, 3, 1]);
	assert.equal(versionTuple('0.3.1-rc.1'), null);
	assert.equal(isNewer('0.3.0', '0.3.1'), true);
	assert.equal(isNewer('0.3.1', '0.3.0'), false);
	assert.equal(isNewer('0.3.1', '0.3.1'), false);
});

test('update assets match the Electron Builder release artifact names by platform', () => {
	assert.deepEqual(assetCandidates('0.3.0', 'win32'), [
		'Fella_0.3.0_x64.exe',
		'Fella_0.3.0_x64.msi'
	]);
	assert.deepEqual(assetCandidates('0.3.0', 'darwin'), [
		'Fella_0.3.0_universal.dmg',
		'Fella_0.3.0_universal.zip'
	]);
	assert.deepEqual(assetCandidates('0.3.0', 'linux'), [
		'Fella_0.3.0_x64.AppImage',
		'Fella_0.3.0_x64.deb'
	]);
	assert.deepEqual(assetCandidates('0.3.0', 'freebsd'), []);
});

test('checksum lookup requires an exact filename and accepts sha256sum formats', () => {
	const sums = `${'a'.repeat(64)} *Fella_0.3.0_x64.AppImage\n${'b'.repeat(64)}  Fella_0.3.0_x64.deb\n`;
	assert.equal(checksumFor(sums, 'Fella_0.3.0_x64.AppImage'), 'a'.repeat(64));
	assert.equal(checksumFor(sums, 'Fella_0.3.0_x64.deb'), 'b'.repeat(64));
	assert.equal(checksumFor(sums, 'Fella_0.3.0_x64.AppImage.backup'), null);
});

test('Windows updater quotes paths and selects msiexec for MSI installers', () => {
	const script = windowsUpdateScript(
		"C:\\Users\\O'Brien\\Fella_0.3.0_x64.msi",
		"C:\\Users\\O'Brien\\Fella\\fella.exe",
		"C:\\Users\\O'Brien\\update.log"
	);
	assert.match(script, /Start-Process -FilePath 'msiexec'/);
	assert.match(script, /'/);
	assert.match(script, /O''Brien/);
	assert.ok(script.indexOf('$p.WaitForExit()') < script.indexOf('Start-Process -FilePath $exe'));
});

test('updating is rejected in an unpackaged development run before network access', async () => {
	await assert.rejects(checkAndApply({ isPackaged: false }), /packaged build/);
});

test('a current build reports no update and never downloads or applies an installer', async () => {
	const fetched = [];
	const status = await checkAndApply(
		{ isPackaged: true, getVersion: () => '0.3.0' },
		{
			platform: 'linux',
			releaseApiUrl: 'https://fixture.invalid/release',
			fetcher: async (url) => {
				fetched.push(url);
				return new Response(JSON.stringify({ tag_name: 'v0.3.0', assets: [] }));
			},
			stage: async () => assert.fail('a current build must not stage an installer'),
			applyUpdate: async () => assert.fail('a current build must not apply an installer')
		}
	);

	assert.deepEqual(status, { current: '0.3.0', latest: '0.3.0', available: false });
	assert.deepEqual(fetched, ['https://fixture.invalid/release']);
});

test('update downloads the matching release, verifies it, then hands it to the installer', async () => {
	const installer = Buffer.from('fixture installer bytes');
	const digest = createHash('sha256').update(installer).digest('hex');
	const name = 'Fella_0.3.0_x64.AppImage';
	const urls = new Map([
		[
			'https://fixture.invalid/release',
			new Response(JSON.stringify({
				tag_name: 'v0.3.0',
				assets: [
					{ name, browser_download_url: 'https://fixture.invalid/installer' },
					{ name: 'SHA256SUMS', browser_download_url: 'https://fixture.invalid/sums' }
				]
			}))
		],
		['https://fixture.invalid/installer', new Response(installer)],
		['https://fixture.invalid/sums', new Response(`${digest} *${name}\n`)]
	]);
	const fetched = [];
	const staged = [];
	const applied = [];
	const status = await checkAndApply(
		{ isPackaged: true, getVersion: () => '0.2.0' },
		{
			platform: 'linux',
			releaseApiUrl: 'https://fixture.invalid/release',
			fetcher: async (url) => {
				fetched.push(url);
				const response = urls.get(url);
				assert.ok(response, `unexpected update URL: ${url}`);
				return response;
			},
			stage: async (filename, bytes) => {
				staged.push({ filename, bytes });
				return `/staged/${filename}`;
			},
			applyUpdate: async (path) => applied.push(path)
		}
	);

	assert.deepEqual(status, { current: '0.2.0', latest: '0.3.0', available: true });
	assert.deepEqual(fetched, [
		'https://fixture.invalid/release',
		'https://fixture.invalid/installer',
		'https://fixture.invalid/sums'
	]);
	assert.equal(staged.length, 1);
	assert.equal(staged[0].filename, name);
	assert.deepEqual(staged[0].bytes, installer);
	assert.deepEqual(applied, [`/staged/${name}`]);
});

test('a mismatched release checksum prevents staging or applying an installer', async () => {
	const name = 'Fella_0.3.0_x64.AppImage';
	const urls = new Map([
		[
			'https://fixture.invalid/release',
			new Response(JSON.stringify({
				tag_name: 'v0.3.0',
				assets: [
					{ name, browser_download_url: 'https://fixture.invalid/installer' },
					{ name: 'SHA256SUMS', browser_download_url: 'https://fixture.invalid/sums' }
				]
			}))
		],
		['https://fixture.invalid/installer', new Response('untrusted bytes')],
		['https://fixture.invalid/sums', new Response(`${'0'.repeat(64)} *${name}\n`)]
	]);
	let staged = false;
	let applied = false;

	await assert.rejects(
		checkAndApply(
			{ isPackaged: true, getVersion: () => '0.2.0' },
			{
				platform: 'linux',
				releaseApiUrl: 'https://fixture.invalid/release',
				fetcher: async (url) => {
					const response = urls.get(url);
					assert.ok(response, `unexpected update URL: ${url}`);
					return response;
				},
				stage: async () => {
					staged = true;
					return '/staged/untrusted';
				},
				applyUpdate: async () => {
					applied = true;
				}
			}
		),
		/checksum mismatch/
	);
	assert.equal(staged, false);
	assert.equal(applied, false);
});
