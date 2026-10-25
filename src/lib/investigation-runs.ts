import type { Message } from './types';

/** Finds workspace-backed runs referenced by assistant messages after the
 * Investigation's original request. Conversation archives remain unchanged. */
export function analysisTurnIdsAfterSourceMessage(
	messages: readonly Message[],
	sourceMessageId: string | undefined,
	workspaceId: string
): string[] {
	if (!sourceMessageId) return [];
	const sourceIndex = messages.findIndex((message) => message.id === sourceMessageId);
	if (sourceIndex < 0) return [];

	const seen = new Set<string>();
	for (const message of messages.slice(sourceIndex + 1)) {
		if (message.role !== 'assistant') continue;
		const turnId = message.answer?.turn_id?.trim();
		if (!turnId || message.answer?.workspace?.path !== workspaceId) continue;
		seen.add(turnId);
	}
	return [...seen];
}
