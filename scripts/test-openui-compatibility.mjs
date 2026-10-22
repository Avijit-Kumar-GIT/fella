// OpenUI compatibility contract v1 (declared before candidate execution).
// Cosmetic Card tones normalize to documented host styles/defaults. Standard
// Vega-Lite numeric/time formats render, but custom format code never does.
// Exact/result-derived Metrics retain evidence references. Literal Metrics,
// missing evidence, JS method calls and malformed programs remain repairable
// failures. Existing suites/expectations are intentionally left untouched.
import assert from 'node:assert/strict';
import { createServer } from 'vite';

const server = await createServer({ configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error', optimizeDeps: { noDiscovery: true, entries: [] }, server: { middlewareMode: true, hmr: false, ws: false } });
let passed = 0;
let failed = 0;
async function check(name, action) {
	try { await action(); passed++; console.log(`PASS ${name}`); }
	catch (error) { failed++; console.error(`FAIL ${name}\n${error.stack}`); }
}
try {
	const { validatePresentation } = await server.ssrLoadModule('/src/lib/presentation/validate.ts');
	const { validateChartSpec, bindChartSpec } = await server.ssrLoadModule('/src/lib/presentation/chart.ts');
	const { presentationSchema, presentationPrompt } = await server.ssrLoadModule('/src/lib/presentation/manifest.ts');
	const { renderBoundChart } = await server.ssrLoadModule('/src/lib/presentation/vega-runtime.ts');
	const result = { id: 'metrics', columns: ['period', 'amount', 'date'], rows: [['Q3', 10.25, '2025-07-01'], ['Q4', 20.5, '2025-10-01']], exact_rows: [['Q3', '10.2500', '2025-07-01'], ['Q4', '20.5000', '2025-10-01']], row_count: 2, complete: true };
	const results = [result];
	const chart = { mark: 'bar', encoding: { x: { field: 'period', type: 'nominal' }, y: { field: 'amount', type: 'quantitative' }, tooltip: [{ field: 'amount', type: 'quantitative', format: '.2f', formatType: 'number' }] } };
	const valid = (program) => { const check = validatePresentation(program, results); assert.equal(check.valid, true, JSON.stringify(check.issues)); return check; };
	const invalid = (program) => { const check = validatePresentation(program, results); assert.equal(check.valid, false, 'Invalid analytical authority or syntax must not be normalized into a success.'); assert.ok(check.issues.length); return check; };
	const card = (tone) => `root = Document([Card([Text("Policy comparison")], "Policy", null, ${JSON.stringify(tone)})])`;
	for (const [tone, canonical] of [['positive', 'quiet'], ['negative', 'quiet'], ['soft', 'quiet'], ['outline', 'plain'], ['highlight', 'accent'], [' QUIET ', 'quiet'], ['brand-new-cosmetic-tone', 'plain']]) await check(`Card style ${JSON.stringify(tone)} normalizes to ${canonical}`, () => {
		const check = valid(card(tone)); assert.equal(check.root.props.children[0].props.tone, canonical);
	});
	await check('omitted/null Card tone defaults to plain without changing content', () => {
		for (const program of ['root = Document([Card([Text("Content")])])', card(null)]) {
			const check = valid(program); assert.equal(check.root.props.children[0].props.tone, 'plain');
		}
	});
	await check('Card tone still rejects objects and oversized cosmetic payloads', () => { invalid(card({ expr: 'run()' })); invalid(card('x'.repeat(1000))); });
	await check('existing colon-named bound Metric retains exact binding and omitted optional value', () => {
		const check = valid('root = Document([Metric(resultId: "metrics", field: "amount", row: 0, title: "Revenue", unit: "USD", format: "currency")])');
		assert.deepEqual(check.resultIds, ['metrics']); assert.equal(check.root.props.children[0].props.value, undefined);
	});
	await check('legacy equals-named bound Metric normalizes to official positional syntax', () => {
		const check = valid('root = Document(children = [Metric(resultId = "metrics", field = "amount", row = 0, title = "Revenue", unit = "USD")])');
		assert.match(check.normalizedProgram, /Metric\("metrics", "amount", 0, "Revenue", "USD"\)/); assert.deepEqual(check.resultIds, ['metrics']);
	});
	await check('official saved-row member expressions remain valid Metric bindings', () => {
		const check = valid('root = Document([Metric(title: "First revenue", value: @First(@Rows("metrics")).amount)])');
		assert.equal(check.root.props.children[0].props.value, 10.25); assert.deepEqual(check.resultIds, ['metrics']);
	});
	await check('official result aliases remain valid derived Metric bindings', () => {
		const check = valid('saved = @Rows("metrics")\nroot = Document([Metric(title: "Total revenue", value: @Sum(saved.amount))])');
		assert.equal(check.root.props.children[0].props.value, 30.75); assert.deepEqual(check.resultIds, ['metrics']);
	});
	await check('bound Metrics inside official Each templates keep their literal result IDs', () => {
		const check = valid('root = Document([@Each(@Rows("metrics"), item, Metric("metrics", "amount", 0, item.period))])');
		assert.equal(check.root.props.children.length, 2); assert.deepEqual(check.resultIds, ['metrics']);
	});
	await check('literal revenue-bridge values stay rejected with actionable binding guidance', () => {
		const check = invalid('root = Document([Metric(value: @Round(425.22, 2), title: "Net revenue change", unit: "USD")])');
		assert.match(check.issues.map(item => item.message).join(' '), /saved result|@Rows|evidence/);
	});
	await check('missing result, field and row remain rejected without inferred references', () => {
		for (const program of ['root = Document([Metric("invented", "amount", 0, "Revenue")])', 'root = Document([Metric("metrics", "invented", 0, "Revenue")])', 'root = Document([Metric("metrics", "amount", 99, "Revenue")])']) invalid(program);
	});
	await check('standard numeric tooltip format and number formatType validate and remain intact', () => {
		validateChartSpec(chart, result); const bound = bindChartSpec(chart, result); assert.equal(bound.encoding.tooltip[0].format, '.2f'); assert.equal(bound.encoding.tooltip[0].formatType, 'number');
		valid(`root = Document([Chart("metrics", ${JSON.stringify(chart)})])`);
	});
	await check('numeric axis/legend/header formatType settings are safe standard formatting', () => {
		validateChartSpec({ ...chart, encoding: { ...chart.encoding, y: { field: 'amount', type: 'quantitative', axis: { format: ',.2f', formatType: 'number' } }, color: { field: 'amount', type: 'quantitative', legend: { format: '.1f', formatType: 'number' } }, facet: { field: 'period', type: 'nominal', header: { format: '.1f', formatType: 'number' } } } }, result);
	});
	const timeChart = { mark: 'text', encoding: { text: { field: 'date', type: 'temporal', format: '%b %Y', formatType: 'time' } } };
	await check('standard time encoding format validates', () => validateChartSpec(timeChart, result));
	await check('documented dynamic time format object validates', () => validateChartSpec({ ...timeChart, encoding: { text: { ...timeChart.encoding.text, format: { year: '%Y', month: '%b %Y', date: '%d %b' } } } }, result));
	await check('format metadata does not introduce custom functions, code, URL loaders or expressions', () => {
		for (const formatType of ['customFunction', 'javascript', { expr: 'run()' }]) assert.throws(() => validateChartSpec({ ...chart, encoding: { tooltip: { field: 'amount', type: 'quantitative', format: '.2f', formatType } } }, result));
		for (const format of [{ expr: 'datum.amount' }, { year: { signal: 'run()' } }, 123, 'javascript:run()']) assert.throws(() => validateChartSpec({ ...chart, encoding: { tooltip: { field: 'amount', type: 'quantitative', format } } }, result));
		assert.throws(() => validateChartSpec({ ...chart, data: { url: 'https://example.invalid/data' } }, result));
	});
	await check('JS slice syntax remains a repairable error with a supported alternative', () => {
		const check = invalid('root = Document([Table("metrics", null, @Sort(@Rows("metrics"), "amount", "desc").slice(0, 5))])');
		assert.match(check.issues.map(item => item.message).join(' '), /method|@Top/);
	});
	await check('incomplete/malformed output and duplicate named props remain rejected', () => {
		for (const program of ['root = Document([Text("unfinished")', 'root = Document([Text("one") Text("two")])', 'root = Document([Card([Text("x")], tone: "quiet", tone: "plain")])']) invalid(program);
	});
	await check('public prompt/schema explain style defaults and supported formatting without promising JS methods', () => {
		const prompt = presentationPrompt(results); assert.match(prompt, /tone|styles/i); assert.match(prompt, /formatType/); assert.match(prompt, /@Top/);
		assert.equal(presentationSchema().$defs.Card.properties.tone.anyOf[0].type, 'string');
	});
	await check('real interpreted Vega execution accepts numeric and time formatting', async () => {
		for (const spec of [chart, timeChart]) {
			const view = await renderBoundChart(null, spec, result, [0, 1], { text: '#333333', grid: '#dddddd', accent: '#556677' });
			try { assert.match(await view.toSVG(), /<svg/); } finally { view.finalize(); }
		}
	});
	console.log(`OpenUI compatibility contract v1: ${passed} passed, ${failed} failed.`);
	if (failed) process.exitCode = 1;
} finally { await server.close(); }
