import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { test } from 'node:test';

import { EmbeddedTerminalSessions } from '../electron/embedded-terminal.mjs';

function fakePty() {
	const calls = [];
	return {
		calls,
		spawn(file, args, options) {
			const listeners = { data: null, exit: null };
			const terminal = {
				writeCalls: [],
				resizeCalls: [],
				killed: false,
				onData(callback) { listeners.data = callback; return { dispose() {} }; },
				onExit(callback) { listeners.exit = callback; return { dispose() {} }; },
				write(data) { this.writeCalls.push(data); },
				resize(cols, rows) { this.resizeCalls.push([cols, rows]); },
				kill() { this.killed = true; },
				emitData(data) { listeners.data?.(data); },
				emitExit(event) { listeners.exit?.(event); }
			};
			calls.push({ file, args, options, terminal });
			return terminal;
		}
	};
}

function fakeOwner() {
	return Object.assign(new EventEmitter(), {
		messages: [],
		destroyed: false,
		isDestroyed() { return this.destroyed; },
		send(channel, message) { this.messages.push({ channel, message }); }
	});
}

test('starts an interactive session with the selected workspace, shell, and terminal dimensions', () => {
	for (const [platform, env, expectedShell] of [
		['win32', { ComSpec: 'C:\\Windows\\System32\\cmd.exe' }, 'C:\\Windows\\System32\\cmd.exe'],
		['darwin', { SHELL: '/bin/zsh' }, '/bin/zsh'],
		['linux', { SHELL: '/usr/bin/fish' }, '/usr/bin/fish']
	]) {
		const pty = fakePty();
		const sessions = new EmbeddedTerminalSessions({ pty, platform, env });
		const owner = fakeOwner();
		const result = sessions.start(owner, {
			terminalId: `test-${platform}`,
			workspacePath: '/work/northwind sales',
			cols: 117,
			rows: 38
		});

		assert.deepEqual(result, {
			terminalId: `test-${platform}`,
			cwd: '/work/northwind sales',
			shell: expectedShell.split(/[\\/]/).at(-1)
		});
		assert.deepEqual(pty.calls[0], {
			file: expectedShell,
			args: [],
			options: {
				name: 'xterm-256color',
				cols: 117,
				rows: 38,
				cwd: '/work/northwind sales',
				env: { ...env, TERM: 'xterm-256color', COLORTERM: 'truecolor' }
			},
			terminal: pty.calls[0].terminal
		});
	}
});

test('routes input, resize, and output only through the owning terminal session', () => {
	const pty = fakePty();
	const sessions = new EmbeddedTerminalSessions({ pty, platform: 'linux', env: { SHELL: '/bin/sh' } });
	const owner = fakeOwner();
	const otherOwner = fakeOwner();
	sessions.start(owner, {
		terminalId: 'terminal-one',
		workspacePath: '/workspace',
		cols: 80,
		rows: 24
	});
	const terminal = pty.calls[0].terminal;

	terminal.emitData('private shell output');
	assert.deepEqual(owner.messages, [{
		channel: 'fella:terminal-data',
		message: { terminalId: 'terminal-one', data: 'private shell output' }
	}]);
	assert.deepEqual(otherOwner.messages, []);
	assert.equal(sessions.write(otherOwner, 'terminal-one', 'echo unauthorized'), false);
	assert.equal(sessions.resize(otherOwner, 'terminal-one', 120, 40), false);
	assert.equal(sessions.close(otherOwner, 'terminal-one'), false);
	assert.deepEqual(terminal.writeCalls, []);
	assert.deepEqual(terminal.resizeCalls, []);

	assert.equal(sessions.write(owner, 'terminal-one', 'echo hello\r'), true);
	assert.equal(sessions.resize(owner, 'terminal-one', 120, 40), true);
	assert.deepEqual(terminal.writeCalls, ['echo hello\r']);
	assert.deepEqual(terminal.resizeCalls, [[120, 40]]);
	terminal.emitExit({ exitCode: 0, signal: 0 });
	assert.deepEqual(owner.messages.at(-1), {
		channel: 'fella:terminal-exit',
		message: { terminalId: 'terminal-one', exitCode: 0, signal: 0 }
	});
});

test('rejects invalid sessions and dimensions and closes shells with their owner', () => {
	const pty = fakePty();
	const sessions = new EmbeddedTerminalSessions({ pty, platform: 'linux', env: {} });
	const owner = fakeOwner();
	assert.throws(() => sessions.start(owner, {
		terminalId: 'bad id', workspacePath: '/workspace', cols: 80, rows: 24
	}), /could not be started/);
	assert.throws(() => sessions.start(owner, {
		terminalId: 'terminal-one', workspacePath: ' ', cols: 80, rows: 24
	}), /Choose a workspace/);

	sessions.start(owner, {
		terminalId: 'terminal-one', workspacePath: '/workspace', cols: 0, rows: 900
	});
	assert.deepEqual([pty.calls[0].options.cols, pty.calls[0].options.rows], [80, 24]);
	sessions.closeForOwner(owner);
	assert.equal(pty.calls[0].terminal.killed, true);
	assert.equal(sessions.write(owner, 'terminal-one', 'ignored'), false);
});
