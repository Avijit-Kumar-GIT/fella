import assert from 'node:assert/strict';
import { createServer } from 'vite';

const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	server: { middlewareMode: true, hmr: false, ws: false }
});

try {
	const { analysisTurnIdsAfterSourceMessage } = await server.ssrLoadModule('/src/lib/investigation-runs.ts');
	const message = (id, role, text, answer) => ({ id, role, text, answer, ts: 1 });
	const source = message('source', 'user', 'Compare revenue by region.');
	const sameWorkspace = (turn_id) => ({ turn_id, workspace: { path: '/data/sales' } });
	const cases = [
		['links runs after the source request in transcript order', () => {
			assert.deepEqual(analysisTurnIdsAfterSourceMessage([
				message('old-run', 'assistant', 'Old result', sameWorkspace('turn-old')),
				source,
				message('answer-1', 'assistant', 'First result', sameWorkspace('turn-1'))
			], 'source', '/data/sales'), ['turn-1']);
		}],
		['deduplicates repeated turn references without changing messages', () => {
			const transcript = [source,
				message('answer-1', 'assistant', 'First result', sameWorkspace('turn-1')),
				message('answer-rerun', 'assistant', 'Rerun result', sameWorkspace('turn-1'))
			];
			assert.deepEqual(analysisTurnIdsAfterSourceMessage(transcript, 'source', '/data/sales'), ['turn-1']);
			assert.equal(transcript.length, 3);
			assert.equal(transcript[1].text, 'First result');
		}],
		['ignores general and different-workspace answers', () => {
			assert.deepEqual(analysisTurnIdsAfterSourceMessage([
				source,
				message('general', 'assistant', 'General answer', { turn_id: 'turn-general' }),
				message('other', 'assistant', 'Other workspace', { turn_id: 'turn-other', workspace: { path: '/other' } })
			], 'source', '/data/sales'), []);
		}],
		['missing source messages fail closed', () => {
			assert.deepEqual(analysisTurnIdsAfterSourceMessage([
				message('answer', 'assistant', 'Result', sameWorkspace('turn-1'))
			], 'source', '/data/sales'), []);
			assert.deepEqual(analysisTurnIdsAfterSourceMessage([source], undefined, '/data/sales'), []);
		}]
	];

	for (const [name, run] of cases) {
		run();
		console.log(`✔ ${name}`);
	}
	console.log(`Validated ${cases.length} Investigation run-link behaviors.`);
} finally {
	await server.close();
}
