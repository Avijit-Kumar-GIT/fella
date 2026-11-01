// Frozen intelligent-ui-v1 expectations are the oracle, not either implementation.
// Run after the parent builds both artifacts; this runner never builds or repairs them.
// Acceptance: all 28 validity results, complete native/WASM response parity,
// deterministic identities, canonical reacceptance, metadata/derived props, exact
// saved rows and values, display-state restoration/visibility, and Markdown snapshots.
// Browser event wiring, keyboard focus, side-effect isolation, and Vega pixels are
// explicitly outside this data runner. No model-quality or numeric performance grade.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { createServer } from 'vite';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const path = relative => fileURLToPath(new URL(relative, new URL('../../../', import.meta.url)));
const fixturePath = 'tests/fixtures/intelligent-ui-v1.json';
const wasmPath = 'src/lib/presentation/generated/contract.wasm';
const nativePath = `backend/target/debug/examples/contract_accept${process.platform === 'win32' ? '.exe' : ''}`;
const plain = value => JSON.parse(JSON.stringify(value));
const sha256 = value => createHash('sha256').update(value).digest('hex');
const failures = [];
const passed = [];
const limitations = [
	'No browser mount, click dispatch, DOM focus, or keyboard-handler assertions; fixture tab keys drive reference state transitions only.',
	'Follow-up checks validate request payloads only; automatic lifecycle emissions and host/query/network isolation require UI/host tests.',
	'Chart checks cover saved data, encodings, composition, and export; actual Vega rendering belongs to test-chart-renderer.mjs.',
	'Frozen complete sales rows do not establish incomplete-result disclosures, real source coverage, or model quality.'
];
const report = { fixture: 'intelligent-ui-v1', cases: 28, passed, failures, limitations, deferred_assertions: [], performance: {} };

function freeze(value) {
	if (value && typeof value === 'object' && !Object.isFrozen(value)) {
		Object.values(value).forEach(freeze);
		Object.freeze(value);
	}
	return value;
}
function check(label, run) {
	try { run(); passed.push(label); return true; }
	catch (error) { failures.push({ label, message: error.stack ?? String(error) }); return false; }
}
async function nativeBatch(requests) {
	const started = performance.now();
	// One outstanding request keeps pipe backpressure independent of fixture size.
	// A synchronous all-at-once transport timed out in this environment; a serial
	// JSON-line probe completed all 28 requests without changing acceptance inputs.
	const outputs = await new Promise((resolve, reject) => {
		const child = spawn(path(nativePath), [], { cwd: root, windowsHide: true });
		const lines = createInterface({ input: child.stdout });
		let stderr = '';
		let settled = false;
		const finish = error => {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			lines.close();
			if (error) { child.kill(); reject(error); } else resolve(outputs);
		};
		const outputs = [];
		const timer = setTimeout(() => finish(new Error(`native transport timed out at request ${outputs.length + 1}/${requests.length}; stderr: ${stderr}`)), 30_000);
		const send = () => child.stdin.write(JSON.stringify(requests[outputs.length]) + '\n');
		child.on('error', finish);
		child.stdin.on('error', finish);
		child.stderr.on('data', data => { stderr = (stderr + data).slice(-16_000); });
		lines.on('line', line => {
			try {
				assert.ok(outputs.length < requests.length, 'no extra native response');
				outputs.push(JSON.parse(line));
				if (outputs.length === requests.length) child.stdin.end(); else send();
			} catch (error) { finish(error); }
		});
		child.on('close', (code, signal) => {
			try {
				assert.equal(code, 0, `native CLI failed (${signal ?? code}): ${stderr}`);
				assert.equal(outputs.length, requests.length, 'one native JSON response per request');
				finish();
			} catch (error) { finish(error); }
		});
		send();
	});
	return { outputs, ms: performance.now() - started };
}

// Intentionally independent of contract.ts: exercise the public memory ABI directly.
function wasmEvaluator(instance) {
	const { memory, allocate, validate, release } = instance.exports;
	assert.ok(memory instanceof WebAssembly.Memory, 'WASM exports memory');
	for (const fn of [allocate, validate, release]) assert.equal(typeof fn, 'function');
	const encoder = new TextEncoder();
	const decoder = new TextDecoder('utf-8', { fatal: true });
	return request => {
		const bytes = encoder.encode(JSON.stringify(request));
		const input = allocate(bytes.length) >>> 0;
		let output;
		let length;
		try {
			new Uint8Array(memory.buffer, input, bytes.length).set(bytes);
			const handle = BigInt.asUintN(64, validate(input, bytes.length));
			output = Number(handle & 0xffffffffn);
			length = Number(handle >> 32n);
			assert.ok(length > 0 && output + length <= memory.buffer.byteLength, 'WASM response lies inside memory');
			return JSON.parse(decoder.decode(new Uint8Array(memory.buffer, output, length)));
		} finally {
			release(input, bytes.length);
			if (output !== undefined) release(output, length);
		}
	};
}

function assertResponse(output, expectedValid) {
	assert.equal(output.valid, expectedValid, JSON.stringify(output.issues));
	assert.ok(Array.isArray(output.issues));
	assert.ok(Array.isArray(output.resultIds));
	assert.equal(typeof output.contractHash, 'string');
	assert.ok(output.contractHash.length > 0);
	if (expectedValid) {
		assert.ok(output.spec?.elements?.[output.spec.root]);
		assert.deepEqual(output.issues, []);
	} else {
		assert.equal(output.spec, null, 'rejection never yields an accepted spec');
		assert.deepEqual(output.resultIds, []);
		assert.ok(output.issues.length > 0, 'rejection has diagnostics');
		for (const issue of output.issues) {
			assert.equal(typeof issue.path, 'string');
			assert.ok(issue.path.length > 0);
			assert.equal(typeof issue.message, 'string');
			assert.ok(issue.message.length > 0);
		}
	}
}

function assertCompilation(test, spec) {
	assert.deepEqual(Object.keys(spec).sort(), ['elements', 'root']);
	assert.equal(spec.elements[spec.root].type, 'Document');
	const author = test.input.version === 'fella-ui-1';
	const metadata = spec.elements[spec.root].props.contract;
	if (author) {
		const expectedMetadata = {
			version: test.input.version, capabilities: test.input.capabilities, state: test.input.state,
			...(Object.hasOwn(test.input, 'intent') ? { intent: test.input.intent } : {})
		};
		assert.deepEqual(metadata, expectedMetadata, 'persist author contract exactly');
		function walk(node, key) {
			const element = spec.elements[key];
			assert.equal(element.type, node.type);
			if (node.id) assert.equal(key, node.id, 'preserve every explicit ID');
			assert.equal(element.children.length, (node.children ?? []).length);
			(node.children ?? []).forEach((child, i) => walk(child, element.children[i]));
		}
		walk(test.input.root, spec.root);
	} else {
		assert.equal(metadata, undefined, 'legacy remains legacy');
		assert.deepEqual(spec, test.input, 'legacy identities and props survive');
	}
	if (test.expected.metadata) assert.deepEqual(metadata, test.expected.metadata);
	if (test.expected.intent_absent) assert.equal(Object.hasOwn(metadata, 'intent'), false);
	for (const id of test.expected.explicit_ids ?? []) assert.ok(Object.hasOwn(spec.elements, id), id);
	for (const [id, expected] of Object.entries(test.expected.derived ?? {})) {
		const element = spec.elements[id];
		assert.ok(element, `derived element ${id} exists`);
		for (const [key, value] of Object.entries(expected)) {
			if (key === 'options_from') {
				const declaration = value.split('.').reduce((object, part) => object[part], test.input);
				assert.deepEqual(element.props.options, declaration);
			} else if (key === 'name_matches') assert.equal(element.props.name, spec.elements[value].props.name);
			else if (key === 'name_from') assert.equal(element.props.name, id, 'Tabs name derives from its stable ID');
			else assert.deepEqual(element.props[key], value, `${id}.${key}`);
		}
	}
}

// Markdown snapshots use the frozen fixture's exact strings, never candidate output.
function tableSnapshot(result, indices, fields = result.columns) {
	const columns = fields.map(field => result.columns.indexOf(field));
	const cell = value => {
		assert.doesNotMatch(value, /[|\\<>\r\n]/, 'these fixtures need no Markdown escaping');
		return value;
	};
	return [
		`| ${fields.join(' | ')} |`, `| ${fields.map(() => '---').join(' | ')} |`,
		...indices.map(row => `| ${columns.map(column => cell(result.exact_rows[row][column])).join(' | ')} |`)
	].join('\n');
}

let server;
try {
	const [fixtureBytes, wasmBytes, nativeBytes] = await Promise.all([
		readFile(path(fixturePath)), readFile(path(wasmPath)), readFile(path(nativePath))
	]);
	const fixture = freeze(JSON.parse(fixtureBytes));
	assert.equal(fixture.fixture_version, 'intelligent-ui-v1');
	assert.equal(fixture.cases.length, 28, 'do not silently exclude frozen cases');
	assert.equal(fixture.cases.filter(test => test.expected.valid).length, 12);
	assert.equal(new Set(fixture.cases.map(test => test.id)).size, 28);
	const baseline = JSON.stringify(fixture);
	const requests = fixture.cases.map(test => freeze({ spec: test.input, results: fixture.results, mode: 'full', charts_enabled: true }));
	report.artifacts = { fixture_sha256: sha256(fixtureBytes), native_sha256: sha256(nativeBytes), wasm_sha256: sha256(wasmBytes) };
	report.performance.setup = {
		platform: process.platform, architecture: process.arch, node: process.version,
		native_profile: 'debug', wasm_artifact: wasmPath, dataset_rows: 4, cases_per_batch: 28,
		cold_definition: 'first compilation and instantiation in this process; OS and engine caches uncontrolled',
		acceptance_scope: 'includes JSON encode/decode and memory copies; excludes file reads and Vite startup',
		warm_init_samples: 5, warm_acceptance_batches: 5, numeric_targets: null
	};
	report.performance.wasm_bytes = wasmBytes.length;
	report.performance.wasm_gzip_bytes = gzipSync(wasmBytes, { level: 9 }).length;
	let started = performance.now();
	const module = await WebAssembly.compile(wasmBytes);
	report.performance.first_compile_ms = performance.now() - started;
	assert.deepEqual(WebAssembly.Module.imports(module), [], 'contract has no imported authority');
	started = performance.now();
	const instance = await WebAssembly.instantiate(module, {});
	report.performance.first_instantiate_ms = performance.now() - started;
	report.performance.cold_init_ms = report.performance.first_compile_ms + report.performance.first_instantiate_ms;
	const evaluateWasm = wasmEvaluator(instance);
	const native = await nativeBatch(requests);
	report.performance.native_first_batch_28_ms_including_process_start = native.ms;
	started = performance.now();
	const wasmOutputs = requests.map(evaluateWasm);
	report.performance.wasm_first_acceptance_28_ms = performance.now() - started;
	report.contract_hash = wasmOutputs[0].contractHash;
	const accepted = [];
	fixture.cases.forEach((test, i) => {
		const good = check(`${test.id}: native/WASM acceptance`, () => {
			assertResponse(native.outputs[i], test.expected.valid);
			assertResponse(wasmOutputs[i], test.expected.valid);
			assert.deepEqual(wasmOutputs[i], native.outputs[i], 'complete response parity, independent of object key order');
			if (test.expected.valid) assertCompilation(test, wasmOutputs[i].spec);
			assert.equal(JSON.stringify(fixture), baseline, 'acceptance must not mutate inputs');
		});
		if (good && test.expected.valid) accepted.push({ test, output: wasmOutputs[i], request: requests[i] });
	});
	if (accepted.length) {
		const repeatNative = (await nativeBatch(accepted.map(item => item.request))).outputs;
		const canonicalNative = (await nativeBatch(accepted.map(({ request, output }) => ({ ...request, spec: output.spec })))).outputs;
		accepted.forEach(({ test, output, request }, i) => check(`${test.id}: deterministic compilation and canonical reacceptance`, () => {
			assert.deepEqual(repeatNative[i], output, 'native generated IDs and canonical JSON are deterministic');
			assert.deepEqual(evaluateWasm(request), output, 'WASM generated IDs and canonical JSON are deterministic');
			assert.deepEqual(canonicalNative[i], output, 'native reaccepts persisted canonical JSON unchanged');
			assert.deepEqual(evaluateWasm({ ...request, spec: output.spec }), output, 'WASM reaccepts persisted canonical JSON unchanged');
		}));
	}

	server = await createServer({
		root, configFile: path('vite.config.ts'), appType: 'custom', logLevel: 'error',
		optimizeDeps: { noDiscovery: true, entries: [] },
		server: { middlewareMode: true, hmr: false, ws: false }
	});
	const [results, exports, interactions, runtime] = await Promise.all([
		server.ssrLoadModule('/src/lib/presentation/results.ts'),
		server.ssrLoadModule('/src/lib/presentation/export.ts'),
		server.ssrLoadModule('/src/lib/presentation/interactions.ts'),
		server.ssrLoadModule('/src/lib/presentation/runtime.ts')
	]);
	const { exactValue, requireResult, filteredRows, resolvePresentationState, validateResults } = results;
	const { exportPresentation } = exports;
	const { isPresentationVisible, displayPresentation, controlValueAllowed } = interactions;
	const { setPresentationValue, evaluatePresentationAction } = runtime;
	validateResults(fixture.results);
	const savedResults = freeze(plain(fixture.results));
	const savedBaseline = JSON.stringify(savedResults);

	for (const { test, output } of accepted) check(`${test.id}: saved-data and display-state evaluation`, () => {
		const spec = freeze(plain(output.spec));
		const specBaseline = JSON.stringify(spec);
		let state = resolvePresentationState(spec, {}, savedResults);
		const get = id => { assert.ok(spec.elements[id], `node ${id}`); return spec.elements[id]; };
		const bound = id => requireResult(savedResults, get(id).props.resultId);
		const rowsFor = id => {
			const props = get(id).props;
			return filteredRows(bound(id), state, props.interactionGroup, props.linkBy, undefined, props.filters);
		};
		function visibleNodes() {
			const visible = new Set();
			const displayed = displayPresentation(spec, state);
			for (const [id, element] of Object.entries(spec.elements)) {
				if (element.visible !== undefined) assert.equal(displayed.elements[id].visible, isPresentationVisible(element, state));
			}
			function walk(id) {
				const element = get(id);
				if (!isPresentationVisible(element, state)) return;
				visible.add(id);
				if (element.type === 'Tabs') {
					const child = element.children[Number(state[`tab:${element.props.name}`] ?? 0)];
					if (child) walk(child);
				} else element.children.forEach(walk);
			}
			walk(spec.root);
			return visible;
		}
		function snapshot() {
			const exported = exportPresentation(spec, savedResults, { state });
			assert.doesNotMatch(exported.markdown, /could not be validated|Presentation unavailable:/);
			assert.deepEqual(plain(exportPresentation(spec, savedResults, { state })), plain(exported), 'export snapshot is deterministic');
			assert.deepEqual([...exported.resultIds].sort(), [...output.resultIds].sort());
			const marker = '## Exact saved source values:';
			const split = exported.markdown.indexOf(marker);
			const body = split < 0 ? exported.markdown : exported.markdown.slice(0, split).trim();
			const appendix = split < 0 ? '' : exported.markdown.slice(split);
			for (const id of output.resultIds) {
				const result = requireResult(savedResults, id);
				const expected = `## Exact saved source values: ${id}\n\nSaved result ${id} contains ${result.rows.length} rows.\n\n${tableSnapshot(result, result.rows.map((_, i) => i))}`;
				assert.ok(appendix.includes(expected), 'appendix preserves the complete exact saved source snapshot');
			}
			return { ...exported, body };
		}
		function assertState(expected, stage = 'initial') {
			for (const key of ['focused_tab_index', 'follow_up_events', 'follow_up_events_delta']) {
				if (Object.hasOwn(expected, key)) report.deferred_assertions.push({ case: test.id, stage, assertion: key, expected: expected[key], reason: 'requires browser event/lifecycle observation' });
			}
			for (const [key, value] of Object.entries(expected.state ?? {})) assert.deepEqual(state[`control:${key}`], value, `state ${key}`);
			const visible = visibleNodes();
			for (const id of expected.visible_nodes ?? []) assert.ok(visible.has(id), `${id} visible`);
			for (const id of expected.hidden_nodes ?? []) assert.equal(visible.has(id), false, `${id} hidden`);
			if (expected.selected_index !== undefined) {
				const tabs = Object.values(spec.elements).find(element => element.type === 'Tabs');
				assert.ok(tabs);
				assert.equal(state[`tab:${tabs.props.name}`], expected.selected_index);
				assert.equal(tabs.children.filter(id => visible.has(id)).length, 1, 'exactly one panel visible');
			}
			const exported = snapshot();
			if (expected.selected_index !== undefined) {
				const tabs = Object.values(spec.elements).find(element => element.type === 'Tabs');
				assert.ok(exported.body.startsWith(`**${tabs.props.labels[expected.selected_index]}**`), 'export snapshots the selected tab heading');
			}
			for (const id of expected.hidden_nodes ?? []) {
				if (get(id).type === 'Metric') assert.equal(exported.body.includes(`**${get(id).props.title}:**`), false, 'hidden metric is excluded from the presentation snapshot');
			}
			if (expected.table) {
				const table = expected.table;
				assert.equal(get(table.node).type, 'Table');
				assert.ok(visible.has(table.node));
				assert.deepEqual(rowsFor(table.node), table.row_indices, 'exact original saved-row indices');
				if (table.exact_revenue) assert.deepEqual(rowsFor(table.node).map(row => exactValue(bound(table.node), row, 'revenue')), table.exact_revenue);
				const expectedTable = tableSnapshot(bound(table.node), table.row_indices, get(table.node).props.fields);
				assert.equal(exported.body.split('\n').filter(line => line.startsWith('| ')).join('\n'), expectedTable, 'selected display table snapshot excludes other rows; appendix checked separately');
			}
			// Freeze provides a mutation trap; snapshots also detect replacement/mutation.
			assert.equal(JSON.stringify(savedResults), savedBaseline);
			assert.equal(JSON.stringify(spec), specBaseline);
		}
		assertState(test.expected.initial ?? {});
		if (test.expected.table) assertState({ table: test.expected.table });
		if (test.expected.metric) {
			const metric = test.expected.metric;
			assert.deepEqual(get(metric.node).props.resultId, metric.resultId);
			assert.equal(exactValue(bound(metric.node), metric.row, metric.field), metric.exact_value);
			assert.ok(snapshot().body.includes(`**${get(metric.node).props.title}:** ${metric.exact_value}`));
		}
		if (test.expected.chart) {
			const chart = test.expected.chart;
			const element = get(chart.node);
			assert.equal(element.type, 'Chart');
			const result = bound(chart.node);
			const encoding = element.props.spec.encoding;
			if (chart.resultId) assert.equal(result.id, chart.resultId);
			if (chart.row_indices) assert.deepEqual(rowsFor(chart.node), chart.row_indices);
			if (chart.mark) assert.equal(element.props.spec.mark, chart.mark);
			if (chart.months) assert.deepEqual([...new Set(result.rows.map(row => row[result.columns.indexOf(encoding.x.field)]))], chart.months);
			if (chart.series) for (const [series, values] of Object.entries(chart.series)) {
				assert.deepEqual(result.rows.filter(row => row[result.columns.indexOf(encoding.color.field)] === series).map(row => row[result.columns.indexOf(encoding.y.field)]), values);
			}
			if (chart.bar_values) {
				assert.equal(encoding.x.field, chart.category_field);
				assert.equal(encoding.xOffset.field, chart.group_field);
				assert.equal(encoding.color.field, chart.group_field);
				assert.deepEqual(result.rows.map(row => row[result.columns.indexOf(encoding.y.field)]), chart.bar_values);
				assert.equal(encoding.y.aggregate, undefined, 'category fixture invents no total');
			}
			if (chart.composition === 'facet') {
				const field = element.props.spec.facet.field;
				for (const [panel, indices] of Object.entries(chart.panels)) assert.deepEqual(result.rows.flatMap((row, i) => row[result.columns.indexOf(field)] === panel ? [i] : []), indices);
			} else if (chart.composition) assert.equal(element.props.spec[chart.composition].length, chart.child_views);
			const visit = value => {
				if (!value || typeof value !== 'object') return;
				assert.equal(Object.hasOwn(value, 'data'), false, 'only the host supplies chart rows');
				assert.equal(Object.hasOwn(value, 'datasets'), false);
				Object.values(value).forEach(visit);
			};
			visit(element.props.spec);
		}
		for (const step of test.expected.interactions ?? []) {
			if (step.action === 'select' || step.action === 'set_checked') {
				const key = get(step.node).props.stateKey;
				assert.equal(controlValueAllowed(spec, key, step.value), true);
				state = resolvePresentationState(spec, setPresentationValue(state, key, step.value), savedResults);
			} else if (step.action === 'reopen') state = resolvePresentationState(spec, plain(state), savedResults);
			else if (step.action === 'restore_viewer_state') {
				const restored = Object.fromEntries(Object.entries(step.value).map(([key, value]) => [`control:${key}`, value]));
				for (const [key, value] of Object.entries(step.value)) assert.equal(controlValueAllowed(spec, key, value), false, 'invalid restored enum is rejected');
				state = resolvePresentationState(spec, restored, savedResults);
			} else if (step.action === 'click_tab' || step.action === 'press_key') {
				const tabs = get(step.node);
				const key = `tab:${tabs.props.name}`;
				const count = tabs.children.length;
				const current = Number(state[key] ?? 0);
				const next = step.action === 'click_tab' ? step.index : ({ ArrowRight: (current + 1) % count, ArrowLeft: (current - 1 + count) % count, Home: 0, End: count - 1 })[step.key];
				assert.ok(Number.isInteger(next), 'known tab interaction');
				state = resolvePresentationState(spec, { ...state, [key]: next }, savedResults);
			} else if (step.action === 'click') {
				const node = get(step.node);
				const action = evaluatePresentationAction(node.type === 'FollowUp' ? { type: 'follow_up', message: node.props.message } : node.props.action);
				assert.equal(action?.type, 'follow_up');
				assert.equal(action.message, step.expect.message);
			} else assert.fail(`unknown frozen interaction ${step.action}`);
			assertState(step.expect, `${step.action}${step.key ? `:${step.key}` : ''}`);
		}
	});
	check('fixture/results remain immutable throughout evaluation', () => {
		assert.equal(JSON.stringify(fixture), baseline);
		assert.equal(JSON.stringify(savedResults), savedBaseline);
	});

	// Timing repetitions start only after correctness passes; never cycle a failing case.
	if (!failures.length) {
		report.performance.warm_instantiate_ms = [];
		for (let i = 0; i < 5; i++) {
			started = performance.now();
			await WebAssembly.instantiate(module, {});
			report.performance.warm_instantiate_ms.push(performance.now() - started);
		}
		report.performance.warm_acceptance_28_ms = [];
		for (let i = 0; i < 5; i++) {
			started = performance.now();
			const outputs = requests.map(evaluateWasm);
			report.performance.warm_acceptance_28_ms.push(performance.now() - started);
			assert.deepEqual(outputs, wasmOutputs, 'timed acceptance retains the frozen results');
		}
	} else report.performance.warm_measurements = 'not run: correctness failures preserved';
} catch (error) {
	failures.push({ label: 'runner setup or execution', message: error.stack ?? String(error) });
} finally {
	if (server) await server.close();
	report.status = failures.length ? 'failed' : 'passed within declared data scope';
	console.log(JSON.stringify(report, null, 2));
	if (failures.length) process.exitCode = 1;
}
