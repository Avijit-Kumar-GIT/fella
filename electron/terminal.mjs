import { spawn } from 'node:child_process';

/**
 * Build native launch requests only. Fella never creates a PTY, writes to the
 * terminal's stdin, or reads its stdout/stderr. The external app owns the full
 * interactive session after launch.
 */
export function terminalLaunchCandidates(platform, cwd, env = process.env) {
	if (platform === 'win32') {
		const preferred = env.ComSpec || env.COMSPEC || 'cmd.exe';
		const candidates = [{ command: preferred, args: [] }];
		// Starting a normal console shell lets Windows use the user's configured
		// default terminal host instead of forcing Windows Terminal specifically.
		if (preferred.toLowerCase() !== 'cmd.exe') candidates.push({ command: 'cmd.exe', args: [] });
		return candidates;
	}

	if (platform === 'darwin') {
		return [{ command: 'open', args: ['-a', 'Terminal', cwd] }];
	}

	if (platform === 'linux') {
		const candidates = [];
		const preferred = env.TERMINAL?.trim();
		// TERMINAL is conventionally an executable name. Do not parse it as a
		// command line or invoke a shell; paths containing spaces are skipped.
		if (preferred && !/[\s\0]/.test(preferred)) {
			candidates.push({ command: preferred, args: [] });
		}
		// xdg-terminal-exec follows the user's configured default when installed.
		candidates.push(
			{ command: 'xdg-terminal-exec', args: ['--dir', cwd] },
			// Older desktop environments expose their selected terminal through
			// this compatibility name. The child process inherits cwd.
			{ command: 'x-terminal-emulator', args: [] }
		);
		return candidates;
	}

	throw new Error(`Opening a system terminal is not supported on ${platform}.`);
}

/** Launch an external terminal at cwd without creating any I/O channel. */
export async function openWorkspaceTerminal(cwd, {
	platform = process.platform,
	env = process.env,
	spawnProcess = spawn
} = {}) {
	if (typeof cwd !== 'string' || cwd.trim() === '') {
		throw new Error('Choose a workspace before opening a terminal.');
	}

	const candidates = terminalLaunchCandidates(platform, cwd, env);
	let lastError;
	for (const candidate of candidates) {
		try {
			const child = spawnProcess(candidate.command, candidate.args, {
				cwd,
				detached: true,
				stdio: 'ignore',
				windowsHide: false,
				shell: false
			});
			await new Promise((resolve, reject) => {
				child.once('spawn', resolve);
				child.once('error', reject);
			});
			child.unref();
			return { opened: true };
		} catch (error) {
			lastError = error;
			if (error?.code !== 'ENOENT') throw error;
		}
	}

	if (platform === 'linux') {
		throw new Error(
			`No system terminal launcher was found. Install or configure your desktop terminal${lastError ? ` (${lastError.message})` : ''}.`
		);
	}
	throw new Error(`Couldn't open the system terminal${lastError ? `: ${lastError.message}` : '.'}`);
}
