import { createServer } from 'node:http';
import { rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { test, expect, question, caveat, answer, columns, exactRows, comparisonSpec, secondSpec, invoke, rejected, chooseWorkspace, engineRequests, startBudget, assertNoExecution, launch, seedRun, placement, createArtifact, createDocument, assertCanonicalUnchanged, openStudio, openStudioDocument, addSavedOutput, assertSavedRows, expectMarkdownDownload, expectPackageDownload } from './generative-workspace-harness.mjs';

/**
 * Organization-only relocation map (2026-10-22), original task set v1.5.
 * Keep: same-turn json-render/state, finalizer-only correction, invalid UI fallback,
 * Markdown conversion/export, portable report export/reopening.
 * Move unchanged: immutable revisions/checkpoints, workspace ownership, chat
 * deletion retention, Studio edits/reordering/restart. Shared setup and helpers
 * live in generative-workspace-harness.mjs. Test titles/bodies/assertions are
 * unchanged; this split is not a new baseline or an exclusion of app coverage.
 */

test('generative v2.0: same-turn json-render answer is interactive and persists local state without rerunning analysis', async ({ desktop }) => {
	test.setTimeout(90_000);
	const { page, workspace, profile } = desktop;
	const modelRequests = [];
	const canonical = 'The workspace contains sales.csv with quarterly revenue data.';
	const spec = {
		root: 'document',
		elements: {
			document: { type: 'Document', props: {}, children: ['intro', 'views'] },
			intro: { type: 'Text', props: { content: 'JSON_RENDER_PRIMARY_SURFACE: workspace file summary.' }, children: [] },
			views: { type: 'Tabs', props: { name: 'Quarter', labels: ['Q3', 'Q4'] }, children: ['q3-panel', 'q4-panel'] },
			'q3-panel': { type: 'TabPanel', props: { name: 'Quarter', index: 0 }, children: ['q3-text'] },
			'q3-text': { type: 'Text', props: { content: 'Q3_VIEW_CONTENT' }, children: [] },
			'q4-panel': { type: 'TabPanel', props: { name: 'Quarter', index: 1 }, children: ['q4-text'] },
			'q4-text': { type: 'Text', props: { content: 'Q4_VIEW_CONTENT' }, children: [] }
		}
	};
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
			: { id: 'call_json_render_answer', function: { name: 'fella_presentation_answer', arguments: JSON.stringify({ answer: canonical, presentation_spec: spec }) } };
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
			settings: { provider: 'custom', base_url: `http://127.0.0.1:${address.port}/v1`, model: 'fella-json-render-e2e' }
		});

		const composer = page.getByRole('combobox', { name: 'Ask about commerce', exact: true });
		await composer.fill('Summarize the files in this workspace.');
		await composer.press('Enter');
		const assistant = page.locator('.msg.assistant').last();
		const primary = assistant.locator('.analytical-answer');
		await expect(primary).toContainText('JSON_RENDER_PRIMARY_SURFACE', { timeout: 30_000 });
		await expect(primary.getByRole('button', { name: /Create interactive view/i })).toHaveCount(0);
		await expect(primary.locator('details.original')).not.toHaveAttribute('open');
		await expect(primary.getByText(canonical, { exact: true })).toBeHidden();
		await expect(primary).toContainText('Analysis · revision 1');
		const tabs = primary.getByRole('tablist', { name: 'Quarter', exact: true });
		await expect(primary.getByRole('tabpanel')).toContainText('Q3_VIEW_CONTENT');
		await tabs.getByRole('tab', { name: 'Q4', exact: true }).click();
		await expect(primary.getByRole('tabpanel')).toContainText('Q4_VIEW_CONTENT');
		await expect(tabs.getByRole('tab', { name: 'Q4', exact: true })).toHaveAttribute('aria-selected', 'true');

		const methods = (await engineRequests(profile)).map(({ method }) => method);
		expect(methods.filter((method) => method === 'ask')).toHaveLength(1);
		expect(methods.filter((method) => method === 'artifact_create')).toHaveLength(1);
		expect(modelRequests).toHaveLength(2);
		const requestedTools = modelRequests.map((payload) => payload.tools?.map((tool) => tool.function.name) ?? []);
		expect(requestedTools[0]).toContain('list_files');
		expect(requestedTools[1]).toContain('fella_presentation_answer');
		for (const names of requestedTools) expect(names).not.toContain('make_chart');
		const systemPrompt = modelRequests[0].messages.find(({ role }) => role === 'system')?.content ?? '';
		expect(systemPrompt).toContain('Workspace answer format');
		expect(systemPrompt).not.toContain('make_chart');

		const turnId = await assistant.getAttribute('data-analysis-turn-id');
		expect(turnId).toBeTruthy();
		const turn = await invoke(page, 'analysis_turn_load', { turnId });
		expect(turn.result).toMatchObject({ text: canonical, presentation_spec: spec });
		const artifacts = await invoke(page, 'artifact_list', { workspaceId: workspace });
		expect(artifacts).toHaveLength(1);
		const saved = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifacts[0].id });
		expect(saved.revision.spec).toEqual(spec);
		expect(saved.run.result.text).toBe(canonical);
		await page.reload();
		await chooseWorkspace(page, 'commerce');
		await page.getByRole('button', { name: `Open conversation: ${question}`, exact: true }).click();
		const reopened = page.locator('.msg.assistant').last().locator('.analytical-answer');
		await expect(reopened).toContainText('JSON_RENDER_PRIMARY_SURFACE: workspace file summary.');
		await expect(reopened.getByRole('tabpanel')).toContainText('Q4_VIEW_CONTENT');
		await expect(reopened.getByRole('tablist', { name: 'Quarter', exact: true }).getByRole('tab', { name: 'Q4', exact: true })).toHaveAttribute('aria-selected', 'true');
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

test('generative v2.0: rejected component tree gets finalizer-only correction without rerunning analysis', async ({ desktop }) => {
	test.setTimeout(90_000);
	const { page, workspace, profile } = desktop;
	const modelRequests = [];
	const canonical = 'The workspace contains sales.csv with quarterly revenue data.';
	const correctedSpec = { root: 'doc', elements: { doc: { type: 'Document', props: {}, children: ['body'] }, body: { type: 'Text', props: { content: 'CORRECTED_JSON_RENDER_FINALIZER: sales.csv contains quarterly revenue data.' }, children: [] } } };
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
				? { id: 'call_invalid_component', function: { name: 'fella_presentation_answer', arguments: JSON.stringify({
					answer: canonical,
					presentation_spec: { root: 'doc', elements: { doc: { type: 'Document', props: {}, children: ['bad'] }, bad: { type: 'UnregisteredWidget', props: {}, children: [] } } }
				}) } }
				: { id: 'call_corrected_json_render', function: { name: 'fella_presentation_answer', arguments: JSON.stringify({ answer: canonical, presentation_spec: correctedSpec }) } };
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
			settings: { provider: 'custom', base_url: `http://127.0.0.1:${address.port}/v1`, model: 'fella-json-render-correction-e2e' }
		});

		const composer = page.getByRole('combobox', { name: 'Ask about commerce', exact: true });
		await composer.fill('Summarize what files are available in this workspace.');
		await composer.press('Enter');
		const assistant = page.locator('.msg.assistant').last();
		const primary = assistant.locator('.analytical-answer');
		await expect(primary).toContainText('CORRECTED_JSON_RENDER_FINALIZER', { timeout: 30_000 });

		expect(modelRequests).toHaveLength(3);
		const repairMessages = modelRequests[2].messages.filter(({ role }) => role === 'tool');
		expect(repairMessages.some(({ content }) => /component outside the registered catalog/i.test(content ?? ''))).toBe(true);
		expect(modelRequests[2].tools.map((tool) => tool.function.name)).toEqual(['fella_presentation_answer']);
		const methods = (await engineRequests(profile)).map(({ method }) => method);
		expect(methods.filter((method) => method === 'ask')).toHaveLength(1);
		expect(methods.filter((method) => method === 'artifact_create')).toHaveLength(1);
		expect(methods).not.toContain('run_sql_direct');

		const turnId = await assistant.getAttribute('data-analysis-turn-id');
		expect(turnId).toBeTruthy();
		const turn = await invoke(page, 'analysis_turn_load', { turnId });
		expect(turn.result).toMatchObject({ text: canonical, presentation_spec: correctedSpec });
		const artifacts = await invoke(page, 'artifact_list', { workspaceId: workspace });
		expect(artifacts).toHaveLength(1);
		const saved = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: artifacts[0].id });
		expect(saved.revision.spec).toEqual(correctedSpec);
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

test('generative v2.0: invalid result bindings fall back to exact saved values without execution authority', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	const unsupported = { root: 'doc', elements: { doc: { type: 'Document', props: {}, children: ['bad'] }, bad: { type: 'UnregisteredWidget', props: {}, children: [] } } };
	const rejection = await rejected(page, 'artifact_create', { workspaceId: workspace, turnId: run.id, title: 'Unsupported component', spec: unsupported });
	expect(rejection.rejected).toBe(true);
	const missingResultSpec = { root: 'doc', elements: { doc: { type: 'Document', props: {}, children: ['table'] }, table: { type: 'Table', props: { resultId: 'invented-result' }, children: [] } } };
	const missingFieldSpec = { root: 'doc', elements: { doc: { type: 'Document', props: {}, children: ['metric'] }, metric: { type: 'Metric', props: { resultId: 'sales-result', field: 'absent-field', row: 0, title: 'Unavailable metric', unit: 'USD' }, children: [] } } };
	const missingResult = await createArtifact(desktop, run, 'Missing result reference', missingResultSpec);
	const missingField = await createArtifact(desktop, run, 'Missing field reference', missingFieldSpec);
	const doc = await createDocument(desktop, [missingResult, missingField], { title: 'Fallback review' });
	await openStudioDocument(desktop, doc);
	const output = page.getByRole('article', { name: 'Saved output: Missing result reference', exact: true });
	for (const title of ['Missing result reference', 'Missing field reference']) {
		await expect(page.getByRole('article', { name: `Saved output: ${title}`, exact: true }).getByRole('status').filter({ hasText: /presentation.*(could not|unavailable|not.*displayed)/i })).toBeVisible();
	}
	await expect(output.getByRole('region', { name: 'Saved result values', exact: true }).getByRole('table').getByRole('row').nth(1).getByRole('cell')).toHaveText(exactRows[0]);
	await output.getByText('Original saved answer', { exact: true }).click();
	await expect(output).toContainText(answer);
	await expect(output.locator('script, iframe, object, embed')).toHaveCount(0);
	await page.reload();
	await expect(page.getByRole('article', { name: 'Saved output: Missing result reference', exact: true })).toBeVisible();
	const resolved = await invoke(page, 'artifact_resolve', { workspaceId: workspace, artifactId: missingResult.id });
	expect(resolved.revision.spec).toEqual(missingResultSpec);
	expect(resolved.run.verification.status).toBe('verified');
	expect(resolved.run.result.status).toBe('verified');
	expect(resolved.run.result.text).toBe(answer);
	expect(resolved.results[0].exact_rows).toEqual(exactRows);
	await assertCanonicalUnchanged(run);
	await assertNoExecution(desktop);
	await page.screenshot({ path: test.info().outputPath('invalid-presentation-fallback.png') });
});

test('generative v2.0: Markdown download preserves exact data, selected state, evidence, and presentation reading order', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	const first = await createArtifact(desktop, run, 'Revenue comparison', comparisonSpec);
	const second = await createArtifact(desktop, run, 'Supporting table', secondSpec);
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
	// A fenced copy of the source spec is not a converted report. Require the
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

test('generative v2.0: portable report snapshots reopen as read-only saved records', async ({ desktop }) => {
	const { app, page, workspace, run } = desktop;
	const artifact = await createArtifact(desktop, run, 'Portable comparison', secondSpec);
	const doc = await createDocument(desktop, [artifact], { title: 'Portable quarterly report', body: 'A user-authored introduction.' });
	const studio = await openStudioDocument(desktop, doc);
	const json = await expectPackageDownload(desktop, () => studio.getByRole('button', { name: 'Export portable report snapshot', exact: true }).click(), 'portable-report');
	const packageData = JSON.parse(json);
	expect(packageData.format).toBe('fella-report');
	expect(packageData.version).toBe(2);
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
