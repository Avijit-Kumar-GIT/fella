import assert from 'node:assert/strict';
import { after, test } from 'node:test';
import { createServer } from 'vite';

const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	server: { middlewareMode: true, hmr: false, ws: false }
});
const { Conversation, session } = await server.ssrLoadModule('/src/lib/session.svelte.ts');
after(() => server.close());

test('a stale Investigation selection cannot rebind or steal another tab', () => {
	const previous = {
		workspaceWindows: session.workspaceWindows,
		environments: session.environments,
		activeEnvironmentId: session.activeEnvironmentId,
		conversations: session.conversations,
		activeConversationIndex: session.activeConversationIndex,
		activeWorkspaceId: session.activeWorkspaceId,
		catalog: session.catalog,
		activeInvestigationId: session.activeInvestigationId,
		workspacePane: session.workspacePane,
		workspaceLayoutPreset: session.workspaceLayoutPreset,
		workspaceView: session.workspaceView,
		workspaceToolChooserOpen: session.workspaceToolChooserOpen
	};

	try {
		const workspacePath = '/test/workspace';
		const firstTab = {
			id: 'tool-tab-1', kind: 'investigation', investigationId: 'investigation-1', conversationId: 'conversation-1'
		};
		const secondTab = {
			id: 'tool-tab-2', kind: 'investigation', investigationId: 'investigation-2', conversationId: 'conversation-2'
		};
		const pane = {
			id: 'test-pane', workspaceId: workspacePath, conversationId: 'conversation-2',
			workspacePane: 'investigation', workspacePaneOpen: true,
			investigationId: 'investigation-2', workspaceToolTabs: [firstTab, secondTab],
			activeWorkspaceToolTabId: secondTab.id
		};
		session.workspaceWindows = [{
			id: workspacePath, kind: 'repository', path: workspacePath,
			catalog: { workspace: workspacePath, sources: [] }, conversationId: 'conversation-2'
		}];
		session.environments = [{
			id: 'test-environment', panes: [pane], activePaneId: pane.id, layout: {}
		}];
		session.activeEnvironmentId = 'test-environment';
		session.conversations = [new Conversation('conversation-1'), new Conversation('conversation-2')];
		session.conversations[0].workspaceScope = workspacePath;
		session.conversations[1].workspaceScope = workspacePath;
		session.activeConversationIndex = 1;
		session.activeWorkspaceId = workspacePath;
		session.catalog = { workspace: workspacePath, sources: [] };
		session.activeInvestigationId = 'investigation-2';
		session.workspacePane = 'investigation';
		session.workspaceLayoutPreset = 'workbench';
		session.workspaceView = 'ask';
		session.workspaceToolChooserOpen = false;

		const originalTabs = structuredClone(pane.workspaceToolTabs);
		session.setActiveInvestigation('investigation-1');
		assert.deepEqual(pane.workspaceToolTabs, originalTabs);
		assert.equal(pane.activeWorkspaceToolTabId, secondTab.id);
		assert.equal(session.activeInvestigationId, secondTab.investigationId);
		assert.equal(pane.investigationId, secondTab.investigationId);

		// Even if a stale caller supplies the active chat alongside another
		// Investigation ID, tab opening must not choose by the conversation side
		// of an ID-or-conversation match.
		assert.equal(
			session.openWorkspaceToolTab('investigation', 'investigation-1', 'conversation-2'),
			null
		);
		assert.deepEqual(pane.workspaceToolTabs, originalTabs);
		assert.equal(pane.activeWorkspaceToolTabId, secondTab.id);

		// If a prior app version already changed only the saved Investigation ID,
		// recover it from the durable conversation-to-Investigation relationship.
		secondTab.investigationId = 'investigation-1';
		session.reconcileWorkspaceToolTabs([
			{ id: 'investigation-1', conversation_id: 'conversation-1' },
			{ id: 'investigation-2', conversation_id: 'conversation-2' }
		]);
		assert.equal(secondTab.investigationId, 'investigation-2');
		assert.equal(session.activeInvestigationId, 'investigation-2');
	} finally {
		Object.assign(session, previous);
	}
});
