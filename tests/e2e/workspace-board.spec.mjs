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

async function dispatchWorkspaceDrag(page, type, workspacePath) {
	await page.evaluate(({ eventType, path }) => {
		const board = document.querySelector('[aria-label="Workspace board"]');
		const dataTransfer = new DataTransfer();
		dataTransfer.setData('application/x-fella-workspace', path);
		board.dispatchEvent(new DragEvent(eventType, { bubbles: true, dataTransfer }));
	}, { eventType: type, path: workspacePath });
}

test('General and repository workspaces render independently, compose, and keep Ask scoped to the focused workspace', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const general = page.getByRole('article', { name: 'General workspace' });
	await expect(general).toBeVisible();
	await expect(page.getByText('1 of 4 open')).toBeVisible();
	await page.getByRole('button', { name: 'New conversation' }).click();
	// Ask acts inside General; history selection is not titlebar-tab creation.
	await expect(page.getByRole('tablist')).toHaveCount(0);
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('What is a useful way to compare trends?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to general.')).toBeVisible();
	const unboundCall = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(unboundCall.workspaceId).toBeNull();
	// A second conversation in General must not overwrite the first one's history.
	await page.getByRole('button', { name: 'New conversation' }).click();

	await page.getByRole('button', { name: 'Add repository' }).first().click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toContainText('sales.csv');
	await expect(page.getByText('2 of 4 open')).toBeVisible();

	await dispatchWorkspaceDrag(page, 'dragover', folders[1].path);
	// Two open workspaces plus the proposed placement produce three preview slots.
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(3);
	await expect(page.locator('.placement-preview .preview-slot.target')).toHaveText('New workspace');
	await dispatchWorkspaceDrag(page, 'drop', folders[1].path);
	const sales = page.getByRole('article', { name: 'northwind-sales workspace' });
	const health = page.getByRole('article', { name: 'health-journal workspace' });
	await expect(sales).toContainText('sales.csv');
	await expect(health).toContainText('sleep.csv');
	await expect(page.getByText('3 of 4 open')).toBeVisible();

	const arrangement = page.getByRole('combobox', { name: 'Workspace arrangement' });
	await arrangement.selectOption('one-left-two-right');
	const salesBox = await sales.boundingBox();
	const healthBox = await health.boundingBox();
	expect(salesBox).not.toBeNull();
	expect(healthBox).not.toBeNull();
	expect(Math.abs(salesBox.x - healthBox.x)).toBeLessThan(3);
	expect(healthBox.y).toBeGreaterThan(salesBox.y);

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

	const salesFocus = sales.locator('.tile-focus');
	const healthFocus = health.locator('.tile-focus');
	await salesFocus.click();
	await expect(salesFocus).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();

	await healthFocus.click();
	await expect(healthFocus).toHaveAttribute('aria-pressed', 'true');
	await salesFocus.click();
	await expect(salesFocus).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();

	// General owns non-mounted conversations. Selecting its history switches
	// the conversation in the existing General window.
	await page.getByRole('button', { name: 'General', exact: true }).click();
	const generalHistory = page.getByRole('button', {
		name: 'Open conversation: What is a useful way to compare trends?'
	});
	await expect(generalHistory).toBeVisible();
	await generalHistory.click();
	await expect(page.getByRole('article', { name: 'General workspace' })).toHaveCount(1);
	await expect(general).toContainText('What is a useful way to compare trends?');
	await expect(general.locator('.tile-focus')).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('combobox', { name: 'Ask a question' })).toBeVisible();
	await expect(page.getByRole('tablist')).toHaveCount(0);

	const calls = await page.evaluate(() => window.__workspaceAskCalls);
	expect(calls.map((call) => call.workspaceId)).toEqual([
		null,
		...folders.slice(0, 2).map((folder) => folder.path)
	]);
});

test('the fifth workspace is refused without replacing a tile, and closing a tile frees a slot', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await expect(page.getByRole('article', { name: 'General workspace' })).toBeVisible();
	await page.getByRole('button', { name: 'Add repository' }).first().click();
	for (let index = 1; index < 3; index += 1) {
		await page.locator('.board-head').getByRole('button', { name: 'Add repository' }).click();
	}
	await expect(page.getByText('4 of 4 open')).toBeVisible();
	const fifth = folders[3];

	await dispatchWorkspaceDrag(page, 'dragover', fifth.path);
	await expect(page.getByText('All four workspaces are in use. Close one before adding another.')).toBeVisible();
	const beforeDrop = await page.getByRole('article').count();
	await dispatchWorkspaceDrag(page, 'drop', fifth.path);
	await expect(page.getByText('All four workspace windows are in use. Close one before opening another.')).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(beforeDrop);

	await page.getByRole('button', { name: 'Close General workspace' }).click();
	await expect(page.getByText('3 of 4 open')).toBeVisible();
	await dispatchWorkspaceDrag(page, 'drop', fifth.path);
	await expect(page.getByRole('article', { name: 'travel-plans workspace' })).toBeVisible();
	await expect(page.getByText('4 of 4 open')).toBeVisible();

	await page.reload();
	await expect(page.getByText('4 of 4 open')).toBeVisible();
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'travel-plans workspace' })).toBeVisible();
});

test('a history-only workspace retries its folder mount and keeps history available when it is still missing', async ({ page }) => {
	await installDesktopMock(page, { seedUnavailableConversation: true });
	await page.goto('/');
	await page.getByRole('button', { name: 'Expand unavailable-archive conversations' }).click();
	await page.getByRole('button', { name: 'Open conversation: Summarize the unavailable archive' }).click();
	const workspace = page.getByRole('article', { name: 'unavailable-archive workspace' });
	await expect(workspace).toContainText('History only');
	await expect(page.getByRole('status', {
		name: 'History only. The folder could not be opened; saved conversations remain available.'
	})).toBeVisible();
	const initialOpenCalls = await page.evaluate(() => window.__workspaceOpenCalls.length);
	expect(initialOpenCalls).toBe(1);

	await page.getByRole('button', { name: 'Try to reopen unavailable-archive' }).click();
	await expect.poll(() => page.evaluate(() => window.__workspaceOpenCalls.length)).toBe(2);
	await expect(page.getByText(/Its conversations remain in history only/)).toBeVisible();
	await expect(workspace).toContainText('Saved result from this folder.');
	await expect(page.getByRole('combobox', { name: 'Workspace unavailable: unavailable-archive' })).toBeDisabled();
});
