// Analytical diff UI contract v1. Fixed before candidate execution. These
// independent tests do not replace or modify the existing artifact tests.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { compile } from 'svelte/compiler';
import { createServer } from 'vite';

const server = await createServer({ configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error', optimizeDeps: { noDiscovery: true, entries: [] }, server: { middlewareMode: true, hmr: false, ws: false } });
let passed = 0;
let failed = 0;
async function check(name, fn) {
	try { await fn(); passed++; console.log(`PASS ${name}`); }
	catch (error) { failed++; console.error(`FAIL ${name}\n${error.stack}`); }
}
try {
	const { render } = await server.ssrLoadModule('svelte/server');
	const { default: AnalyticalDiff } = await server.ssrLoadModule('/src/lib/components/AnalyticalDiff.svelte');
	const dimension = (summary) => ({ status: 'changed', summary, items: [{ label: 'Recorded field', before: 'Original', after: 'Proposed', changed: true }], limitations: [] });
	const comparison = {
		compatible: true, reasons: [], warnings: ['Numerical changes do not establish causation.'], before_revision: 1, after_revision: 2, source_revision_changed: true, definitions_or_filters_changed: true,
		tables: [{ before_result_id: 'old-result', after_result_id: 'new-result', key_fields: ['region'], values: [
			{ key: ['East'], field: 'revenue', unit: 'USD', before: '9007199254740993', after: '9007199254740994', delta: { kind: 'exact_integer', delta: '1' } },
			{ key: ['West'], field: 'revenue', unit: 'USD', before: 'null', after: '0', delta: { kind: 'unavailable', reason: 'Missing values are not zero.' } },
			{ key: ['North'], field: 'revenue', unit: 'USD', before: '1.25', after: '2.5', delta: { kind: 'finite_float', delta: 1.25 } }
		] }],
		analytical_diff: { version: 1,
			before: { artifact_id: 'artifact', revision: 1, run_id: 'before-run', role: 'current', change_note: 'Original' },
			after: { artifact_id: 'artifact', revision: 2, run_id: 'after-run', role: 'proposed', change_note: 'Exclude refunded orders' },
			meaning: dimension('Refund scope changed.'), inputs: dimension('Source revision changed.'), method: dimension('Saved SQL changed.'), results: dimension('Aligned saved results changed.'),
			impact: { scanned: true, complete: true, summary: 'A saved output remains pinned.', consumers: [{ document_id: 'doc', document_title: 'Quarterly review', placement_id: 'placement', checkpoint_id: null, checkpoint_title: null, pinned_revision: 1 }], limitations: ['Exported reports are not indexed.', 'Pins never update automatically.'] }
		}
	};
	const html = (props = {}) => render(AnalyticalDiff, { props: { comparison, ...props } }).body;
	await check('five dimensions have a clear reading order and recorded-role labels', () => {
		const output = html(); let previous = -1;
		for (const name of ['Meaning', 'Inputs', 'Method', 'Results', 'Impact']) { const index = output.indexOf(`aria-label="${name}"`); assert.ok(index > previous); previous = index; }
		assert.match(output, /Current · revision 1/); assert.match(output, /Proposed · revision 2/); assert.match(output, /Nothing is rerun or adopted/);
	});
	await check('integer/null values stay exact and floating deltas are explicitly approximate', () => {
		const output = html(); assert.match(output, /9007199254740993/); assert.match(output, /9007199254740994/); assert.doesNotMatch(output, /9007199254740992/); assert.match(output, />null</); assert.match(output, /Missing values are not zero/); assert.match(output, /Approx\. 1\.25/);
	});
	await check('incompatible results never expose a plausible-looking delta table', () => {
		const output = html({ comparison: { ...comparison, compatible: false, reasons: ['Declared units differ.'] } });
		assert.match(output, /cannot be compared numerically/); assert.match(output, /Declared units differ/); assert.doesNotMatch(output, /9007199254740993/);
	});
	await check('proposal actions are explicit host callbacks and never fire during render', () => {
		let actions = 0; const onadopt = () => actions++; const onkeep = () => actions++;
		const output = html({ onadopt, onkeep }); assert.match(output, /Use proposed revision/); assert.match(output, /Keep current revision/); assert.equal(actions, 0);
		const current = { ...comparison, analytical_diff: { ...comparison.analytical_diff, after: { ...comparison.analytical_diff.after, role: 'current' } } };
		assert.doesNotMatch(html({ comparison: current, onadopt }), /Use proposed revision/);
	});
	await check('busy state prevents proposal interaction and exposes host errors safely', () => {
		const output = html({ busy: true, onadopt: () => {}, onkeep: () => {}, error: '<script>unsafe()</script>' });
		assert.match(output, /aria-busy="true"/); assert.match(output, /disabled/); assert.match(output, /role="alert"/); assert.doesNotMatch(output, /<script>/);
	});
	await check('impact lists explicit pins and the scope of dependency discovery', () => {
		const output = html({ onopenconsumer: () => {} }); assert.match(output, /Quarterly review/); assert.match(output, /Revision 1 remains pinned/); assert.match(output, /Exported reports are not indexed/); assert.match(output, /Pins never update automatically/);
	});
	await check('legacy comparisons do not invent semantic metadata or current/proposed roles', () => {
		const output = html({ comparison: { ...comparison, analytical_diff: undefined } });
		assert.match(output, /Detailed metadata was not recorded/); assert.match(output, /Dependencies have not been scanned/); assert.doesNotMatch(output, /Current ·|Proposed ·|Historical ·/); assert.match(output, /9007199254740993/);
	});
	await check('client compiler accepts compact responsive layout and keyboard controls', async () => {
		const source = await readFile('src/lib/components/AnalyticalDiff.svelte', 'utf8');
		const compiled = compile(source, { filename: 'src/lib/components/AnalyticalDiff.svelte', generate: 'client', runes: true });
		assert.deepEqual(compiled.warnings.map(warning => warning.code), []);
		assert.match(source, /@container \(max-width: 520px\)/); assert.match(source, /overflow: auto/); assert.doesNotMatch(source, /window\.fella|\bipc\.|fetch\(/);
	});
	console.log(`Analytical diff UI contract v1: ${passed} passed, ${failed} failed.`);
	if (failed) process.exitCode = 1;
} finally { await server.close(); }
