// Contract v1, task set v1.2: data-aware design planning, real OpenUI parsing/registration, strict
// invalid-output and binding rejection, exact values, safe broad Vega-Lite
// composition, deterministic export, and visible fallback. v1.1 corrects two
// fixture calls from OpenUI's reserved Filter built-in to Fella's ResultFilter;
// test intent and assertions are unchanged. No model or IPC calls.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createServer } from 'vite';

const server = await createServer({ configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error', optimizeDeps: { noDiscovery: true, entries: [] }, server: { middlewareMode: true, hmr: false, ws: false } });
let passed = 0;
let failed = 0;
async function test(name, fn) {
	try { await fn(); console.log(`PASS ${name}`); passed++; }
	catch (error) { console.error(`FAIL ${name}\n${error.stack ?? error}`); failed++; }
}
try {
	const { validatePresentation } = await server.ssrLoadModule('/src/lib/presentation/validate.ts');
	const { presentationSchema, presentationPrompt, PRESENTATION_EXAMPLE, PRESENTATION_DESIGN_PROMPT, PRESENTATION_DESIGN_VERSION, presentationManifest } = await server.ssrLoadModule('/src/lib/presentation/manifest.ts');
	const { analyticalLibrary } = await server.ssrLoadModule('/src/lib/presentation/library.ts');
	const { exportPresentation } = await server.ssrLoadModule('/src/lib/presentation/export.ts');
	const { validateChartSpec, bindChartSpec } = await server.ssrLoadModule('/src/lib/presentation/chart.ts');
	const { renderBoundChart } = await server.ssrLoadModule('/src/lib/presentation/vega-runtime.ts');
	const { render } = await server.ssrLoadModule('svelte/server');
	const { default: AnalyticalPresentation } = await server.ssrLoadModule('/src/lib/components/presentation/AnalyticalPresentation.svelte');
	const result = { id: 'revenue', columns: ['quarter', 'revenue', 'region', 'low', 'high'], rows: [['Q3', 123.45, 'North', 100, 130], ['Q4', 200, 'South', 180, 220]], exact_rows: [['Q3', '123.450000', 'North', '100', '130'], ['Q4', '200.00', 'South', '180', '220']], row_count: 2, complete: true };
	const results = [result];
	const generated = JSON.parse(await readFile(new URL('../backend/resources/presentation-library.json', import.meta.url), 'utf8'));
	const chart = (spec, id = 'revenue') => `root = Document([Chart(${JSON.stringify(id)}, ${JSON.stringify(spec)}, "Revenue")])`;
	const basic = { mark: 'bar', encoding: { x: { field: 'quarter', type: 'nominal' }, y: { field: 'revenue', type: 'quantitative' } } };
	const layered = { encoding: { x: { field: 'quarter', type: 'nominal' } }, layer: [{ mark: 'bar', encoding: { y: { field: 'revenue', type: 'quantitative' } } }, { mark: { type: 'rule', strokeWidth: 2 }, encoding: { y: { field: 'low', type: 'quantitative' }, y2: { field: 'high' } } }, { mark: { type: 'text', dy: -8 }, encoding: { y: { field: 'revenue', type: 'quantitative' }, text: { field: 'revenue', type: 'quantitative' } } }] };
	const faceted = { facet: { field: 'region', type: 'nominal' }, columns: 2, spec: basic };

	await test('actual registry, manifest and parser share component names and ordered schemas', () => {
		const schema = presentationSchema();
		assert.deepEqual(Object.keys(schema.$defs), Object.keys(analyticalLibrary.components));
		for (const [name, definition] of Object.entries(schema.$defs)) assert.deepEqual(definition.properties, analyticalLibrary.toJSONSchema().$defs[name].properties, name);
		assert.equal(presentationManifest().version, 'fella-openui-v1');
		assert.match(presentationPrompt(results), /Metric\(resultId: string, field: string, row: integer, title: string, unit\?: string\)/);
		assert.match(presentationPrompt(results), /No expressions/);
		assert.match(presentationPrompt(results), /"id":"revenue"/);
	});
	await test('generated prompt has no canonical visualization template and carries an open-ended design brief', () => {
		assert.equal(generated.design_version, PRESENTATION_DESIGN_VERSION);
		assert.match(generated.planner_prompt, /no closed list of acceptable visualization forms/i);
		assert.match(generated.prompt, /design plan and result profiles/i);
		assert.doesNotMatch(generated.prompt, /Quarterly revenue|Metric\("revenue"/);
		assert.doesNotMatch(presentationPrompt(results), /Quarterly revenue|Metric\("revenue"/);
		assert.match(PRESENTATION_DESIGN_PROMPT, /single saved scalar often needs a metric and brief context, not a chart/i);
		assert.equal('example' in presentationManifest(), false);
	});
	await test('canonical fixture and resolved official OpenUI references validate', () => {
		assert.equal(validatePresentation(PRESENTATION_EXAMPLE, results).valid, true);
		const validation = validatePresentation('root = Document([intro, Table("revenue")])\nintro = Text("A comparison.")', results);
		assert.equal(validation.valid, true, JSON.stringify(validation.issues));
		assert.deepEqual(validation.resultIds, ['revenue']);
	});
	for (const [name, program] of [
		['empty', ''], ['truncated', 'root = Document([Text("unfinished")'],
		['unknown component', 'root = Document([Iframe("https://evil.invalid")])'],
		['bad prop types', 'root = Document([Metric("revenue","revenue","zero","Revenue")])'],
		['missing result', 'root = Document([Table("other-workspace")])'],
		['missing field', 'root = Document([Metric("revenue","invented",0,"Revenue")])'],
		['missing row', 'root = Document([Metric("revenue","revenue",50,"Revenue")])'],
		['unresolved reference', 'root = Document([missing])'],
		['unused statement', 'root = Document([Text("ok")])\nunused = Text("hidden")'],
		['noncomponent children', 'root = Document(["invented",42])'],
		['prop expressions', 'root = Document([Text(1 + 2)])'],
		['query plan', 'root = Document([Text("ok")])\ndata = Query("execute",{})'],
		['reactive state', '$state = 1\nroot = Document([Text("ok")])'],
		['duplicate control names', 'root = Document([Tabs("same",["A"],[Text("ok")]),ResultFilter("revenue","region","same")])'],
		['tab arity', 'root = Document([Tabs("view",["A","B"],[Text("ok")])])'],
		['nested root', 'root = Document([Document([Text("ok")])])']
	]) await test(`reject ${name}`, () => { const check = validatePresentation(program, results); assert.equal(check.valid, false, JSON.stringify(check)); assert.equal(check.root, null); assert.ok(check.issues.length); });

	await test('exact metrics/tables preserve large integers, decimal scale, null and markup escaping', () => {
		const exact = { id: 'exact', columns: ['id', 'money', 'missing', 'text'], rows: [[9007199254740992, 0.1, null, '<script>x</script>']], exact_rows: [['9007199254740993', '0.100000000000000001', 'not zero', '<script>x</script>']], row_count: 1, complete: true };
		const program = 'root = Document([Metric("exact","id",0,"Identifier"),Table("exact")])';
		const exported = exportPresentation(program, [exact]);
		assert.match(exported.markdown, /9007199254740993/);
		assert.match(exported.markdown, /0\.100000000000000001/);
		assert.match(exported.markdown, /\| null \|/);
		assert.doesNotMatch(exported.markdown, /<script>/);
		const html = render(AnalyticalPresentation, { props: { program, results: [exact] } }).body;
		assert.match(html, /9007199254740993/); assert.match(html, /&lt;script/); assert.doesNotMatch(html, /<script>/);
	});
	await test('incomplete results retain explicit scope in renderer and export', () => {
		const partial = { ...result, row_count: 100, complete: false };
		assert.match(exportPresentation('root = Document([Table("revenue")])', [partial]).markdown, /Incomplete saved result/);
		assert.match(render(AnalyticalPresentation, { props: { program: 'root = Document([Table("revenue")])', results: [partial] } }).body, /not the full dataset/);
	});
	for (const [name, spec] of [['layered and field annotations', layered], ['faceted', faceted], ['concatenated', { hconcat: [basic, { mark: 'point', encoding: basic.encoding }] }], ['repeated', { repeat: ['revenue', 'low'], spec: { mark: 'point', encoding: { y: { field: { repeat: 'repeat' }, type: 'quantitative' } } } }]]) await test(`accept broad ${name} chart`, () => assert.equal(validatePresentation(chart(spec), results).valid, true));
	for (const [name, spec] of [
		['inline authoritative data', { ...basic, data: { values: [{ revenue: 999 }] } }],
		['remote resource', { ...basic, data: { url: 'https://evil.invalid/data' } }],
		['nested data', { layer: [{ ...basic, data: { values: [] } }] }],
		['raw datasets', { ...basic, datasets: { malicious: [] } }],
		['image mark', { mark: 'image', encoding: basic.encoding }],
		['href', { ...basic, encoding: { ...basic.encoding, href: { field: 'region' } } }],
		['signals', { ...basic, signals: [{ name: 'x', update: 'fetch()' }] }],
		['expressions', { ...basic, transform: [{ calculate: 'datum.revenue*99', as: 'revenue' }] }],
		['axis expression', { ...basic, encoding: { x: { field: 'quarter', axis: { labelExpr: 'evil()' } } } }],
		['raw numeric positions', { mark: 'rule', encoding: { y: { value: 100000 } } }],
		['invented field', { ...basic, encoding: { y: { field: 'fake', type: 'quantitative' } } }],
		['CSS URL', { ...basic, mark: { type: 'bar', fill: 'url(https://evil.invalid)' } }],
		['huge canvas', { ...basic, width: 999999 }],
		['param actions', { ...basic, params: [{ name: 'a', select: 'interval' }] }],
		['prototype keys', JSON.parse('{"mark":"bar","__proto__":{"polluted":true}}')],
		['unknown property', { ...basic, unsafe_unknown: true }],
		['imputation', { ...basic, encoding: { y: { field: 'revenue', type: 'quantitative', impute: { value: 0 } } } }]
	]) await test(`reject chart ${name}`, () => assert.throws(() => validateChartSpec(spec, result)));
	await test('binding cannot mutate original spec or read dotted paths as nested objects', () => {
		const source = JSON.stringify(basic);
		const bound = bindChartSpec(basic, result);
		assert.equal(JSON.stringify(basic), source); assert.equal(bound.data.values[0].fella_col_1, 123.45);
		assert.equal(bound.encoding.y.field, 'fella_col_1'); assert.equal(bound.encoding.y.title, 'revenue');
		const dotted = { ...result, columns: ['a.b', 'revenue', 'region', 'low', 'high'] };
		const mapped = bindChartSpec({ mark: 'bar', encoding: { x: { field: 'a.b', type: 'nominal' } } }, dotted);
		assert.equal(mapped.encoding.x.field, 'fella_col_0');
	});
	await test('real Svelte rendering and invalid-output fallback are visible, not blank', () => {
		const html = render(AnalyticalPresentation, { props: { program: PRESENTATION_EXAMPLE, results } }).body;
		assert.match(html, /Quarterly revenue/); assert.match(html, /123\.450000/); assert.match(html, /Exact saved values/);
		const fallback = render(AnalyticalPresentation, { props: { program: 'root = Broken()', results } }).body;
		assert.match(fallback, /could not be displayed/); assert.match(fallback, /saved analysis has not changed/); assert.match(fallback, /123\.450000/);
	});
	await test('exports are deterministic and include state, inactive tabs, exact values and explicit complex-chart fallback', () => {
		const program = `root = Document([ResultFilter("revenue","region","regions"),Tabs("views",["Chart","Table"],[Chart("revenue",${JSON.stringify(layered)},"Revenue"),Table("revenue")])])`;
		const options = { state: { 'filter:regions': { resultId: 'revenue', field: 'region', value: 'North' }, 'tab:views': 1 } };
		const a = exportPresentation(program, results, options); const b = exportPresentation(program, results, options);
		assert.deepEqual(a, b); assert.match(a.markdown, /North/); assert.match(a.markdown, /selected view/); assert.match(a.markdown, /additional view/); assert.match(a.markdown, /123\.450000/); assert.match(a.markdown, /1 of 2 saved rows/); assert.ok(a.warnings.length); assert.match(a.markdown, /Vega-Lite/);
	});
	await test('portable simple text figures never replace exact data; snapshots cannot become remote links', () => {
		const text = exportPresentation(chart(basic), results);
		assert.match(text.markdown, /Text figure/); assert.match(text.markdown, /123\.450000/);
		const remote = exportPresentation(chart(layered), results, { chartSnapshots: { Revenue: 'https://evil.invalid/image.svg' } });
		assert.doesNotMatch(remote.markdown, /!\[/); assert.ok(remote.warnings.some((warning) => warning.includes('safe local image')));
		assert.match(exportPresentation(chart(layered), results, { chartSnapshots: { Revenue: 'images/revenue.svg' } }).markdown, /\.\/images\/revenue\.svg/);
	});
	await test('real Vega-Lite layer/facet compile and execute with interpreted expressions', async () => {
		for (const spec of [layered, faceted]) {
			const view = await renderBoundChart(null, spec, result, [0, 1], { text: '#eeeeee', grid: '#333333', accent: '#8888ff' });
			try { assert.match(await view.toSVG(), /<svg/); } finally { view.finalize(); }
		}
	});
	console.log(`\nAnalytical presentation contract v1 (task set v1.2): ${passed} passed, ${failed} failed.`);
	if (failed) process.exitCode = 1;
} finally { await server.close(); }
