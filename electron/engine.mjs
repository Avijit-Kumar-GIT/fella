import { existsSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { spawn } from 'node:child_process';

/** Thin client for the Rust engine's line-delimited JSON protocol. */
export class EngineClient {
	#nextId = 1;
	#pending = new Map();

	constructor(binary, dataDir) {
		this.child = spawn(binary, ['--engine-stdio', '--data-dir', dataDir], {
			stdio: ['pipe', 'pipe', 'inherit'],
			windowsHide: true
		});

		this.lines = createInterface({ input: this.child.stdout });
		this.lines.on('line', (line) => this.#receive(line));
		this.child.on('error', (error) => this.#failAll(error));
		this.child.on('exit', (code, signal) => {
			if (code !== 0 || signal) {
				this.#failAll(new Error(`Fella engine exited (${code ?? signal ?? 'unknown'})`));
			}
		});
	}

	request(method, params = {}, onEvent) {
		const id = this.#nextId++;
		return new Promise((resolve, reject) => {
			this.#pending.set(id, { resolve, reject, onEvent });
			try {
				this.child.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
			} catch (error) {
				this.#pending.delete(id);
				reject(error);
			}
		});
	}

	dispose() {
		this.lines.close();
		if (!this.child.killed) this.child.kill();
		this.#failAll(new Error('Fella engine stopped'));
	}

	#receive(line) {
		let message;
		try {
			message = JSON.parse(line);
		} catch {
			return;
		}
		const pending = this.#pending.get(message.id);
		if (!pending) return;
		if (message.event) {
			pending.onEvent?.(message.event);
			return;
		}
		this.#pending.delete(message.id);
		if (message.ok) {
			pending.resolve(message.result);
		} else {
			const error = new Error(message.error?.message ?? 'Fella engine request failed');
			error.kind = message.error?.kind;
			pending.reject(error);
		}
	}

	#failAll(error) {
		for (const { reject } of this.#pending.values()) reject(error);
		this.#pending.clear();
	}
}

export function assertBinary(binary) {
	if (!binary || !existsSync(binary)) {
		throw new Error(
			`Fella's Rust engine was not found at ${binary ?? '(no path)'}. ` +
			'Run pnpm electron:build first, or set FELLA_ENGINE_PATH.'
		);
	}
}
