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
		workspaceContexts: session.workspaceContexts,
		conversations: session.conversations,
		activeConversationIndex: session.activeConversationIndex,
		activeWorkspaceId: session.activeWorkspaceId,
		catalog: session.catalog,
		activeInvestigationId: session.activeInvestigationId,
		workspacePane: session.workspacePane,
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
		const context = {
			workspaceId: workspacePath, conversationId: 'conversation-2',
			workspacePane: 'investigation', workspacePaneOpen: true,
			investigationId: 'investigation-2', workspaceToolTabs: [firstTab, secondTab],
			activeWorkspaceToolTabId: secondTab.id
		};
		session.workspaceWindows = [{
			id: workspacePath, kind: 'repository', path: workspacePath,
			catalog: { workspace: workspacePath, sources: [] }, conversationId: 'conversation-2'
		}];
		session.workspaceContexts = [context];
		session.conversations = [new Conversation('conversation-1'), new Conversation('conversation-2')];
		session.conversations[0].workspaceScope = workspacePath;
		session.conversations[1].workspaceScope = workspacePath;
		session.activeConversationIndex = 1;
		session.activeWorkspaceId = workspacePath;
		session.catalog = { workspace: workspacePath, sources: [] };
		session.activeInvestigationId = 'investigation-2';
		session.workspacePane = 'investigation';
		session.workspaceView = 'ask';
		session.workspaceToolChooserOpen = false;

		const originalTabs = structuredClone(context.workspaceToolTabs);
		session.setActiveInvestigation('investigation-1');
		assert.deepEqual(context.workspaceToolTabs, originalTabs);
		assert.equal(context.activeWorkspaceToolTabId, secondTab.id);
		assert.equal(session.activeInvestigationId, secondTab.investigationId);
		assert.equal(context.investigationId, secondTab.investigationId);

		// Even if a stale caller supplies the active chat alongside another
		// Investigation ID, tab opening must not choose by the conversation side
		// of an ID-or-conversation match.
		assert.equal(
			session.openWorkspaceToolTab('investigation', 'investigation-1', 'conversation-2'),
			null
		);
		assert.deepEqual(context.workspaceToolTabs, originalTabs);
		assert.equal(context.activeWorkspaceToolTabId, secondTab.id);

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

test('workspace Shell tabs reuse by default and allow an explicit independent instance', () => {
	const previous = {
		workspaceWindows: session.workspaceWindows,
		workspaceContexts: session.workspaceContexts,
		conversations: session.conversations,
		activeConversationIndex: session.activeConversationIndex,
		activeWorkspaceId: session.activeWorkspaceId,
		catalog: session.catalog,
		activeInvestigationId: session.activeInvestigationId,
		workspacePane: session.workspacePane,
		workspaceView: session.workspaceView,
		workspaceToolChooserOpen: session.workspaceToolChooserOpen
	};

	try {
		const workspacePath = '/test/workspace-terminal';
		const conversation = new Conversation('conversation-terminal');
		conversation.workspaceScope = workspacePath;
		const context = {
			workspaceId: workspacePath,
			conversationId: conversation.id,
			workspacePane: 'investigation',
			workspacePaneOpen: true,
			workspaceToolTabs: [],
			activeWorkspaceToolTabId: null
		};
		session.workspaceWindows = [{
			id: workspacePath,
			kind: 'repository',
			path: workspacePath,
			catalog: { workspace: workspacePath, sources: [] },
			conversationId: conversation.id
		}];
		session.workspaceContexts = [context];
		session.conversations = [conversation];
		session.activeConversationIndex = 0;
		session.activeWorkspaceId = workspacePath;
		session.catalog = { workspace: workspacePath, sources: [] };
		session.activeInvestigationId = null;
		session.workspacePane = 'investigation';
		session.workspaceView = 'ask';
		session.workspaceToolChooserOpen = true;

		const first = session.openWorkspaceToolTab('terminal');
		const second = session.openWorkspaceToolTab('terminal');
		assert.equal(first?.id, 'terminal');
		assert.equal(second?.id, first?.id);
		assert.equal(context.workspaceToolTabs.length, 1);
		assert.equal(context.activeWorkspaceToolTabId, 'terminal');
		assert.equal(context.workspacePane, 'context');
		assert.equal(session.activeChat.id, conversation.id);
		assert.equal(session.catalog.workspace, workspacePath);

		const secondInstance = session.openWorkspaceToolTab('terminal', null, null, true);
		assert.ok(secondInstance);
		assert.notEqual(secondInstance.id, first?.id);
		assert.equal(context.workspaceToolTabs.length, 2);
		assert.equal(context.activeWorkspaceToolTabId, secondInstance.id);
		assert.equal(session.activeChat.id, conversation.id);
	} finally {
		Object.assign(session, previous);
	}
});
