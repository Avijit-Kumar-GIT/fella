import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

function validDimension(value, fallback) {
	return Number.isInteger(value) && value >= 2 && value <= 500 ? value : fallback;
}

function shellName(shellPath) {
	return shellPath.split(/[\\/]/).at(-1) || shellPath;
}

function writeCommand(session, command) {
	if (session.child.stdin.destroyed) return false;
	try {
		session.child.stdin.write(`${JSON.stringify(command)}\n`);
		return true;
	} catch {
		return false;
	}
}

/**
 * Owns user-only shell sessions. Each session is a separate Rust PTY helper
 * process; shell bytes travel only to the renderer that opened that session.
 */
export class EmbeddedTerminalSessions {
	#sessions = new Map();

	constructor({ binary, spawnProcess = spawn } = {}) {
		if (typeof binary !== 'string' || !binary.trim()) {
			throw new Error('The Fella Rust helper is unavailable.');
		}
		this.binary = binary;
		this.spawnProcess = spawnProcess;
	}

	start(owner, { terminalId, workspacePath, shellPath, cols, rows }) {
		if (!owner || owner.isDestroyed?.()) throw new Error('The terminal window is no longer available.');
		if (typeof terminalId !== 'string' || !/^[\w-]{1,100}$/.test(terminalId)) {
			throw new Error('The shell session could not be started.');
		}
		if (this.#sessions.has(terminalId)) throw new Error('That shell session is already open.');
		if (typeof workspacePath !== 'string' || !workspacePath.trim()) {
			throw new Error('Choose a workspace before opening a shell.');
		}
		if (typeof shellPath !== 'string' || !shellPath.trim()) {
			throw new Error('Choose a shell before opening it.');
		}

		const dimensions = {
			cols: validDimension(cols, 80),
			rows: validDimension(rows, 24)
		};
		const child = this.spawnProcess(this.binary, [
			'--terminal-session',
			'--shell', shellPath,
			'--cwd', workspacePath,
			'--cols', String(dimensions.cols),
			'--rows', String(dimensions.rows)
		], {
			stdio: ['pipe', 'pipe', 'pipe'],
			windowsHide: true
		});
		const session = {
			owner,
			child,
			workspacePath,
			shellPath,
			started: false,
			stderr: '',
			lines: createInterface({ input: child.stdout }),
			closeTimer: null
		};
		this.#sessions.set(terminalId, session);
		session.lines.on('line', (line) => this.#receive(terminalId, session, line));
		child.stderr.on('data', (chunk) => {
			session.stderr = (session.stderr + chunk.toString()).slice(-8192);
		});
		child.on('error', (error) => this.#fail(terminalId, session, error));
		child.on('exit', (code, signal) => {
			if (session.closeTimer) clearTimeout(session.closeTimer);
			if (this.#sessions.get(terminalId) !== session) return;
			if (!session.started) {
				this.#fail(terminalId, session, new Error(this.#startupMessage(session, code, signal)));
				return;
			}
			this.#finish(terminalId, session, {
				exitCode: Number.isInteger(code) ? code : -1,
				signal: signal ?? null
			});
		});

		return new Promise((resolve, reject) => {
			session.resolveStart = resolve;
			session.rejectStart = reject;
			session.startTimer = setTimeout(() => {
				this.#fail(terminalId, session, new Error('The selected shell did not start in time.'));
			}, 10_000);
			session.startTimer.unref?.();
		});
	}

	write(owner, terminalId, data) {
		const session = this.#ownedSession(owner, terminalId);
		if (!session || typeof data !== 'string' || data.length > 1_000_000) return false;
		return writeCommand(session, { type: 'data', data });
	}

	resize(owner, terminalId, cols, rows) {
		const session = this.#ownedSession(owner, terminalId);
		if (!session) return false;
		const nextCols = validDimension(cols, 0);
		const nextRows = validDimension(rows, 0);
		if (!nextCols || !nextRows) return false;
		return writeCommand(session, { type: 'resize', cols: nextCols, rows: nextRows });
	}

	close(owner, terminalId) {
		const session = this.#ownedSession(owner, terminalId);
		if (!session) return false;
		this.#stop(terminalId, session);
		return true;
	}

	closeForOwner(owner) {
		for (const [terminalId, session] of this.#sessions) {
			if (session.owner === owner) this.#stop(terminalId, session);
		}
	}

	dispose() {
		for (const [terminalId, session] of this.#sessions) this.#stop(terminalId, session);
	}

	#receive(terminalId, session, line) {
		let message;
		try {
			message = JSON.parse(line);
		} catch {
			return;
		}
		if (this.#sessions.get(terminalId) !== session) return;
		if (message.type === 'started') {
			if (session.started) return;
			session.started = true;
			clearTimeout(session.startTimer);
			session.resolveStart?.({
				terminalId,
				cwd: session.workspacePath,
				shell: shellName(session.shellPath),
				shellPath: session.shellPath
			});
			session.resolveStart = null;
			session.rejectStart = null;
			return;
		}
		if (message.type === 'data' && typeof message.data === 'string') {
			if (!session.owner.isDestroyed?.()) {
				session.owner.send('fella:terminal-data', { terminalId, data: message.data });
			}
			return;
		}
		if (message.type === 'error') {
			this.#fail(terminalId, session, new Error(message.message || 'The shell could not be started.'));
			return;
		}
		if (message.type === 'exit') {
			this.#finish(terminalId, session, {
				exitCode: Number.isInteger(message.exitCode) ? message.exitCode : -1,
				signal: typeof message.signal === 'string' ? message.signal : null
			});
		}
	}

	#finish(terminalId, session, event) {
		if (this.#sessions.get(terminalId) !== session) return;
		this.#sessions.delete(terminalId);
		session.lines.close();
		if (!session.owner.isDestroyed?.()) {
			session.owner.send('fella:terminal-exit', { terminalId, ...event });
		}
	}

	#fail(terminalId, session, error) {
		if (this.#sessions.get(terminalId) !== session) return;
		this.#sessions.delete(terminalId);
		clearTimeout(session.startTimer);
		session.lines.close();
		if (session.resolveStart) {
			session.rejectStart?.(error);
			session.resolveStart = null;
			session.rejectStart = null;
		} else if (!session.owner.isDestroyed?.()) {
			session.owner.send('fella:terminal-exit', { terminalId, exitCode: -1, signal: null });
		}
		try {
			session.child.kill();
		} catch {
			// The helper may already have exited.
		}
	}

	#stop(terminalId, session) {
		this.#sessions.delete(terminalId);
		clearTimeout(session.startTimer);
		session.lines.close();
		writeCommand(session, { type: 'close' });
		try {
			session.child.stdin.end();
		} catch {
			// The helper may already have exited.
		}
		session.closeTimer = setTimeout(() => {
			if (session.child.exitCode === null && session.child.signalCode === null) {
				session.child.kill();
			}
		}, 2000);
		session.closeTimer.unref?.();
	}

	#startupMessage(session, code, signal) {
		const detail = session.stderr.trim();
		return detail || `The shell helper exited before starting (${code ?? signal ?? 'unknown'}).`;
	}

	#ownedSession(owner, terminalId) {
		const session = this.#sessions.get(terminalId);
		return session?.owner === owner ? session : null;
	}
}
