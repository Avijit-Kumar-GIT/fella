import assert from 'node:assert/strict';
import { createServer } from 'vite';

const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	server: { middlewareMode: true, hmr: false, ws: false }
});

try {
	const { buildInvestigationReport, investigationReportFilename } =
		await server.ssrLoadModule('/src/lib/investigation-report.ts');
	const investigation = {
		id: 'investigation-sales',
		workspace_id: '/data/sales',
		title: 'Weekly sales review',
		brief: {
			goal: 'Explain the change in weekly sales.',
			audience: 'Finance lead',
			desired_output: 'A concise weekly report',
			constraints: ['Exclude test orders']
		},
		questions: [
			{
				id: 'question-main', investigation_id: 'investigation-sales', text: 'Explain the change in weekly sales.',
				position: 0, status: 'active', answer_status: 'answered', latest_analysis_turn_id: 'run-sales',
				created_at_ms: 1, updated_at_ms: 2
			},
			{
				id: 'question-gap', investigation_id: 'investigation-sales', parent_id: 'question-main',
				text: 'Compare the trend by region.', position: 1, status: 'active', answer_status: 'unsupported',
				answer_status_reason: 'The export has no region field.', created_at_ms: 1, updated_at_ms: 2
			}
		],
		findings: [
			{
				id: 'finding-sales', investigation_id: 'investigation-sales', question_id: 'question-main',
				analysis_turn_id: 'run-sales', evidence_ids: ['evidence-weekly-sales'],
				claim: 'Weekly sales rose 12% to $40.', caveats: ['Only 10 of 12 weeks are present.'],
				assumptions: ['Test orders are excluded.'], revision: 2, revision_note: 'Checked the source export.',
				revisions: [], created_at_ms: 1, updated_at_ms: 2
			},
			{
				id: 'finding-missing-run', investigation_id: 'investigation-sales', question_id: 'question-gap',
				analysis_turn_id: 'missing-run', evidence_ids: ['old-evidence'], claim: 'Historical claim.',
				caveats: [], assumptions: [], revision: 1, revision_note: '', revisions: [], created_at_ms: 1, updated_at_ms: 2
			}
		],
		definitions: [
			{ id: 'definition-net', investigation_id: 'investigation-sales', kind: 'definition', text: 'Revenue is net of discounts.', revision: 2, history: [], status: 'active', created_at_ms: 1, updated_at_ms: 2 },
			{ id: 'definition-old', investigation_id: 'investigation-sales', kind: 'assumption', text: 'Removed assumptions must stay out.', revision: 1, history: [], status: 'removed', created_at_ms: 1, updated_at_ms: 2 }
		],
		source_selection: { mode: 'selected_sources', included_paths: ['sales.csv'] },
		analysis_turn_ids: ['run-sales'], status: 'active', created_at_ms: 1, updated_at_ms: 2
	};
	investigation.transcript_only_text = 'Transcript-only private context should not appear.';
	const run = {
		id: 'run-sales', conversation_id: 'conversation-sales', question: 'Explain the change in weekly sales.',
		workspace: '/data/sales', workspace_revision: 'sales-r7', state: 'accepted',
		plan: { strategy: 'compiled_sql', steps: ['inspect_schema', 'run_sql'] },
		trace: {
			id: 'trace-sales', turn_id: 'run-sales', workspace_revision: 'sales-r7',
			steps: [{ id: 'step-sales', operation: 'run_sql', duration_ms: 10, success: true, sources: ['sales.csv'] }]
		},
		verification: {
			status: 'needs_review',
			checks: [{ label: 'The requested twelve weeks are present.', ok: false, outcome: 'flagged', detail: 'Only ten weeks were available.' }]
		},
		result: {
			text: 'Sales rose 12% to $40.',
			status: 'needs_review',
			verification: [],
			evidence: [
				{ id: 'evidence-weekly-sales', tool: 'run_sql', result_summary: 'Ten weekly totals from sales.csv.' },
				{
					id: 'evidence-weekly-sales-chart', tool: 'make_chart', args: {},
					result_summary: 'Weekly sales visualization.', ms: 2,
					chart: {
						kind: 'line', title: 'Weekly net sales', labels: ['W1', 'W2'],
						series: [{ name: 'Net sales', values: [31, 40] }],
						metadata: { source_evidence_id: 'evidence-weekly-sales', finding_snapshot: 'Weekly sales rose to $40.', fields: ['week', 'net_sales'], filters: [], part_to_whole: false }
					}
				}
			]
		},
		source_scope: {
			used_sources: [{ source: 'sales.csv', table: 'sales', evidence_ids: ['evidence-weekly-sales'] }],
			skipped_sources: ['legacy.xls']
		},
		investigation_context: {
			investigation_id: 'investigation-sales', question_id: 'question-main', question_text: 'Explain the change in weekly sales.',
			definitions: [{ id: 'definition-revenue', revision: 3, kind: 'definition', text: 'Net sales exclude refunds.' }],
			source_selection: { mode: 'selected_sources', included_paths: ['sales.csv'] }
		}
	};

	const report = buildInvestigationReport(investigation, { 'run-sales': run }, new Date('2026-10-11T12:00:00.000Z'));
	for (const expected of [
		'# Weekly sales review',
		'Finance lead',
		'## Findings',
		'Weekly sales rose 12% to $40.',
		'Only 10 of 12 weeks are present.',
		'## Question coverage',
		'The export has no region field.',
		'## Method, QA, and provenance',
		'compiled sql',
		'needs review',
		'Only ten weeks were available.',
		'sales.csv (sales)',
		'Weekly net sales',
		'not an analyst-reviewed Finding',
		'evidence-weekly-sales-chart',
		'| W2 | 40 |',
		'legacy.xls',
		'Net sales exclude refunds.',
		'Revenue is net of discounts.',
		'evidence-weekly-sales',
		'Saved run record could not be loaded',
		'conversation transcript is excluded'
	]) {
		assert.ok(report.toLowerCase().includes(expected.toLowerCase()), `Report should include: ${expected}`);
	}
	assert.ok(!report.includes('Transcript-only private context should not appear.'));
	assert.ok(!report.includes('Removed assumptions must stay out.'));
	const completionReport = buildInvestigationReport(
		{ ...investigation, status: 'complete' },
		{ 'run-sales': run },
		new Date('2026-10-11T12:00:00.000Z')
	);
	assert.ok(completionReport.includes('Complete with 1 open Question; not fully answered'));
	assert.ok(!completionReport.includes('Investigation fully answered'));
	const noDefinitionsReport = buildInvestigationReport({ ...investigation, definitions: [] }, { 'run-sales': run });
	assert.ok(noDefinitionsReport.includes('No active definitions or assumptions are recorded.'));
	assert.equal(investigationReportFilename('Weekly sales / review'), 'weekly-sales-review-report.md');
	assert.equal(investigationReportFilename('🌿'), 'fella-investigation-report.md');
	const pathReport = buildInvestigationReport(investigation, {
		'run-sales': {
			...run,
			workspace: 'C:\\Users\\Alex\\Private',
			source_scope: {
				used_sources: [{ source: 'C:\\Users\\Alex\\Private\\datasets\\sales.csv', table: 'sales', evidence_ids: [] }],
				skipped_sources: ['C:\\Users\\Alex\\Secret\\legacy.xls: encrypted workbook']
			}
		}
	}, new Date('2026-10-11T12:00:00.000Z'));
	assert.ok(pathReport.includes('datasets/sales.csv (sales)'));
	assert.ok(pathReport.includes('legacy.xls: encrypted workbook'));
	assert.ok(!pathReport.includes('C:\\Users\\Alex'));
	console.log('✔ saved answers, findings, gaps, QA, method, definitions, evidence, and missing-run disclosure are exported');
	console.log('✔ report excludes transcript-only content, redacts absolute source paths, and produces a safe Markdown filename');
} finally {
	await server.close();
}
