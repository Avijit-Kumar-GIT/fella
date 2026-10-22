import { test as base, expect, _electron as electron } from '@playwright/test';
import { createServer } from 'node:http';
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

/**
 * Generative workspace contract v1; executable task set v1.5 (2026-10-22).
 * Expected semantics are fixed before candidate execution and come from
 * docs/testing/GENERATIVE-WORKSPACE-v1.md, not observed rendering output.
 * v1.3 makes OpenUI the same-turn primary answer contract and removes the
 * separate presentation-propose request. v1.4 adds the approved local-state
 * interaction and chat persistence path; changing it must make no new model,
 * analytics, or source requests. This suite makes no live-model quality claim.
 *
 * 1. Recorded analysis -> persisted artifact -> Studio; narrative/order survive
 *    restart with exact saved values and no model call or source query.
 * 2. Immutable revisions and pinned placements; stale writes are rejected and
 *    checkpoint restore preserves later history rather than erasing it.
 * 3. A foreign workspace cannot create/resolve/revise/adopt another workspace's
 *    artifact or load/save/restore its document, nor attach foreign placements.
 * 4. Deleting a source chat preserves saved artifacts and their canonical runs.
 * 5. Invalid presentation keeps the original answer and exact values accessible
 *    without gaining execution privileges or changing analytical verification.
 * 6. Deterministic export preserves exact values, null/empty/zero, evidence,
 *    caveats, reading order, and selected state without computation.
 * 7. OpenUI controls update only local presentation state, derived content
 *    responds in place, and the selected state survives reopening the chat.
 * 8. A rejected literal Metric receives one finalizer-only correction turn;
 *    the original analysis is not rerun and the corrected artifact is saved.
 *
 * Independent oracle: literal Q3=10/Q4=20 and integers beyond JS safe range.
 * No hosted credentials, generated model answer, existing test, or app module
 * is used to derive the expected analytical values.
 */
const root = resolve('.');
const question = 'What changed between Q3 and Q4 revenue?';
const caveat = 'Fixture caveat: refunds are included; currency is USD.';
const answer = `Revenue increased from 10 to 20 USD.\n\n${caveat}`;
const columns = ['period', 'revenue', 'zero', 'missing', 'account_id'];
const exactRows = [
	['Q3', '10', '0', 'null', '9007199254740993'],
	['Q4', '20', '0', '', '9007199254740995']
];
const noExecutionMethods = [
	'ask', 'analysis_turn_rerun', 'presentation_propose',
	'investigation_propose_questions', 'run_sql_direct', 'reindex',
	'sample_source', 'preview_file'
];
// Positional syntax confirmed against the Fella OpenUI component schema.
const comparisonProgram = `root = Document([
  Text("Presentation begins with the saved comparison."),
  Section("Quarter comparison", [
    Metric("sales-result", "revenue", 0, "Q3 revenue", "USD"),
    Metric("sales-result", "revenue", 1, "Q4 revenue", "USD"),
    ResultFilter("sales-result", "period", "selected-period", "Period"),
    Chart("sales-result", {"mark":"bar","encoding":{"x":{"field":"period","type":"nominal"},"y":{"field":"revenue","type":"quantitative"}}}, "Revenue by quarter"),
    Table("sales-result")
  ]),
  Text("Presentation ends after the exact values.")
])`;
const secondProgram = 'root = Document([Section("Second saved output", [Text("An independently placed analytical output."), Table("sales-result")])])';

async function invoke(page, command, args = {}) {
	return page.evaluate(({ command, args }) => window.fella.invoke(command, args), { command, args });
}

async function rejected(page, command, args) {
	return page.evaluate(async ({ command, args }) => {
		try {
			await window.fella.invoke(command, args);
			return { rejected: false, message: '' };
		} catch (error) {
			return { rejected: true, message: String(error?.message ?? error) };
		}
	}, { command, args });
}

async function chooseWorkspace(page, label) {
	await page.getByRole('button', { name: /^Switch workspace\. Current:/ }).click();
	await page.getByRole('menuitemradio', { name: new RegExp(`^${label}(?:\\s|$)`) }).click();
	await expect(page.getByRole('button', { name: `Switch workspace. Current: ${label}`, exact: true })).toBeVisible();
}

function engineRequests(profile) {
	return readFile(join(profile, 'engine-requests.jsonl'), 'utf8')
		.then((body) => body.split('\n').filter(Boolean).map((line) => JSON.parse(line)))
		.catch(() => []);
}

async function startBudget(desktop) {
	const prior = await engineRequests(desktop.profile);
	expect(prior.length, 'the Electron main-process observer must record real Rust-engine requests').toBeGreaterThan(0);
	await rm(join(desktop.profile, 'engine-requests.jsonl'), { force: true });
	await writeFile(join(desktop.profile, 'enforce-engine-budget'), '1');
}

async function assertNoExecution(desktop) {
	const requests = await engineRequests(desktop.profile);
	expect(requests.filter(({ method }) => noExecutionMethods.includes(method)), 'saved-work interactions must not invoke a model or live data execution').toEqual([]);
	return requests;
}

async function launch(profile) {
	const engineLog = join(profile, 'engine-requests.jsonl');
	const engineBudget = join(profile, 'enforce-engine-budget');
	const app = await electron.launch({
		args: [join(root, 'tests/e2e/electron-entry.mjs'), `--user-data-dir=${join(profile, 'chromium')}`],
		cwd: root,
		env: {
			...process.env,
			OPENAI_API_KEY: '', ANTHROPIC_API_KEY: '', OLLAMA_API_KEY: '',
			FELLA_DATA_DIR: profile,
			FELLA_ENGINE_PATH: process.env.FELLA_ENGINE_PATH || join(root, 'electron/engine', `fella-engine-${process.arch}${process.platform === 'win32' ? '.exe' : ''}`),
			FELLA_ELECTRON_URL: '',
			FELLA_ACCEPTANCE_LOG: engineLog,
			FELLA_ACCEPTANCE_ENFORCE_FILE: engineBudget,
			FELLA_ACCEPTANCE_FORBIDDEN: JSON.stringify(noExecutionMethods)
		}
	});
	try {
		const page = await app.firstWindow();
		await page.setViewportSize({ width: 1440, height: 960 });
		await page.getByRole('combobox', { name: /^(Ask a question|Ask about )/ }).waitFor();
		return { app, page };
	} catch (error) {
		await app.close();
		throw error;
	}
}

async function seedRun({ page, profile, workspace }, { id = 'gen-saved-run', conversationId = 'gen-revenue-chat', amounts = [10, 20], resultId = 'sales-result', excluded = false, complete = true } = {}) {
	const catalog = await invoke(page, 'get_catalog', { workspaceId: workspace });
	const run = {
		id, conversation_id: conversationId, question,
		workspace, workspace_revision: catalog.revision,
		source_scope: { used_sources: [{ source: 'sales.csv', table: 'sales', evidence_ids: [resultId] }], skipped_sources: [] },
		context_refs: [], prior_turn_refs: [], state: 'accepted',
		plan: { strategy: 'direct_tools', steps: ['Inspect available sales', 'Compare saved quarters'] },
		trace: { id: `${id}-trace`, turn_id: id, workspace_revision: catalog.revision, steps: [] },
		verification: { status: 'verified', checks: [] },
		result: {
			text: answer, status: 'verified', verification: [],
			evidence: [{
				id: resultId, tool: 'run_sql', args: {},
				sql: 'SELECT period, revenue, zero, missing, account_id FROM sales ORDER BY period',
				sources: [{ table: 'sales', source: 'sales.csv' }],
				result_summary: 'Two saved quarter observations; amounts in USD.',
				columns, rows: [
					['Q3', amounts[0], 0, null, '__EXACT_ID_1__'],
					['Q4', amounts[1], 0, '', '__EXACT_ID_2__']
				], row_count: complete ? 2 : 3, ms: 1,
				...(excluded ? { verifier_disposition: { state: 'excluded', reason: 'Fixture evidence is excluded by verification.' } } : {})
			}]
		}
	};
	const directory = join(profile, 'analysis', 'turns');
	await mkdir(directory, { recursive: true });
	// Write raw JSON integers. JS must not round these while seeding the Rust
	// canonical record; exact_rows, not a rounded Number, is the display oracle.
	const raw = JSON.stringify(run)
		.replace('"__EXACT_ID_1__"', '9007199254740993')
		.replace('"__EXACT_ID_2__"', '9007199254740995');
	const path = join(directory, `${id}.json`);
	await writeFile(path, raw);
	return { id, path, raw, resultId };
}

function placement(artifact, id, revision = artifact.current_revision, viewState = {}) {
	return { id, artifact_id: artifact.id, revision, view_state: viewState };
}

async function createArtifact(harness, run, title, program) {
	return invoke(harness.page, 'artifact_create', { workspaceId: harness.workspace, turnId: run.id, title, program });
}

async function createDocument(harness, artifacts, { title = 'Quarterly review', body = 'Independent saved analytical work.' } = {}) {
	const args = { workspaceId: harness.workspace };
	const created = await invoke(harness.page, 'studio_create', { ...args, title });
	return invoke(harness.page, 'studio_save', {
		...args, documentId: created.id, baseVersion: created.version,
		update: { title, body, placements: artifacts.map((artifact, index) => placement(artifact, `placement-${index + 1}`)) }
	});
}

async function assertCanonicalUnchanged(run) {
	expect(await readFile(run.path, 'utf8'), 'presentation and Studio must not rewrite canonical analytical history').toBe(run.raw);
}

async function openStudio(desktop) {
	const { page } = desktop;
	const closed = page.getByRole('button', { name: 'Open workspace tools', exact: true });
	if (await closed.isVisible()) await closed.click();
	const add = page.getByRole('button', { name: 'Add tool tab', exact: true });
	if (await add.isVisible()) await add.click();
	await page.getByTestId('workspace-tool-studio').click();
	const studio = page.getByRole('region', { name: 'Studio', exact: true });
	await expect(studio).toHaveAttribute('aria-busy', 'false');
	return studio;
}

async function openStudioDocument(desktop, doc) {
	const studio = await openStudio(desktop);
	await studio.getByRole('button', { name: 'Choose Studio document', exact: true }).click();
	await desktop.page.getByRole('menuitem', { name: doc.title, exact: true }).click();
	await expect(studio.getByRole('textbox', { name: 'Studio document title', exact: true })).toHaveValue(doc.title);
	return studio;
}

async function addSavedOutput(desktop, studio, title) {
	await studio.getByRole('button', { name: 'Add saved output', exact: true }).click();
	const dialog = desktop.page.getByRole('dialog', { name: 'Add saved output', exact: true });
	await dialog.getByRole('button', { name: `${title} Revision 1`, exact: true }).click();
	await expect(dialog).toBeHidden();
	await expect(studio.getByRole('article', { name: `Saved output: ${title}`, exact: true })).toBeVisible();
}

async function assertSavedRows(output, expected) {
	// A chart has its own collapsed exact-values disclosure. Scope this check to
	// the presentation's top-level Table rather than counting hidden copies.
	const table = output.getByRole('region', { name: 'Saved result values', exact: true }).getByRole('table');
	await expect(table.getByRole('row')).toHaveCount(expected.length + 1);
	for (let index = 0; index < expected.length; index++) {
		await expect(table.getByRole('row').nth(index + 1).getByRole('cell')).toHaveText(expected[index]);
	}
}

// Use the actual Chromium/Electron download, only choosing a disposable output
// path in place of the native save dialog. The exporter itself is not mocked.
async function expectMarkdownDownload({ app, profile }, click, name) {
	const path = join(profile, `${name}.md`);
	await app.evaluate(({ BrowserWindow }, path) => {
		globalThis.__generativeDownload = { state: 'waiting' };
		BrowserWindow.getAllWindows()[0].webContents.session.once('will-download', (_event, item) => {
			item.setSavePath(path);
			item.once('done', (_event, state) => { globalThis.__generativeDownload = { state, filename: item.getFilename() }; });
		});
	}, path);
	await click();
	await expect.poll(() => app.evaluate(() => globalThis.__generativeDownload.state)).toBe('completed');
	return readFile(path, 'utf8');
}

async function expectPackageDownload({ app, profile }, click, name) {
	const path = join(profile, `${name}.fella-report.json`);
	await app.evaluate(({ BrowserWindow }, path) => {
		globalThis.__generativePackageDownload = { state: 'waiting' };
		BrowserWindow.getAllWindows()[0].webContents.session.once('will-download', (_event, item) => {
			item.setSavePath(path);
			item.once('done', (_event, state) => { globalThis.__generativePackageDownload = { state, filename: item.getFilename() }; });
		});
	}, path);
	await click();
	await expect.poll(() => app.evaluate(() => globalThis.__generativePackageDownload.state)).toBe('completed');
	return readFile(path, 'utf8');
}

const test = base.extend({
	desktop: async ({}, use, testInfo) => {
		const profile = await mkdtemp(join(tmpdir(), 'fella-generative-v1-'));
		const workspace = join(profile, 'commerce');
		const otherWorkspace = join(profile, 'personal');
		for (const folder of [workspace, otherWorkspace]) {
			await mkdir(folder);
			await writeFile(join(folder, 'sales.csv'), 'period,revenue\nQ3,10\nQ4,20\n');
			await writeFile(join(folder, 'fella.md'), 'Revenue is expressed in USD. Refunds are included.\n');
		}
		const launched = await launch(profile);
		const desktop = { ...launched, profile, workspace, otherWorkspace };
		try {
			for (const [id, folder, title] of [['gen-revenue-chat', workspace, question], ['gen-personal-chat', otherWorkspace, 'Personal fixture history']]) {
				await invoke(desktop.page, 'archive_conversation', {
					id, body: JSON.stringify({ id, workspace: folder, saved_at_ms: Date.now(), messages: [
						{ id: `${id}-user`, role: 'user', text: title, ts: 1 },
						{ id: `${id}-answer`, role: 'assistant', text: answer, ts: 2 }
					] })
				});
			}
			await desktop.page.reload();
			await chooseWorkspace(desktop.page, 'commerce');
			await desktop.page.getByRole('button', { name: `Open conversation: ${question}`, exact: true }).click();
			await expect(desktop.page.getByRole('log', { name: 'conversation', exact: true })).toContainText(answer);
			desktop.run = await seedRun(desktop);
			await startBudget(desktop);
			await use(desktop);
		} finally {
			if (desktop.app) {
				const requests = await engineRequests(profile);
				await testInfo.attach('engine-requests', { body: JSON.stringify(requests, null, 2), contentType: 'application/json' });
			}
			await testInfo.attach('disposable-profile', { body: profile, contentType: 'text/plain' });
			if (testInfo.status !== testInfo.expectedStatus && desktop.page && !desktop.page.isClosed()) {
				await desktop.page.screenshot({ path: testInfo.outputPath('failure.png') }).catch(() => {});
				await testInfo.attach('visible-state', { body: await desktop.page.locator('body').innerText().catch(() => ''), contentType: 'text/plain' });
			}
			await desktop.app?.close();
		}
	}
});

test('generative v1.4: same-turn OpenUI is interactive and persists local state without rerunning analysis', async ({ desktop }) => {
	test.setTimeout(90_000);
	const { page, workspace, profile } = desktop;
	const modelRequests = [];
	const canonical = 'The workspace contains sales.csv with quarterly revenue data.';
	const program = `$quarter = "Q3"
root = Document([Section("Workspace files", [SegmentedControl("Quarter", [{"value":"Q3","label":"Q3"},{"value":"Q4","label":"Q4"}], $quarter), Text("OPENUI_PRIMARY_SURFACE: " + $quarter + " revenue data from sales.csv.")])])`;
	const server = createServer(async (request, response) => {
		if (request.method !== 'POST' || request.url !== '/v1/chat/completions') {
			response.writeHead(404).end();
			return;
		}
		let body = '';
		for await (const chunk of request) body += chunk;
		const payload = JSON.parse(body);
		modelRequests.push(payload);
		const inspected = payload.messages?.some((message) => message.role === 'assistant' &&
			message.tool_calls?.some((tool) => tool.function?.name === 'list_files'));
		const call = !inspected
			? { id: 'call_inventory', function: { name: 'list_files', arguments: JSON.stringify({ kind: 'all', offset: 0, limit: 40 }) } }
			: { id: 'call_openui_answer', function: { name: 'fella_openui_answer', arguments: JSON.stringify({ answer: canonical, presentation_program: program }) } };
		response.writeHead(200, { 'content-type': 'application/json' }).end(`${JSON.stringify({
			choices: [{ index: 0, message: { role: 'assistant', content: '', tool_calls: [{ type: 'function', ...call }] }, finish_reason: 'tool_calls' }],
			usage: { prompt_tokens: 12, completion_tokens: 8 }
		})}\n`);
	});
	await new Promise((resolve, reject) => {
		server.once('error', reject);
		server.listen(0, '127.0.0.1', resolve);
	});
	const address = server.address();
	expect(address && typeof address === 'object').toBeTruthy();

	try {
		// This journey intentionally performs a real ask, so remove only the
		// fixture's no-model-call guard. The independent request observer stays on.
		await rm(join(profile, 'enforce-engine-budget'), { force: true });
		await invoke(page, 'set_api_key', { provider: 'custom', key: 'generative-e2e-local-key' });
		await invoke(page, 'set_settings', {
			settings: { provider: 'custom', base_url: `http://127.0.0.1:${address.port}/v1`, model: 'fella-openui-e2e' }
		});

		const composer = page.getByRole('combobox', { name: 'Ask about commerce', exact: true });
		await composer.fill('Summarize the files in this workspace.');
		await composer.press('Enter');
		const assistant = page.locator('.msg.assistant').last();
		const primary = assistant.locator('.analytical-answer');
		await expect(primary).toContainText('OPENUI_PRIMARY_SURFACE', { timeout: 30_000 });
		await expect(primary.getByRole('button', { name: /Create interactive view/i })).toHaveCount(0);
		await expect(primary.locator('details.original')).not.toHaveAttribute('open');
		await expect(primary.getByText(canonical, { exact: true })).toBeHidden();
		await expect(primary).toContainText('Analysis · revision 1');
		await expect(primary.getByRole('button', { name: 'Q3', exact: true })).toHaveAttribute('aria-pressed', 'true');
		await primary.getByRole('button', { name: 'Q4', exact: true }).click();
		await expect(primary).toContainText('OPENUI_PRIMARY_SURFACE: Q4 revenue data from sales.csv.');
		await expect(primary.getByRole('button', { name: 'Q4', exact: true })).toHaveAttribute('aria-pressed', 'true');

		const methods = (await engineRequests(profile)).map(({ method }) => method);
		expect(methods.filter((method) => method === 'ask')).toHaveLength(1);
		expect(methods.filter((method) => method === 'artifact_create')).toHaveLength(1);
		expect(methods).not.toContain('presentation_propose');
		expect(modelRequests).toHaveLength(2);
		const requestedTools = modelRequests.map((payload) => payload.tools?.map((tool) => tool.function.name) ?? []);
		expect(requestedTools[0]).toContain('list_files');
		expect(requestedTools[1]).toContain('fella_openui_answer');
		for (const names of requestedTools) expect(names).not.toContain('make_chart');
		const systemPrompt = modelRequests[0].messages.find(({ role }) => role === 'system')?.content ?? '';
		expect(systemPrompt).toContain('Workspace answer format');
		expect(systemPrompt).not.toContain('make_chart');

		const turnId = await assistant.getAttribute('data-analysis-turn-id');
		expect(turnId).toBeTruthy();
		const turn = await invoke(page, 'analysis_turn_load', { turnId });
		expect(turn.result).toMatchObject({ text: canonical, presentation_program: program });
		const artifacts = await invoke(page, 'artifact_list', { workspaceId: workspace });
		expect(artifacts).toHaveLength(1);
		const saved = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifacts[0].id });
		expect(saved.revision.program).toBe(program);
		expect(saved.run.result.text).toBe(canonical);
		await page.reload();
		await chooseWorkspace(page, 'commerce');
		await page.getByRole('button', { name: `Open conversation: ${question}`, exact: true }).click();
		const reopened = page.locator('.msg.assistant').last().locator('.analytical-answer');
		await expect(reopened).toContainText('OPENUI_PRIMARY_SURFACE: Q4 revenue data from sales.csv.');
		await expect(reopened.getByRole('button', { name: 'Q4', exact: true })).toHaveAttribute('aria-pressed', 'true');
		expect(modelRequests).toHaveLength(2);
		expect((await engineRequests(profile)).filter(({ method }) => method === 'ask')).toHaveLength(1);
	} finally {
		await test.info().attach('mock-provider-requests', {
			body: JSON.stringify(modelRequests.map(({ tools, messages }) => ({
				tools: tools?.map((tool) => tool.function.name) ?? [],
				messages: messages?.map(({ role, content, tool_calls, name }) => ({ role, content, tool_calls, name })) ?? []
			})), null, 2),
			contentType: 'application/json'
		});
		await writeFile(join(profile, 'mock-provider-requests.json'), JSON.stringify(modelRequests, null, 2));
		await new Promise((resolve) => server.close(resolve));
	}
});

test('generative v1.5: rejected literal Metric gets finalizer-only repair without rerunning analysis', async ({ desktop }) => {
	test.setTimeout(90_000);
	const { page, workspace, profile } = desktop;
	const modelRequests = [];
	const canonical = 'The workspace contains sales.csv with quarterly revenue data.';
	const correctedProgram = 'root = Document([Section("Workspace summary", [Text("CORRECTED_OPENUI_FINALIZER: sales.csv contains quarterly revenue data.")])])';
	const server = createServer(async (request, response) => {
		if (request.method !== 'POST' || request.url !== '/v1/chat/completions') {
			response.writeHead(404).end();
			return;
		}
		let body = '';
		for await (const chunk of request) body += chunk;
		const payload = JSON.parse(body);
		modelRequests.push(payload);
		const call = modelRequests.length === 1
			? { id: 'call_repair_inventory', function: { name: 'list_files', arguments: JSON.stringify({ kind: 'all', offset: 0, limit: 40 }) } }
			: modelRequests.length === 2
				? { id: 'call_invalid_metric', function: { name: 'fella_openui_answer', arguments: JSON.stringify({
					answer: canonical,
					presentation_program: 'root = Document([Metric(null, null, null, "Q4 revenue", "USD", 80)])'
				}) } }
				: { id: 'call_corrected_openui', function: { name: 'fella_openui_answer', arguments: JSON.stringify({ answer: canonical, presentation_program: correctedProgram }) } };
		response.writeHead(200, { 'content-type': 'application/json' }).end(`${JSON.stringify({
			choices: [{ index: 0, message: { role: 'assistant', content: '', tool_calls: [{ type: 'function', ...call }] }, finish_reason: 'tool_calls' }],
			usage: { prompt_tokens: 12, completion_tokens: 8 }
		})}\n`);
	});
	await new Promise((resolve, reject) => {
		server.once('error', reject);
		server.listen(0, '127.0.0.1', resolve);
	});
	const address = server.address();
	expect(address && typeof address === 'object').toBeTruthy();

	try {
		await rm(join(profile, 'enforce-engine-budget'), { force: true });
		await invoke(page, 'set_api_key', { provider: 'custom', key: 'generative-e2e-local-key' });
		await invoke(page, 'set_settings', {
			settings: { provider: 'custom', base_url: `http://127.0.0.1:${address.port}/v1`, model: 'fella-openui-repair-e2e' }
		});

		const composer = page.getByRole('combobox', { name: 'Ask about commerce', exact: true });
		await composer.fill('Summarize what files are available in this workspace.');
		await composer.press('Enter');
		const assistant = page.locator('.msg.assistant').last();
		const primary = assistant.locator('.analytical-answer');
		await expect(primary).toContainText('CORRECTED_OPENUI_FINALIZER', { timeout: 30_000 });

		expect(modelRequests).toHaveLength(3);
		const repairMessages = modelRequests[2].messages.filter(({ role }) => role === 'tool');
		expect(repairMessages.some(({ content }) => /Metric values cannot be copied into the presentation/.test(content ?? ''))).toBe(true);
		expect(modelRequests[2].tools.map((tool) => tool.function.name)).toEqual(['fella_openui_answer']);
		const methods = (await engineRequests(profile)).map(({ method }) => method);
		expect(methods.filter((method) => method === 'ask')).toHaveLength(1);
		expect(methods.filter((method) => method === 'artifact_create')).toHaveLength(1);
		expect(methods).not.toContain('run_sql_direct');

		const turnId = await assistant.getAttribute('data-analysis-turn-id');
		expect(turnId).toBeTruthy();
		const turn = await invoke(page, 'analysis_turn_load', { turnId });
		expect(turn.result).toMatchObject({ text: canonical, presentation_program: correctedProgram });
		const artifacts = await invoke(page, 'artifact_list', { workspaceId: workspace });
		expect(artifacts).toHaveLength(1);
		const saved = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifacts[0].id });
		expect(saved.revision.program).toBe(correctedProgram);
	} finally {
		await test.info().attach('mock-provider-requests', {
			body: JSON.stringify(modelRequests.map(({ tools, messages }) => ({
				tools: tools?.map((tool) => tool.function.name) ?? [],
				messages: messages?.map(({ role, content, tool_calls, name }) => ({ role, content, tool_calls, name })) ?? []
			})), null, 2),
			contentType: 'application/json'
		});
		await new Promise((resolve) => server.close(resolve));
	}
});

test('generative v1: immutable revisions, pinned placements, optimistic writes, and checkpoints', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	// Blank programs are intentional for this storage-only contract. Rendering
	// syntax is exercised by the separate UI journeys, not invented here.
	const artifact = await createArtifact(desktop, run, 'Revenue baseline', '');
	const original = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id });
	expect(original.results).toHaveLength(1);
	expect(original.results[0]).toMatchObject({ id: 'sales-result', columns, exact_rows: exactRows, complete: true, row_count: 2 });
	const doc = await createDocument(desktop, [artifact], { body: 'Before the presentation edit.' });
	const checkpointed = await invoke(page, 'studio_checkpoint', { workspaceId: workspace, documentId: doc.id, baseVersion: doc.version, title: 'Original review' });
	expect(checkpointed.checkpoints).toHaveLength(1);
	const revision = await invoke(page, 'artifact_revise', {
		workspaceId: workspace, artifactId: artifact.id, baseRevision: artifact.current_revision,
		program: '', changeNote: 'Presentation-only revision; no new calculation.'
	});
	expect(revision.artifact.current_revision).toBe(artifact.current_revision);
	expect(revision.artifact.revisions).toHaveLength(2);
	const candidate = revision.revision;
	expect(candidate.run_id).toBe(run.id);
	expect(candidate.parent_revision).toBe(artifact.current_revision);
	const adopted = await invoke(page, 'artifact_adopt', { workspaceId: workspace, artifactId: artifact.id, baseRevision: artifact.current_revision, revision: candidate.revision });
	expect(adopted.artifact.current_revision).toBe(candidate.revision);
	for (const command of ['artifact_revise', 'artifact_adopt']) {
		const failure = await rejected(page, command, {
			workspaceId: workspace, artifactId: artifact.id, baseRevision: artifact.current_revision,
			revision: artifact.current_revision, program: '', changeNote: 'Stale mutation must fail.'
		});
		expect(failure.rejected, `${command} must reject a stale adopted base`).toBe(true);
		expect(failure.message).toMatch(/changed|reload|conflict|stale/i);
	}
	const pinned = await invoke(page, 'studio_load', { workspaceId: workspace, documentId: doc.id });
	expect(pinned.placements[0].revision).toBe(artifact.current_revision);
	const changed = await invoke(page, 'studio_save', {
		workspaceId: workspace, documentId: doc.id, baseVersion: pinned.version,
		update: { title: doc.title, body: 'After the presentation edit.', placements: [placement(adopted.artifact, 'placement-1')] }
	});
	expect((await rejected(page, 'studio_save', {
		workspaceId: workspace, documentId: doc.id, baseVersion: pinned.version,
		update: { title: doc.title, body: 'A stale write must not win.', placements: [] }
	})).rejected).toBe(true);
	const afterCheckpoint = await invoke(page, 'studio_checkpoint', { workspaceId: workspace, documentId: doc.id, baseVersion: changed.version, title: 'Revised review' });
	const restored = await invoke(page, 'studio_restore', {
		workspaceId: workspace, documentId: doc.id, baseVersion: afterCheckpoint.version,
		checkpointId: checkpointed.checkpoints[0].id
	});
	expect(restored.body).toBe('Before the presentation edit.');
	expect(restored.placements).toEqual(doc.placements);
	expect(restored.version).toBeGreaterThan(afterCheckpoint.version);
	expect(restored.checkpoints.map(item => item.title)).toEqual(['Original review', 'Revised review']);
	const latest = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id });
	expect(latest.artifact.revisions).toHaveLength(2);
	expect(latest.artifact.current_revision).toBe(candidate.revision);
	expect(latest.results[0].exact_rows).toEqual(exactRows);
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
});

test('generative v1: workspace ownership cannot be bypassed by artifact or Studio IDs', async ({ desktop }) => {
	const { app, page, workspace, otherWorkspace, run } = desktop;
	const artifact = await createArtifact(desktop, run, 'Private commerce result', '');
	const doc = await createDocument(desktop, [artifact]);
	const checkpointed = await invoke(page, 'studio_checkpoint', { workspaceId: workspace, documentId: doc.id, baseVersion: doc.version, title: 'Commerce checkpoint' });
	const foreign = { workspaceId: otherWorkspace };
	const attempts = [
		['artifact_create', { ...foreign, turnId: run.id, title: 'Stolen run', program: '' }],
		['artifact_resolve', { ...foreign, artifactId: artifact.id }],
		['artifact_revise', { ...foreign, artifactId: artifact.id, baseRevision: artifact.current_revision, program: '', changeNote: 'Foreign revision' }],
		['artifact_adopt', { ...foreign, artifactId: artifact.id, baseRevision: artifact.current_revision, revision: artifact.current_revision }],
		['studio_load', { ...foreign, documentId: doc.id }],
		['studio_save', { ...foreign, documentId: doc.id, baseVersion: checkpointed.version, update: { title: 'Stolen document', body: '', placements: [] } }],
		['studio_checkpoint', { ...foreign, documentId: doc.id, baseVersion: checkpointed.version, title: 'Foreign checkpoint' }],
		['studio_restore', { ...foreign, documentId: doc.id, baseVersion: checkpointed.version, checkpointId: checkpointed.checkpoints[0].id }]
	];
	for (const [command, args] of attempts) {
		const failure = await rejected(page, command, args);
		expect(failure.rejected, `${command} must enforce the requested workspace, not only the record ID`).toBe(true);
		expect(failure.message).not.toContain('9007199254740993');
	}
	expect(await invoke(page, 'artifact_list', foreign)).toEqual([]);
	expect(await invoke(page, 'studio_list', foreign)).toEqual([]);
	const otherDocument = await invoke(page, 'studio_create', { ...foreign, title: 'Personal notes' });
	expect((await rejected(page, 'studio_save', {
		...foreign, documentId: otherDocument.id, baseVersion: otherDocument.version,
		update: { title: otherDocument.title, body: 'Must not attach foreign results.', placements: [placement(artifact, 'foreign-placement')] }
	})).rejected).toBe(true);
	const untouched = await invoke(page, 'studio_load', { ...foreign, documentId: otherDocument.id });
	expect(untouched.version).toBe(otherDocument.version);
	expect(untouched.placements).toEqual([]);
	expect(await invoke(page, 'studio_load', { workspaceId: workspace, documentId: doc.id })).toEqual(checkpointed);
	const excluded = await seedRun(desktop, { id: 'gen-excluded-run', resultId: 'excluded-result', excluded: true });
	const eligible = await invoke(page, 'presentation_results', { workspaceId: workspace, turnId: excluded.id });
	expect(eligible.results, 'verifier-excluded evidence must not enter presentation bindings').toEqual([]);
	const partial = await seedRun(desktop, { id: 'gen-partial-run', complete: false });
	const partialBindings = await invoke(page, 'presentation_results', { workspaceId: workspace, turnId: partial.id });
	expect(partialBindings.results[0]).toMatchObject({ complete: false, row_count: 3, exact_rows: exactRows });
	await assertCanonicalUnchanged(excluded);
	await assertCanonicalUnchanged(partial);
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
});

test('generative v1: deleting a chat retains saved analytical work but not unrelated unsaved runs', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	const unsaved = await seedRun(desktop, { id: 'gen-unsaved-run' });
	const artifact = await createArtifact(desktop, run, 'Retained revenue comparison', comparisonProgram);
	const doc = await createDocument(desktop, [artifact], { title: 'Retained review' });
	const row = page.locator('.item-wrap').filter({ has: page.getByRole('button', { name: `Open conversation: ${question}`, exact: true }) });
	await row.hover();
	const remove = row.getByRole('button', { name: 'Delete conversation', exact: true });
	await remove.click();
	const confirmation = page.getByRole('dialog', { name: 'Delete conversation?', exact: true });
	await expect(confirmation).toContainText(/saved Studio outputs, their supporting analysis records, and revisions will remain available/i);
	await confirmation.getByRole('button', { name: 'Delete conversation', exact: true }).click();
	await expect(page.getByRole('button', { name: `Open conversation: ${question}`, exact: true })).toHaveCount(0);
	expect((await invoke(page, 'conversations_list')).some(chat => chat.id === 'gen-revenue-chat')).toBe(false);
	const retained = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id });
	expect(retained.artifact.saved).toBe(true);
	expect(retained.run.id).toBe(run.id);
	expect(retained.run.question).toBe(question);
	expect(retained.run.result.text).toBe(answer);
	expect(retained.results[0].exact_rows).toEqual(exactRows);
	expect((await invoke(page, 'studio_load', { workspaceId: workspace, documentId: doc.id })).placements).toEqual(doc.placements);
	await assertCanonicalUnchanged(run);
	await expect(readFile(unsaved.path, 'utf8')).rejects.toMatchObject({ code: 'ENOENT' });
	await page.reload();
	expect((await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id })).results[0].exact_rows).toEqual(exactRows);
	await page.screenshot({ path: test.info().outputPath('saved-work-after-chat-deletion.png') });
	await assertNoExecution(desktop);
});

test('generative v1: invalid presentation falls back to saved answer and exact values without execution authority', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	const invalidProgram = 'root = UnregisteredAnalyticalWidget("window.fella.invoke(\\\"run_sql_direct\\\")");';
	const artifact = await createArtifact(desktop, run, 'Unsupported presentation', invalidProgram);
	const missingResult = await createArtifact(desktop, run, 'Missing result reference', 'root = Document([Table("invented-result")])');
	const missingField = await createArtifact(desktop, run, 'Missing field reference', 'root = Document([Metric("sales-result", "absent-field", 0, "Unavailable metric", "USD")])');
	const doc = await createDocument(desktop, [artifact, missingResult, missingField], { title: 'Fallback review' });
	await openStudioDocument(desktop, doc);
	const output = page.getByRole('article', { name: 'Saved output: Unsupported presentation', exact: true });
	for (const title of ['Unsupported presentation', 'Missing result reference', 'Missing field reference']) {
		await expect(page.getByRole('article', { name: `Saved output: ${title}`, exact: true }).getByRole('status').filter({ hasText: /presentation.*(could not|unavailable|not.*displayed)/i })).toBeVisible();
	}
	await expect(output.getByRole('region', { name: 'Saved result values', exact: true }).getByRole('table').getByRole('row').nth(1).getByRole('cell')).toHaveText(exactRows[0]);
	await output.getByText('Original saved answer', { exact: true }).click();
	await expect(output).toContainText(answer);
	await expect(output.locator('script, iframe, object, embed')).toHaveCount(0);
	await page.reload();
	await expect(page.getByRole('article', { name: 'Saved output: Unsupported presentation', exact: true })).toBeVisible();
	const resolved = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifact.id });
	expect(resolved.revision.program).toBe(invalidProgram);
	expect(resolved.run.verification.status).toBe('verified');
	expect(resolved.run.result.status).toBe('verified');
	expect(resolved.run.result.text).toBe(answer);
	expect(resolved.results[0].exact_rows).toEqual(exactRows);
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
	await page.screenshot({ path: test.info().outputPath('invalid-presentation-fallback.png') });
});

test('generative v1: recorded analysis to Studio, user edits and keyboard reorder survive a desktop restart', async ({ desktop }) => {
	const { page, workspace, profile, run } = desktop;
	const baselineFiles = (await readdir(join(profile, 'analysis', 'turns'))).sort();
	const first = await createArtifact(desktop, run, 'Revenue comparison', comparisonProgram);
	const second = await createArtifact(desktop, run, 'Supporting table', secondProgram);
	const studio = await openStudio(desktop);
	await studio.getByRole('button', { name: 'New document', exact: true }).click();
	const create = page.getByRole('dialog', { name: 'New Studio document', exact: true });
	await create.getByRole('textbox', { name: 'New Studio document name', exact: true }).fill('Quarterly working document');
	await create.getByRole('button', { name: 'Create document', exact: true }).click();
	await expect(create).toBeHidden();
	await addSavedOutput(desktop, studio, first.title);
	await addSavedOutput(desktop, studio, second.title);
	const comparison = studio.getByRole('article', { name: 'Saved output: Revenue comparison', exact: true });
	await expect(comparison.getByRole('heading', { name: 'Quarter comparison', exact: true })).toBeVisible();
	await expect(comparison.getByRole('img', { name: 'Revenue by quarter', exact: true }).locator('svg, canvas')).toBeVisible();
	await assertSavedRows(comparison, exactRows);
	const notes = '## Review notes\n\nKeep the supporting evidence before the main comparison.';
	await studio.getByRole('button', { name: 'Add notes', exact: true }).click();
	await studio.getByRole('textbox', { name: 'Studio notes', exact: true }).fill(notes);
	await studio.getByRole('button', { name: 'Done', exact: true }).click();
	// The same move control must work without a pointer or drag operation.
	const move = studio.getByRole('button', { name: 'Move output 2 up', exact: true });
	await move.focus();
	await move.press('Enter');
	await comparison.locator('summary').filter({ hasText: /^All saved values/ }).click();
	await comparison.getByRole('group', { name: 'Period', exact: true }).getByRole('button', { name: 'Q4', exact: true }).click();
	await assertSavedRows(comparison, [exactRows[1]]);
	const [doc] = await invoke(page, 'studio_list', { workspaceId: workspace });
	await expect.poll(async () => {
		const saved = await invoke(page, 'studio_load', { workspaceId: workspace, documentId: doc.id });
		return {
			body: saved.body,
			order: saved.placements.map(item => item.artifact_id),
			selection: saved.placements.find(item => item.artifact_id === first.id)?.view_state?.['filter:selected-period']
		};
	}).toEqual({ body: notes, order: [second.id, first.id], selection: { resultId: 'sales-result', field: 'period', value: 'Q4' } });
	await assertCanonicalUnchanged(run);
	const beforeRestart = await assertNoExecution(desktop);
	await test.info().attach('before-restart-engine-requests', { body: JSON.stringify(beforeRestart, null, 2), contentType: 'application/json' });
	await page.screenshot({ path: test.info().outputPath('studio-before-restart.png') });
	await desktop.app.close();
	desktop.app = null;
	Object.assign(desktop, await launch(profile));
	await startBudget(desktop);
	// This reload is observed from its first renderer request, so the zero-call
	// claim does not depend on a race attaching instrumentation during startup.
	await desktop.page.reload();
	const reopened = desktop.page.getByRole('region', { name: 'Studio', exact: true });
	await expect(reopened.getByRole('textbox', { name: 'Studio document title', exact: true })).toHaveValue('Quarterly working document');
	await expect(reopened.getByRole('region', { name: 'Document notes', exact: true })).toContainText('Keep the supporting evidence before the main comparison.');
	await expect.poll(() => reopened.getByRole('list', { name: 'Studio outputs', exact: true }).getByRole('article').evaluateAll(elements => elements.map(element => element.getAttribute('aria-label')))).toEqual(['Saved output: Supporting table', 'Saved output: Revenue comparison']);
	await assertSavedRows(reopened.getByRole('article', { name: 'Saved output: Revenue comparison', exact: true }), [exactRows[1]]);
	expect((await readdir(join(profile, 'analysis', 'turns'))).sort()).toEqual(baselineFiles);
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
	await desktop.page.setViewportSize({ width: 960, height: 680 });
	await expect(reopened.getByRole('textbox', { name: 'Studio document title', exact: true })).toBeInViewport();
	await expect(reopened.getByRole('button', { name: 'Export', exact: true })).toBeInViewport();
	await desktop.page.screenshot({ path: test.info().outputPath('studio-reopened-compact.png') });
});

test('generative v1: Markdown download preserves exact data, selected state, evidence, and presentation reading order', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	const first = await createArtifact(desktop, run, 'Revenue comparison', comparisonProgram);
	const second = await createArtifact(desktop, run, 'Supporting table', secondProgram);
	const created = await createDocument(desktop, [second, first], { title: 'Quarterly export', body: '## Export introduction\n\nA user-authored report, not a regenerated answer.' });
	const doc = await invoke(page, 'studio_save', {
		workspaceId: workspace, documentId: created.id, baseVersion: created.version,
		update: {
			title: created.title, body: created.body,
			placements: created.placements.map(item => ({ ...item, view_state: item.artifact_id === first.id ? { 'filter:selected-period': { resultId: 'sales-result', field: 'period', value: 'Q4' } } : {} }))
		}
	});
	const studio = await openStudioDocument(desktop, doc);
	await expect(studio.getByRole('article', { name: 'Saved output: Revenue comparison', exact: true }).getByRole('heading', { name: 'Quarter comparison', exact: true })).toBeVisible();
	const markdown = await expectMarkdownDownload(desktop, () => studio.getByRole('button', { name: 'Export', exact: true }).click(), 'quarterly-export-first');
	const secondDownload = await expectMarkdownDownload(desktop, () => studio.getByRole('button', { name: 'Export', exact: true }).click(), 'quarterly-export-second');
	expect(secondDownload, 'exporting the unchanged document must be deterministic').toBe(markdown);
	expect(markdown).toMatch(/^# Quarterly export\n/);
	expect(markdown.indexOf('## Export introduction')).toBeLessThan(markdown.indexOf('## Supporting table'));
	expect(markdown.indexOf('## Supporting table')).toBeLessThan(markdown.indexOf('## Revenue comparison'));
	for (const text of [
		'| Q3 | 10 | 0 | null | 9007199254740993 |',
		'| Q4 | 20 | 0 |  | 9007199254740995 |',
		caveat, 'USD', question, 'sales.csv', 'sales-result', run.id,
		JSON.parse(run.raw).workspace_revision,
		'SELECT period, revenue, zero, missing, account_id FROM sales ORDER BY period',
		'"value": "Q4"'
	]) expect(markdown, `export must retain ${JSON.stringify(text)}`).toContain(text);
	expect(markdown).not.toContain('9007199254740992');
	expect(markdown).not.toContain('9007199254740996');
	expect(markdown).not.toContain(workspace);
	expect(markdown).not.toContain('Personal fixture history');
	// A fenced copy of the OpenUI program is not a converted report. Require the
	// actual snapshot section, including a supported text figure for the chart.
	const firstSection = markdown.indexOf('## Revenue comparison');
	const snapshotStart = markdown.indexOf('### Presentation snapshot', firstSection);
	expect(snapshotStart).toBeGreaterThan(firstSection);
	const snapshotEnd = markdown.indexOf('Presentation source', snapshotStart);
	expect(snapshotEnd).toBeGreaterThan(snapshotStart);
	const snapshot = markdown.slice(snapshotStart, snapshotEnd);
	expect(snapshot).toContain('Quarter comparison');
	expect(snapshot).toContain('Q3 revenue');
	expect(snapshot).toContain('Q4 revenue');
	expect(snapshot).toContain('```text');
	expect(snapshot).toMatch(/Q4:.*20/);
	expect(snapshot).not.toContain('Q3:');
	expect(snapshot.indexOf('Presentation begins')).toBeLessThan(snapshot.indexOf('Quarter comparison'));
	expect(snapshot.indexOf('Quarter comparison')).toBeLessThan(snapshot.indexOf('Presentation ends'));
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
	await test.info().attach('exported-markdown', { body: markdown, contentType: 'text/markdown' });
});

test('generative v1: portable report snapshots reopen as read-only saved records', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	const artifact = await createArtifact(desktop, run, 'Portable comparison', secondProgram);
	const doc = await createDocument(desktop, [artifact], { title: 'Portable quarterly report', body: 'A user-authored introduction.' });
	const studio = await openStudioDocument(desktop, doc);
	const json = await expectPackageDownload(desktop, () => studio.getByRole('button', { name: 'Export portable report snapshot', exact: true }).click(), 'portable-report');
	const packageData = JSON.parse(json);
	expect(packageData.format).toBe('fella-report');
	expect(packageData.version).toBe(1);
	expect(packageData.document.title).toBe('Portable quarterly report');
	expect(packageData.runs[0].results[0].exact_rows).toEqual(exactRows);
	expect(json).not.toContain(workspace);
	await assertNoExecution(desktop);

	await studio.getByRole('button', { name: 'Reports', exact: true }).click();
	const manager = page.getByRole('dialog', { name: 'Report snapshots', exact: true });
	await page.locator('input[aria-label="Choose a Fella report snapshot"]').setInputFiles({
		name: 'portable.fella-report.json',
		mimeType: 'application/vnd.fella.report+json',
		buffer: Buffer.from(json)
	});
	await expect(manager.getByRole('heading', { name: 'Portable quarterly report', exact: true })).toBeVisible();
	await expect(manager).toContainText('read-only snapshot');
	await manager.getByRole('button', { name: 'Import read-only snapshot', exact: true }).click();
	const report = page.getByRole('article', { name: 'Read-only report: Portable quarterly report', exact: true });
	await expect(report).toContainText(question);
	await expect(report).toContainText('9007199254740993');
	await expect(report).toContainText('This imported report cannot query its original files.');
	await assertNoExecution(desktop);
	await page.getByRole('dialog', { name: 'Portable quarterly report', exact: true }).getByRole('button', { name: 'Close imported report', exact: true }).click();
	await studio.getByRole('button', { name: 'Reports', exact: true }).click();
	await manager.getByRole('button', { name: /Portable quarterly report.*Open/ }).click();
	await expect(page.getByRole('article', { name: 'Read-only report: Portable quarterly report', exact: true })).toContainText('9007199254740993');
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
});
