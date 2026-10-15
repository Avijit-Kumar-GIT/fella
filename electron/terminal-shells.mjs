import { accessSync, constants, existsSync, statSync } from 'node:fs';
import path from 'node:path';

const LABELS = new Map([
	['bash', 'Bash'],
	['cmd', 'Command Prompt'],
	['fish', 'Fish'],
	['ksh', 'KornShell'],
	['nu', 'Nushell'],
	['powershell', 'Windows PowerShell'],
	['pwsh', 'PowerShell'],
	['sh', 'POSIX shell'],
	['tcsh', 'tcsh'],
	['zsh', 'Zsh']
]);

function envValue(env, name) {
	const key = Object.keys(env).find((candidate) => candidate.toLowerCase() === name.toLowerCase());
	return key ? env[key] : undefined;
}

function defaultFileExists(filePath, platform) {
	try {
		if (!existsSync(filePath) || !statSync(filePath).isFile()) return false;
		if (platform !== 'win32') accessSync(filePath, constants.X_OK);
		return true;
	} catch {
		return false;
	}
}

function labelFor(filePath, pathApi) {
	const executable = pathApi.basename(filePath).replace(/\.exe$/i, '').toLowerCase();
	return LABELS.get(executable) ?? pathApi.basename(filePath);
}

function findShell(command, { platform, env, fileExists, pathApi, pathEntries, extensions }) {
	if (!command) return null;
	const hasSeparator = command.includes('/') || command.includes('\\');
	const alreadyHasExtension = extensions.some((extension) => command.toLowerCase().endsWith(extension.toLowerCase()));
	const suffixes = alreadyHasExtension ? [''] : extensions;
	const candidates = pathApi.isAbsolute(command) || hasSeparator
		? [command]
		: pathEntries.flatMap((directory) => suffixes.map((extension) => pathApi.join(directory, `${command}${extension}`)));
	return candidates.find((candidate) => fileExists(candidate, platform)) ?? null;
}

/** Finds shells already installed on the user's system; it installs nothing. */
export function discoverTerminalShells({
	platform = process.platform,
	env = process.env,
	fileExists = defaultFileExists
} = {}) {
	const pathApi = platform === 'win32' ? path.win32 : path;
	const pathValue = envValue(env, 'PATH') ?? '';
	const pathEntries = pathValue.split(platform === 'win32' ? ';' : path.delimiter).filter(Boolean);
	const extensions = platform === 'win32'
		? (envValue(env, 'PATHEXT') ?? '.COM;.EXE;.BAT;.CMD').split(';').filter(Boolean)
		: [''];
	const isWindows = platform === 'win32';
	const candidates = isWindows
		? [
			envValue(env, 'ComSpec') ?? envValue(env, 'COMSPEC'),
			pathApi.join(envValue(env, 'SystemRoot') ?? 'C:\\Windows', 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe'),
			'pwsh.exe',
			'powershell.exe',
			'cmd.exe',
			'bash.exe',
			'nu.exe'
		]
		: [
			envValue(env, 'SHELL'),
			'zsh',
			'bash',
			'fish',
			'sh',
			'nu',
			'pwsh',
			'ksh',
			'tcsh'
		];

	const shells = [];
	const seen = new Set();
	for (const candidate of candidates) {
		const found = findShell(candidate, { platform, env, fileExists, pathApi, pathEntries, extensions });
		if (!found) continue;
		const key = isWindows ? found.toLowerCase() : found;
		if (seen.has(key)) continue;
		seen.add(key);
		shells.push({ path: found, name: labelFor(found, pathApi) });
	}

	return {
		shells,
		defaultShellPath: shells[0]?.path ?? null
	};
}
