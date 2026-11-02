// ResultTable defaults contract v1, frozen before execution:
// Numeric columns use grouping and at most six decimals, retaining exact titles.
// Integers have no decimal padding; strings (including large IDs), mixed columns,
// and all-null columns stay raw. Names never imply currency or percent. Explicit
// config wins, including USD precision and label-only config. Null follows the
// existing helper policy. Rendering never mutates rows or exact provenance.
import assert from 'node:assert/strict';
import { createServer } from 'vite';

// The table uses the host locale; freeze it before constructing any formatters.
process.env.LANG = 'en_US.UTF-8';
const server = await createServer({
	configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error',
	optimizeDeps: { noDiscovery: true, entries: [] },
	server: { middlewareMode: true, hmr: false, ws: false }
});
let passed = 0;
let failed = 0;
function check(name, fn) {
	try { fn(); passed++; console.log(`PASS ${name}`); }
	catch (error) { failed++; console.error(`FAIL ${name}`, error); }
}
try {
	const { render } = await server.ssrLoadModule('svelte/server');
	const { default: ResultTable } = await server.ssrLoadModule('/src/lib/components/presentation/ResultTable.svelte');
	function table(values, columnFormats, exact = values.map(String), field = 'value') {
		const result = {
			id: 'defaults', columns: [field], rows: values.map(value => [value]),
			exact_rows: exact.map(value => [value]), row_count: values.length, complete: true
		};
		const before = structuredClone(result);
		const html = render(ResultTable, { props: { result, columnFormats } }).body;
		assert.deepEqual(result, before, 'render must preserve rows and exact_rows');
		return [...html.matchAll(/<td\b([^>]*)>([\s\S]*?)<\/td>/g)].map(([, attrs, content]) => ({
			attrs, display: content.replace(/<span\b[\s\S]*?<\/span>/g, '').replace(/<!--[\s\S]*?-->/g, '').trim()
		}));
	}
	check('IEEE tail is readable and exact saved provenance remains available', () => {
		const [cell] = table([1715.6500000000003]);
		assert.equal(cell.display, '1,715.65');
		assert.match(cell.attrs, /title="Exact saved value: 1715\.6500000000003"/);
	});
	check('integers remain unpadded and numeric precision is capped at six places', () => {
		assert.deepEqual(table([100, 1234.1234567, 0]).map(cell => cell.display), ['100', '1,234.123457', '0']);
	});
	check('numeric strings and large string IDs remain raw', () => {
		assert.deepEqual(table(['9007199254740993', '1715.6500000000003', '00100']).map(cell => cell.display),
			['9007199254740993', '1715.6500000000003', '00100']);
	});
	check('mixed columns do not acquire a numeric default', () => {
		assert.deepEqual(table([1715.6500000000003, '9007199254740993']).map(cell => cell.display),
			['1715.6500000000003', '9007199254740993']);
	});
	check('English field names imply neither currency nor percent', () => {
		assert.equal(table([1715.6500000000003], undefined, undefined, 'revenue_usd')[0].display, '1,715.65');
		assert.equal(table([0.125], undefined, undefined, 'profit_percent')[0].display, '0.125');
	});
	check('explicit USD two-decimal config wins', () => {
		const [cell] = table([1715.6500000000003], { value: { format: 'currency', unit: 'USD', decimals: 2 } });
		assert.equal(cell.display, '$1,715.65');
		assert.match(cell.attrs, /title="Exact saved value: 1715\.6500000000003"/);
		assert.equal(table([100], { value: { format: 'currency', unit: 'USD', decimals: 2 } })[0].display, '$100.00');
	});
	check('explicit precision and label-only config win without host merging', () => {
		assert.equal(table([100], { value: { format: 'number', decimals: 2 } })[0].display, '100.00');
		assert.equal(table([1715.6500000000003], { value: { label: 'Saved value' } })[0].display, '1715.6500000000003');
	});
	check('null uses a dash only when the existing helper has a format policy', () => {
		assert.deepEqual(table([100, null]).map(cell => cell.display), ['100', '—']);
		assert.equal(table([null])[0].display, 'null');
		assert.equal(table([null], { value: { format: 'number' } })[0].display, '—');
	});
	check('formatting reads exact_rows without replacing saved provenance', () => {
		const [cell] = table([1715.65], undefined, ['1715.6500000000003']);
		assert.equal(cell.display, '1,715.65');
		assert.match(cell.attrs, /title="Exact saved value: 1715\.6500000000003"/);
	});
} finally {
	await server.close();
}
console.log(`${passed} passed, ${failed} failed`);
if (failed) process.exitCode = 1;
