// json-render contract v1.0. Acceptance criteria are fixed before execution:
// - the generated Rust catalog and Svelte registry have the same component set;
// - a flat json-render tree validates and renders through CatalogRenderer;
// - result-bound components reject absent IDs/fields and unsafe chart bindings;
// - nested layout, tabs, local state and charts survive composition;
// - Markdown export retains exact values and selected presentation state.
// No model, IPC, workspace scan, or analytical execution is used here.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createServer } from 'vite';

const server = await createServer({
	configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error',
	optimizeDeps: { noDiscovery: true, entries: [] },
	server: { middlewareMode: true, hmr: false, ws: false }
});

try {
	const { componentDefinitions } = await server.ssrLoadModule('/src/lib/presentation/schemas.ts');
	const { presentationCatalog, presentationRegistry } = await server.ssrLoadModule('/src/lib/presentation/library.ts');
	const { presentationManifest, presentationPrompt } = await server.ssrLoadModule('/src/lib/presentation/manifest.ts');
	const { validatePresentation } = await server.ssrLoadModule('/src/lib/presentation/validate.ts');
	const { exportPresentation } = await server.ssrLoadModule('/src/lib/presentation/export.ts');
	const { setPresentationValue, evaluatePresentationAction } = await server.ssrLoadModule('/src/lib/presentation/runtime.ts');
	const { render } = await server.ssrLoadModule('svelte/server');
	const { default: AnalyticalPresentation } = await server.ssrLoadModule('/src/lib/components/presentation/AnalyticalPresentation.svelte');
	const generated = JSON.parse(await readFile('backend/resources/presentation-catalog.json', 'utf8'));
	const names = Object.keys(componentDefinitions);

	assert.equal(generated.version, 'fella-json-render-v1');
	assert.deepEqual(generated.componentNames, names);
	assert.deepEqual(Object.keys(presentationRegistry), names);
	assert.equal(presentationManifest().version, generated.version);
	assert.match(presentationPrompt(), /json-render spec/);
	assert.match(presentationPrompt(), /Do not use a table merely because analysis returned rows/);

	const results = [{
		id: 'quarterly-revenue', columns: ['quarter', 'revenue', 'account_id'],
		rows: [['Q3', 100, '9007199254740993'], ['Q4', 120, '9007199254740995']],
		exact_rows: [['Q3', '100.00', '9007199254740993'], ['Q4', '120.00', '9007199254740995']],
		row_count: 2, complete: true
	}];
	const spec = {
		root: 'document',
		elements: {
			document: { type: 'Document', props: {}, children: ['title', 'intro', 'metrics', 'chart', 'tabs'] },
			title: { type: 'Title', props: { content: 'Quarterly revenue' }, children: [] },
			intro: { type: 'Text', props: { content: 'Revenue increased from Q3 to Q4.' }, children: [] },
			metrics: { type: 'Grid', props: { columns: 2 }, children: ['q3', 'q4'] },
			q3: { type: 'Metric', props: { resultId: 'quarterly-revenue', field: 'revenue', row: 0, title: 'Q3 revenue', unit: 'USD' }, children: [] },
			q4: { type: 'Metric', props: { resultId: 'quarterly-revenue', field: 'revenue', row: 1, title: 'Q4 revenue', unit: 'USD' }, children: [] },
			chart: { type: 'Chart', props: { resultId: 'quarterly-revenue', title: 'Revenue by quarter', spec: { mark: 'line', encoding: { x: { field: 'quarter', type: 'ordinal' }, y: { field: 'revenue', type: 'quantitative' } } } }, children: [] },
			tabs: { type: 'Tabs', props: { name: 'Detail', labels: ['Summary', 'Records'] }, children: ['summary-panel', 'records-panel'] },
			'summary-panel': { type: 'TabPanel', props: { name: 'Detail', index: 0 }, children: ['summary'] },
		summary: { type: 'Text', props: { content: 'Selected summary panel.' }, children: [] },
			'records-panel': { type: 'TabPanel', props: { name: 'Detail', index: 1 }, children: ['table'] },
			table: { type: 'Table', props: { resultId: 'quarterly-revenue', fields: ['quarter', 'revenue', 'account_id'], title: 'Exact rows' }, children: [] }
		}
	};
	const valid = validatePresentation(spec, results);
	assert.equal(valid.valid, true, JSON.stringify(valid.issues));
	assert.deepEqual(valid.resultIds, ['quarterly-revenue']);
	assert.deepEqual(presentationCatalog.validate(spec).success, true);

	const html = render(AnalyticalPresentation, { props: {
		spec, results, initialState: { 'tab:Detail': 1, 'filter:untrusted': { resultId: 'other', field: 'quarter', value: 'Q3' } }
	} }).body;
	assert.match(html, /Quarterly revenue/);
	assert.match(html, /Revenue by quarter/);
	assert.match(html, /class="chart-target/);
	assert.doesNotMatch(html, /Selected summary panel\./);
	assert.match(html, /Exact rows/);
	assert.match(html, /9007199254740993/);

	const missingResult = structuredClone(spec);
	missingResult.elements.chart.props.resultId = 'not-supplied';
	assert.equal(validatePresentation(missingResult, results).valid, false);
	const missingField = structuredClone(spec);
	missingField.elements.q4.props.field = 'invented';
	assert.equal(validatePresentation(missingField, results).valid, false);
	const unsafeChart = structuredClone(spec);
	unsafeChart.elements.chart.props.spec.encoding.y.field = 'invented';
	assert.equal(validatePresentation(unsafeChart, results).valid, false);
	const orphan = structuredClone(spec);
	orphan.elements.unused = { type: 'Text', props: { content: 'orphan' }, children: [] };
	assert.equal(validatePresentation(orphan, results).valid, false);

	const state = setPresentationValue({}, 'quarter', 'Q4');
	assert.deepEqual(state, { 'control:quarter': 'Q4' });
	assert.equal(evaluatePresentationAction({ type: 'set_state', stateKey: 'quarter', value: 'Q3' }).type, 'set_state');
	assert.equal(evaluatePresentationAction({ type: 'run_query', sql: 'SELECT 1' }), null);
	const exported = exportPresentation(spec, results, { state: { 'tab:Detail': 1 } });
	assert.match(exported.markdown, /Q3 revenue/);
	assert.match(exported.markdown, /9007199254740993/);
	assert.match(exported.markdown, /\*\*Records\*\*/);
	assert.deepEqual(exported.resultIds, ['quarterly-revenue']);

	console.log('✔ json-render catalog, Svelte renderer, and generated Rust schema agree');
	console.log('✔ result bindings reject unavailable data while nested layouts, charts, and controls render');
	console.log('✔ deterministic export retains exact values and the selected state');
} finally {
	await server.close();
}
