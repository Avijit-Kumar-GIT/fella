import { expect, test } from '@playwright/test';

const folders = [
	{
		path: 'C:\\FellaFixture\\northwind-sales',
		catalog: {
			workspace: 'C:\\FellaFixture\\northwind-sales',
			revision: 'sales-r1',
			sources: [
				{ name: 'sales.csv', path: 'C:\\FellaFixture\\northwind-sales\\sales.csv', kind: 'csv', view: 'sales', row_count: 124, size_bytes: 4096, mtime: 1 }
			]
		}
	},
	{
		path: 'C:\\FellaFixture\\health-journal',
		catalog: {
			workspace: 'C:\\FellaFixture\\health-journal',
			revision: 'health-r1',
			sources: [
				{ name: 'sleep.csv', path: 'C:\\FellaFixture\\health-journal\\sleep.csv', kind: 'csv', view: 'sleep', row_count: 38, size_bytes: 2048, mtime: 1 }
			]
		}
	},
	...['weekly-budget', 'travel-plans', 'reading-notes'].map((name, index) => ({
		path: `C:\\FellaFixture\\${name}`,
		catalog: {
			workspace: `C:\\FellaFixture\\${name}`,
			revision: `${name}-r1`,
			sources: [
				{ name: `${name}.csv`, path: `C:\\FellaFixture\\${name}\\${name}.csv`, kind: 'csv', view: name.replaceAll('-', '_'), row_count: (index + 1) * 12, size_bytes: 1024, mtime: 1 }
			]
		}
	}))
];

async function installDesktopMock(page) {
	await page.addInitScript((fixtureFolders) => {
		const calls = [];
		let pickerIndex = 0;
		const settings = {
			provider: 'openai',
			base_url: 'https://api.openai.com/v1',
			model: 'gpt-4.1-mini',
			embed_model: '',
			has_credential: true,
			capabilities: { table_analysis: true, document_analysis: true, python_analysis: true, visualizations: true }
		};
		window.__workspaceAskCalls = calls;
		window.fella = {
			invoke: async (command, args = {}) => {
				switch (command) {
					case 'app_ready': return 1;
					case 'get_settings': return settings;
					case 'list_providers': return [];
					case 'provider_health': return { reachable: true, rejected: false, models: ['gpt-4.1-mini'] };
					case 'get_catalog': return { workspace: null, sources: [] };
					case 'last_workspace_path': return null;
					case 'conversations_list': return [];
					case 'archive_conversation': return 'fixture-archive';
					case 'close_workspace': return null;
					case 'run_log_recent': return [];
					default: return null;
				}
			},
			openWorkspace: async (path) => {
				const folder = fixtureFolders.find((entry) => entry.path === path);
				if (!folder) throw new Error(`Unexpected fixture folder: ${path}`);
				return folder.catalog;
			},
			pickFolder: async () => fixtureFolders[pickerIndex++]?.path ?? null,
			ask: async (params, onEvent) => {
				calls.push(params);
				const answer = {
					text: `Scoped to ${params.workspaceId ?? 'general'}.`,
					status: 'complete',
					evidence: [],
					verification: [],
					workspace: params.workspaceId ? { path: params.workspaceId, revision: 'fixture-r1' } : null
				};
				onEvent({ kind: 'answer_done', answer });
				return answer;
			},
			rerunAnalysisTurn: async () => { throw new Error('Rerun is not part of this journey'); },
			setWindowAppearance: async () => {},
			openExternal: async () => {},
			windowAction: async () => {},
			pathForFile: () => ''
		};
	}, folders);
}

async function dispatchWorkspaceDrag(page, type, workspacePath) {
	await page.evaluate(({ eventType, path }) => {
		const board = document.querySelector('[aria-label="Repository workspace board"]');
		const dataTransfer = new DataTransfer();
		dataTransfer.setData('application/x-fella-workspace', path);
		board.dispatchEvent(new DragEvent(eventType, { bubbles: true, dataTransfer }));
	}, { eventType: type, path: workspacePath });
}

test('repository workspaces render independently, compose, and keep Ask scoped to the focused tile', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'New conversation' }).click();
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('What is a useful way to compare trends?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to general.')).toBeVisible();
	const unboundCall = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(unboundCall.workspaceId).toBeNull();

	await page.getByRole('button', { name: 'Add repository' }).first().click();
	await expect(page.getByRole('region', { name: 'Repository workspace board' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales repository workspace' })).toContainText('sales.csv');

	await dispatchWorkspaceDrag(page, 'dragover', folders[1].path);
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(2);
	await expect(page.locator('.placement-preview .preview-slot.target')).toHaveText('New workspace');
	await dispatchWorkspaceDrag(page, 'drop', folders[1].path);
	const sales = page.getByRole('article', { name: 'northwind-sales repository workspace' });
	const health = page.getByRole('article', { name: 'health-journal repository workspace' });
	await expect(sales).toContainText('sales.csv');
	await expect(health).toContainText('sleep.csv');
	await expect(page.getByText('2 of 4 open')).toBeVisible();

	const arrangement = page.getByRole('combobox', { name: 'Workspace arrangement' });
	await arrangement.selectOption('stacked');
	const salesBox = await sales.boundingBox();
	const healthBox = await health.boundingBox();
	expect(salesBox).not.toBeNull();
	expect(healthBox).not.toBeNull();
	expect(Math.abs(salesBox.x - healthBox.x)).toBeLessThan(3);
	expect(healthBox.y).toBeGreaterThan(salesBox.y);
	await arrangement.selectOption('side-by-side');

	await sales.locator('.tile-focus').click();
	const salesComposer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await salesComposer.fill('Summarize sales trends');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(sales).toContainText('Scoped to C:\\FellaFixture\\northwind-sales.');

	await health.locator('.tile-focus').click();
	const healthComposer = page.getByRole('combobox', { name: 'Ask about health-journal' });
	await healthComposer.fill('Summarize sleep trends');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(health).toContainText('Scoped to C:\\FellaFixture\\health-journal.');
	const calls = await page.evaluate(() => window.__workspaceAskCalls);
	expect(calls.map((call) => call.workspaceId)).toEqual([
		null,
		...folders.slice(0, 2).map((folder) => folder.path)
	]);
});

test('the fifth workspace is refused without replacing a tile, and closing a tile frees a slot', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).first().click();
	for (let index = 1; index < 4; index += 1) {
		await page.locator('.board-head').getByRole('button', { name: 'Add repository' }).click();
	}
	await expect(page.getByText('4 of 4 open')).toBeVisible();
	const fifth = folders[4];

	await dispatchWorkspaceDrag(page, 'dragover', fifth.path);
	await expect(page.getByText('All four spaces are in use. Close a workspace before adding another.')).toBeVisible();
	const beforeDrop = await page.getByRole('article').count();
	await dispatchWorkspaceDrag(page, 'drop', fifth.path);
	await expect(page.getByText('All four workspace spaces are in use. Close a repository tile before opening another.')).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(beforeDrop);

	await page.getByRole('button', { name: 'Close northwind-sales workspace' }).click();
	await expect(page.getByText('3 of 4 open')).toBeVisible();
	await page.locator('.board-head').getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'reading-notes repository workspace' })).toBeVisible();
	await expect(page.getByText('4 of 4 open')).toBeVisible();

	await page.reload();
	await expect(page.getByText('4 of 4 open')).toBeVisible();
	await expect(page.getByRole('article', { name: 'health-journal repository workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'reading-notes repository workspace' })).toBeVisible();
});
