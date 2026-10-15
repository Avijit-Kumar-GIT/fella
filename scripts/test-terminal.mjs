import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import test from 'node:test';

import { openWorkspaceTerminal, terminalLaunchCandidates } from '../electron/terminal.mjs';

test('platform launch candidates preserve the workspace path as one argument', () => {
	const workspace = 'C:\\Users\\Avi Projects\\sales';
	assert.deepEqual(terminalLaunchCandidates('win32', workspace, { ComSpec: 'cmd.exe' })[0], {
		command: 'cmd.exe',
		args: []
	});
	assert.deepEqual(terminalLaunchCandidates('darwin', '/Users/avi/sales')[0], {
		command: 'open',
		args: ['-a', 'Terminal', '/Users/avi/sales']
	});
	assert.deepEqual(terminalLaunchCandidates('linux', '/home/avi/sales', {})[0], {
		command: 'xdg-terminal-exec',
		args: ['--dir', '/home/avi/sales']
	});
	assert.deepEqual(terminalLaunchCandidates('linux', '/home/avi/sales', { TERMINAL: 'kitty' })[0], {
		command: 'kitty',
		args: []
	});
	assert.throws(() => terminalLaunchCandidates('plan9', '/workspace'), /not supported/);
});

test('terminal launch detaches without exposing terminal input or output to Fella', async () => {
	const cwd = '/workspace/northwind sales';
	const calls = [];
	let unrefCalled = false;
	const spawnProcess = (command, args, options) => {
		calls.push({ command, args, options });
		const child = new EventEmitter();
		child.unref = () => { unrefCalled = true; };
		queueMicrotask(() => child.emit('spawn'));
		return child;
	};

	assert.deepEqual(await openWorkspaceTerminal(cwd, {
		platform: 'linux',
		env: {},
		spawnProcess
	}), { opened: true });
	assert.equal(calls.length, 1);
	assert.deepEqual(calls[0], {
		command: 'xdg-terminal-exec',
		args: ['--dir', cwd],
		options: {
			cwd,
			detached: true,
			stdio: 'ignore',
			windowsHide: false,
			shell: false
		}
	});
	assert.equal(unrefCalled, true);
});

test('only a missing launcher falls through to the next system fallback', async () => {
	const calls = [];
	const spawnProcess = (command, args, options) => {
		calls.push({ command, args, options });
		const child = new EventEmitter();
		child.unref = () => {};
		queueMicrotask(() => {
			if (calls.length === 1) {
				const error = new Error('not installed');
				error.code = 'ENOENT';
				child.emit('error', error);
			} else {
				child.emit('spawn');
			}
		});
		return child;
	};

	await openWorkspaceTerminal('C:\\Users\\Avi\\sales', {
		platform: 'win32',
		env: { ComSpec: 'custom-shell.exe' },
		spawnProcess
	});
	assert.deepEqual(calls.map(({ command, args }) => ({ command, args })), [
		{ command: 'custom-shell.exe', args: [] },
		{ command: 'cmd.exe', args: [] }
	]);
	assert.ok(calls.every(({ options }) => options.stdio === 'ignore' && options.shell === false));
	assert.ok(calls.every(({ options }) => options.cwd === 'C:\\Users\\Avi\\sales'));
});

test('terminal launcher rejects empty paths and reports a missing Linux default', async () => {
	await assert.rejects(openWorkspaceTerminal('  ', { platform: 'linux' }), /Choose a workspace/);
	const spawnProcess = (command) => {
		const child = new EventEmitter();
		queueMicrotask(() => {
			const error = new Error(`${command} not found`);
			error.code = 'ENOENT';
			child.emit('error', error);
		});
		return child;
	};
	await assert.rejects(
		openWorkspaceTerminal('/workspace', { platform: 'linux', env: {}, spawnProcess }),
		/No system terminal launcher was found/
	);
});
