import type { Message } from './types';

export type BriefProposalTarget = 'goal' | 'desired_output' | 'constraint';

export interface BriefProposal {
	messageId: string;
	target: BriefProposalTarget;
	text: string;
}

/**
 * Deliberately conservative, local-only signals. This does not ask a model to
 * reinterpret every turn: ordinary follow-ups stay conversational, while
 * explicit revisions and source restrictions can be reviewed by the analyst.
 */
export function directionChangeTarget(text: string): BriefProposalTarget | null {
	if (/\b(?:include only|only include|exclude|leave out|omit|filter out|restrict(?: the analysis)? to|limit (?:the analysis )?to|only use|use only|only consider|only analy[sz]e|do not include|don't include)\b/i.test(text)) {
		return 'constraint';
	}
	const revisionSignal = /\b(?:instead(?: of)?|rather than|I meant|I mean|change (?:the )?(?:goal|question|scope|direction|focus)|revise (?:the )?(?:goal|question|scope)|focus on|shift (?:the )?(?:focus|scope|question)|the real question|ignore (?:my )?(?:last|previous) (?:request|question))\b/i.test(text)
		|| /^\s*actually\s*[,—:-]/i.test(text)
		|| /\bnot\b.{1,100}\bbut\b/i.test(text);
	if (!revisionSignal) return null;

	if (/\b(?:chart|graph|visuali[sz]ation|dashboard|table|report|memo|summary|spreadsheet|csv|slide deck)\b/i.test(text)
		&& /\b(?:instead|rather|switch|change|make|deliver|format|need|want|prefer)\b/i.test(text)) {
		return 'desired_output';
	}
	return 'goal';
}

/** Return the earliest explicit, unreviewed user direction change after the
 * original request. Missing anchors fail closed so unrelated or truncated
 * conversations cannot silently rewrite this Investigation's intent. */
export function findBriefProposal(
	messages: readonly Message[],
	sourceMessageId: string | undefined,
	reviewedMessageId?: string
): BriefProposal | null {
	if (!sourceMessageId) return null;
	const sourceIndex = messages.findIndex((message) => message.id === sourceMessageId);
	if (sourceIndex < 0) return null;
	const reviewedId = reviewedMessageId ?? sourceMessageId;
	const reviewedIndex = messages.findIndex((message) => message.id === reviewedId);
	if (reviewedIndex < sourceIndex) return null;

	for (const message of messages.slice(reviewedIndex + 1)) {
		const text = message.text.trim();
		// Keep in sync with isActualQuestion in session.svelte.ts: commands and
		// assistant/model messages are not analyst direction changes.
		if (message.role !== 'user' || !text || text.startsWith('/')) continue;
		const target = directionChangeTarget(text);
		if (target) return { messageId: message.id, target, text };
	}
	return null;
}
