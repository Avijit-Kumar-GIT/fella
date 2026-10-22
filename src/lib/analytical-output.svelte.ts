import { ipc } from './ipc';
import { session } from './session.svelte';
import type { ArtifactRecord, ResolvedArtifact } from './analytics-artifacts';
import type { Message } from './types';
import { cachePresentation } from './presentation/snapshot';

/** Ephemeral artifact-link state. The presentation itself is part of the saved answer. */
export const outputJobs = $state<Record<string, { busy: boolean; error: string; attempted: boolean }>>({});
const pending = new Map<string, Promise<ResolvedArtifact | null>>();
type OutputBridge = Pick<
	typeof ipc,
	'presentationResults' | 'artifactList' | 'artifactCreate' | 'artifactResolve' | 'artifactRevise'
> & Partial<Pick<typeof ipc, 'presentationRepair'>>;

export interface OutputTarget {
	artifactId: string;
	baseRevision: number;
	instruction: string;
}

/** Validate and persist the OpenUI program already returned with the analysis. */
export async function ensureAnalyticalOutput(
	message: Message,
	workspace: string,
	bridge: OutputBridge = ipc,
	persist: () => void = () => session.persist()
): Promise<ResolvedArtifact | null> {
	const turnId = message.answer?.turn_id;
	if (!turnId || message.pending || message.answer?.clarification) return null;
	const active = pending.get(turnId);
	if (active) return active;
	outputJobs[turnId] = { busy: true, error: '', attempted: true };
	const work = (async (): Promise<ResolvedArtifact | null> => {
		try {
			if (message.artifact_id) {
				const saved = await bridge.artifactResolve(workspace, message.artifact_id, message.artifact_revision);
				const { validatePresentation } = await import('./presentation/validate');
				const checked = validatePresentation(saved.revision.program, saved.results);
				if (!checked.valid) {
					throw new Error(`The saved OpenUI answer could not be validated. ${checked.issues.map(item => item.message).join('; ').slice(0, 1600)}`);
				}
			cachePresentation(message, checked.normalizedProgram, saved.results);
			persist();
			return saved;
			}
			let program = message.answer?.presentation_program?.trim() ?? '';
			const { run, results } = await bridge.presentationResults(workspace, turnId);
			const { validatePresentation } = await import('./presentation/validate');
			let checked = validatePresentation(program, results);
			// Repair only the presentation, using saved evidence and the actual
			// parser errors. This never reruns analytical tools or edits the run.
			for (let attempt = 0; !checked.valid && bridge.presentationRepair && attempt < 2; attempt++) {
				const repaired = await bridge.presentationRepair(workspace, turnId, program, checked.issues.map(issue => issue.message), session.model || undefined);
				program = repaired.program;
				checked = validatePresentation(program, results);
			}
			if (!checked.valid) {
				throw new Error(`The OpenUI answer could not be validated. ${checked.issues.map(item => item.message).join('; ').slice(0, 2400)}`);
			}
			const canonicalProgram = checked.normalizedProgram;
			const target = message.artifact_target;
			let resolved: ResolvedArtifact;
			if (target) {
				const existing = await bridge.artifactResolve(workspace, target.artifactId);
				const prior = existing.artifact.revisions.find(revision => revision.run_id === turnId);
				if (prior) {
					resolved = await bridge.artifactResolve(workspace, target.artifactId, prior.revision);
				} else {
					resolved = await bridge.artifactRevise(workspace, target.artifactId, target.baseRevision, {
						run_id: turnId,
						program: canonicalProgram,
						change_note: target.instruction
					});
				}
			} else {
				const artifacts = await bridge.artifactList(workspace);
				const existing = artifacts.find(artifact => artifact.revisions.some(revision => revision.run_id === turnId));
				let artifact: ArtifactRecord;
				if (existing) artifact = existing;
				else {
					const title = run.question.replace(/\s+/g, ' ').trim().slice(0, 120) || 'Analysis';
					artifact = await bridge.artifactCreate(workspace, turnId, title, canonicalProgram);
				}
				resolved = await bridge.artifactResolve(workspace, artifact.id);
			}
			message.artifact_id = resolved.artifact.id;
			message.artifact_revision = resolved.revision.revision;
			cachePresentation(message, resolved.revision.program, resolved.results);
			persist();
			return resolved;
		} catch (error) {
			outputJobs[turnId].error = error instanceof Error ? error.message : String(error);
			return null;
		} finally {
			outputJobs[turnId].busy = false;
		}
	})();
	pending.set(turnId, work);
	try {
		return await work;
	} finally {
		if (pending.get(turnId) === work) pending.delete(turnId);
	}
}

export async function saveOutputToStudio(resolved: ResolvedArtifact): Promise<void> {
	const workspace = resolved.artifact.workspace;
	if (session.catalog.workspace !== workspace) throw new Error('Open the result’s workspace before creating a report.');
	const documents = await ipc.studioList(workspace);
	const existing = documents.find(document => document.placements.some(placement => placement.artifact_id === resolved.artifact.id && placement.revision === resolved.revision.revision));
	if (existing) { session.openStudioToolTab(existing.id); return; }
	const document = await ipc.studioCreate(workspace, resolved.artifact.title);
	await ipc.studioSave(workspace, document.id, document.version, { title: document.title, body: '', placements: [{ id: `placement_${crypto.randomUUID().replaceAll('-', '')}`, artifact_id: resolved.artifact.id, revision: resolved.revision.revision, view_state: {} }] });
	// Recheck navigation after the async save: never open this under another folder.
	if (session.catalog.workspace === workspace) session.openStudioToolTab(document.id);
}
