import assert from 'node:assert/strict';
import { createServer } from 'vite';

// The desktop transport uses structured cloning, not JSON.stringify itself.
// Test nested reactive-like data against that real serialization constraint.
const server = await createServer({ configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error', server: { middlewareMode: true, hmr: false, ws: false } });
const calls = [];
const previous = globalThis.window;
globalThis.window = { fella: { invoke: async (command, args) => { calls.push({ command, args: structuredClone(args) }); return {}; } } };
try {
	const { ipc } = await server.ssrLoadModule('/src/lib/ipc.ts');
	const questionIds = new Proxy(['question-one'], {});
	await ipc.investigationSetAnalysisTurnQuestions('investigation-one', 'run-one', questionIds);
	assert.deepEqual(calls[0], { command: 'investigation_set_analysis_turn_questions', args: { investigationId: 'investigation-one', analysisTurnId: 'run-one', questionIds: ['question-one'] } });
	const definitions = new Proxy([new Proxy({ id: 'meaning-one', kind: 'definition', text: 'Revenue excludes refunds.' }, {})], {});
	await ipc.investigationUpdateDefinitions('investigation-one', definitions);
	assert.equal(calls[1].args.definitions[0].text, 'Revenue excludes refunds.');
	await ipc.investigationUpdateBrief('investigation-one', new Proxy({ goal: 'Compare revenue', audience: '', desired_output: '', constraints: new Proxy(['Exclude cancelled orders'], {}) }, {}));
	assert.deepEqual(calls[2].args.brief.constraints, ['Exclude cancelled orders']);
	assert.deepEqual([...questionIds], ['question-one']);
	console.log('✔ nested renderer payloads cross a structured-clone boundary without mutating their sources');
} finally { globalThis.window = previous; await server.close(); }
