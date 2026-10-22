import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createServer } from 'vite';

// Composition regression v1.1: user-approved Svelte CSS selector correction.
// Every documented layout must render an individual element as well as arrays.
// A chart and metric cannot silently vanish while a sibling table remains.
const server = await createServer({ configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error', optimizeDeps: { noDiscovery: true, entries: [] }, server: { middlewareMode: true, hmr: false, ws: false } });
let failures = 0;
let passed = 0;
async function check(name, action) {
	try { await action(); console.log(`PASS ${name}`); passed++; }
	catch (error) { failures++; console.error(`FAIL ${name}\n${error.stack}`); }
}
try {
	const { render } = await server.ssrLoadModule('svelte/server');
	const { default: Presentation } = await server.ssrLoadModule('/src/lib/components/presentation/AnalyticalPresentation.svelte');
	const { validatePresentation } = await server.ssrLoadModule('/src/lib/presentation/validate.ts');
	const { exportPresentation } = await server.ssrLoadModule('/src/lib/presentation/export.ts');
	const results = [{ id: 'observations', columns: ['period', 'amount'], rows: [['Q3', 100], ['Q4', 120]], exact_rows: [['Q3', '100'], ['Q4', '120']], row_count: 2, complete: true }];
	const content = 'Section("Nested answer", [Metric("observations", "amount", 0, "Metric sentinel"), Chart("observations", {"mark":"bar","encoding":{"x":{"field":"period","type":"nominal"},"y":{"field":"amount","type":"quantitative"}}}, "Chart sentinel"), Select("Control sentinel", [{"value":"all","label":"All periods"}], "all")])';
	const layouts = {
		Row: (child) => `Row([${child}])`,
		Grid: (child) => `Grid([${child}], 2)`,
		Columns: (child) => `Columns([${child}], 2)`,
		Tabs: (child) => `Tabs("Views", ["Summary"], [${child}])`,
		Nested: (child) => `Grid([Columns([Row([Tabs("Views", ["Summary"], [${child}])])])])`
	};
	for (const [name, wrap] of Object.entries(layouts)) await check(`${name} keeps charts, metrics and controls visible beside a table`, () => {
		const program = `root = Document([${wrap(content)}, Table("observations")])`;
		const validation = validatePresentation(program, results);
		assert.equal(validation.valid, true, JSON.stringify(validation.issues));
		const html = render(Presentation, { props: { program, results } }).body;
		assert.match(html, /Metric sentinel/, 'nested metric must be rendered');
		assert.match(html, /Chart sentinel/, 'nested chart must be mounted');
		assert.match(html, /class="chart-target(?:\s[^"]*)?"/, 'chart must have a rendering target');
		assert.match(html, /Control sentinel/, 'nested control must remain usable');
		assert.match(html, /Saved result values/, 'sibling table remains visible');
	});
	await check('switching a saved local tab selects its actual content', () => {
		const program = 'root = Document([Tabs("Views", ["First", "Second"], [Text("FIRST_CONTENT"), Text("SECOND_CONTENT")])])';
		const html = render(Presentation, { props: { program, results, initialState: { 'tab:Views': 1 } } }).body;
		assert.match(html, /SECOND_CONTENT/);
		assert.doesNotMatch(html, /FIRST_CONTENT/);
	});
	await check('model instructions do not funnel comparisons into tables', async () => {
		const { prompt } = JSON.parse(await readFile('backend/resources/presentation-library.json', 'utf8'));
		assert.doesNotMatch(prompt, /tables for comparisons/i);
		assert.match(prompt, /@Rows\(resultId\)/, 'result binding must have a documented signature');
		assert.match(prompt, /## Reactive State/, 'generated language instructions must enable the supported state syntax');
	});
	await check('explicit currency labels format saved values without losing exact evidence in the UI or report', () => {
		const results = [{ id: 'money', columns: ['category', 'change'], rows: [['Packs', -3616.3799999999999]], exact_rows: [['Packs', '-3616.3799999999999']], row_count: 1, complete: true }];
		const program = 'root = Document([Metric("money", "change", 0, "Change", "USD", null, "currency"), Table("money", ["category", "change"], null, "Category changes", {"category":{"label":"Category"},"change":{"label":"Revenue change","format":"currency","unit":"USD","decimals":2}})])';
		const validation = validatePresentation(program, results);
		assert.equal(validation.valid, true, JSON.stringify(validation.issues));
		const html = render(Presentation, { props: { program, results } }).body;
		assert.match(html, /Revenue change/);
		assert.match(html, /-\$3,616\.38/);
		assert.match(html, /-3616\.3799999999999/, 'exact source remains inspectable');
		const exported = exportPresentation(program, results);
		assert.match(exported.markdown, /Revenue change/);
		assert.match(exported.markdown, /-\$3,616\.38/);
		assert.match(exported.markdown, /-3616\.3799999999999/, 'report preserves exact source');
		assert.match(exported.markdown, /Exact saved source values/);
	});
	await check('a table cannot attach formatting to an invented field', () => {
		const program = 'root = Document([Table("observations", null, null, null, {"invented":{"format":"currency","unit":"USD"}})])';
		assert.equal(validatePresentation(program, results).valid, false);
	});
	await check('cosmetic spacing overflow clamps to the design scale without dropping the answer', () => {
		const program = 'root = Document([Row([Metric("observations", "amount", 0, "Saved amount")], 20)])';
		const validation = validatePresentation(program, results);
		assert.equal(validation.valid, true, JSON.stringify(validation.issues));
		const html = render(Presentation, { props: { program, results } }).body;
		assert.match(html, /Saved amount/);
		assert.match(html, /--gap: 32px/);
	});
	console.log(`OpenUI composition regression v1.1: ${passed} passed, ${failures} failed.`);
	if (failures) process.exitCode = 1;
} finally { await server.close(); }
