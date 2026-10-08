import assert from 'node:assert/strict';
import { after, test } from 'node:test';
import { createServer } from 'vite';

const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	server: { middlewareMode: true, hmr: false, ws: false }
});
const { isRenderableChartEvidence, paneFreshness, parseCompanionPane } = await server.ssrLoadModule('/src/lib/workbench.ts');
const { render } = await server.ssrLoadModule('svelte/server');
const { session } = await server.ssrLoadModule('/src/lib/session.svelte.ts');
const { default: CompanionPane } = await server.ssrLoadModule('/src/lib/components/CompanionPane.svelte');
after(() => server.close());

test('conversation archive accepts a chart reference without copying chart data', () => {
	const saved = {
		kind: 'chart',
		messageId: 'assistant-7',
		evidenceIndex: 2,
		evidenceId: 'chart-evidence-2',
		workspacePath: 'C:/data/finance',
		revision: 'rev-42'
	};
	assert.deepEqual(parseCompanionPane(saved), saved);
});

test('conversation archive accepts an exact-path source preview reference', () => {
	const saved = {
		kind: 'source',
		path: 'C:/data/finance/exports/current.csv',
		name: 'exports/current.csv',
		workspacePath: 'C:/data/finance',
		revision: 'rev-42'
	};
	assert.deepEqual(parseCompanionPane(saved), saved);
});

test('malformed or unsupported saved panes are ignored without a partial reference', () => {
	assert.equal(parseCompanionPane(null), null);
	assert.equal(parseCompanionPane({ kind: 'plugin', code: 'arbitrary' }), null);
	assert.equal(parseCompanionPane({ kind: 'chart', messageId: 'm', evidenceIndex: -1 }), null);
	assert.equal(parseCompanionPane({ kind: 'source', path: '/data/sales.csv', name: 'sales.csv' }), null);
});

test('a companion cannot revive a chart excluded or withheld by its answer', () => {
	const evidence = {
		id: 'chart-1',
		tool: 'make_chart', chart: { kind: 'bar', labels: [], series: [] },
		args: {}, result_summary: 'chart', ms: 0
	};
	assert.equal(isRenderableChartEvidence(evidence), true);
	assert.equal(isRenderableChartEvidence({ ...evidence, error: 'failed' }), false);
	assert.equal(isRenderableChartEvidence({ ...evidence, verifier_disposition: { state: 'excluded', reason: 'test' } }), false);
	assert.equal(isRenderableChartEvidence(evidence, [{
		label: 'withheld', ok: false,
		finding: { code: 'chart_mismatch', effect: 'withhold_artifact', target: 'artifact', target_id: 'chart-1' }
	}]), false);
	assert.equal(isRenderableChartEvidence(evidence, [{
		label: 'unrelated artifact withheld', ok: false,
		finding: { code: 'chart_mismatch', effect: 'withhold_artifact', target: 'artifact', target_id: 'another-chart' }
	}]), true);
});

test('pane freshness distinguishes current, changed revision, changed workspace, and unknown', () => {
	const pane = parseCompanionPane({
		kind: 'source', path: '/data/sales.csv', name: 'sales.csv',
		workspacePath: '/data', revision: 'rev-1'
	});
	assert.ok(pane);
	assert.equal(paneFreshness(pane, '/data', 'rev-1'), 'current');
	assert.equal(paneFreshness(pane, '/data', 'rev-2'), 'different-revision');
	assert.equal(paneFreshness(pane, '/other', 'rev-1'), 'different-workspace');
	assert.equal(paneFreshness(pane, '/data', null), 'unknown');
});

test('a source pane does not silently preview a changed workspace revision', () => {
	const oldCatalog = session.catalog;
	const oldPane = session.activeChat.companionPane;
	try {
		session.catalog = {
			workspace: '/data', revision: 'rev-2', mtime: 2,
			sources: [{
				name: 'sales.csv', path: '/data/sales.csv', kind: 'csv', view: 'sales',
				size_bytes: 20, mtime: 2
			}]
		};
		session.activeChat.companionPane = {
			kind: 'source', path: '/data/sales.csv', name: 'sales.csv',
			workspacePath: '/data', revision: 'rev-1'
		};
		const stale = render(CompanionPane).body;
		assert.match(stale, /workspace changed after this preview was opened/i);
		assert.doesNotMatch(stale, /First look/);
		assert.match(stale, /Open current version/);

		session.catalog = { ...session.catalog, revision: 'rev-1' };
		const current = render(CompanionPane).body;
		assert.match(current, /sales\.csv/);
		assert.match(current, /Preview only/);
		assert.match(current, /First look/);
	} finally {
		session.catalog = oldCatalog;
		session.activeChat.companionPane = oldPane;
	}
});

test('a chart companion resolves the selected conversation evidence by identity', () => {
	const oldCatalog = session.catalog;
	const oldPane = session.activeChat.companionPane;
	const oldMessages = session.activeChat.messages;
	try {
		const chart = {
			kind: 'bar', title: 'Quarterly revenue', labels: ['Q1', 'Q2'],
			series: [{ name: 'Revenue', values: [12, 18] }]
		};
		session.catalog = { ...oldCatalog, workspace: '/data', revision: 'rev-1' };
		session.activeChat.messages = [{
			id: 'answer-1', role: 'assistant', text: 'Revenue increased.', ts: 1,
			answer: {
				text: 'Revenue increased.', workspace: { path: '/data', revision: 'rev-1' },
				evidence: [{ id: 'chart-1', tool: 'make_chart', args: {}, result_summary: '2 quarters', chart }],
				verification: []
			}
		}];
		session.activeChat.companionPane = {
			kind: 'chart', messageId: 'answer-1', evidenceIndex: 0, evidenceId: 'chart-1',
			workspacePath: '/data', revision: 'rev-1'
		};
		const rendered = render(CompanionPane).body;
		assert.match(rendered, /Quarterly revenue/);
		assert.match(rendered, /aria-label="Quarterly revenue"/);
	} finally {
		session.catalog = oldCatalog;
		session.activeChat.messages = oldMessages;
		session.activeChat.companionPane = oldPane;
	}
});
