import type { VerificationCheck } from './types';

// Mirrors `hard_fail` in src-tauri/src/engine/verify.rs the subset of failed
// checks that mean the answer is probably *wrong*, not merely worth a look.
const HARD_FAIL_LABELS = [
	'different result now',
	'no longer runs',
	'not found in any result',
	'disagrees with this one',
	'returned no value for at least one row'
];

/** The first hard-failing check's label (with its detail folded in), if any. */
export function hardFail(checks: VerificationCheck[]): string | undefined {
	const bad = checks.find((c) => !c.ok && HARD_FAIL_LABELS.some((h) => c.label.includes(h)));
	if (!bad) return undefined;
	return bad.detail ? `${bad.label} (${bad.detail})` : bad.label;
}
