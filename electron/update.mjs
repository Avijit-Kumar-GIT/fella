import { createHash } from 'node:crypto';
import { mkdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawn } from 'node:child_process';

const REPO = 'Avijit-Kumar-GIT/fella';

function releaseUrl() {
	return process.env.FELLA_RELEASE_API_URL?.trim() ||
		`https://api.github.com/repos/${REPO}/releases/latest`;
}

function versionTuple(value) {
	const parts = value.trim().replace(/^v/i, '').split('.');
	if (parts.length !== 3 || parts.some((part) => !/^\d+$/.test(part))) return null;
	return parts.map(Number);
}

function isNewer(current, latest) {
	const a = versionTuple(current);
	const b = versionTuple(latest);
	if (!a || !b) return false;
	return b[0] > a[0] || (b[0] === a[0] && (b[1] > a[1] || (b[1] === a[1] && b[2] > a[2])));
}

async function fetchResponse(url, headers, timeoutMs) {
	const response = await fetch(url, {
		headers,
		signal: AbortSignal.timeout(timeoutMs)
	});
	if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
	return response;
}

async function fetchRelease() {
	const response = await fetchResponse(
		releaseUrl(),
		{ Accept: 'application/vnd.github+json', 'User-Agent': 'fella-app' },
		20000
	);
	return response.json();
}

async function fetchBytes(url) {
	const response = await fetchResponse(url, { 'User-Agent': 'fella-app' }, 60000);
	return Buffer.from(await response.arrayBuffer());
}

function assetCandidates(version) {
	if (process.platform === 'win32') {
		return [`Fella_${version}_x64-setup.exe`, `Fella_${version}_x64_en-US.msi`];
	}
	if (process.platform === 'darwin') {
		return [`Fella_${version}_universal.dmg`, `Fella_${version}_universal.app.tar.gz`];
	}
	if (process.platform === 'linux') {
		return [`Fella_${version}_amd64.AppImage`, `Fella_${version}_amd64.deb`];
	}
	return [];
}

function findAsset(assets, candidates) {
	return candidates.map((name) => assets.find((asset) => asset.name === name)).find(Boolean);
}

function checksumFor(text, filename) {
	for (const line of text.split(/\r?\n/)) {
		const match = /^([0-9a-fA-F]{64})\s+\*?(.+?)\s*$/.exec(line);
		if (match?.[2] === filename) return match[1].toLowerCase();
	}
	return null;
}

async function verifyChecksum(release, asset, bytes) {
	const sums = release.assets?.find((candidate) => candidate.name === 'SHA256SUMS');
	if (!sums) throw new Error('the latest release has no SHA256SUMS nothing installed');
	const expected = checksumFor((await fetchBytes(sums.browser_download_url)).toString('utf8'), asset.name);
	if (!expected) throw new Error(`SHA256SUMS has no entry for ${asset.name}`);
	const actual = createHash('sha256').update(bytes).digest('hex');
	if (expected !== actual) {
		throw new Error(`checksum mismatch for ${asset.name} (expected ${expected}, got ${actual}) nothing installed`);
	}
}

function psQuote(value) {
	return value.replaceAll("'", "''");
}

function windowsUpdateScript(installer, executable, log) {
	const isMsi = installer.toLowerCase().endsWith('.msi');
	const file = isMsi ? 'msiexec' : psQuote(installer);
	const args = isMsi
		? `'/i','${psQuote(installer)}','/passive'`
		: "'/S'";
	return `$ErrorActionPreference = 'Stop'\r\n` +
		`$exe = '${psQuote(executable)}'\r\n` +
		`$log = '${psQuote(log)}'\r\n` +
		`function Log($m) { "$((Get-Date).ToString('o')) $m" | Out-File -Append -Encoding utf8 $log }\r\n` +
		`for ($i = 0; $i -lt 120; $i++) { try { $f = [IO.File]::Open($exe, 'Open', 'ReadWrite', 'None'); $f.Close(); break } catch { Start-Sleep -Milliseconds 500 } }\r\n` +
		`Log "waited $([int]($i * 0.5))s for exe lock"\r\n` +
		`try { $p = Start-Process -FilePath '${file}' -ArgumentList ${args} -PassThru; $p.WaitForExit(); Log "installer exit $($p.ExitCode)" } catch { Log "installer error: $_" }\r\n` +
		`try { Start-Process -FilePath $exe; Log 'relaunched' } catch { Log "relaunch error: $_" }\r\n`;
}

function shQuote(value) {
	return `'${value.replaceAll("'", "'\\''")}'`;
}

function spawnDetached(command, args) {
	const child = spawn(command, args, { detached: true, stdio: 'ignore', windowsHide: true });
	child.unref();
}

function applyWindows(installer, app) {
	const executable = process.execPath;
	const directory = dirname(installer);
	const script = join(directory, 'apply-update.ps1');
	const log = join(directory, 'update.log');
	return writeFile(script, windowsUpdateScript(installer, executable, log), 'utf8').then(() => {
		spawnDetached('powershell.exe', [
			'-NoProfile',
			'-NonInteractive',
			'-ExecutionPolicy',
			'Bypass',
			'-WindowStyle',
			'Hidden',
			'-File',
			script
		]);
		app.quit();
	});
}

function applyMac(installer, app) {
	const executable = process.execPath;
	const appBundle = join(dirname(dirname(dirname(executable))), '');
	const destination = dirname(appBundle);
	const name = appBundle.split('/').filter(Boolean).at(-1);
	if (!name?.endsWith('.app')) throw new Error('could not locate the packaged .app bundle');
	const script = [
		'sleep 1',
		`mnt="$(mktemp -d)"`,
		`hdiutil attach -nobrowse -quiet -mountpoint "$mnt" ${shQuote(installer)}`,
		`src="$(find "$mnt" -maxdepth 1 -name '*.app' -print -quit)"`,
		`rm -rf ${shQuote(join(destination, name))}`,
		`cp -R "$src" ${shQuote(destination)}`,
		`hdiutil detach -quiet "$mnt" || true`,
		`xattr -dr com.apple.quarantine ${shQuote(join(destination, name))} 2>/dev/null || true`,
		`open ${shQuote(join(destination, name))}`
	].join('\n');
	spawnDetached('sh', ['-c', script]);
	app.quit();
}

function applyLinux(installer, app) {
	if (installer.toLowerCase().endsWith('.deb')) {
		throw new Error("this install can't self-update (installed via .deb, which needs sudo) re-run the install command by hand instead");
	}
	const target = process.env.APPIMAGE || process.execPath;
	const script = `sleep 1\ncp ${shQuote(installer)} ${shQuote(target)}\nchmod +x ${shQuote(target)}\nexec ${shQuote(target)}\n`;
	spawnDetached('sh', ['-c', script]);
	app.quit();
}

async function apply(installer, app) {
	if (!app.isPackaged) throw new Error('Electron updates are only available from a packaged build.');
	if (process.platform === 'win32') return applyWindows(installer, app);
	if (process.platform === 'darwin') return applyMac(installer, app);
	if (process.platform === 'linux') return applyLinux(installer, app);
	throw new Error('no update path for this OS yet');
}

/** Check GitHub, verify the matching installer, and hand it to the shell. */
export async function checkAndApply(app) {
	if (!app.isPackaged) throw new Error('Electron updates are only available from a packaged build.');
	const current = app.getVersion();
	const release = await fetchRelease();
	const latest = String(release.tag_name ?? '').trim().replace(/^v/i, '');
	const status = { current, latest, available: isNewer(current, latest) };
	if (!status.available) return status;

	const candidates = assetCandidates(latest);
	const asset = findAsset(release.assets ?? [], candidates);
	if (!asset) {
		throw new Error(`the latest release has no installer for this platform (looked for ${candidates.join(' or ')})`);
	}
	const bytes = await fetchBytes(asset.browser_download_url);
	await verifyChecksum(release, asset, bytes);
	const directory = join(tmpdir(), 'fella-update');
	await mkdir(directory, { recursive: true });
	const staged = join(directory, asset.name);
	await writeFile(staged, bytes);
	await apply(staged, app);
	return status;
}
