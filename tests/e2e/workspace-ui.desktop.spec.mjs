import { test, expect, _electron as electron } from '@playwright/test';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

// Acceptance cases correspond to docs/testing/WORKSPACE-UI-2026-10-16.md.
// No live model, user profile, backend grading, or existing test is modified.
const root = resolve('.');
const request = 'What changed in total revenue between Q3 and Q4, excluding cancelled orders and including refunds under the current merchandise policy?';
let app, page, folderA, folderB, firstInvestigation, secondInvestigation;

async function chooseWorkspace(label) {
	await page.getByRole('button', { name: /^Switch workspace\. Current:/ }).click();
	await page.getByRole('menuitemradio', { name: new RegExp(`^${label}(?:\\s|$)`) }).click();
	await expect(page.getByRole('button', { name: `Switch workspace. Current: ${label}`, exact: true })).toBeVisible();
}
async function openInvestigation(id) {
	const closed = page.getByRole('button', { name: 'Open workspace tools', exact: true });
	if (await closed.isVisible()) await closed.click();
	const add = page.getByRole('button', { name: 'Add tool tab', exact: true });
	if (await add.isVisible()) await add.click();
	await page.getByTestId('workspace-tool-investigations').click();
	await page.getByTestId(`workspace-investigation-option-${id}`).click();
	await expect(page.getByRole('tablist', { name: 'Investigation sections' })).toBeVisible();
}
async function addContext() {
	await page.getByRole('button', { name: 'Add tool tab', exact: true }).click();
	await page.getByTestId('workspace-tool-context').click();
	await expect(page.getByRole('heading', { name: 'Workspace context', exact: true })).toBeVisible();
}
async function screenshot(name) { await page.screenshot({ path: test.info().outputPath(`${name}.png`) }); }

test.beforeEach(async () => {
	const profile = await mkdtemp(join(tmpdir(), 'fella-ui-acceptance-'));
	folderA = join(profile, 'northstar'); folderB = join(profile, 'housing');
	for (const folder of [folderA, folderB]) {
		await mkdir(folder);
		await writeFile(join(folder, 'sales.csv'), 'quarter,amount\nQ3,10\nQ4,20\n');
		await writeFile(join(folder, 'fella.md'), 'Amounts are USD.\n');
	}
	app = await electron.launch({ args: [join(root, 'electron/main.mjs'), `--user-data-dir=${join(profile, 'chromium')}`], cwd: root, env: { ...process.env, FELLA_DATA_DIR: profile, FELLA_ENGINE_PATH: join(root, 'electron/engine', `fella-engine-${process.arch}${process.platform === 'win32' ? '.exe' : ''}`), FELLA_ELECTRON_URL: '' } });
	page = await app.firstWindow();
	await page.setViewportSize({ width: 1440, height: 960 });
	await page.getByRole('combobox', { name: 'Ask a question', exact: true }).waitFor();
	const records = await page.evaluate(async ({ folderA, folderB, request }) => {
		const rows = [ ['ui-one', folderA, request, 'First analysis answer.'], ['ui-two', folderA, 'Which categories explain the change?', 'Second analysis answer.'], ['ui-other', folderB, 'How much is rent?', 'Housing answer.'] ];
		for (const [id, workspace, text, response] of rows) {
			const answer = id === 'ui-one' ? { text: response, status: 'verified', evidence: [{ id: 'chart-one', tool: 'make_chart', args: {}, result_summary: 'Sales by quarter', ms: 1, chart: { kind: 'bar', title: 'Quarterly sales', labels: ['Q3', 'Q4'], series: [{ name: 'Revenue', values: [10, 20] }] } }], verification: [], workspace: { path: folderA, revision: 'fixture-revision' } } : undefined;
			await window.fella.invoke('archive_conversation', { id, body: JSON.stringify({ id, saved_at_ms: Date.now(), workspace, messages: [{ id: `${id}-u`, role: 'user', text, ts: 1 }, { id: `${id}-a`, role: 'assistant', text: response, ts: 2, ...(answer ? { answer } : {}) }] }) });
		}
		const a = await window.fella.invoke('investigation_create', { workspaceId: folderA, title: request, initialRequest: request, sourceMessageId: 'ui-one-u', conversationId: 'ui-one' });
		const b = await window.fella.invoke('investigation_create', { workspaceId: folderA, title: 'Category analysis', initialRequest: 'Which categories explain the change?', sourceMessageId: 'ui-two-u', conversationId: 'ui-two' });
		return { a, b };
	}, { folderA, folderB, request });
	firstInvestigation = records.a; secondInvestigation = records.b;
	await page.reload();
	await chooseWorkspace('northstar');
	// v1.1: user-approved locator correction. Archive labels are capped at 80
	// characters; the original full question is still checked in the transcript.
	await expect(page.getByRole('button', { name: /^Open conversation: What changed in total revenue/ })).toBeVisible();
	await page.getByRole('button', { name: /^Open conversation: What changed in total revenue/ }).click();
	await expect(page.getByRole('log', { name: 'conversation' })).toContainText('First analysis answer.');
});
test.afterEach(async () => { await app?.close(); });

test('workspace UI: sidebar scopes Chats and Files and keeps other workspaces in the picker', async () => {
	const sidebar = page.locator('.sidebar');
	await expect(sidebar.getByRole('button', { name: 'Open conversation: How much is rent?', exact: true })).toHaveCount(0);
	await sidebar.getByRole('button', { name: 'Files', exact: true }).click();
	await expect(sidebar.getByRole('region', { name: 'Workspace file hierarchy' })).toContainText('sales.csv');
	await chooseWorkspace('housing');
	await sidebar.getByRole('button', { name: 'Chats', exact: true }).click();
	await expect(sidebar.getByRole('button', { name: 'Open conversation: How much is rent?', exact: true })).toBeVisible();
	await expect(sidebar.getByRole('button', { name: /^Open conversation: What changed in total revenue/ })).toHaveCount(0);
	await chooseWorkspace('General');
	await expect(page.getByRole('combobox', { name: 'Ask a question', exact: true })).toBeVisible();
	await screenshot('workspace-scoped-sidebar');
});

test('workspace UI: Investigation controls remain visible across desktop sizes', async () => {
	await openInvestigation(firstInvestigation.id);
	for (const [width, height] of [[1440, 960], [1280, 800], [1100, 760], [960, 680]]) {
		await page.setViewportSize({ width, height });
		const sections = page.getByRole('tablist', { name: 'Investigation sections' });
		await sections.getByRole('tab', { name: 'Questions', exact: true }).click();
		await page.locator('.investigation-page').evaluate((element) => { element.scrollTop = 0; });
		await expect(page.getByRole('button', { name: 'Edit main question', exact: true })).toBeInViewport();
		await expect(sections.getByRole('tab', { name: 'Share', exact: true })).toBeInViewport();
		const heading = await page.getByRole('heading', { name: request, exact: true }).boundingBox();
		expect(heading.height).toBeLessThanOrEqual(52);
		if (width <= 1100) {
			await expect(page.getByRole('region', { name: 'Conversation', exact: true })).toBeHidden();
			expect((await page.locator('.workspace-tools-pane').boundingBox()).height).toBeGreaterThan(500);
		}
		await screenshot(`questions-${width}`);
	}
	await page.getByRole('button', { name: 'Conversation', exact: true }).click();
	await expect(page.getByRole('log', { name: 'conversation' })).toContainText('First analysis answer.');
	await expect(page.getByRole('combobox', { name: 'Ask about northstar', exact: true })).toBeVisible();
});

test('workspace UI: tools resize with keyboard and pointer, persist width, and expand', async () => {
	await openInvestigation(firstInvestigation.id);
	const pane = page.locator('.workspace-tools-pane');
	const divider = page.getByRole('separator', { name: 'Resize workspace tools' });
	const before = (await pane.boundingBox()).width;
	await divider.focus(); await divider.press('ArrowLeft');
	await expect.poll(async () => Math.round((await pane.boundingBox()).width)).toBe(Math.round(before + 32));
	const split = await divider.boundingBox();
	await page.mouse.move(split.x, split.y + 80); await page.mouse.down(); await page.mouse.move(split.x - 40, split.y + 80); await page.mouse.up();
	const resized = Math.round((await pane.boundingBox()).width);
	expect(resized).toBeGreaterThan(before + 60);
	await page.reload();
	await expect.poll(async () => Math.round((await pane.boundingBox()).width)).toBe(resized);
	await page.getByRole('button', { name: 'Expand workspace tools', exact: true }).click();
	await expect(page.getByRole('region', { name: 'Conversation', exact: true })).toBeHidden();
	await page.getByRole('button', { name: 'Restore split view', exact: true }).click();
	await expect(page.getByRole('region', { name: 'Conversation', exact: true })).toBeVisible();
});

test('workspace UI: independent tool tabs reveal the selection and Investigation tabs switch their chats', async () => {
	await openInvestigation(firstInvestigation.id);
	for (let index = 0; index < 4; index++) await addContext();
	await expect(page.getByRole('tab', { name: /^Workspace context \d$/ })).toHaveCount(4);
	const selected = page.getByRole('tablist', { name: 'Open workspace tools' }).getByRole('tab', { selected: true });
	await expect.poll(async () => {
		const tab = await selected.boundingBox(), strip = await page.getByRole('tablist', { name: 'Open workspace tools' }).boundingBox();
		return tab.x >= strip.x - 1 && tab.x + tab.width <= strip.x + strip.width + 1;
	}).toBe(true);
	await openInvestigation(secondInvestigation.id);
	await expect(page.getByRole('log', { name: 'conversation' })).toContainText('Second analysis answer.');
	await page.getByRole('tab', { name: request, exact: true }).click();
	await expect(page.getByRole('log', { name: 'conversation' })).toContainText('First analysis answer.');
	await screenshot('tool-tab-selection');
});

test('workspace UI: questions edit and autosave without changing the original request; completion lives in Results', async () => {
	await openInvestigation(firstInvestigation.id);
	const sections = page.getByRole('tablist', { name: 'Investigation sections' });
	await sections.getByRole('tab', { name: 'Questions', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Mark complete', exact: true })).toBeHidden();
	await page.getByRole('button', { name: 'Edit main question', exact: true }).click();
	const revised = 'What is revenue after refunds?';
	await page.getByRole('textbox', { name: 'Main question', exact: true }).fill(revised);
	await page.getByRole('button', { name: 'Done editing', exact: true }).click();
	await expect.poll(() => page.evaluate(async (id) => (await window.fella.invoke('investigations_list')).find((item) => item.id === id)?.questions?.[0]?.text, firstInvestigation.id)).toBe(revised);
	await sections.getByRole('tab', { name: 'Results', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Mark complete', exact: true })).toBeDisabled();
	await page.getByRole('button', { name: 'Close workspace tools', exact: true }).click();
	await expect(page.getByRole('log', { name: 'conversation' })).toContainText(request);
});

test('workspace UI: file and chart previews use Tools tabs and context actions fit the pane', async () => {
	await page.locator('.workspace-conversation-pane').getByRole('button', { name: 'Open beside', exact: true }).click();
	await expect(page.getByRole('tab', { name: 'Quarterly sales', exact: true })).toBeVisible();
	await expect(page.getByRole('tabpanel', { name: 'Quarterly sales', exact: true })).toContainText('Quarterly sales');
	await page.getByRole('button', { name: 'Files', exact: true }).click();
	await page.locator('.tree-file').filter({ hasText: 'sales.csv' }).click();
	await expect(page.getByRole('tab', { name: 'sales.csv', exact: true })).toBeVisible();
	await expect(page.getByRole('region', { name: 'First 25 rows of sales.csv', exact: true })).toContainText('Q3');
	await addContext();
	for (const button of await page.locator('.context-page').getByRole('button').all()) {
		if (!(await button.isVisible())) continue;
		const control = await button.boundingBox(), pane = await page.locator('.workspace-tools-pane').boundingBox();
		expect(control.x + control.width).toBeLessThanOrEqual(pane.x + pane.width + 1);
	}
	await page.getByRole('button', { name: 'Settings', exact: true }).click();
	await page.getByRole('button', { name: 'Light', exact: true }).click();
	await expect(page.locator('html')).toHaveAttribute('data-color-mode', 'light');
	await page.getByRole('button', { name: 'New conversation', exact: true }).click();
	await expect(page.getByRole('combobox', { name: 'Ask about northstar', exact: true })).toBeVisible();
	await screenshot('new-conversation-light');
});

test('workspace UI: workspace selection exits Settings and the dark layout preserves the Investigation', async () => {
	await page.getByRole('button', { name: 'Settings', exact: true }).click();
	await page.getByRole('button', { name: 'Dark', exact: true }).click();
	await expect(page.locator('html')).toHaveAttribute('data-color-mode', 'dark');
	await chooseWorkspace('housing');
	await expect(page.getByRole('combobox', { name: 'Ask about housing', exact: true })).toBeVisible();
	await chooseWorkspace('northstar');
	await openInvestigation(firstInvestigation.id);
	await page.getByRole('tablist', { name: 'Investigation sections' }).getByRole('tab', { name: 'Questions', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Edit main question', exact: true })).toBeVisible();
	await screenshot('questions-dark-wide');
	await page.setViewportSize({ width: 960, height: 680 });
	await expect(page.getByRole('button', { name: 'Edit main question', exact: true })).toBeInViewport();
	await screenshot('questions-dark-compact');
});

test('workspace UI: the picker adds folders and unavailable workspaces reconnect without losing history', async () => {
	const extra = join(folderA, '..', 'new-folder');
	await mkdir(extra); await writeFile(join(extra, 'orders.csv'), 'amount\n4\n');
	await app.evaluate(({ dialog }, path) => { dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [path] }); }, extra);
	await page.getByRole('button', { name: /^Switch workspace\. Current:/ }).click();
	await page.getByRole('menuitem', { name: 'Add workspace', exact: true }).click();
	await expect(page.getByRole('combobox', { name: 'Ask about new-folder', exact: true })).toBeVisible();
	const missing = join(folderA, '..', 'missing-folder');
	await page.evaluate(async (workspace) => {
		await window.fella.invoke('archive_conversation', { id: 'missing-history', body: JSON.stringify({ id: 'missing-history', workspace, saved_at_ms: Date.now(), messages: [{ id: 'missing-u', role: 'user', text: 'Keep this saved history.', ts: 1 }, { id: 'missing-a', role: 'assistant', text: 'Archived answer.', ts: 2 }] }) });
	}, missing);
	await page.reload();
	await chooseWorkspace('missing-folder');
	await expect(page.getByRole('status').filter({ hasText: 'History only' })).toBeVisible();
	await page.getByRole('button', { name: 'Open conversation: Keep this saved history.', exact: true }).click();
	await expect(page.getByRole('log', { name: 'conversation' })).toContainText('Archived answer.');
	await mkdir(missing); await writeFile(join(missing, 'sales.csv'), 'amount\n8\n');
	await page.getByRole('button', { name: 'Reconnect folder', exact: true }).click();
	await expect(page.getByRole('combobox', { name: 'Ask about missing-folder', exact: true })).toBeVisible();
	await expect(page.getByRole('log', { name: 'conversation' })).toContainText('Archived answer.');
});

test('workspace UI: picker keyboard focus and global search remain available across workspace scopes', async () => {
	const trigger = page.getByRole('button', { name: 'Switch workspace. Current: northstar', exact: true });
	await trigger.click();
	await page.keyboard.press('Home');
	await expect(page.getByRole('menuitemradio', { name: 'General', exact: true })).toBeFocused();
	await page.keyboard.press('ArrowDown');
	await expect(page.getByRole('menuitemradio', { name: /^northstar\s/ })).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(trigger).toBeFocused();
	await page.getByRole('button', { name: 'Search', exact: true }).click();
	await page.getByRole('textbox', { name: 'Search Fella', exact: true }).fill('How much is rent?');
	await page.getByRole('button', { name: /^How much is rent\?/ }).click();
	await expect(page.getByRole('log', { name: 'conversation' })).toContainText('Housing answer.');
	await expect(page.getByRole('button', { name: 'Switch workspace. Current: housing', exact: true })).toBeVisible();
});
