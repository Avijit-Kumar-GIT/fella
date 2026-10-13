import type {
	AnalysisTurn,
	EvidenceItem,
	Investigation,
	InvestigationFinding,
	InvestigationQuestion,
	VisualizationSpec
} from '$lib/types';
import {
	investigationQuestionStatusLabel,
	investigationQuestionStatusNextStep
} from '$lib/investigation-question-status';
import { isRenderableChartEvidence } from '$lib/workbench';

export type InvestigationReportRuns = Record<string, AnalysisTurn | null | undefined>;

function oneLine(value: string | undefined): string {
	return (value ?? '').replace(/\s+/g, ' ').trim();
}

function questionStatusLabel(question: InvestigationQuestion): string {
	const status = investigationQuestionStatusLabel(question.answer_status);
	return question.status === 'removed' ? `${status} · removed from the active plan` : status;
}

function reportQuestionTitle(question: InvestigationQuestion, questions: InvestigationQuestion[]): string {
	if (!question.parent_id) return `Main Question: ${oneLine(question.text) || 'Untitled question'}`;
	const siblings = questions.filter((item) => item.parent_id === question.parent_id);
	const number = siblings.findIndex((item) => item.id === question.id) + 1;
	return `Subquestion ${number}: ${oneLine(question.text) || 'Untitled question'}`;
}

function workspaceLabel(path: string): string {
	return path.split(/[\\/]+/).filter(Boolean).at(-1) || 'Workspace';
}

function reportPathLabel(path: string, workspace?: string): string {
	const normalizedPath = path.replace(/\\/g, '/');
	const normalizedWorkspace = workspace?.replace(/\\/g, '/').replace(/\/$/, '');
	const isWindowsPath = /^[a-z]:\//i.test(normalizedPath);
	const isAbsolute = normalizedPath.startsWith('/') || isWindowsPath;
	if (!isAbsolute) return path;

	if (normalizedWorkspace) {
		const matchesWorkspace = isWindowsPath
			? normalizedPath.toLowerCase().startsWith(`${normalizedWorkspace.toLowerCase()}/`)
			: normalizedPath.startsWith(`${normalizedWorkspace}/`);
		if (matchesWorkspace) return normalizedPath.slice(normalizedWorkspace.length + 1);
	}

	return normalizedPath.split('/').filter(Boolean).at(-1) || 'Source';
}

function skippedSourceLabel(source: string, workspace?: string): string {
	const reasonSeparator = source.indexOf(': ');
	const path = reasonSeparator < 0 ? source : source.slice(0, reasonSeparator);
	const reason = reasonSeparator < 0 ? '' : source.slice(reasonSeparator + 2);
	const label = reportPathLabel(path, workspace);
	return reason ? `${label}: ${reason}` : label;
}

function investigationStatusLabel(investigation: Investigation, questions: InvestigationQuestion[]): string {
	const openQuestions = questions.filter((question) => question.answer_status !== 'answered').length;
	if (investigation.status === 'complete' && openQuestions) {
		return `Complete with ${openQuestions} open Question${openQuestions === 1 ? '' : 's'}; not fully answered`;
	}
	return investigation.status;
}

function requestedSourceScope(run: AnalysisTurn): string {
	const selection = run.investigation_context?.source_selection;
	if (!selection) return 'Not recorded for this run.';
	if (selection.mode === 'all_sources') return 'All sources in the Investigation scope.';
	const count = selection.included_paths.length;
	return `${count} selected source${count === 1 ? '' : 's'} in the Question scope.`;
}

function isEvidenceItem(value: unknown): value is EvidenceItem {
	if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
	const item = value as Record<string, unknown>;
	return typeof item.tool === 'string'
		&& Boolean(item.args && typeof item.args === 'object' && !Array.isArray(item.args))
		&& typeof item.result_summary === 'string';
}

function markdownCell(value: unknown): string {
	const text = value === null || value === undefined ? '—' : String(value);
	return text.replace(/\\/g, '\\\\').replace(/[\r\n]+/g, ' ').replace(/\|/g, '\\|')
		.replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/`/g, '\\`');
}

function chartDataTable(chart: VisualizationSpec): string[] {
	if (chart.series.length) {
		const headers = ['Category', ...chart.series.map((series) => series.name)];
		const rows = chart.labels.map((label, index) => [label, ...chart.series.map((series) => series.values[index] ?? null)]);
		return [
			`| ${headers.map(markdownCell).join(' | ')} |`,
			`| ${headers.map(() => '---').join(' | ')} |`,
			...rows.map((row) => `| ${row.map(markdownCell).join(' | ')} |`)
		];
	}

	const payload = chart.payload;
	if (!payload) return ['No exact plotted values were recorded for this visualization.'];
	if (payload.type === 'scatter') {
		const headers = ['Point', 'X', 'Y', 'Group'];
		return [
			`| ${headers.join(' | ')} |`, `| ${headers.map(() => '---').join(' | ')} |`,
			...payload.points.map((point) => `| ${[point.label, point.x, point.y, point.group ?? '—'].map(markdownCell).join(' | ')} |`)
		];
	}
	if (payload.type === 'box_plot') {
		const headers = ['Group', 'Low whisker', 'Q1', 'Median', 'Q3', 'High whisker', 'N', 'Outliers'];
		return [
			`| ${headers.join(' | ')} |`, `| ${headers.map(() => '---').join(' | ')} |`,
			...payload.groups.map((group) => `| ${[
				group.label, group.low_whisker, group.q1, group.median, group.q3,
				group.high_whisker, group.n, group.outliers.join(', ')
			].map(markdownCell).join(' | ')} |`)
		];
	}
	if (payload.type === 'heatmap') {
		const headers = ['Row', ...payload.x_labels];
		return [
			`| ${headers.map(markdownCell).join(' | ')} |`, `| ${headers.map(() => '---').join(' | ')} |`,
			...payload.y_labels.map((label, index) => `| ${[label, ...(payload.values[index] ?? [])].map(markdownCell).join(' | ')} |`)
		];
	}
	if (payload.type === 'forecast') {
		const headers = ['Period', 'Observed', 'Forecast', 'Lower bound', 'Upper bound'];
		const length = Math.max(chart.labels.length, payload.observed.length, payload.forecast.length, payload.lower.length, payload.upper.length);
		return [
			`| ${headers.join(' | ')} |`, `| ${headers.map(() => '---').join(' | ')} |`,
			...Array.from({ length }, (_, index) => `| ${[
				chart.labels[index] ?? `Point ${index + 1}`, payload.observed[index], payload.forecast[index],
				payload.lower[index], payload.upper[index]
			].map(markdownCell).join(' | ')} |`),
			...(payload.uncertainty_note ? ['', `Uncertainty: ${payload.uncertainty_note}`] : [])
		];
	}
	return ['No exact plotted values were recorded for this visualization.'];
}

function visualizationSections(run: AnalysisTurn): string[] {
	const evidence = run.result.evidence.filter(isEvidenceItem);
	const charts = evidence.filter((item) => isRenderableChartEvidence(item, run.verification?.checks ?? run.result.verification));
	if (!charts.length) return ['No renderable visualization was recorded for this run.'];
	const lines: string[] = [];
	for (const [index, item] of charts.entries()) {
		const chart = item.chart!;
		lines.push(
			`### ${chart.title || `Visualization ${index + 1}`}`,
			'',
			'**Model-produced visualization · not an analyst-reviewed Finding**',
			'',
			`- Chart type: ${chart.kind}`,
			`- Supporting run: \`${run.id}\``,
			`- Chart evidence: ${item.id ? `\`${item.id}\`` : 'ID not recorded'}`,
			`- Source evidence: ${chart.metadata?.source_evidence_id ? `\`${chart.metadata.source_evidence_id}\`` : 'Not recorded'}`,
			`- Interpretation: ${oneLine(chart.metadata?.finding_snapshot) || 'No model interpretation was recorded.'}`,
			'',
			'**Plotted values**',
			'',
			...chartDataTable(chart),
			''
		);
	}
	return lines;
}

function evidenceSummary(run: AnalysisTurn): string[] {
	return run.result.evidence.map((item, index) => {
		if (!item || typeof item !== 'object' || Array.isArray(item)) {
			return `- Evidence ${index + 1}: no structured reference was recorded.`;
		}
		const evidence = item as Record<string, unknown>;
		const id = typeof evidence.id === 'string' && evidence.id.trim() ? evidence.id.trim() : '';
		const tool = typeof evidence.tool === 'string' && evidence.tool.trim() ? evidence.tool.trim() : 'Evidence';
		const summary = typeof evidence.result_summary === 'string' ? oneLine(evidence.result_summary) : '';
		return `- ${id ? `\`${id}\` · ` : ''}${tool}${summary ? ` — ${summary}` : ''}`;
	});
}

function findingSection(finding: InvestigationFinding, index: number, investigation: Investigation): string[] {
	const question = investigation.questions?.find((item) => item.id === finding.question_id);
	const latestRunId = question?.latest_analysis_turn_id;
	const freshness = !question
		? 'Question record is unavailable; freshness could not be checked.'
		: !latestRunId
			? 'No latest run is recorded for the linked Question; freshness could not be checked.'
			: latestRunId === finding.analysis_turn_id
				? 'Linked to the Question’s latest run.'
				: `Linked to an earlier run; latest Question run is \`${latestRunId}\`. Review before treating this Finding as current.`;
	return [
		`### Finding ${index + 1} · Revision ${finding.revision ?? 1}`,
		'',
		finding.claim,
		'',
		`- Question: ${question?.text ?? `Question ${finding.question_id}`}`,
		`- Supporting run: \`${finding.analysis_turn_id}\``,
		`- Freshness: ${freshness}`,
		`- Evidence IDs: ${finding.evidence_ids.length ? finding.evidence_ids.map((id) => `\`${id}\``).join(', ') : 'None recorded'}`,
		`- Caveats: ${finding.caveats.length ? finding.caveats.join('; ') : 'None recorded'}`,
		`- Assumptions: ${finding.assumptions.length ? finding.assumptions.join('; ') : 'None recorded'}`,
		...(finding.revision_note ? [`- Revision note: ${finding.revision_note}`] : [])
	];
}

function runSection(runId: string, run: AnalysisTurn | null | undefined, investigation: Investigation): string[] {
	if (!run) {
		return [
			`### Run \`${runId}\``,
			'',
			'Saved run record could not be loaded. Its answer, method, QA, and source provenance are not included.'
		];
	}

	const revision = run.workspace_revision ?? run.trace.workspace_revision ?? run.workspace_snapshot?.revision;
	const verificationStatus = run.verification?.status ?? run.result.status;
	const checks = run.verification?.checks ?? run.result.verification ?? [];
	const definitions = run.investigation_context?.definitions ?? [];
	const usedSources = run.source_scope?.used_sources;
	const skippedSources = run.source_scope?.skipped_sources;
	const workspace = run.workspace ?? run.workspace_snapshot?.path;
	const execution = run.trace.steps;
	const linkedQuestion = investigation.questions?.find((question) =>
		question.id === run.investigation_context?.question_id
		|| question.latest_analysis_turn_id === runId
		|| question.analysis_turn_ids?.includes(runId)
	);
	const questionFreshness = linkedQuestion
		? !linkedQuestion.latest_analysis_turn_id
			? 'No latest run is recorded for the linked Question; currentness could not be checked.'
			: linkedQuestion.latest_analysis_turn_id === runId
				? 'Latest saved run for the linked Question.'
				: `Earlier than the linked Question’s latest answer (run \`${linkedQuestion.latest_analysis_turn_id}\`).`
		: 'Question linkage was not recorded; currentness could not be checked.';
	const lines = [
		`### Run \`${run.id}\``,
		'',
		`- Run state: ${run.state.replaceAll('_', ' ')}`,
		`- Question freshness: ${questionFreshness}`,
		`- Method: ${run.plan.strategy.replaceAll('_', ' ')}${run.plan.steps.length ? ` · planned steps: ${run.plan.steps.map((step) => step.replaceAll('_', ' ')).join(' → ')}` : ''}`,
		`- Verification: ${verificationStatus.replaceAll('_', ' ')}${checks.length ? ` · ${checks.length} check${checks.length === 1 ? '' : 's'}` : ' · no checks recorded'}`,
		`- Requested source scope: ${requestedSourceScope(run)}`,
		`- Sources used: ${usedSources === undefined ? 'Not recorded for this run.' : usedSources.length ? usedSources.map((source) => `${reportPathLabel(source.source, workspace)}${source.table ? ` (${source.table})` : ''}`).join(', ') : 'No sources were linked to this run.'}`,
		`- Sources skipped: ${skippedSources === undefined ? 'Not recorded for this run.' : skippedSources.length ? skippedSources.map((source) => skippedSourceLabel(source, workspace)).join(', ') : 'None recorded.'}`,
		...(revision ? [`- Workspace revision: \`${revision}\``] : ['- Workspace revision: Not recorded.']),
		...(definitions.length
			? [`- Definitions and assumptions used: ${definitions.map((item) => `${item.kind} rev ${item.revision}: ${item.text}`).join('; ')}`]
			: [`- Definitions and assumptions used: ${run.investigation_context ? 'None recorded.' : 'No Investigation context snapshot was recorded.'}`]),
		...(run.contract?.assumptions.length ? [`- Runtime interpretation assumptions: ${run.contract.assumptions.join('; ')}`] : [])
	];

	if (checks.length) {
		lines.push('', '**QA checks**');
		for (const check of checks) {
			const informational =
				!check.ok &&
				check.finding?.effect === 'informational' &&
				check.finding.code !== 'advisory';
			const outcome = informational ? 'informational' : check.outcome ?? (check.ok ? 'passed' : 'flagged');
			lines.push(`- ${outcome.replaceAll('_', ' ')}: ${check.label}${check.detail ? ` — ${oneLine(check.detail)}` : ''}`);
		}
	}
	if (execution.length) {
		lines.push('', `**Recorded execution:** ${execution.map((step) => `${step.operation.replaceAll('_', ' ')} (${step.success ? 'succeeded' : 'failed'})`).join('; ')}`);
	} else {
		lines.push('', '**Recorded execution:** No execution steps were recorded for this run.');
	}
	if (run.result.evidence.length) {
		lines.push('', '**Evidence references**', ...evidenceSummary(run));
	}
	lines.push('', '**Visualizations**', '', ...visualizationSections(run));
	return lines;
}

export function buildInvestigationReport(
	investigation: Investigation,
	runs: InvestigationReportRuns,
	generatedAt = new Date()
): string {
	const title = oneLine(investigation.title) || oneLine(investigation.brief.goal) || 'Investigation report';
	const questions = (investigation.questions ?? []).filter((question) => question.status === 'active');
	const findings = investigation.findings ?? [];
	const runIds = new Set<string>();
	for (const question of questions) {
		if (question.latest_analysis_turn_id) runIds.add(question.latest_analysis_turn_id);
	}
	for (const finding of findings) runIds.add(finding.analysis_turn_id);

	const lines = [
		`# ${title}`,
		'',
		`Generated ${generatedAt.toISOString()} · Investigation status: ${investigationStatusLabel(investigation, questions)}.`,
		'',
		'This report is assembled from saved Investigation records. The conversation transcript is excluded.',
		'',
		'## Brief',
		'',
		`- Workspace: ${workspaceLabel(investigation.workspace_id)}`,
		`- Goal: ${investigation.brief.goal || 'Not recorded'}`,
		`- Audience: ${investigation.brief.audience || 'Not specified'}`,
		`- Desired output: ${investigation.brief.desired_output || 'Not specified'}`,
		...(investigation.brief.constraints.length
			? [`- Constraints: ${investigation.brief.constraints.join('; ')}`]
			: ['- Constraints: None recorded']),
		'',
		'## Investigation scope',
		'',
		...(investigation.source_selection.mode === 'all_sources'
			? ['All sources in this workspace were in scope.']
			: investigation.source_selection.included_paths.length
				? investigation.source_selection.included_paths.map((path) => `- ${reportPathLabel(path, investigation.workspace_id)}`)
				: ['Selected-source scope is empty; no files were selected.']),
		'',
		'## Definitions and assumptions',
		'',
		...((investigation.definitions ?? []).filter((definition) => definition.status === 'active').length
			? (investigation.definitions ?? []).filter((definition) => definition.status === 'active')
				.map((definition) => `- **${definition.kind} · revision ${definition.revision}:** ${definition.text}`)
			: ['No active definitions or assumptions are recorded.']),
		'',
		'## Findings',
		''
	];

	if (findings.length) {
		findings.forEach((finding, index) => lines.push(...findingSection(finding, index, investigation), ''));
	} else {
		lines.push('No Findings have been saved for this Investigation.', '');
	}

	lines.push('## Question coverage', '');
	if (!questions.length) {
		lines.push('No active Questions are recorded.', '');
	} else {
		questions.forEach((question) => {
			const run = question.latest_analysis_turn_id ? runs[question.latest_analysis_turn_id] : null;
			lines.push(
				`### ${reportQuestionTitle(question, questions)}`,
				'',
				`**Status:** ${questionStatusLabel(question)}.`,
				''
			);
			if (question.answer_status_reason) lines.push(`**Coverage note:** ${question.answer_status_reason}`, '');
			else if (question.answer_status !== 'answered') lines.push('**Coverage note:** No reason was recorded for this unresolved status.', '');
			const nextStep = investigationQuestionStatusNextStep(question.answer_status);
			if (nextStep) lines.push(`**Next step:** ${nextStep}`, '');
			if (question.source_selection) {
				lines.push(`**Question source scope:** ${question.source_selection.mode === 'all_sources'
					? 'All Investigation sources.'
					: question.source_selection.included_paths.length
						? question.source_selection.included_paths.map((path) => reportPathLabel(path, investigation.workspace_id)).join(', ')
						: 'No sources selected.'}`, '');
			}
			if (run) {
				lines.push('**Saved answer**', '', run.result.text || 'The saved run has no answer text.', '', `Run: \`${run.id}\`.`, '');
			} else if (question.latest_analysis_turn_id) {
				lines.push(`The saved run \`${question.latest_analysis_turn_id}\` could not be loaded; no answer is included.`, '');
			} else {
				lines.push('No saved run is linked to this Question yet.', '');
			}
		});
	}

	lines.push('## Method, QA, and provenance', '');
	if (!runIds.size) {
		lines.push('No saved analysis runs are linked to the active Questions or Findings.', '');
	} else {
		for (const runId of runIds) lines.push(...runSection(runId, runs[runId], investigation), '');
	}

	return `${lines.join('\n').trimEnd()}\n`;
}

export function investigationReportFilename(title: string): string {
	const slug = title
		.normalize('NFKD')
		.replace(/[\u0300-\u036f]/g, '')
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-+|-+$/g, '')
		.slice(0, 80);
	return `${slug || 'fella-investigation'}-report.md`;
}
