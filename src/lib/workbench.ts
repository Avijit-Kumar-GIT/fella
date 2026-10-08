import type { EvidenceItem, VerificationCheck } from './types';

/** The first composable Ask layout: one conversation plus one optional,
 * conversation-scoped companion artifact. This is presentation state only. */
export type CompanionPane =
	| {
			kind: 'chart';
			messageId: string;
			evidenceIndex: number;
			evidenceId?: string;
			workspacePath: string | null;
			revision: string | null;
	  }
	| {
			kind: 'source';
			path: string;
			name: string;
			workspacePath: string;
			revision: string | null;
	  };

export type PaneFreshness = 'current' | 'different-workspace' | 'different-revision' | 'unknown';

/** Keep artifact visibility consistent between an answer and its companion
 * pane; moving a chart must not revive an excluded or withheld artifact. */
export function isRenderableChartEvidence(
	evidence: EvidenceItem,
	verification: VerificationCheck[] = []
): boolean {
	if (
		evidence.tool !== 'make_chart' ||
		!evidence.chart ||
		evidence.error ||
		evidence.verifier_disposition?.state === 'excluded' ||
		(evidence.verifier_disposition?.state === 'artifact_withheld' && evidence.verifier_disposition.artifact === 'chart')
	) return false;

	return !verification.some(
		(check) =>
			!check.ok &&
			check.finding?.effect === 'withhold_artifact' &&
			check.finding.target_id === evidence.id
	);
}

/** Parse pane state from local conversation archives without trusting their
 * shape. A bad/old layout is ignored while the conversation still opens. */
export function parseCompanionPane(value: unknown): CompanionPane | null {
	if (!value || typeof value !== 'object') return null;
	const candidate = value as Record<string, unknown>;
	const nullableString = (item: unknown): item is string | null => item === null || typeof item === 'string';
	if (!nullableString(candidate.revision)) return null;

	if (candidate.kind === 'chart') {
		if (
			typeof candidate.messageId !== 'string' ||
			!candidate.messageId ||
			!Number.isInteger(candidate.evidenceIndex) ||
			(candidate.evidenceIndex as number) < 0 ||
			(candidate.evidenceId !== undefined && typeof candidate.evidenceId !== 'string') ||
			!nullableString(candidate.workspacePath)
		) return null;
		return {
			kind: 'chart',
			messageId: candidate.messageId,
			evidenceIndex: candidate.evidenceIndex as number,
			...(typeof candidate.evidenceId === 'string' ? { evidenceId: candidate.evidenceId } : {}),
			workspacePath: candidate.workspacePath,
			revision: candidate.revision
		};
	}

	if (
		candidate.kind === 'source' &&
		typeof candidate.path === 'string' && candidate.path.length > 0 &&
		typeof candidate.name === 'string' && candidate.name.length > 0 &&
		typeof candidate.workspacePath === 'string' && candidate.workspacePath.length > 0
	) {
		return {
			kind: 'source',
			path: candidate.path,
			name: candidate.name,
			workspacePath: candidate.workspacePath,
			revision: candidate.revision
		};
	}

	return null;
}

/** A mismatch never rewrites artifact state. Callers can label or suppress a
 * source preview, while a chart remains viewable as the historical result. */
export function paneFreshness(
	pane: CompanionPane,
	workspacePath: string | null,
	revision: string | null | undefined
): PaneFreshness {
	if (!pane.workspacePath || !pane.revision || !workspacePath || !revision) return 'unknown';
	if (pane.workspacePath !== workspacePath) return 'different-workspace';
	if (pane.revision !== revision) return 'different-revision';
	return 'current';
}
