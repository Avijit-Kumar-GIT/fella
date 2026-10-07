import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { PassThrough } from 'node:stream';
import { tmpdir } from 'node:os';
import test from 'node:test';

import { assertBinary, EngineClient, isRegularFile } from '../electron/engine.mjs';

class FakeEngineProcess extends EventEmitter {
	stdin = new PassThrough();
	stdout = new PassThrough();
	killed = false;
	requests = [];
	#pendingLine = '';

	constructor() {
		super();
		this.stdin.on('data', (chunk) => {
			this.#pendingLine += chunk.toString();
			for (;;) {
				const newline = this.#pendingLine.indexOf('\n');
				if (newline < 0) break;
				const line = this.#pendingLine.slice(0, newline);
				this.#pendingLine = this.#pendingLine.slice(newline + 1);
				this.requests.push(JSON.parse(line));
			}
		});
	}

	respond(message) {
		this.stdout.write(`${JSON.stringify(message)}\n`);
	}

	kill() {
		this.killed = true;
		setImmediate(() => this.emit('exit', null, 'SIGTERM'));
	}
}

async function waitForRequests(child, count) {
	for (let attempt = 0; attempt < 20 && child.requests.length < count; attempt++) {
		await new Promise((resolve) => setImmediate(resolve));
	}
	assert.equal(child.requests.length, count, 'engine requests should be written as JSON lines');
}

test('sidecar resolution skips the packaged resource directory and accepts its binary', () => {
	const temp = mkdtempSync(join(tmpdir(), 'fella-sidecar-test-'));
	const resourceDirectory = join(temp, 'fella-engine');
	const sidecar = join(resourceDirectory, 'fella-engine-x64');
	mkdirSync(resourceDirectory);
	writeFileSync(sidecar, 'test executable');

	try {
		assert.equal(isRegularFile(resourceDirectory), false);
		assert.equal(isRegularFile(sidecar), true);
		assert.throws(() => assertBinary(resourceDirectory), /engine was not found/);
		assert.doesNotThrow(() => assertBinary(sidecar));
	} finally {
		rmSync(temp, { recursive: true, force: true });
	}
});

test('correlates concurrent bridge requests and forwards streamed events', async () => {
	const child = new FakeEngineProcess();
	const client = new EngineClient('engine', '/app-data', { spawnProcess: () => child });
	const events = [];
	const answer = client.request('ask', { question: 'count rows' }, (event) => events.push(event));
	const settings = client.request('get_settings');
	await waitForRequests(child, 2);

	const askRequest = child.requests.find((request) => request.method === 'ask');
	const settingsRequest = child.requests.find((request) => request.method === 'get_settings');
	assert.ok(askRequest);
	assert.ok(settingsRequest);
	assert.deepEqual(askRequest.params, { question: 'count rows' });

	child.respond({ id: askRequest.id, event: { type: 'assistant_delta', text: 'Rows: ' } });
	child.respond({ id: settingsRequest.id, ok: true, result: { model: 'test-model' } });
	child.respond({ id: askRequest.id, ok: true, result: { text: 'Rows: 12' } });

	assert.deepEqual(await settings, { model: 'test-model' });
	assert.deepEqual(await answer, { text: 'Rows: 12' });
	assert.deepEqual(events, [{ type: 'assistant_delta', text: 'Rows: ' }]);
	client.dispose();
});

test('preserves engine error kind and message across the bridge', async () => {
	const child = new FakeEngineProcess();
	const client = new EngineClient('engine', '/app-data', { spawnProcess: () => child });
	const request = client.request('open_workspace', { path: '/missing' });
	await waitForRequests(child, 1);
	child.respond({
		id: child.requests[0].id,
		ok: false,
		error: { kind: 'io', message: 'folder unavailable' }
	});

	await assert.rejects(request, (error) => {
		assert.equal(error.kind, 'io');
		assert.equal(error.message, 'folder unavailable');
		return true;
	});
	client.dispose();
});

test('rejects pending requests when the shell shuts down the engine', async () => {
	const child = new FakeEngineProcess();
	const client = new EngineClient('engine', '/app-data', { spawnProcess: () => child });
	const request = client.request('ask', { question: 'unfinished' });
	await waitForRequests(child, 1);
	client.dispose();

	assert.equal(child.killed, true);
	await assert.rejects(request, /Fella engine stopped/);
});
