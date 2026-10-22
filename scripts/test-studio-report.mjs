import assert from 'node:assert/strict';
import { createServer } from 'vite';

// Acceptance contract v1, written before running the implementation:
// exact canonical strings (never display-row Numbers); pinned placement order;
// explicit selected state; missing/mismatched output disclosure; saved QA,
// definitions, source revision and units; unknown presentation preservation;
// pure deterministic output with no transcript or source access.
const server = await createServer({
	configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error',
	server: { middlewareMode: true, hmr: false, ws: false }
});

try {
	const { buildStudioReport, studioReportFilename } = await server.ssrLoadModule('/src/lib/studio-report.ts');
	const first = {
		artifact: { id: 'revenue', workspace: '/private/work', title: 'Revenue', current_revision: 3, saved: true, revisions: [], created_at_ms: 1, updated_at_ms: 2 },
		revision: { revision: 2, run_id: 'run-revenue', program: 'root = UnregisteredWidget("Keep this content");', library_version: 'fella-v1', change_note: 'Exclude refunds.', created_at_ms: 2 },
		run: {
			id: 'run-revenue', conversation_id: 'private-chat', workspace: '/private/work', workspace_revision: 'sources-r7', question: 'What is net revenue?',
			state: 'accepted', plan: { strategy: 'compiled_sql', steps: [] }, trace: { id: 'trace', turn_id: 'run-revenue', steps: [] },
			contract: { interpretation: 'grounded', measures: [{ concept: 'Net revenue', operation: 'sum', unit: 'USD' }], filters: [], group_by: [], assumptions: ['Refunds excluded.'], unresolved: [] },
			verification: { status: 'needs_review', checks: [{ label: 'Coverage', ok: false, outcome: 'flagged', detail: 'December is missing.' }] },
			source_scope: { used_sources: [{ source: '/private/work/data/orders.csv', table: 'orders', evidence_ids: ['ev-revenue'] }], skipped_sources: ['archive.csv'] },
			investigation_context: { investigation_id: 'inv', definitions: [{ id: 'net', revision: 4, kind: 'definition', text: 'Revenue excludes refunds.' }] },
			result: { text: 'Revenue is **saved**, not regenerated.', status: 'needs_review', verification: [], evidence: [{ id: 'ev-revenue', tool: 'run_sql', sql: 'SELECT SUM(net) FROM orders', result_summary: 'Net revenue in USD', args: {}, ms: 1 }] },
			transcript_only: 'Do not export unrelated private chat.'
		},
		results: [{ id: 'ev-revenue', columns: ['period', 'net_usd', 'count', 'nothing'], rows: [['Q4', 0.1, 9007199254740992, null]], exact_rows: [['Q4', '0.100000000000000001', '9007199254740993', 'null']], row_count: 8, complete: false }]
	};
	const second = structuredClone(first);
	second.artifact.id = 'orders'; second.artifact.title = 'Orders'; second.revision.revision = 1;
	second.run.id = 'run-orders'; second.revision.run_id = 'run-orders'; second.run.question = 'How many orders?';
	second.results = [{ id: 'orders-result', columns: ['label', 'count'], rows: [['a', 0]], exact_rows: [['a|b\n<c>', '0']], row_count: 1, complete: true }];
	second.revision.program = '';
	const document = {
		id: 'studio-one', workspace: '/private/work', title: 'Quarterly / review', version: 5,
		body: '## Executive notes\n\nA hand-written introduction.',
		placements: [
			{ id: 'place-orders', artifact_id: 'orders', revision: 1, view_state: {} },
			{ id: 'place-revenue', artifact_id: 'revenue', revision: 2, view_state: { region: 'West', hidden: ['forecast'] } },
			{ id: 'place-missing', artifact_id: 'missing', revision: 9, view_state: {} }
		], checkpoints: [], created_at_ms: 1, updated_at_ms: 2
	};
	const inputsBefore = JSON.stringify({ document, first, second });
	const resolved = { 'place-orders': second, 'place-revenue': first };
	const report = buildStudioReport(document, resolved);
	assert.equal(report, buildStudioReport(document, resolved), 'Export is deterministic, without implicit current time.');
	assert.ok(report.indexOf('## Executive notes') < report.indexOf('## Orders'));
	assert.ok(report.indexOf('## Orders') < report.indexOf('## Revenue'));
	for (const value of ['0.100000000000000001', '9007199254740993', '| null |', '| 0 |', 'December is missing.', 'sources-r7', 'USD', 'Revenue excludes refunds.', 'Refunds excluded.', 'ev-revenue', 'SELECT SUM(net) FROM orders', 'West', 'forecast', 'UnregisteredWidget', 'Keep this content', 'revision 2', 'missing', 'unavailable', 'incomplete', '1 of 8']) {
		assert.ok(report.includes(value), `Report preserves/discloses ${JSON.stringify(value)}`);
	}
	assert.ok(report.includes('not converted'), 'Unconverted presentation is explicitly disclosed.');
	assert.ok(report.includes('a\\|b &lt;c&gt;'), 'Table cells escape Markdown and raw HTML.');
	assert.ok(report.includes('data/orders.csv'));
	assert.ok(!report.includes('/private/work'), 'Source paths and workspace names do not expose the absolute workspace path.');
	assert.ok(!report.includes('Do not export unrelated private chat.'));
	assert.ok(!report.includes('9007199254740992'), 'Do not substitute rounded JS display values.');
	assert.equal(JSON.stringify({ document, first, second }), inputsBefore, 'Export does not modify its inputs.');
	assert.equal(studioReportFilename('Quarterly / review'), 'quarterly-review.md');
	assert.equal(studioReportFilename('🌿'), 'fella-studio.md');

	const wrongWorkspace = structuredClone(first); wrongWorkspace.artifact.workspace = '/other/workspace';
	const rejectedWorkspace = buildStudioReport(document, { ...resolved, 'place-revenue': wrongWorkspace });
	assert.ok(rejectedWorkspace.includes('does not match'));
	assert.ok(!rejectedWorkspace.includes('0.100000000000000001'));
	const wrongRevision = structuredClone(first); wrongRevision.revision.revision = 3;
	assert.ok(!buildStudioReport(document, { ...resolved, 'place-revenue': wrongRevision }).includes('0.100000000000000001'));
	const missingExact = structuredClone(first); missingExact.results[0].exact_rows = [];
	const missingExactReport = buildStudioReport(document, { ...resolved, 'place-revenue': missingExact });
	assert.ok(missingExactReport.includes('Exact values are unavailable'));
	assert.ok(!missingExactReport.includes('9007199254740992'));

	const withAdapter = buildStudioReport(document, resolved, {
		presentation: ({ program, results, state }) => {
			assert.equal(program, first.revision.program);
			assert.equal(results[0].exact_rows[0][2], '9007199254740993');
			assert.equal(state.region, 'West');
			return { markdown: '### Converted layout\n\nAn ordered, deterministic figure.', disclosures: ['Widget needs an image fallback.'] };
		}
	});
	assert.ok(withAdapter.includes('### Converted layout'));
	assert.ok(withAdapter.includes('Widget needs an image fallback.'));
	const failedAdapter = buildStudioReport(document, resolved, { presentation: () => { throw new Error('Unsupported node'); } });
	assert.ok(failedAdapter.includes('not converted'));
	assert.ok(failedAdapter.includes('0.100000000000000001'));
	assert.ok(failedAdapter.includes(first.run.result.text));
	console.log('✔ Studio report v1: exact values, state, ordered pins, QA, units, evidence and definitions');
	console.log('✔ missing/mismatched outputs and unknown presentation are disclosed without recomputation');
	console.log('✔ export is deterministic, nonmutating, path-redacted and adapter-failure safe');
} finally {
	await server.close();
}
