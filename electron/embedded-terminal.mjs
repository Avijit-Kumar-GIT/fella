function shellFor(platform, env) {
	if (platform === 'win32') return env.ComSpec || env.COMSPEC || 'cmd.exe';
	if (platform === 'darwin') return env.SHELL || '/bin/zsh';
	return env.SHELL || '/bin/sh';
}

function shellName(shell) {
	return shell.split(/[\\/]/).at(-1) || shell;
}

function validDimension(value, fallback) {
	return Number.isInteger(value) && value >= 2 && value <= 500 ? value : fallback;
}

/**
 * Owns interactive terminal sessions for the desktop shell. Terminal data is
 * sent only to the renderer that opened the session; it is never forwarded to
 * the analytics engine or included in app state/logging.
 */
export class EmbeddedTerminalSessions {
	#sessions = new Map();

	constructor({ pty, platform = process.platform, env = process.env } = {}) {
		if (!pty || typeof pty.spawn !== 'function') {
			throw new Error('The terminal PTY provider is unavailable.');
		}
		this.pty = pty;
		this.platform = platform;
		this.env = env;
	}

	start(owner, { terminalId, workspacePath, cols, rows }) {
		if (!owner || owner.isDestroyed?.()) throw new Error('The terminal window is no longer available.');
		if (typeof terminalId !== 'string' || !/^[\w-]{1,100}$/.test(terminalId)) {
			throw new Error('The terminal session could not be started.');
		}
		if (this.#sessions.has(terminalId)) throw new Error('That terminal session is already open.');
		if (typeof workspacePath !== 'string' || !workspacePath.trim()) {
			throw new Error('Choose a workspace before opening a terminal.');
		}

		const shell = shellFor(this.platform, this.env);
		const terminal = this.pty.spawn(shell, [], {
			name: 'xterm-256color',
			cols: validDimension(cols, 80),
			rows: validDimension(rows, 24),
			cwd: workspacePath,
			env: { ...this.env, TERM: 'xterm-256color', COLORTERM: 'truecolor' }
		});
		const session = { owner, terminal, workspacePath, shell };
		this.#sessions.set(terminalId, session);

		terminal.onData((data) => {
			if (this.#sessions.get(terminalId) !== session || owner.isDestroyed?.()) return;
			owner.send('fella:terminal-data', { terminalId, data });
		});
		terminal.onExit(({ exitCode, signal }) => {
			if (this.#sessions.get(terminalId) !== session) return;
			this.#sessions.delete(terminalId);
			if (!owner.isDestroyed?.()) {
				owner.send('fella:terminal-exit', { terminalId, exitCode, signal });
			}
		});

		return { terminalId, cwd: workspacePath, shell: shellName(shell) };
	}

	write(owner, terminalId, data) {
		const session = this.#ownedSession(owner, terminalId);
		if (!session || typeof data !== 'string' || data.length > 1_000_000) return false;
		session.terminal.write(data);
		return true;
	}

	resize(owner, terminalId, cols, rows) {
		const session = this.#ownedSession(owner, terminalId);
		if (!session) return false;
		const nextCols = validDimension(cols, 0);
		const nextRows = validDimension(rows, 0);
		if (!nextCols || !nextRows) return false;
		session.terminal.resize(nextCols, nextRows);
		return true;
	}

	close(owner, terminalId) {
		const session = this.#ownedSession(owner, terminalId);
		if (!session) return false;
		this.#sessions.delete(terminalId);
		session.terminal.kill();
		return true;
	}

	closeForOwner(owner) {
		for (const [terminalId, session] of this.#sessions) {
			if (session.owner !== owner) continue;
			this.#sessions.delete(terminalId);
			try {
				session.terminal.kill();
			} catch {
				// The shell may have exited between the lifecycle event and cleanup.
			}
		}
	}

	dispose() {
		for (const [terminalId, session] of this.#sessions) {
			this.#sessions.delete(terminalId);
			try {
				session.terminal.kill();
			} catch {
				// The shell may already have exited.
			}
		}
	}

	#ownedSession(owner, terminalId) {
		const session = this.#sessions.get(terminalId);
		return session?.owner === owner ? session : null;
	}
}
