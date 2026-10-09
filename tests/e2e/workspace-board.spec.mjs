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

async function installDesktopMock(page, { seedUnavailableConversation = false } = {}) {
	await page.addInitScript(({ fixtureFolders, seedUnavailableConversation }) => {
		const calls = [];
		const archivedConversations = new Map();
		const unavailablePath = 'C:\\FellaFixture\\unavailable-archive';
		if (seedUnavailableConversation) {
			const id = 'archived-unavailable-workspace';
			const saved = {
				id,
				workspace: unavailablePath,
				title: null,
				messages: [
					{ id: 'archived-user', role: 'user', text: 'Summarize the unavailable archive', ts: 1 },
					{ id: 'archived-assistant', role: 'assistant', text: 'Saved result from this folder.', ts: 2 }
				]
			};
			archivedConversations.set(id, {
				saved,
				summary: {
					id,
					saved_at_ms: 2,
					workspace: unavailablePath,
					preview: 'Summarize the unavailable archive',
					message_count: 2,
					title: null
				}
			});
		}
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
		window.__workspaceOpenCalls = [];
		window.fella = {
			invoke: async (command, args = {}) => {
				switch (command) {
					case 'app_ready': return 1;
					case 'get_settings': return settings;
					case 'list_providers': return [];
					case 'provider_health': return { reachable: true, rejected: false, models: ['gpt-4.1-mini'] };
					case 'get_catalog': return { workspace: null, sources: [] };
					case 'last_workspace_path': return null;
					case 'conversations_list':
						return [...archivedConversations.values()].map(({ summary }) => summary);
					case 'archive_conversation': {
						const saved = JSON.parse(args.body);
						const firstQuestion = saved.messages?.find((message) =>
							message.role === 'user' && message.text?.trim() && !message.text.trimStart().startsWith('/')
						);
						archivedConversations.set(args.id, {
							saved,
							summary: {
								id: args.id,
								saved_at_ms: saved.saved_at_ms,
								workspace: saved.workspace ?? null,
								preview: firstQuestion?.text ?? 'New conversation',
								message_count: saved.messages?.length ?? 0,
								title: saved.title ?? null
							}
						});
						return 'fixture-archive';
					}
					case 'conversation_load': {
						const saved = archivedConversations.get(args.id)?.saved;
						if (!saved) throw new Error(`Unknown archived conversation: ${args.id}`);
						return JSON.stringify(saved);
					}
					case 'close_workspace': return null;
					case 'run_log_recent': return [];
					default: return null;
				}
			},
			openWorkspace: async (path) => {
				window.__workspaceOpenCalls.push(path);
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
	}, { fixtureFolders: folders, seedUnavailableConversation });
}

async function dispatchWorkspaceDrag(page, type, workspacePath, { x = 0.5, y = 0.5, existing = false } = {}) {
	await page.evaluate(({ eventType, path, targetX, targetY, alreadyOpen }) => {
		const board = document.querySelector('[aria-label="Workspace board"]');
		if (!board) throw new Error('The workspace board is not open');
		const rect = board.getBoundingClientRect();
		const dataTransfer = new DataTransfer();
		dataTransfer.setData('application/x-fella-workspace', path);
		if (alreadyOpen) dataTransfer.setData('application/x-fella-existing-workspace', path);
		board.dispatchEvent(new DragEvent(eventType, {
			bubbles: true,
			cancelable: true,
			clientX: rect.left + rect.width * targetX,
			clientY: rect.top + rect.height * targetY,
			dataTransfer
		}));
	}, { eventType: type, path: workspacePath, targetX: x, targetY: y, alreadyOpen: existing });
}

test('starts unbound, opens real conversation tabs, groups repository artifacts, and places workspaces by drag', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const tabs = page.getByRole('tablist', { name: 'Conversations' });
	await expect(tabs).toBeVisible();
	await expect(tabs.getByRole('tab')).toHaveCount(1);
	await expect(page.getByRole('heading', { name: 'New conversation', level: 1 })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(0);
	await expect(page.getByRole('button', { name: 'Mount a folder' })).toHaveCount(0);
	const initialSurfaces = await page.evaluate(() => ({
		window: getComputedStyle(document.querySelector('.workspace-window')).backgroundColor,
		dock: getComputedStyle(document.querySelector('.dock')).backgroundColor,
		composer: getComputedStyle(document.querySelector('.field')).backgroundColor
	}));
	expect(initialSurfaces.dock).toBe(initialSurfaces.window);
	expect(initialSurfaces.composer).toBe(initialSurfaces.window);

	await page.keyboard.press('Control+t');
	await expect(tabs.getByRole('tab')).toHaveCount(2);
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('What is a useful way to compare trends?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to general.')).toBeVisible();
	const unboundCall = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(unboundCall.workspaceId).toBeNull();
	await expect(tabs.getByRole('tab', { name: /What is a useful way to compare trends/ })).toHaveAttribute('aria-selected', 'true');
	await page.keyboard.press('Control+t');
	await expect(tabs.getByRole('tab')).toHaveCount(3);
	await expect(tabs.getByRole('tab').nth(2)).toHaveAttribute('aria-selected', 'true');
	await page.keyboard.press('Control+[');
	await expect(tabs.getByRole('tab').nth(1)).toHaveAttribute('aria-selected', 'true');
	await page.keyboard.press('Control+1');
	await expect(tabs.getByRole('tab').nth(0)).toHaveAttribute('aria-selected', 'true');
	await tabs.getByRole('button', { name: 'Close tab: What is a useful way to compare trends?' }).click();
	await expect(tabs.getByRole('tab')).toHaveCount(2);
	const archivedGeneral = page.getByRole('button', { name: 'Open conversation: What is a useful way to compare trends?' });
	await expect(archivedGeneral).toBeVisible();
	await archivedGeneral.click();
	await expect(tabs.getByRole('tab')).toHaveCount(3);
	await expect(tabs.getByRole('tab', { name: /What is a useful way to compare trends/ })).toHaveAttribute('aria-selected', 'true');

	const workspaceNavigation = page.getByRole('region', { name: 'Workspaces' });
	await workspaceNavigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
	const salesWorkspaceRow = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await expect(page.getByRole('button', { name: 'Open sources in northwind-sales' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Create project for northwind-sales' })).toBeVisible();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
	await salesWorkspaceRow.click();
	await salesWorkspaceRow.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();
	await expect(tabs.getByRole('tab').last()).toHaveAttribute('aria-selected', 'true');
	const salesComposer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await salesComposer.fill('Summarize sales trends');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	const salesCall = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(salesCall.workspaceId).toBe(folders[0].path);

	await page.getByRole('button', { name: 'Create project for northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'Create a project' })).toBeVisible();
	await expect(page.getByLabel('Repository', { exact: true })).toHaveValue(folders[0].path);
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales review');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Open project Sales review' })).toBeVisible();

	await salesWorkspaceRow.click();

	await dispatchWorkspaceDrag(page, 'dragover', folders[1].path, { x: 0.9, y: 0.5 });
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(2);
	await expect(page.locator('.placement-preview .preview-slot.target')).toHaveText('Drop to place');
	await dispatchWorkspaceDrag(page, 'drop', folders[1].path, { x: 0.9, y: 0.5 });
	const sales = page.getByRole('article', { name: 'northwind-sales workspace' });
	const health = page.getByRole('article', { name: 'health-journal workspace' });
	await expect(sales).toBeVisible();
	await expect(health).toBeVisible();
	await expect(page.getByRole('button', { name: 'Open sources in health-journal' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(2);
	await expect(page.getByRole('combobox', { name: 'Workspace arrangement' })).toHaveCount(0);
	const salesBox = await sales.boundingBox();
	const healthBox = await health.boundingBox();
	expect(salesBox).not.toBeNull();
	expect(healthBox).not.toBeNull();
	expect(healthBox.x).toBeGreaterThan(salesBox.x);

	await dispatchWorkspaceDrag(page, 'dragover', folders[2].path, { x: 0.1, y: 0.5 });
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(3);
	await dispatchWorkspaceDrag(page, 'drop', folders[2].path, { x: 0.1, y: 0.5 });
	const budget = page.getByRole('article', { name: 'weekly-budget workspace' });
	await expect(budget).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(3);
	const budgetBox = await budget.boundingBox();
	const salesThreeBox = await sales.boundingBox();
	const healthThreeBox = await health.boundingBox();
	expect(budgetBox).not.toBeNull();
	expect(salesThreeBox).not.toBeNull();
	expect(healthThreeBox).not.toBeNull();
	expect(Math.abs(budgetBox.x - salesThreeBox.x)).toBeLessThan(3);
	expect(budgetBox.y).not.toBe(salesThreeBox.y);
	expect(healthThreeBox.x).toBeGreaterThan(budgetBox.x);

	await dispatchWorkspaceDrag(page, 'dragover', folders[0].path, { x: 0.9, y: 0.9, existing: true });
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(3);
	await dispatchWorkspaceDrag(page, 'drop', folders[0].path, { x: 0.9, y: 0.9, existing: true });
	const movedSalesBox = await sales.boundingBox();
	const movedHealthBox = await health.boundingBox();
	expect(movedSalesBox).not.toBeNull();
	expect(movedHealthBox).not.toBeNull();
	expect(movedSalesBox.x).toBeGreaterThan(budgetBox.x);
	expect(movedSalesBox.y).toBeGreaterThan(movedHealthBox.y);
});

test('the fifth workspace is refused without replacing a tile, and closing a tile frees a slot', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const workspaceNavigation = page.getByRole('region', { name: 'Workspaces' });
	for (let index = 0; index < 4; index += 1) {
		await workspaceNavigation.getByRole('button', { name: 'Add repository' }).click();
		await expect(page.getByRole('article')).toHaveCount(index + 1);
	}
	await expect(page.getByRole('article')).toHaveCount(4);
	const fifth = folders[4];

	await dispatchWorkspaceDrag(page, 'dragover', fifth.path);
	await expect(page.getByText('Close a workspace to add another.')).toBeVisible();
	const beforeDrop = await page.getByRole('article').count();
	await dispatchWorkspaceDrag(page, 'drop', fifth.path);
	await expect(page.getByText('Close a workspace to add another.')).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(beforeDrop);

	await page.getByRole('button', { name: 'Close northwind-sales workspace' }).click();
	await expect(page.getByRole('article')).toHaveCount(3);
	await dispatchWorkspaceDrag(page, 'drop', fifth.path);
	await expect(page.getByRole('article', { name: 'reading-notes workspace' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(4);

	await page.reload();
	await expect(page.getByRole('article')).toHaveCount(4);
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'reading-notes workspace' })).toBeVisible();
});

test('a history-only workspace retries its folder mount and keeps history available when it is still missing', async ({ page }) => {
	await installDesktopMock(page, { seedUnavailableConversation: true });
	await page.goto('/');
	await page.getByRole('button', { name: 'Expand unavailable-archive' }).click();
	await page.getByRole('button', { name: 'Open conversation: Summarize the unavailable archive' }).click();
	const workspace = page.getByRole('article', { name: 'unavailable-archive workspace' });
	await expect(workspace).toBeVisible();
	await expect(page.getByRole('status', {
		name: 'History only. The folder could not be opened; saved conversations remain available.'
	})).toBeVisible();
	const initialOpenCalls = await page.evaluate(() => window.__workspaceOpenCalls.length);
	expect(initialOpenCalls).toBe(1);

	await page.getByRole('button', { name: 'Reconnect unavailable-archive' }).click();
	await expect.poll(() => page.evaluate(() => window.__workspaceOpenCalls.length)).toBe(2);
	await expect(page.getByText(/Its conversations remain in history only/)).toBeVisible();
	await expect(workspace).toContainText('Saved result from this folder.');
	await expect(page.getByRole('combobox', { name: 'Workspace unavailable: unavailable-archive' })).toBeDisabled();
});
