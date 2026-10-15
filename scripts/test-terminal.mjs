import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough, Writable } from 'node:stream';
import { test } from 'node:test';

import { EmbeddedTerminalSessions } from '../electron/embedded-terminal.mjs';
import { discoverTerminalShells } from '../electron/terminal-shells.mjs';
import { createTerminalOutputFormatter } from '../src/lib/terminal-output.js';

test('removes GNU ls green directory backgrounds while preserving foreground colors', () => {
	const format = createTerminalOutputFormatter();
	assert.equal(format('\u001b[34;42mreports\u001b[0m'), '\u001b[34;49mreports\u001b[0m');
	assert.equal(format('\u001b[1;102mbright\u001b[0m'), '\u001b[1;49mbright\u001b[0m');
	assert.equal(format('\u001b[38;5;42mforeground\u001b[0m'), '\u001b[38;5;42mforeground\u001b[0m');
	assert.equal(format('\u001b[34;44mother background\u001b[0m'), '\u001b[34;44mother background\u001b[0m');
});

test('removes a green background when its SGR sequence spans PTY chunks', () => {
	const format = createTerminalOutputFormatter();
	assert.equal(format('before\u001b[34;4'), 'before');
	assert.equal(format('2mfolder\u001b[0m'), '\u001b[34;49mfolder\u001b[0m');
});

class FakeHelper extends EventEmitter {
	constructor() {
		super();
		this.stdout = new PassThrough();
		this.stderr = new PassThrough();
		this.exitCode = null;
		this.signalCode = null;
		this.commands = [];
		this.killed = false;
		this.stdin = new Writable({
			write: (chunk, _encoding, callback) => {
				this.commands.push(chunk.toString());
				callback();
			}
		});
	}

	start() {
		setImmediate(() => this.send({ type: 'started' }));
	}

	send(message) {
		this.stdout.write(`${JSON.stringify(message)}\n`);
	}

	kill() {
		this.killed = true;
		this.exitCode = 1;
		this.emit('exit', 1, null);
		return true;
	}
}

function fakeOwner() {
	return Object.assign(new EventEmitter(), {
		messages: [],
		destroyed: false,
		isDestroyed() { return this.destroyed; },
		send(channel, message) { this.messages.push({ channel, message }); }
	});
}

function makeSessions() {
	const calls = [];
	const sessions = new EmbeddedTerminalSessions({
		binary: '/app/fella-engine',
		spawnProcess: (file, args, options) => {
			const child = new FakeHelper();
			calls.push({ file, args, options, child });
			child.start();
			return child;
		}
	});
	return { sessions, calls };
}

test('discovers installed Windows shells and prefers the configured command shell', () => {
	const installed = new Set([
		'C:\\Windows\\System32\\cmd.exe',
		'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe',
		'D:\\Tools\\pwsh.exe'
	].map((item) => item.toLowerCase()));
	const found = discoverTerminalShells({
		platform: 'win32',
		env: {
			ComSpec: 'C:\\Windows\\System32\\cmd.exe',
			Path: 'D:\\Tools;C:\\Windows\\System32',
			SystemRoot: 'C:\\Windows',
			PATHEXT: '.COM;.EXE;.BAT;.CMD'
		},
		fileExists: (filePath) => installed.has(filePath.toLowerCase())
	});
	assert.equal(found.defaultShellPath, 'C:\\Windows\\System32\\cmd.exe');
	assert.deepEqual(found.shells.map(({ name }) => name), ['Command Prompt', 'Windows PowerShell', 'PowerShell']);
	assert.equal(found.shells[2].path, 'D:\\Tools\\pwsh.exe');
});

test('discovers the configured Unix shell and other installed shells without adding dependencies', () => {
	const installed = new Set(['/usr/bin/zsh', '/usr/bin/bash', '/bin/sh']);
	const found = discoverTerminalShells({
		platform: 'linux',
		env: { SHELL: '/usr/bin/zsh', PATH: '/usr/bin:/bin' },
		fileExists: (filePath) => installed.has(filePath)
	});
	assert.equal(found.defaultShellPath, '/usr/bin/zsh');
	assert.deepEqual(found.shells, [
		{ path: '/usr/bin/zsh', name: 'Zsh' },
		{ path: '/usr/bin/bash', name: 'Bash' },
		{ path: '/bin/sh', name: 'POSIX shell' }
	]);
});

test('starts the selected shell with the workspace path and requested PTY dimensions', async () => {
	const { sessions, calls } = makeSessions();
	const owner = fakeOwner();
	const result = await sessions.start(owner, {
		terminalId: 'shell-one',
		workspacePath: '/work/northwind sales',
		shellPath: '/usr/bin/fish',
		cols: 117,
		rows: 38
	});
	assert.deepEqual(result, {
		terminalId: 'shell-one',
		cwd: '/work/northwind sales',
		shell: 'fish',
		shellPath: '/usr/bin/fish'
	});
	assert.equal(calls[0].file, '/app/fella-engine');
	assert.deepEqual(calls[0].args, [
		'--terminal-session',
		'--shell', '/usr/bin/fish',
		'--cwd', '/work/northwind sales',
		'--cols', '117',
		'--rows', '38'
	]);
	assert.deepEqual(calls[0].options.stdio, ['pipe', 'pipe', 'pipe']);
	assert.equal(calls[0].options.windowsHide, true);
});

test('routes shell input, resize and output only through the owning renderer', async () => {
	const { sessions, calls } = makeSessions();
	const owner = fakeOwner();
	const otherOwner = fakeOwner();
	await sessions.start(owner, {
		terminalId: 'shell-one',
		workspacePath: '/workspace',
		shellPath: '/bin/sh',
		cols: 80,
		rows: 24
	});
	const child = calls[0].child;
	child.send({ type: 'data', data: 'private shell output' });
	assert.deepEqual(owner.messages, [{
		channel: 'fella:terminal-data',
		message: { terminalId: 'shell-one', data: 'private shell output' }
	}]);
	assert.deepEqual(otherOwner.messages, []);

	assert.equal(sessions.write(otherOwner, 'shell-one', 'echo unauthorized'), false);
	assert.equal(sessions.resize(otherOwner, 'shell-one', 120, 40), false);
	assert.equal(sessions.close(otherOwner, 'shell-one'), false);
	assert.deepEqual(child.commands, []);

	assert.equal(sessions.write(owner, 'shell-one', 'echo hello\r'), true);
	assert.equal(sessions.resize(owner, 'shell-one', 120, 40), true);
	assert.deepEqual(child.commands.map((line) => JSON.parse(line)), [
		{ type: 'data', data: 'echo hello\r' },
		{ type: 'resize', cols: 120, rows: 40 }
	]);

	child.send({ type: 'exit', exitCode: 0, signal: null });
	assert.deepEqual(owner.messages.at(-1), {
		channel: 'fella:terminal-exit',
		message: { terminalId: 'shell-one', exitCode: 0, signal: null }
	});
});

test('rejects invalid starts and stops the shell helper when its owner closes it', async () => {
	const { sessions, calls } = makeSessions();
	const owner = fakeOwner();
	assert.throws(() => sessions.start(owner, {
		terminalId: 'bad id', workspacePath: '/workspace', shellPath: '/bin/sh', cols: 80, rows: 24
	}), /could not be started/);
	assert.throws(() => sessions.start(owner, {
		terminalId: 'shell-one', workspacePath: ' ', shellPath: '/bin/sh', cols: 80, rows: 24
	}), /Choose a workspace/);
	assert.throws(() => sessions.start(owner, {
		terminalId: 'shell-one', workspacePath: '/workspace', shellPath: ' ', cols: 80, rows: 24
	}), /Choose a shell/);

	await sessions.start(owner, {
		terminalId: 'shell-one', workspacePath: '/workspace', shellPath: '/bin/sh', cols: 0, rows: 900
	});
	assert.equal(sessions.write(owner, 'shell-one', 'x'.repeat(1_000_001)), false);
	assert.equal(sessions.resize(owner, 'shell-one', 1, 501), false);
	assert.equal(sessions.close(owner, 'shell-one'), true);
	assert.deepEqual(calls[0].child.commands.map((line) => JSON.parse(line)), [{ type: 'close' }]);
	assert.equal(sessions.write(owner, 'shell-one', 'ignored'), false);
});

test('surfaces a Rust helper startup error to the shell view', async () => {
	const child = new FakeHelper();
	const sessions = new EmbeddedTerminalSessions({
		binary: '/app/fella-engine',
		spawnProcess: () => {
			setImmediate(() => child.send({ type: 'error', message: 'shell executable not found' }));
			return child;
		}
	});
	await assert.rejects(sessions.start(fakeOwner(), {
		terminalId: 'shell-fail', workspacePath: '/workspace', shellPath: '/missing/sh', cols: 80, rows: 24
	}), /shell executable not found/);
	assert.equal(child.killed, true);
});
