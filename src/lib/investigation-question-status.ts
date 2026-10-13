import type { InvestigationQuestion, InvestigationQuestionAnswerStatus } from '$lib/types';

const statusLabel: Record<InvestigationQuestionAnswerStatus, string> = {
	not_started: 'Not started',
	running: 'Running',
	answered: 'Answered',
	incomplete: 'Partial answer',
	unsupported: 'Unsupported',
	insufficient_data: 'Insufficient data',
	needs_review: 'Needs review',
	waiting_for_input: 'Waiting for your input',
	blocked: 'Blocked',
	outdated: 'Outdated'
};

const statusDescription: Record<InvestigationQuestionAnswerStatus, string> = {
	not_started: 'No analysis run is linked to this Question yet.',
	running: 'Analysis is still running.',
	answered: 'This Question has a complete, current answer.',
	incomplete: 'The answer covers only part of the requested scope.',
	unsupported: 'The available data or analysis tools do not support this Question.',
	insufficient_data: 'The run did not produce enough accepted evidence to answer this Question.',
	needs_review: 'The answer is available, but its verification is not conclusive.',
	waiting_for_input: 'Fella needs your choice to continue this Question.',
	blocked: 'The latest run could not produce an answer.',
	outdated: 'The Question or its analysis context changed after the latest run.'
};

const nextStep: Partial<Record<InvestigationQuestionAnswerStatus, string>> = {
	not_started: 'Run this Question to create an answer.',
	incomplete: 'Add the missing data, narrow the Question, or document this limitation.',
	unsupported: 'Add a supporting source or reframe the Question around available data.',
	insufficient_data: 'Select relevant data or narrow the scope, then run the Question again.',
	needs_review: 'Review the saved run and its evidence; correct the concern or rerun the Question.',
	waiting_for_input: 'Answer the clarification in the linked conversation to continue.',
	blocked: 'Fix the issue and run this Question again.',
	outdated: 'Run this Question again to refresh the result.'
};

export function investigationQuestionStatusLabel(status: InvestigationQuestionAnswerStatus): string {
	return statusLabel[status];
}

export function investigationQuestionStatusNextStep(
	status: InvestigationQuestionAnswerStatus
): string | undefined {
	return nextStep[status];
}

/** A short reason and a concrete next step for Investigation Question states. */
export function investigationQuestionStatusDetail(question: InvestigationQuestion): string {
	const status = question.answer_status;
	const reason = question.answer_status_reason?.trim() || statusDescription[status];
	const action = nextStep[status];
	return action ? `${reason} ${action}` : reason;
}

export function investigationQuestionStatusDescription(
	status: InvestigationQuestionAnswerStatus
): string {
	return statusDescription[status];
}
