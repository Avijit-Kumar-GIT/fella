import { test as base, expect, _electron as electron } from '@playwright/test';
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

/**
 * Generative workspace contract v2.1; executable task set v2.1 (2026-10-23).
 * Expected semantics are fixed before candidate execution and come from
 * docs/testing/GENERATIVE-WORKSPACE-v2.md, not observed rendering output.
 * Useful json-render specs are returned in the same turn; canonical answer text
 * remains usable when presentation composition fails. Local-state interactions
 * and saved artifact persistence make no extra model, analytics, or source
 * requests. This suite makes no live-model quality claim.
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
 * 7. Presentation controls update only local state, derived content
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
	'ask', 'analysis_turn_rerun',
	'investigation_propose_questions', 'run_sql_direct', 'reindex',
	'sample_source', 'preview_file'
];
const comparisonSpec = {
	root: 'document',
	elements: {
		document: { type: 'Document', props: {}, children: ['opening', 'comparison', 'ending'] },
		opening: { type: 'Text', props: { content: 'Presentation begins with the saved comparison.' }, children: [] },
		comparison: { type: 'Section', props: { title: 'Quarter comparison' }, children: ['q3', 'q4', 'period-filter', 'chart', 'table'] },
		q3: { type: 'Metric', props: { resultId: 'sales-result', field: 'revenue', row: 0, title: 'Q3 revenue', unit: 'USD' }, children: [] },
		q4: { type: 'Metric', props: { resultId: 'sales-result', field: 'revenue', row: 1, title: 'Q4 revenue', unit: 'USD' }, children: [] },
		'period-filter': { type: 'ResultFilter', props: { resultId: 'sales-result', field: 'period', name: 'selected-period', title: 'Period' }, children: [] },
		chart: { type: 'Chart', props: { resultId: 'sales-result', spec: { mark: 'bar', encoding: { x: { field: 'period', type: 'nominal' }, y: { field: 'revenue', type: 'quantitative' } } }, title: 'Revenue by quarter' }, children: [] },
		table: { type: 'Table', props: { resultId: 'sales-result' }, children: [] },
		ending: { type: 'Text', props: { content: 'Presentation ends after the exact values.' }, children: [] }
	}
};
const secondSpec = {
	root: 'document',
	elements: {
		document: { type: 'Document', props: {}, children: ['section'] },
		section: { type: 'Section', props: { title: 'Second saved output' }, children: ['text', 'table'] },
		text: { type: 'Text', props: { content: 'An independently placed analytical output.' }, children: [] },
		table: { type: 'Table', props: { resultId: 'sales-result' }, children: [] }
	}
};

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

async function seedRun({ page, profile, workspace }, { id = 'gen-saved-run', conversationId = 'gen-revenue-chat', amounts = [10, 20], resultId = 'sales-result', excluded = false, complete = true, question: runQuestion = question, investigationContext } = {}) {
	const catalog = await invoke(page, 'get_catalog', { workspaceId: workspace });
	const run = {
		id, conversation_id: conversationId, question: runQuestion,
		...(investigationContext ? { investigation_context: investigationContext } : {}),
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

async function createArtifact(harness, run, title, spec) {
	return invoke(harness.page, 'artifact_create', { workspaceId: harness.workspace, turnId: run.id, title, spec });
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

export { test, expect, question, caveat, answer, columns, exactRows, comparisonSpec, secondSpec, invoke, rejected, chooseWorkspace, engineRequests, startBudget, assertNoExecution, launch, seedRun, placement, createArtifact, createDocument, assertCanonicalUnchanged, openStudio, openStudioDocument, addSavedOutput, assertSavedRows, expectMarkdownDownload, expectPackageDownload };
