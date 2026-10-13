import { session, type WorkspaceLayoutPreset } from '$lib/session.svelte';
import { ipc } from '$lib/ipc';
import type { ConversationSummary, Investigation, Message } from '$lib/types';

/** Focus the durable conversation owned by an Investigation. Older records
 *  may need their source message resolved to a conversation first. */
export async function focusInvestigationConversation(
	item: Investigation,
	layout: WorkspaceLayoutPreset | 'preserve' = 'preserve',
	focusTurnId?: string,
	isCurrent: () => boolean = () => true
): Promise<Investigation | null> {
	let conversationId = item.conversation_id;
	let linkedInvestigation = item;
	let summary: ConversationSummary | null = null;

	if (!conversationId && item.brief_source_message_id) {
		const summaries = await ipc.conversationsList();
		if (!isCurrent()) return null;
		summary = summaries.find((candidate) =>
			candidate.workspace === item.workspace_id &&
			candidate.first_question_message_id === item.brief_source_message_id
		) ?? null;
		conversationId = summary?.id;
		if (conversationId) {
			linkedInvestigation = await ipc.investigationEnsure(
				item.workspace_id,
				conversationId,
				item.brief.goal,
				item.brief_source_message_id
			);
			if (!isCurrent()) return null;
		}
	}
	if (!conversationId) throw new Error('This Investigation has no linked conversation yet.');

	const existingConversation = session.conversations.find((conversation) => conversation.id === conversationId);
	if (existingConversation && existingConversation.workspaceScope !== item.workspace_id) {
		throw new Error('The linked conversation belongs to a different workspace.');
	}
	const loadedIndex = session.conversations.findIndex((conversation) =>
		conversation.id === conversationId && conversation.workspaceScope === item.workspace_id
	);
	if (loadedIndex < 0) {
		summary ??= (await ipc.conversationsList()).find((candidate) =>
			candidate.id === conversationId && candidate.workspace === item.workspace_id
		) ?? null;
		if (!isCurrent()) return null;
		if (summary) {
			const saved = JSON.parse(await ipc.conversationLoad(conversationId)) as {
				workspace?: string | null;
				messages?: unknown;
				title?: string | null;
				companionPane?: unknown;
			};
			if (!isCurrent()) return null;
			if (saved.workspace && saved.workspace !== item.workspace_id) {
				throw new Error('The linked conversation belongs to a different workspace.');
			}
			const messages = Array.isArray(saved.messages) ? saved.messages as Message[] : [];
			session.loadConversation(conversationId, messages, saved.title ?? null, item.workspace_id, saved.companionPane);
		} else {
			const hasRecordedAnalysis = Boolean(
				item.brief_source_message_id ||
				item.analysis_turn_ids?.length ||
				item.findings?.length ||
				item.questions?.some((question) => question.latest_analysis_turn_id || question.analysis_turn_ids?.length)
			);
			if (hasRecordedAnalysis) throw new Error('The linked conversation is missing from local history.');
			// Empty chats are intentionally omitted from conversation history. The
			// durable Investigation still owns this ID, so restore that same blank
			// conversation rather than creating a different one in Focus.
			session.loadConversation(conversationId, [], null, item.workspace_id);
		}
	}

	if (!isCurrent()) return null;
	if (!session.focusWorkspace(item.workspace_id, conversationId)) {
		throw new Error('Fella could not focus the linked conversation.');
	}
	if (layout === 'focus') session.setWorkspacePaneOpen(false);
	else if (layout === 'investigation' || layout === 'workbench') session.setWorkspacePane('investigation');
	session.setActiveInvestigation(linkedInvestigation.id);
	if (focusTurnId) session.focusConversationAnalysis(conversationId, focusTurnId);
	return linkedInvestigation;
}
