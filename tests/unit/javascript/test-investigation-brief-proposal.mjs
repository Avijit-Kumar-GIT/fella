import assert from 'node:assert/strict';
import { createServer } from 'vite';

const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	server: { middlewareMode: true, hmr: false, ws: false }
});
const { findBriefProposal } = await server.ssrLoadModule('/src/lib/investigation-brief-proposal.ts');

const message = (id, role, text) => ({ id, role, text, ts: 1 });
const root = message('root', 'user', 'Compare revenue by region.');

function directionTargetFor(messages) {
	const proposal = findBriefProposal(messages, 'root');
	return proposal?.target ?? null;
}

const cases = [
	['ordinary follow-up questions do not propose a brief change', () => {
		assert.equal(findBriefProposal([
			root,
			message('follow-up', 'user', 'Can you calculate the monthly average too?')
		], 'root'), null);
	}],
	['explicit goal changes are surfaced as editable goal proposals', () => {
		assert.deepEqual(findBriefProposal([
			root,
			message('change', 'user', 'Actually, focus on gross margin by region instead.')
		], 'root'), {
			messageId: 'change', target: 'goal', text: 'Actually, focus on gross margin by region instead.'
		});
	}],
	['explicit source restrictions propose an additional constraint', () => {
		assert.equal(directionTargetFor([
			root,
			message('constraint', 'user', 'Exclude internal test accounts from the analysis.')
		]), 'constraint');
	}],
	['an explicit deliverable revision proposes the desired output', () => {
		assert.equal(directionTargetFor([
			root,
			message('output', 'user', 'Instead of prose, make it a chart.')
		]), 'desired_output');
	}],
	['only the first unreviewed direction change is proposed', () => {
		const messages = [
			root,
			message('first', 'user', 'Focus on gross margin instead.'),
			message('second', 'user', 'Exclude internal test accounts.')
		];
		assert.equal(findBriefProposal(messages, 'root')?.messageId, 'first');
		assert.equal(findBriefProposal(messages, 'root', 'first')?.messageId, 'second');
		assert.equal(findBriefProposal(messages, 'root', 'second'), null);
	}],
	['assistant messages and slash commands are never treated as analyst direction', () => {
		assert.equal(findBriefProposal([
			root,
			message('assistant', 'assistant', 'Actually, change the goal.'),
			message('command', 'user', '/clear'),
			message('follow-up', 'user', 'Show me the current chart.')
		], 'root'), null);
	}],
	['missing or out-of-conversation anchors fail closed', () => {
		assert.equal(findBriefProposal([
			message('change', 'user', 'Focus on gross margin instead.')
		], 'root'), null);
		assert.equal(findBriefProposal([root], undefined), null);
	}]
];

try {
	for (const [name, run] of cases) {
		run();
		console.log(`✔ ${name}`);
	}
	console.log(`Validated ${cases.length} brief-proposal behaviors.`);
} finally {
	await server.close();
}
