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
const { dispatch, openRepository } = await server.ssrLoadModule('/src/lib/commands.ts');
const { default: CompanionPane } = await server.ssrLoadModule('/src/lib/components/CompanionPane.svelte');
const { default: Sidebar } = await server.ssrLoadModule('/src/lib/components/Sidebar.svelte');
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

test('an unavailable repository becomes history-only without adding a raw mount error to chat', async () => {
	const oldWindow = globalThis.window;
	const oldCatalog = session.catalog;
	const oldProgress = session.mountProgress;
	const oldMessages = session.activeChat.messages;
	const oldRepositories = session.repositoryPaths;
	const oldHistoryOnly = session.historyOnlyRepositoryPaths;
	const oldHidden = session.hiddenRepositoryPaths;
	const path = 'C:/Users/test/Documents/archived-workspace';
	try {
		Object.defineProperty(globalThis, 'window', {
			configurable: true,
			writable: true,
			value: {
				fella: {
					invoke: async () => undefined,
					openWorkspace: async () => {
						throw new Error(`That doesn't look like a folder: ${path}`);
					}
				}
			}
		});
		session.catalog = { workspace: null, sources: [] };
		session.mountProgress = null;
		session.activeChat.messages = [];
		session.repositoryPaths = [path];
		session.historyOnlyRepositoryPaths = [];
		session.hiddenRepositoryPaths = [];

		assert.equal(await openRepository(path), false);
		assert.ok(session.repositoryPaths.includes(path));
		assert.ok(session.historyOnlyRepositoryPaths.includes(path));
		assert.equal(session.activeChat.messages.length, 0);

		const html = render(Sidebar).body;
		assert.match(html, /History only · folder unavailable/);
		assert.match(html, /saved conversations remain available/i);
		assert.doesNotMatch(html, /That doesn't look like a folder/);
	} finally {
		session.catalog = oldCatalog;
		session.mountProgress = oldProgress;
		session.activeChat.messages = oldMessages;
		session.repositoryPaths = oldRepositories;
		session.historyOnlyRepositoryPaths = oldHistoryOnly;
		session.hiddenRepositoryPaths = oldHidden;
		if (oldWindow === undefined) delete globalThis.window;
		else Object.defineProperty(globalThis, 'window', { configurable: true, writable: true, value: oldWindow });
	}
});

test('a later successful repository mount restores the normal live state', async () => {
	const oldWindow = globalThis.window;
	const oldCatalog = session.catalog;
	const oldProgress = session.mountProgress;
	const oldMessages = session.activeChat.messages;
	const oldRepositories = session.repositoryPaths;
	const oldHistoryOnly = session.historyOnlyRepositoryPaths;
	const path = 'C:/Users/test/Documents/reconnected-workspace';
	try {
		Object.defineProperty(globalThis, 'window', {
			configurable: true,
			writable: true,
			value: {
				fella: {
					invoke: async () => undefined,
					openWorkspace: async (chosen) => ({ workspace: chosen, sources: [], skipped: [] })
				}
			}
		});
		session.catalog = { workspace: null, sources: [] };
		session.mountProgress = null;
		session.activeChat.messages = [];
		session.repositoryPaths = [path];
		session.historyOnlyRepositoryPaths = [path];

		assert.equal(await openRepository(path), true);
		assert.equal(session.catalog.workspace, path);
		assert.ok(!session.historyOnlyRepositoryPaths.includes(path));
	} finally {
		session.catalog = oldCatalog;
		session.mountProgress = oldProgress;
		session.activeChat.messages = oldMessages;
		session.repositoryPaths = oldRepositories;
		session.historyOnlyRepositoryPaths = oldHistoryOnly;
		if (oldWindow === undefined) delete globalThis.window;
		else Object.defineProperty(globalThis, 'window', { configurable: true, writable: true, value: oldWindow });
	}
});

test('a history-only conversation cannot silently analyze the currently mounted different folder', async () => {
	const oldCatalog = session.catalog;
	const oldMessages = session.activeChat.messages;
	const oldWorkspaceScope = session.activeChat.workspaceScope;
	const oldHistoryOnly = session.historyOnlyRepositoryPaths;
	const origin = 'C:/Users/test/Documents/archived-workspace';
	try {
		session.catalog = { workspace: 'C:/Users/test/Documents/another-workspace', sources: [] };
		session.activeChat.messages = [];
		session.activeChat.workspaceScope = origin;
		session.historyOnlyRepositoryPaths = [origin];

		await dispatch('Continue the analysis from my saved conversation');

		assert.equal(session.activeChat.messages.length, 1);
		assert.equal(session.activeChat.messages[0].role, 'system');
		assert.match(session.activeChat.messages[0].text, /available as history only/i);
		assert.doesNotMatch(session.activeChat.messages.map((message) => message.text).join('\n'), /Continue the analysis/);
	} finally {
		session.catalog = oldCatalog;
		session.activeChat.messages = oldMessages;
		session.activeChat.workspaceScope = oldWorkspaceScope;
		session.historyOnlyRepositoryPaths = oldHistoryOnly;
	}
});
