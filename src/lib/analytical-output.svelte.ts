import { ipc } from './ipc';
import { session } from './session.svelte';
import type { ArtifactRecord, PresentationDesignPlan, ResolvedArtifact } from './analytics-artifacts';
import type { Message } from './types';

/** Ephemeral work state: an app restart never regenerates a saved answer. */
export const outputJobs = $state<Record<string, { busy: boolean; error: string; operationId: string }>>({});

export interface OutputTarget { artifactId: string; baseRevision: number; instruction: string }

export async function createAnalyticalOutput(message: Message, workspace: string, model?: string, target?: OutputTarget): Promise<ArtifactRecord | null> {
	const turnId = message.answer?.turn_id;
	if (!turnId || message.pending || message.answer?.clarification || outputJobs[turnId]?.busy) return null;
	const operationId = crypto.randomUUID();
	outputJobs[turnId] = { busy: true, error: '', operationId };
	const current = () => outputJobs[turnId]?.operationId === operationId && outputJobs[turnId]?.busy;
	try {
		const { run, results } = await ipc.presentationResults(workspace, turnId);
		if (!current()) return null;
		if (!results.length) throw new Error('No reusable table result was recorded. The original answer is unchanged.');
		const { validatePresentation } = await import('./presentation/validate');
		let program = '';
		let designPlan: PresentationDesignPlan | undefined;
		let repair = target?.instruction ?? '';
		for (let attempt = 0; attempt < 2; attempt++) {
			const response = await ipc.presentationPropose(workspace, turnId, model, repair, operationId, designPlan);
			if (!current()) return null;
			designPlan = response.design_plan;
			program = response.program.trim().replace(/^```(?:openui|openui-lang)?\s*\n/i, '').replace(/\n```\s*$/, '');
			const checked = validatePresentation(program, results);
			if (checked.valid) break;
			const errors = checked.issues.map(item => item.message).join('; ').slice(0, 2400);
			if (attempt === 1) throw new Error(`The interactive view could not be validated. ${errors}`);
			repair = `${target?.instruction ?? ''}\nThe previous presentation was invalid: ${errors}. Return a corrected complete program using only supplied result IDs and fields.`;
		}
		if (!current()) return null;
		let artifact: ArtifactRecord;
		if (target) {
			const revised = await ipc.artifactRevise(workspace, target.artifactId, target.baseRevision, { run_id: turnId, program, change_note: target.instruction });
			artifact = revised.artifact;
			message.artifact_revision = revised.revision.revision;
		} else {
			const title = run.question.replace(/\s+/g, ' ').trim().slice(0, 120) || 'Analysis';
			artifact = await ipc.artifactCreate(workspace, turnId, title, program);
			message.artifact_revision = artifact.current_revision;
		}
		if (!current()) return artifact;
		message.artifact_id = artifact.id;
		session.persist();
		return artifact;
	} catch (error) {
		if (current()) outputJobs[turnId].error = error instanceof Error ? error.message : String(error);
		return null;
	} finally {
		if (outputJobs[turnId]?.operationId === operationId) outputJobs[turnId].busy = false;
	}
}

export function cancelAnalyticalOutput(turnId: string, workspace: string): void {
	const job = outputJobs[turnId];
	if (!job?.busy) return;
	job.busy = false;
	job.error = 'Interactive view cancelled. The analytical answer is unchanged.';
	void ipc.cancel(`presentation:${job.operationId}`, workspace).catch(() => {});
}

export async function saveOutputToStudio(resolved: ResolvedArtifact): Promise<void> {
	const workspace = resolved.artifact.workspace;
	if (session.catalog.workspace !== workspace) throw new Error('Open the output’s workspace before adding it to Studio.');
	const documents = await ipc.studioList(workspace);
	const existing = documents.find(document => document.placements.some(placement => placement.artifact_id === resolved.artifact.id && placement.revision === resolved.revision.revision));
	if (existing) { session.openStudioToolTab(existing.id); return; }
	const document = await ipc.studioCreate(workspace, resolved.artifact.title);
	await ipc.studioSave(workspace, document.id, document.version, { title: document.title, body: '', placements: [{ id: `placement_${crypto.randomUUID().replaceAll('-', '')}`, artifact_id: resolved.artifact.id, revision: resolved.revision.revision, view_state: {} }] });
	// Recheck navigation after the async save: never open this under another folder.
	if (session.catalog.workspace === workspace) session.openStudioToolTab(document.id);
}
