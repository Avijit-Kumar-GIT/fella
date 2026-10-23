import type { AnalysisTurn, EvidenceItem } from './types';
import type { BoundResult, ResolvedArtifact, StudioDocument } from './analytics-artifacts';
import type { PresentationSpec } from './presentation/types';

/** Keys are placement IDs: the same artifact may be placed at different revisions. */
export type StudioReportArtifacts = Record<string, ResolvedArtifact | null | undefined>;
export interface StudioReportOptions {
	/** An optional deterministic adapter; never a model/provider callback. */
	presentation?: (input: { spec: PresentationSpec; results: BoundResult[]; state: Record<string, unknown> }) => {
		markdown: string;
		disclosures?: string[];
	};
}

function line(text: string): string { return text.replace(/[\r\n]+/g, ' ').trim(); }
function cell(text: string): string {
	return line(text).replace(/\\/g, '\\\\').replace(/\|/g, '\\|').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/`/g, '\\`');
}
function fence(text: string, language = ''): string {
	const longest = Math.max(2, ...(text.match(/`+/g) ?? []).map(part => part.length));
	const delimiter = '`'.repeat(longest + 1);
	return `${delimiter}${language}\n${text}\n${delimiter}`;
}
function stableJson(value: unknown): string {
	return JSON.stringify(value, (_key, item) => item && typeof item === 'object' && !Array.isArray(item)
		? Object.fromEntries(Object.keys(item).sort().map(key => [key, item[key]]))
		: item, 2);
}
function safeSource(path: string, workspace: string): string {
	const normalized = path.replace(/\\/g, '/');
	const root = workspace.replace(/\\/g, '/').replace(/\/$/, '');
	const windows = /^[a-z]:\//i.test(normalized);
	const inside = windows ? normalized.toLowerCase().startsWith(`${root.toLowerCase()}/`) : normalized.startsWith(`${root}/`);
	if (inside) return normalized.slice(root.length + 1);
	return normalized.startsWith('/') || windows ? normalized.split('/').filter(Boolean).at(-1) ?? 'Source' : normalized;
}

export function artifactSourceRevision(resolved: ResolvedArtifact): string {
	return resolved.run.workspace_revision ?? resolved.run.trace?.workspace_revision ?? resolved.run.workspace_snapshot?.revision ?? 'Not recorded';
}

/** Values come only from exact_rows, never the Number-valued chart/display rows. */
export function exactResultMarkdown(result: BoundResult): string {
	const lines = [`#### Result ${cell(result.id)}`, ''];
	if (!result.complete) lines.push(`**Saved result is incomplete: ${result.exact_rows.length} of ${result.row_count} rows retained.**`, '');
	else lines.push(`${result.row_count} rows · complete saved result.`, '');
	if (!result.columns.length || (!result.exact_rows.length && result.row_count > 0)) {
		lines.push('Exact values are unavailable in this saved result. No rounded display values have been substituted.');
		return lines.join('\n');
	}
	lines.push(`| ${result.columns.map(cell).join(' | ')} |`, `| ${result.columns.map(() => '---').join(' | ')} |`);
	for (const row of result.exact_rows) {
		lines.push(`| ${result.columns.map((_, index) => typeof row[index] === 'string' ? cell(row[index]) : '[exact value unavailable]').join(' | ')} |`);
	}
	return lines.join('\n');
}

export function artifactCaveats(resolved: ResolvedArtifact): string[] {
	const run = resolved.run;
	const checks = run.verification?.checks ?? run.result.verification ?? [];
	return checks.filter(check => !check.ok || check.outcome === 'not_run')
		.map(check => `${check.outcome === 'not_run' ? 'Not run' : 'Review'}: ${check.label}${check.detail ? ` — ${check.detail}` : ''}`);
}

function provenance(run: AnalysisTurn, workspace: string): string[] {
	const lines = ['### Evidence and method', '', `- Saved run: ${cell(run.id)}`, `- Verification: ${cell(run.verification?.status ?? run.result.status ?? 'Not recorded')}`];
	const checks = run.verification?.checks ?? run.result.verification ?? [];
	if (!checks.length) lines.push('- No verification checks recorded.');
	for (const check of checks) lines.push(`- ${check.outcome === 'not_run' ? 'Not run' : check.ok ? 'Passed' : 'Review'}: ${cell(check.label)}${check.detail ? ` — ${cell(check.detail)}` : ''}`);
	for (const source of run.source_scope?.used_sources ?? []) {
		lines.push(`- Source: ${cell(safeSource(source.source, workspace))}${source.table ? ` (${cell(source.table)})` : ''}${source.evidence_ids.length ? ` · evidence ${source.evidence_ids.map(cell).join(', ')}` : ''}`);
	}
	for (const skipped of run.source_scope?.skipped_sources ?? []) lines.push(`- Source not used: ${cell(safeSource(skipped, workspace))}`);
	for (const definition of run.investigation_context?.definitions ?? []) lines.push(`- ${cell(definition.kind)} ${cell(definition.id)}, revision ${definition.revision}: ${cell(definition.text)}`);
	for (const assumption of run.contract?.assumptions ?? []) lines.push(`- Assumption: ${cell(assumption)}`);
	for (const unresolved of run.contract?.unresolved ?? []) lines.push(`- Unresolved: ${cell(unresolved)}`);
	for (const measure of run.contract?.measures ?? []) lines.push(`- Measure: ${cell(measure.concept)} · ${cell(measure.operation)}${measure.field ? ` · ${cell(measure.field)}` : ''}${measure.unit ? ` · unit ${cell(measure.unit)}` : ''}`);
	for (const item of run.result.evidence) {
		if (!item || typeof item !== 'object') continue;
		const evidence = item as Partial<EvidenceItem>;
		lines.push(`- Evidence ${cell(evidence.id ?? '(no ID)')}: ${cell(evidence.tool ?? 'Saved evidence')} — ${cell(evidence.result_summary ?? '')}`);
		if (evidence.sql) lines.push('', fence(evidence.sql, 'sql'), '');
		if (evidence.chart?.unit) lines.push(`  Unit: ${cell(evidence.chart.unit)}`);
		if (evidence.verifier_disposition) lines.push(`  Disposition: ${cell(evidence.verifier_disposition.state)} · ${cell(evidence.verifier_disposition.reason)}`);
	}
	return lines;
}

/** Pure export of persisted pins and values. Does not fetch files, run SQL, or call a model. */
export function buildStudioReport(document: StudioDocument, artifacts: StudioReportArtifacts, options: StudioReportOptions = {}): string {
	const lines = [`# ${line(document.title) || 'Untitled Studio'}`, '', `Studio ${cell(document.id)} · document version ${document.version}`, '', ...(document.body.trim() ? [document.body, ''] : [])];
	for (const placement of document.placements) {
		const resolved = artifacts[placement.id];
		if (!resolved) {
			lines.push(`## Saved output ${cell(placement.artifact_id)}`, '', `Saved output is unavailable: artifact ${cell(placement.artifact_id)}, revision ${placement.revision}. Nothing was recomputed or silently omitted.`, '');
			continue;
		}
		if (resolved.artifact.workspace !== document.workspace || resolved.artifact.id !== placement.artifact_id || resolved.revision.revision !== placement.revision || resolved.revision.run_id !== resolved.run.id) {
			lines.push(`## Saved output ${cell(placement.artifact_id)}`, '', 'Saved output does not match the pinned workspace, artifact, revision, or run. Its content has not been exported.', '');
			continue;
		}
		lines.push(`## ${line(resolved.artifact.title) || 'Untitled output'}`, '', `Artifact ${cell(placement.artifact_id)} · revision ${placement.revision}`, '', `Source revision: ${cell(artifactSourceRevision(resolved))}`, '', `Original question: ${line(resolved.run.question)}`, '');
		if (resolved.revision.change_note) lines.push(`Revision note: ${line(resolved.revision.change_note)}`, '');
		lines.push('### Saved answer', '', resolved.run.result.text || 'No narrative answer was recorded.', '');
		const caveats = artifactCaveats(resolved);
		if (caveats.length) lines.push('### Caveats', '', ...caveats.map(caveat => `- ${cell(caveat)}`), '');
		lines.push('### Selected display state', '', fence(stableJson(placement.view_state ?? {}), 'json'), '', 'Selections describe the saved display, not a recomputed population. Exact tables below contain the retained result data.', '');
		if (resolved.revision.spec) {
			let converted = false;
			if (options.presentation) {
				try {
					const presentation = options.presentation({ spec: resolved.revision.spec, results: resolved.results, state: placement.view_state ?? {} });
					if (presentation.markdown.trim()) {
						lines.push('### Presentation snapshot', '', presentation.markdown, '', ...(presentation.disclosures ?? []).map(disclosure => `- Export note: ${cell(disclosure)}`), '');
						converted = true;
					}
				} catch { /* The source spec, answer and exact values remain exportable. */ }
			}
			if (!converted) lines.push('### Presentation snapshot', '', 'The generated layout was not converted to Markdown. Unsupported components, visual encodings, annotations, and interactions remain in the presentation source below; this is not a faithful visual reproduction. The saved answer and exact values are preserved.', '');
			lines.push(`Presentation source · schema ${cell(resolved.revision.schema_version)}`, '', fence(stableJson(resolved.revision.spec), 'json'), '');
		}
		lines.push('### Exact saved values', '');
		if (!resolved.results.length) lines.push('No bound tabular results were saved for this output.', '');
		for (const result of resolved.results) lines.push(exactResultMarkdown(result), '');
		lines.push(...provenance(resolved.run, document.workspace), '');
	}
	lines.push('---', '', 'Exported from saved analytical records. No model call or live source query is required. Conversation transcripts are excluded. Source revisions identify historical inputs; they do not guarantee those original files remain available for recomputation.');
	return `${lines.join('\n').trim()}\n`;
}

export function studioReportFilename(title: string): string {
	const stem = title.normalize('NFKD').replace(/[\u0300-\u036f]/g, '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 90).replace(/-$/, '');
	return `${stem || 'fella-studio'}.md`;
}
