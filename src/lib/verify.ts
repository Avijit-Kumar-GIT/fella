import type { Answer, VerificationCheck, VerificationStatus } from './types';

// Fallback for archived answers that predate the serialized status. New answers
// use `Answer.status` from the Rust verifier directly.
// Mirrors `hard_fail` in src-tauri/src/engine/analytics/verify.rs the subset of failed
// checks that mean the answer is probably *wrong*, not merely worth a look.
const HARD_FAIL_LABELS = [
	'different result now',
	'no longer runs',
	'not found in any result',
	'disagrees with this one',
	'returned no value for at least one row',
	'actually came from'
];

/** The first hard-failing check's label (with its detail folded in), if any. */
export function hardFail(checks: VerificationCheck[]): string | undefined {
	const bad = checks.find((c) => !c.ok && HARD_FAIL_LABELS.some((h) => c.label.includes(h)));
	if (!bad) return undefined;
	return bad.detail ? `${bad.label} (${bad.detail})` : bad.label;
}

/** Use the backend's typed status, with a compatibility fallback for archives. */
export function answerStatus(answer: Answer): VerificationStatus {
	if (answer.status) return answer.status;
	if (!answer.evidence.some((e) => !e.error)) return 'insufficient_data';
	if (hardFail(answer.verification)) return 'failed';
	if (answer.verification.some((c) => !c.ok)) return 'needs_review';
	if (answer.verification.some((c) => c.ok && c.label.includes('re-checked the queries'))) {
		return 'verified';
	}
	return 'needs_review';
}
