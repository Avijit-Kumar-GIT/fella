import { ipc } from './ipc';
import type { StudioDesignProposal, StudioDocument } from './analytics-artifacts';

/** A presentation revision reuses the exact producing run. Only this Report's
 * pin moves; other Reports and the artifact's current pointer are unchanged. */
export async function applyReportDesign(document: StudioDocument, proposal: StudioDesignProposal, bridge = ipc): Promise<StudioDocument> {
	if (proposal.requires_analysis || !proposal.update || proposal.base_version !== document.version) throw new Error('The Report changed. Request the design change again.');
	const update = structuredClone(proposal.update);
	const { validatePresentation } = await import('./presentation/validate');
	const prepared = [];
	for (const edit of proposal.presentation_edits ?? []) {
		const placement = document.placements.find(item => item.id === edit.placement_id);
		if (!placement || !update.placements.some(item => item.id === placement.id)) throw new Error('The selected output is no longer in the Report.');
		const saved = await bridge.artifactResolveSaved(placement.artifact_id, placement.revision);
		const checked = validatePresentation(edit.spec, saved.results);
		if (!checked.valid || !checked.spec) throw new Error(`The design could not be applied: ${checked.issues.map(item => item.message).join('; ')}`);
		prepared.push({ edit, saved, spec: checked.spec });
	}
	for (const { edit, saved, spec } of prepared) {
		const candidate = await bridge.artifactRevise(saved.artifact.workspace, saved.artifact.id, saved.artifact.current_revision, { run_id: saved.run.id, spec, change_note: proposal.message || 'Report presentation change' });
		const revision = candidate.revision;
		update.placements = update.placements.map(item => item.id === edit.placement_id ? { ...item, revision: revision.revision } : item);
	}
	return bridge.studioSave(document.workspace, document.id, document.version, update);
}
