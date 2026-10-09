import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';

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

async function installDesktopMock(
	page,
	{
		seedUnavailableConversation = false,
		fixtureFolders = folders,
		replayStatus = null,
		replayStatusError = false,
		reindexedCatalog = null,
		chartEvidence = null,
		detailEvidence = null,
		providerList = [],
		providerHealth = { reachable: true, rejected: false, models: ['gpt-4.1-mini'] },
		runLogEntries = []
	} = {}
) {
	await page.addInitScript(({ fixtureFolders, seedUnavailableConversation, replayStatus, replayStatusError, reindexedCatalog, chartEvidence, detailEvidence, providerList, providerHealth, runLogEntries }) => {
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
		window.__replayStatusCalls = 0;
		window.fella = {
			invoke: async (command, args = {}) => {
				switch (command) {
					case 'app_ready': return 1;
					case 'get_settings': return settings;
					case 'list_providers': return providerList;
					case 'provider_health': return providerHealth;
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
					case 'analysis_turn_replay_status':
						window.__replayStatusCalls += 1;
						if (replayStatusError) throw new Error('Replay lookup unavailable.');
						return replayStatus;
					case 'reindex': return reindexedCatalog;
					case 'run_log_recent': return runLogEntries;
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
				const hasReplayEvidence = replayStatus !== null || replayStatusError;
				const answer = {
					text: `Scoped to ${params.workspaceId ?? 'general'}.`,
					status: 'complete',
					turn_id: hasReplayEvidence ? 'fixture-turn' : undefined,
					evidence: [
						...(hasReplayEvidence
							? [{ id: 'fixture-replay-step', tool: 'run_sql', args: {}, result_summary: 'One row inspected.', ms: 12 }]
							: []),
						...(detailEvidence ? [detailEvidence] : []),
						...(chartEvidence ? [chartEvidence] : [])
					],
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
	}, { fixtureFolders, seedUnavailableConversation, replayStatus, replayStatusError, reindexedCatalog, chartEvidence, detailEvidence, providerList, providerHealth, runLogEntries });
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

function expectSameRenderedBox(actual, expected) {
	expect(actual).not.toBeNull();
	expect(expected).not.toBeNull();
	for (const edge of ['x', 'y', 'width', 'height']) {
		expect(Math.abs(actual[edge] - expected[edge])).toBeLessThanOrEqual(1);
	}
}

test('environments compose General and repository panes while conversations retain their original scope', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	await expect(switcher).toBeVisible();
	await expect(switcher).toContainText('General');
	await expect(page.locator('.titlebar .environment-switcher-trigger')).toHaveCount(0);
	await switcher.click();
	await expect(page.getByRole('menuitemradio')).toHaveCount(1);
	await page.keyboard.press('Escape');
	await expect(page.getByRole('heading', { name: 'New conversation', level: 1 })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(0);
	await expect(page.getByRole('button', { name: 'Mount a folder' })).toHaveCount(0);
	const surfaceColors = await page.evaluate(() => {
		const root = document.documentElement;
		const previous = root.getAttribute('data-color-mode');
		const measure = () => ({
			main: getComputedStyle(document.querySelector('main')).backgroundColor,
			dock: getComputedStyle(document.querySelector('.dock')).backgroundColor,
			composer: getComputedStyle(document.querySelector('.field')).backgroundColor
		});
		root.setAttribute('data-color-mode', 'light');
		const light = measure();
		root.setAttribute('data-color-mode', 'dark');
		const dark = measure();
		if (previous === null) root.removeAttribute('data-color-mode');
		else root.setAttribute('data-color-mode', previous);
		return { light, dark };
	});
	for (const surfaces of [surfaceColors.light, surfaceColors.dark]) {
		expect(surfaces.dock).toBe(surfaces.main);
		expect(surfaces.composer).toBe(surfaces.main);
	}

	await page.getByRole('combobox', { name: 'Ask a question' }).fill('What is a useful way to compare trends?');
	const initialSend = page.getByRole('button', { name: 'Send' });
	await expect(initialSend).toHaveAttribute('data-slot', 'button');
	await initialSend.click();
	await expect(page.getByText('Scoped to general.')).toBeVisible();
	const unboundCall = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(unboundCall.workspaceId).toBeNull();

	await page.keyboard.press('Control+t');
	await expect(switcher).toContainText('General 2');
	await switcher.click();
	await expect(page.getByRole('menuitemradio')).toHaveCount(2);
	await expect(page.getByRole('menuitemradio').nth(1)).toHaveAttribute('aria-checked', 'true');
	await page.keyboard.press('Escape');
	await expect(page.getByRole('heading', { name: 'New conversation', level: 1 })).toBeVisible();
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('What does a seasonal pattern look like?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to general.')).toBeVisible();
	const conversationsAfterQuestion = await page.evaluate(() => window.__workspaceAskCalls.length);
	await page.keyboard.press('Control+[');
	await expect(switcher).toContainText('General');
	await expect(page.getByText('Scoped to general.')).toBeVisible();
	await page.keyboard.press('Control+]');
	await expect(switcher).toContainText('General 2');
	expect(await page.evaluate(() => window.__workspaceAskCalls.length)).toBe(conversationsAfterQuestion);
	await page.keyboard.press('Control+1');
	await expect(switcher).toContainText('General');

	const workspaceNavigation = page.getByRole('region', { name: 'Workspaces' });
	await workspaceNavigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(switcher).toContainText('General +1');
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'General workspace' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(2);
	const salesTile = page.getByRole('article', { name: 'northwind-sales workspace' });
	const generalWorkspaceTile = page.getByRole('article', { name: 'General workspace' });
	const salesFocus = salesTile.locator('.tile-focus');
	const salesClose = salesTile.getByRole('button', { name: 'Close northwind-sales workspace' });
	const generalPreview = generalWorkspaceTile.locator('.tile-preview');
	await expect(salesFocus).toHaveAttribute('data-slot', 'button');
	await expect(salesClose).toHaveAttribute('data-slot', 'button');
	await expect(salesClose).toHaveCSS('width', '26px');
	await expect(salesClose).toHaveCSS('height', '26px');
	await expect(generalPreview).toHaveAttribute('data-slot', 'button');
	await generalPreview.click();
	await expect(generalWorkspaceTile.locator('.tile-focus')).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('combobox', { name: 'Ask a question' })).toBeVisible();
	await salesFocus.click();
	await expect(salesFocus).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	const salesWorkspaceRow = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await expect(page.getByRole('button', { name: 'Open sources in northwind-sales' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Create project for northwind-sales' })).toBeVisible();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
	await salesWorkspaceRow.click();
	await salesWorkspaceRow.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();
	await expect(switcher).toContainText('General +1');
	const salesComposer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await salesComposer.fill('Summarize sales trends');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	const salesCall = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(salesCall.workspaceId).toBe(folders[0].path);

	const generalTile = page.getByRole('article', { name: 'General workspace' });
	await generalTile.locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('How should I compare two trend lines?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to general.').last()).toBeVisible();
	expect((await page.evaluate(() => window.__workspaceAskCalls.at(-1))).workspaceId).toBeNull();
	await expect(switcher).toContainText('General +1');

	await salesWorkspaceRow.click();

	await page.getByRole('button', { name: 'Create project for northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'Create a project' })).toBeVisible();
	await expect(page.getByLabel('Repository', { exact: true })).toHaveValue(folders[0].path);
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales review');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Open project Sales review' })).toBeVisible();

	await salesWorkspaceRow.click();

	await dispatchWorkspaceDrag(page, 'dragover', folders[1].path, { x: 0.9, y: 0.5 });
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(3);
	const healthTarget = page.locator('.placement-preview .preview-slot.target');
	await expect(healthTarget).toHaveText('Drop to place');
	const healthTargetBox = await healthTarget.boundingBox();
	await dispatchWorkspaceDrag(page, 'drop', folders[1].path, { x: 0.9, y: 0.5 });
	const sales = page.getByRole('article', { name: 'northwind-sales workspace' });
	const health = page.getByRole('article', { name: 'health-journal workspace' });
	await expect(sales).toBeVisible();
	await expect(health).toBeVisible();
	await expect(page.getByRole('button', { name: 'Open sources in health-journal' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(3);
	await expect(page.getByRole('combobox', { name: 'Workspace arrangement' })).toHaveCount(0);
	const healthBox = await health.boundingBox();
	expectSameRenderedBox(healthBox, healthTargetBox);

	await dispatchWorkspaceDrag(page, 'dragover', folders[2].path, { x: 0.1, y: 0.5 });
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(4);
	const budgetTargetBox = await page.locator('.placement-preview .preview-slot.target').boundingBox();
	await dispatchWorkspaceDrag(page, 'drop', folders[2].path, { x: 0.1, y: 0.5 });
	const budget = page.getByRole('article', { name: 'weekly-budget workspace' });
	await expect(budget).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(4);
	const budgetBox = await budget.boundingBox();
	expectSameRenderedBox(budgetBox, budgetTargetBox);
	const otherBoxes = await Promise.all([sales, health, page.getByRole('article', { name: 'General workspace' })].map((tile) => tile.boundingBox()));
	const allFourBoxes = [budgetBox, ...otherBoxes];
	expect(allFourBoxes.every(Boolean)).toBe(true);
	// Four panes should occupy each cell of a balanced 2×2, regardless of
	// which workspace identity occupies each cell after prior drag/reordering.
	expect(new Set(allFourBoxes.map((box) => Math.round(box.x))).size).toBe(2);
	expect(new Set(allFourBoxes.map((box) => Math.round(box.y))).size).toBe(2);
	expect(new Set(allFourBoxes.map((box) => `${Math.round(box.x)},${Math.round(box.y)}`)).size).toBe(4);

	await dispatchWorkspaceDrag(page, 'dragover', folders[0].path, { x: 0.1, y: 0.1, existing: true });
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(4);
	const salesTargetBox = await page.locator('.placement-preview .preview-slot.target').boundingBox();
	await dispatchWorkspaceDrag(page, 'drop', folders[0].path, { x: 0.1, y: 0.1, existing: true });
	const movedSalesBox = await sales.boundingBox();
	expectSameRenderedBox(movedSalesBox, salesTargetBox);
});

test('the four-pane limit is scoped to one environment and closing a pane frees its slot', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const workspaceNavigation = page.getByRole('region', { name: 'Workspaces' });
	await workspaceNavigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article')).toHaveCount(2); // General and northwind-sales
	for (let index = 1; index < 3; index += 1) {
		await dispatchWorkspaceDrag(page, 'drop', folders[index].path);
		await expect(page.getByRole('article')).toHaveCount(index + 2);
	}
	await expect(page.getByRole('article')).toHaveCount(4); // General plus three repositories
	const fourth = folders[3];
	const fifth = folders[4];

	await dispatchWorkspaceDrag(page, 'dragover', fourth.path);
	await expect(page.locator('.placement-preview .full-notice')).toContainText('Close a workspace');
	const beforeDrop = await page.getByRole('article').count();
	await dispatchWorkspaceDrag(page, 'drop', fourth.path);
	await expect(page.getByText(/environment already has four workspaces/i)).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(beforeDrop);

	const closeGeneral = page.getByRole('button', { name: 'Close General workspace' });
	await expect(closeGeneral).toHaveAttribute('data-slot', 'button');
	await closeGeneral.click();
	await expect(page.getByRole('article')).toHaveCount(3);
	await dispatchWorkspaceDrag(page, 'drop', fourth.path);
	await expect(page.getByRole('article', { name: 'travel-plans workspace' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(4);

	await dispatchWorkspaceDrag(page, 'drop', fifth.path);
	await expect(page.getByText(/environment already has four workspaces/i)).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(4);

	const closeSales = page.getByRole('button', { name: 'Close northwind-sales workspace' });
	await expect(closeSales).toHaveAttribute('data-slot', 'button');
	await closeSales.click();
	await expect(page.getByRole('article')).toHaveCount(3);
	await dispatchWorkspaceDrag(page, 'drop', fifth.path);
	await expect(page.getByRole('article', { name: 'reading-notes workspace' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(4);

	await page.reload();
	await expect(page.getByRole('article')).toHaveCount(4);
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'reading-notes workspace' })).toBeVisible();
});

test('Composer source search filters and attaches a workspace source', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const workspaceNavigation = page.getByRole('region', { name: 'Workspaces' });
	await workspaceNavigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
	const salesWorkspaceRow = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await salesWorkspaceRow.click();
	await salesWorkspaceRow.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();

	const addSource = page.getByRole('button', { name: 'Add a source or field to this question' });
	await expect(addSource).toHaveAttribute('data-slot', 'button');
	await addSource.click();
	const closePicker = page.getByRole('button', { name: 'Close context picker' });
	await expect(closePicker).toHaveAttribute('data-slot', 'button');
	await closePicker.click();
	await expect(page.getByRole('textbox', { name: 'Find a source or field' })).toHaveCount(0);
	await addSource.click();
	const search = page.getByRole('textbox', { name: 'Find a source or field' });
	await expect(search).toBeFocused();
	await expect(search).toHaveClass(/fella-ui-input/);
	await search.fill('sales.csv');
	const source = page.locator('.context-main').filter({ hasText: 'sales.csv' });
	await expect(source).toBeVisible();
	await expect(source).toHaveAttribute('data-slot', 'button');
	await source.click();
	const reference = page.locator('.ref-pill').filter({ hasText: 'sales.csv' });
	await expect(reference).toBeVisible();
	const removeReference = reference.getByRole('button', { name: 'Remove sales.csv from this question' });
	await expect(removeReference).toHaveAttribute('data-slot', 'button');
	await removeReference.click();
	await expect(reference).toHaveCount(0);
});

test('Composer model search filters the available models', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const modelTrigger = page.locator('.model-trigger');
	await expect(modelTrigger).toBeVisible();
	await expect(modelTrigger).toHaveAttribute('data-slot', 'button');
	await modelTrigger.click();
	const search = page.getByRole('textbox', { name: 'Find a model' });
	await expect(search).toBeFocused();
	await expect(search).toHaveClass(/fella-ui-input/);
	await search.fill('no-such-model');
	await expect(page.getByText('No models available from openai.')).toBeVisible();
	await search.fill('gpt-4.1-mini');
	const modelOption = page.getByRole('option', { name: 'gpt-4.1-mini' });
	await expect(modelOption).toBeVisible();
	await expect(modelOption).toHaveAttribute('data-slot', 'button');
	await modelOption.click();
	await expect(page.getByRole('dialog', { name: 'Choose a model' })).toHaveCount(0);
	await modelTrigger.click();
	const providerSettings = page.getByRole('button', { name: 'Provider settings' });
	await expect(providerSettings).toHaveAttribute('data-slot', 'button');
	await providerSettings.click();
	await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
});

test('Composer command completion uses shared buttons and keeps keyboard selection', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	const workspace = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await workspace.click();
	await workspace.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();

	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('/fi');
	const completion = page.getByRole('option', { name: /\/files/ });
	await expect(completion).toHaveAttribute('data-slot', 'button');
	await expect(completion).toHaveAttribute('aria-selected', 'false');
	await page.keyboard.press('ArrowDown');
	await expect(completion).toHaveAttribute('aria-selected', 'true');
	await page.keyboard.press('Enter');
	await expect(composer).toHaveValue('/files ');
	await expect(page.getByRole('listbox', { name: 'completions' })).toHaveCount(0);
});

test('environment dropdown saves different workspace arrangements without leaking panes between them', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const navigation = page.getByRole('region', { name: 'Workspaces' });
	await navigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();

	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	await switcher.click();
	await page.getByRole('menuitem', { name: /New environment/ }).click();
	await expect(switcher).toContainText('General');
	await navigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();

	await switcher.click();
	await page.getByRole('menuitemradio').filter({ hasText: 'northwind-sales' }).click();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toHaveCount(0);
	await expect(page.getByRole('article', { name: 'General workspace' })).toBeVisible();

	await switcher.click();
	await page.getByRole('menuitemradio').filter({ hasText: 'health-journal' }).click();
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toHaveCount(0);
	await expect(page.getByRole('article', { name: 'General workspace' })).toBeVisible();

	await page.reload();
	const restoredSwitcher = page.locator('.sidebar .environment-switcher-trigger');
	await restoredSwitcher.click();
	await expect(page.getByRole('menuitemradio').filter({ hasText: 'health-journal' })).toHaveAttribute('aria-checked', 'true');
	await page.keyboard.press('Escape');
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toHaveCount(0);
});

test('environment dropdown creates and closes the current environment', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	await expect(switcher).toHaveAttribute('aria-haspopup', 'menu');
	await switcher.click();
	await page.getByRole('menuitem', { name: /New environment/ }).click();
	await expect(switcher).toContainText('General 2');

	await switcher.click();
	await expect(page.getByRole('menuitemradio')).toHaveCount(2);
	await page.getByRole('menuitem', { name: /Close current environment/ }).click();
	await expect(switcher).toContainText('General');
	await switcher.click();
	await expect(page.getByRole('menuitemradio')).toHaveCount(1);
	await expect(page.getByRole('menuitemradio').first()).toHaveAttribute('aria-checked', 'true');
});

test('environment dropdown supports keyboard selection and focus restoration', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	await switcher.click();
	await page.getByRole('menuitem', { name: /New environment/ }).click();

	await switcher.focus();
	await page.keyboard.press('Enter');
	const menu = page.getByRole('menu');
	await expect(menu).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(menu).toHaveCount(0);
	await expect(switcher).toBeFocused();

	await page.keyboard.press('Enter');
	await page.keyboard.press('Home');
	const firstEnvironment = page.getByRole('menuitemradio').first();
	await expect(firstEnvironment).toBeFocused();
	await page.keyboard.press('Enter');
	await expect(switcher).toContainText('General');
	await expect(switcher).toBeFocused();
});

test('environment dropdown remains available when the sidebar is collapsed', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const sidebarSwitcher = page.locator('.sidebar .environment-switcher-trigger');
	await sidebarSwitcher.click();
	await page.getByRole('menuitem', { name: /New environment/ }).click();
	await page.getByRole('button', { name: 'Collapse sidebar' }).click();

	const titlebarSwitcher = page.locator('.titlebar .environment-switcher-trigger');
	await expect(titlebarSwitcher).toBeVisible();
	await expect(titlebarSwitcher).toContainText('General 2');
	await titlebarSwitcher.click();
	await page.getByRole('menuitemradio').first().click();
	await expect(titlebarSwitcher.locator('.environment-switcher-label')).toHaveText('General');
	await expect(page.locator('.sidebar')).toHaveCount(0);
});

test('Composer mode selection uses the shared radio menu interaction', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const modeTrigger = page.getByRole('button', { name: 'Ask', exact: true });
	await modeTrigger.click();
	const menu = page.getByRole('menu');
	const askOption = page.getByRole('menuitemradio', { name: /^Ask/ });
	const inspectOption = page.getByRole('menuitemradio', { name: /^Check data/ });
	await expect(menu).toBeVisible();
	await expect(askOption).toHaveAttribute('aria-checked', 'true');
	await page.keyboard.press('Escape');
	await expect(menu).toHaveCount(0);
	await expect(modeTrigger).toBeFocused();

	await page.keyboard.press('Enter');
	await expect(askOption).toBeFocused();
	await page.keyboard.press('ArrowDown');
	await expect(inspectOption).toBeFocused();
	await page.keyboard.press('Enter');
	const selectedModeTrigger = page.getByRole('button', { name: 'Check data', exact: true });
	await expect(selectedModeTrigger).toContainText('Check data');
	await expect(selectedModeTrigger).toHaveAttribute('aria-expanded', 'false');
});

test('many environments remain in a bounded, scrollable dropdown', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	const create = page.getByRole('menuitem', { name: /New environment/ });
	for (let index = 0; index < 11; index += 1) {
		await switcher.click();
		await create.click();
	}
	await switcher.click();
	const environments = page.getByRole('menuitemradio');
	await expect(environments).toHaveCount(12);
	await expect(environments.last()).toHaveAttribute('aria-checked', 'true');

	const metrics = await page.evaluate(() => {
		const menu = document.querySelector('.environment-menu-content');
		const selected = menu?.querySelector('[aria-checked="true"]');
		if (!(menu instanceof HTMLElement) || !(selected instanceof HTMLElement)) return null;
		const menuRect = menu.getBoundingClientRect();
		const selectedRect = selected.getBoundingClientRect();
		return {
			menuHeight: menu.clientHeight,
			menuScrollHeight: menu.scrollHeight,
			selectedVisible: selectedRect.top >= menuRect.top - 1 && selectedRect.bottom <= menuRect.bottom + 1,
			pageWidth: document.documentElement.scrollWidth,
			viewportWidth: window.innerWidth
		};
	});
	expect(metrics).not.toBeNull();
	expect(metrics.menuScrollHeight).toBeGreaterThan(metrics.menuHeight);
	expect(metrics.selectedVisible).toBe(true);
	expect(metrics.pageWidth).toBeLessThanOrEqual(metrics.viewportWidth);
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
	const reconnect = page.getByRole('button', { name: 'Reconnect unavailable-archive' });
	await expect(reconnect).toHaveAttribute('data-slot', 'button');
	await expect(reconnect).toHaveCSS('width', '22px');
	await expect(reconnect).toHaveCSS('height', '22px');
	const unavailableComposer = page.getByRole('combobox', { name: 'Workspace unavailable: unavailable-archive' });
	await expect(unavailableComposer).toBeDisabled();
	await expect(page.getByRole('button', { name: 'Open workspace', exact: true })).toHaveAttribute('data-slot', 'button');
});

test('shared workspace tabs and modal/menu primitives preserve keyboard interaction', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');

	const search = page.getByRole('button', { name: 'Search' });
	await search.focus();
	await page.keyboard.press('Control+k');
	const palette = page.getByRole('dialog', { name: 'Search Fella' });
	const searchInput = page.getByRole('textbox', { name: 'Search Fella' });
	await expect(palette).toBeVisible();
	await expect(searchInput).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(palette).toHaveCount(0);
	await expect(search).toBeFocused();

	const navigation = page.getByRole('region', { name: 'Workspaces' });
	await navigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();

	const workspaceTabs = page.getByRole('tablist', { name: 'Workspace sections' });
	const sourcesTab = workspaceTabs.getByRole('tab', { name: 'Sources' });
	const guideTab = workspaceTabs.getByRole('tab', { name: 'Guide' });
	await expect(sourcesTab).toHaveAttribute('aria-selected', 'true');
	await sourcesTab.focus();
	await page.keyboard.press('ArrowRight');
	await expect(guideTab).toHaveAttribute('aria-selected', 'true');
	await expect(page.getByRole('heading', { name: 'Guide', level: 1 })).toBeVisible();
	await expect(page.getByRole('textbox', { name: 'Workspace guide' })).toBeVisible();

	const createProject = page.getByRole('button', { name: 'Create project for northwind-sales' });
	await createProject.click();
	const projectDialog = page.getByRole('dialog', { name: 'Create a project' });
	const projectName = page.locator('#project-name');
	await expect(projectDialog).toBeVisible();
	await expect(projectName).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(projectDialog).toHaveCount(0);
	await expect(createProject).toBeFocused();

	await navigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	const priorRepository = page.locator('.repository').filter({ hasText: 'northwind-sales' });
	await priorRepository.hover();
	const repositoryActions = priorRepository.getByRole('button', { name: 'Repository actions' });
	await repositoryActions.click();
	await expect(repositoryActions).toHaveAttribute('aria-expanded', 'true');
	const menuItem = page.getByRole('menuitem', { name: 'Hide repository' });
	await expect(menuItem).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(menuItem).toHaveCount(0);
	await expect(priorRepository.getByRole('button', { name: 'Repository actions' })).toBeFocused();
});

test('Sidebar navigation actions use shared buttons and retain compact keyboard access', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const newConversation = page.getByRole('button', { name: 'New conversation' });
	const search = page.getByRole('button', { name: 'Search' });
	const settings = page.getByRole('button', { name: 'Settings' });

	for (const action of [newConversation, search, settings]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
		expect(await action.evaluate((element) => getComputedStyle(element).justifyContent)).toBe('flex-start');
		const bounds = await action.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds.height).toBeLessThanOrEqual(32);
	}

	await search.focus();
	await page.keyboard.press('Control+k');
	const palette = page.getByRole('dialog', { name: 'Search Fella' });
	await expect(palette).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(palette).toHaveCount(0);
	await expect(search).toBeFocused();

	await settings.click();
	await expect(settings).toHaveAttribute('aria-current', 'page');
});

test('Sidebar icon actions use shared compact buttons and Add repository still opens the board', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const collapse = page.getByRole('button', { name: 'Collapse sidebar' });
	const addRepository = page.getByRole('button', { name: 'Add repository' });

	for (const action of [collapse, addRepository]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
		const bounds = await action.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds.width).toBeLessThanOrEqual(32);
		expect(bounds.height).toBeLessThanOrEqual(32);
	}

	await addRepository.click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
});

test('Titlebar Fella actions use shared buttons and remain clickable', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const paletteTrigger = page.locator('.titlebar .hint');
	await expect(paletteTrigger).toHaveAttribute('data-slot', 'button');
	await paletteTrigger.click();
	const palette = page.getByRole('dialog', { name: 'Search Fella' });
	await expect(palette).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(palette).toHaveCount(0);
	await expect(paletteTrigger).toBeFocused();

	await page.getByRole('button', { name: 'Collapse sidebar' }).click();
	const expandSidebar = page.locator('.titlebar .navbtn');
	await expect(expandSidebar).toHaveAttribute('data-slot', 'button');
	await expandSidebar.click();
	await expect(page.getByRole('button', { name: 'Collapse sidebar' })).toBeVisible();
});

test('Sidebar workspace tools use shared buttons and preserve source and project actions', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
	const workspaceRow = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await expect(workspaceRow).toHaveAttribute('data-slot', 'button');
	const collapseWorkspace = page.getByRole('button', { name: 'Collapse northwind-sales' });
	await expect(collapseWorkspace).toHaveAttribute('data-slot', 'button');
	await collapseWorkspace.click();
	const expandWorkspace = page.getByRole('button', { name: 'Expand northwind-sales' });
	await expect(expandWorkspace).toHaveAttribute('data-slot', 'button');
	await expect(page.getByRole('button', { name: 'Open sources in northwind-sales' })).toHaveCount(0);
	await expandWorkspace.click();
	await expect(page.getByRole('button', { name: 'Open sources in northwind-sales' })).toBeVisible();

	const sources = page.getByRole('button', { name: 'Open sources in northwind-sales' });
	const createProject = page.getByRole('button', { name: 'Create project for northwind-sales' });
	for (const action of [sources, createProject]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
		const bounds = await action.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds.height).toBeLessThanOrEqual(32);
	}

	await sources.click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
	await createProject.click();
	await expect(page.getByRole('heading', { name: 'Create a project' })).toBeVisible();
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales review');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Open project Sales review' })).toBeVisible();
});

test('Sidebar workspace title stays clear of the hover new-conversation action', async ({ page }) => {
	const workspaceName = 'quarterly-revenue-analysis-workspace-with-a-long-name';
	const longFolder = {
		...folders[0],
		path: `C:\\FellaFixture\\${workspaceName}`,
		catalog: {
			...folders[0].catalog,
			workspace: `C:\\FellaFixture\\${workspaceName}`
		}
	};
	await installDesktopMock(page, { fixtureFolders: [longFolder, folders[1]] });
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Add repository' }).click();

	const workspace = page.getByRole('button', { name: `Open workspace ${workspaceName}` });
	const repository = page.locator('.repository').filter({ has: workspace });
	const title = repository.locator('.repository-name');
	await workspace.hover();

	const newConversation = repository.getByRole('button', { name: `New conversation in ${workspaceName}` });
	await expect(newConversation).toBeVisible();
	const [titleBounds, actionBounds] = await Promise.all([title.boundingBox(), newConversation.boundingBox()]);
	expect(titleBounds).not.toBeNull();
	expect(actionBounds).not.toBeNull();
	expect(titleBounds.x + titleBounds.width + 4).toBeLessThanOrEqual(actionBounds.x);
});

test('Sidebar conversation actions use shared buttons and Input, cancel rename, and delete history', async ({ page }) => {
	await installDesktopMock(page, { seedUnavailableConversation: true });
	await page.goto('/');
	await page.getByRole('button', { name: 'Expand unavailable-archive' }).click();
	const conversation = page.getByRole('button', {
		name: 'Open conversation: Summarize the unavailable archive'
	});
	await expect(conversation).toBeVisible();
	await expect(conversation).toHaveAttribute('data-slot', 'button');
	const conversationBounds = await conversation.boundingBox();
	expect(conversationBounds).not.toBeNull();
	expect(conversationBounds.height).toBeLessThanOrEqual(32);
	await conversation.hover();

	const rename = page.getByRole('button', { name: 'Rename conversation' });
	const remove = page.getByRole('button', { name: 'Delete conversation' });
	for (const action of [rename, remove]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
		const bounds = await action.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds.width).toBe(24);
		expect(bounds.height).toBe(24);
	}

	await rename.click();
	const renameInput = page.getByRole('textbox', { name: 'Rename conversation' });
	await expect(renameInput).toHaveAttribute('data-slot', 'input');
	await expect(renameInput).toBeFocused();
	await renameInput.fill('Unsaved title');
	await page.keyboard.press('Escape');
	await expect(renameInput).toHaveCount(0);
	await expect(conversation).toBeVisible();

	await conversation.hover();
	await page.getByRole('button', { name: 'Delete conversation' }).click();
	await expect(conversation).toHaveCount(0);
});

test('Sources catalog filter uses the shared input and preserves source selection', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();

	const filter = page.getByRole('textbox', { name: 'Filter sources' });
	await expect(filter).toHaveClass(/fella-ui-input/);
	await filter.fill('no-such-source');
	await expect(page.getByText('No sources match “no-such-source”.')).toBeVisible();
	await filter.fill('sales.csv');
	const source = page.getByRole('option', { name: /sales\.csv/ });
	await expect(source).toBeVisible();
	await expect(source).toHaveAttribute('data-slot', 'button');
	await expect(source).toHaveAttribute('aria-selected', 'true');
});

test('empty Sources action uses a shared button and opens the selected folder', async ({ page }) => {
	const emptyFolder = {
		...folders[0],
		catalog: { ...folders[0].catalog, revision: 'empty-r1', sources: [] }
	};
	await installDesktopMock(page, { fixtureFolders: [emptyFolder, folders[1]] });
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();

	const chooseAnother = page.getByRole('button', { name: 'Choose another folder' });
	await expect(chooseAnother).toHaveAttribute('data-slot', 'button');
	await chooseAnother.click();
	await expect.poll(() => page.evaluate(() => window.__workspaceOpenCalls)).toContain(folders[1].path);
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('combobox', { name: 'Ask about health-journal' })).toBeVisible();
});

test('Settings actions use shared buttons and folder selection still works', async ({ page }) => {
	await installDesktopMock(page, {
		runLogEntries: [{
			id: 'run-42',
			at_ms: 1_700_000_000_000,
			kind: 'turn',
			mode: 'workspace_ask',
			model: 'gpt-4.1-mini',
			elapsed_ms: 1200,
			model_calls: [{ model: 'gpt-4.1-mini', duration_ms: 520, success: true }],
			operations: [{ operation: 'search_files', duration_ms: 38, success: true }],
			prior_analysis_count: 0,
			context_reference_count: 0,
			clarification_continuation: false,
			rerun: false,
			outcome: 'complete'
		}],
		providerList: [
			{ id: 'openai', display: 'OpenAI', auth: 'key', base_url: 'https://api.openai.com/v1', get_key_url: '', embeddings: true, authed: true, current: true },
			{ id: 'anthropic', display: 'Anthropic', auth: 'key', base_url: 'https://api.anthropic.com', get_key_url: '', embeddings: false, authed: false, current: false }
		]
	});
	await page.goto('/');
	await page.getByRole('button', { name: 'Settings' }).click();
	const runEntry = page.locator('.run-log-entry');
	await expect(runEntry).toHaveCount(1);
	const runEntryToggle = runEntry.getByRole('button');
	await expect(runEntryToggle).toHaveAttribute('aria-expanded', 'false');
	await runEntryToggle.focus();
	await page.keyboard.press('Enter');
	await expect(runEntryToggle).toHaveAttribute('aria-expanded', 'true');
	await expect(runEntry).toContainText('Tool steps');
	await expect(runEntry).toContainText('search_files');
	await page.keyboard.press('Space');
	await expect(runEntryToggle).toHaveAttribute('aria-expanded', 'false');
	await expect(runEntry.locator('.run-log-detail')).not.toBeVisible();

	for (const name of ['Change', 'Choose', 'Privacy and security', 'Refresh connection status', 'Choose a folder', 'Refresh']) {
		await expect(page.getByRole('button', { name, exact: true })).toHaveAttribute('data-slot', 'button');
	}
	const providers = page.locator('.provider-row');
	await expect(providers).toHaveCount(2);
	for (const provider of await providers.all()) await expect(provider).toHaveAttribute('data-slot', 'button');
	await expect(page.getByRole('button', { name: 'Anthropic Connect' })).toHaveAttribute('data-slot', 'button');
	const appearance = page.locator('.appearance-row');
	await expect(appearance).toHaveCount(3);
	for (const option of await appearance.all()) await expect(option).toHaveAttribute('data-slot', 'button');
	const darkAppearance = appearance.filter({ hasText: 'Dark' });
	await expect(darkAppearance).toHaveAttribute('aria-pressed', 'false');
	await darkAppearance.click();
	await expect(darkAppearance).toHaveAttribute('aria-pressed', 'true');
	await expect(page.locator('html')).toHaveAttribute('data-appearance', 'dark');

	await page.getByRole('button', { name: 'Choose a folder' }).click();
	await expect.poll(() => page.evaluate(() => window.__workspaceOpenCalls)).toContain(folders[0].path);
});

test('CommandPalette filters and results use shared buttons without losing search behavior', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Search' }).click();
	const palette = page.getByRole('dialog', { name: 'Search Fella' });
	const searchInput = page.getByRole('textbox', { name: 'Search Fella' });
	await searchInput.fill('northwind-sales');

	const repositoryFilter = palette.getByRole('button', { name: 'Repositories', exact: true });
	await expect(repositoryFilter).toHaveAttribute('data-slot', 'button');
	await expect(repositoryFilter).toHaveAttribute('aria-pressed', 'false');
	await repositoryFilter.click();
	await expect(repositoryFilter).toHaveAttribute('aria-pressed', 'true');
	const result = palette.locator('.search-result');
	await expect(result).toHaveCount(1);
	await expect(result).toHaveAttribute('data-slot', 'button');
	await expect(result).toContainText('northwind-sales');
	await result.click();
	await expect(palette).toHaveCount(0);
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
});

test('Sources pagination uses shared buttons and preserves catalog and skipped-file boundaries', async ({ page }) => {
	const paginatedFolders = [...folders];
	const salesFolder = folders[0];
	const paginatedCatalog = {
		...salesFolder.catalog,
		sources: Array.from({ length: 205 }, (_, index) => {
			const number = String(index + 1).padStart(3, '0');
			return {
				name: `sales-${number}.csv`,
				path: `${salesFolder.path}\\sales-${number}.csv`,
				kind: 'csv',
				view: `sales_${number}`,
				row_count: 10,
				size_bytes: 1024,
				mtime: 1
			};
		}),
		skipped: Array.from({ length: 105 }, (_, index) => ({
			name: `unsupported-${String(index + 1).padStart(3, '0')}.bin`,
			reason: 'Unsupported file type'
		}))
	};
	paginatedFolders[0] = { ...salesFolder, catalog: paginatedCatalog };

	await installDesktopMock(page, { fixtureFolders: paginatedFolders });
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();

	const sourcePages = page.getByRole('navigation', { name: 'Source pages' });
	const sourcePrevious = sourcePages.getByRole('button', { name: 'Previous' });
	const sourceNext = sourcePages.getByRole('button', { name: 'Next' });
	for (const action of [sourcePrevious, sourceNext]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
	}
	await expect(sourcePages).toContainText('1–100 of 205');
	await expect(sourcePrevious).toBeDisabled();
	await sourceNext.click();
	await expect(sourcePages).toContainText('101–200 of 205');
	await expect(page.getByRole('option').first()).toContainText('sales-101.csv');
	await sourceNext.click();
	await expect(sourcePages).toContainText('201–205 of 205');
	await expect(sourceNext).toBeDisabled();
	await expect(page.getByRole('option')).toHaveCount(5);

	const skippedFiles = page.locator('.skipped');
	const skippedToggle = skippedFiles.getByRole('button', { name: '105 skipped files Why?' });
	await skippedToggle.click();
	await expect(skippedToggle).toHaveAttribute('aria-expanded', 'true');
	const skippedFilter = page.getByRole('textbox', { name: 'Filter skipped files' });
	await expect(skippedFilter).toHaveAttribute('data-slot', 'input');
	await skippedFilter.fill('unsupported-001');
	await expect(skippedFiles.locator('.skipped-count')).toHaveText('1–1 of 1 files');
	await expect(skippedFiles.getByText('unsupported-001.bin')).toBeVisible();
	await skippedFilter.fill('');
	await expect(skippedFiles.locator('.skipped-count')).toHaveText('1–100 of 105 files');

	const skippedPages = page.getByRole('navigation', { name: 'Skipped file pages' });
	const skippedPrevious = skippedPages.getByRole('button', { name: 'Previous' });
	const skippedNext = skippedPages.getByRole('button', { name: 'Next' });
	for (const action of [skippedPrevious, skippedNext]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
	}
	await expect(skippedPages).toContainText('1–100 of 105');
	await expect(skippedPrevious).toBeDisabled();
	await skippedNext.click();
	await expect(skippedPages).toContainText('101–105 of 105');
	await expect(skippedNext).toBeDisabled();
	await skippedToggle.click();
	await expect(skippedToggle).toHaveAttribute('aria-expanded', 'false');
	await expect(skippedFilter).not.toBeVisible();
});

test('Project title and wiki use shared editing controls and retain saved content', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Create project for northwind-sales' }).click();
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales notes');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	await page.getByRole('button', { name: 'Open project Sales notes' }).click();

	const title = page.getByRole('textbox', { name: 'Project name' });
	const wiki = page.getByRole('textbox', { name: 'Project wiki' });
	await expect(title).toHaveClass(/fella-ui-input/);
	await expect(wiki).toHaveClass(/fella-ui-textarea/);
	await title.fill('Sales field guide');
	await title.press('Tab');
	await expect(page.getByRole('button', { name: 'Open project Sales field guide' })).toBeVisible();

	await wiki.fill('Net revenue excludes refunds and chargebacks.');
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Open project Sales field guide' }).click();
	await expect(page.getByRole('textbox', { name: 'Project wiki' })).toHaveValue(
		'Net revenue excludes refunds and chargebacks.'
	);
});

test('Project actions use shared buttons and retain their navigation and delete behavior', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Create project for northwind-sales' }).click();
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales actions');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	const openProject = page.getByRole('button', { name: 'Open project Sales actions' });
	await openProject.click();

	const askRepository = page.getByRole('button', { name: 'Ask repository' });
	const sources = page.getByRole('button', { name: 'Sources', exact: true });
	const deleteProject = page.getByRole('button', { name: 'Delete project' });
	for (const action of [askRepository, sources, deleteProject]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
	}

	await sources.click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
	await openProject.click();
	await askRepository.click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();

	await openProject.click();
	const dialogPromise = page.waitForEvent('dialog');
	const confirmationCheck = dialogPromise.then(async (confirmation) => {
		expect(confirmation.message()).toContain('Delete the local project');
		await confirmation.accept();
	});
	await Promise.all([deleteProject.click(), confirmationCheck]);
	await expect(openProject).toHaveCount(0);
});

test('Analysis details uses the shared button and preserves disclosure state', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Summarize the workspace.');
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await expect(answer).toContainText('Scoped to general.');
	const details = answer.getByRole('button', { name: 'Analysis details' });
	await expect(details).toHaveAttribute('data-slot', 'button');
	await expect(details).toHaveAttribute('aria-expanded', 'false');

	await details.click();
	await expect(details).toHaveAttribute('aria-expanded', 'true');
	await expect(answer).toContainText('Source details were not recorded for this archived answer.');

	await details.click();
	await expect(details).toHaveAttribute('aria-expanded', 'false');
	await expect(page.getByText('Source details were not recorded for this archived answer.')).toHaveCount(0);
});

test('Evidence query disclosure uses a shared button and preserves expansion behavior', async ({ page }) => {
	await installDesktopMock(page, {
		detailEvidence: {
			id: 'fixture-query',
			tool: 'run_sql',
			args: { query: 'SELECT total FROM sales' },
			sql: 'SELECT total FROM sales',
			result_summary: 'One row returned.',
			ms: 8
		}
	});
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Show the evidence.');
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await answer.getByRole('button', { name: 'Analysis details' }).click();
	const reveal = answer.locator('.detailtoggle');
	await expect(reveal).toHaveAttribute('data-slot', 'button');
	await expect(reveal).toHaveText('show the query');
	await expect(reveal).toHaveAttribute('aria-expanded', 'false');
	await reveal.click();
	await expect(reveal).toHaveAttribute('aria-expanded', 'true');
	await expect(reveal).toHaveText('hide');
	await expect(answer.locator('pre.sql')).toContainText('SELECT total FROM sales');
	await reveal.click();
	await expect(reveal).toHaveAttribute('aria-expanded', 'false');
	await expect(answer.locator('pre.sql')).toHaveCount(0);
});

test('Transcript setup links use shared buttons and retain the sign-in action', async ({ page }) => {
	await installDesktopMock(page, {
		providerHealth: { reachable: false, rejected: true, models: [] }
	});
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Check my data.');
	await page.getByRole('button', { name: 'Send' }).click();

	const setup = page.locator('.setup.compact');
	await expect(setup).toContainText('key was refused');
	const enterKey = setup.getByRole('button', { name: 'Enter a new key' });
	await expect(enterKey).toHaveAttribute('data-slot', 'button');
	await enterKey.click();
	await expect(page.getByText('unknown provider: openai')).toBeVisible();
});

test('Replay freshness uses shared status badges and rerun buttons', async ({ page }) => {
	await installDesktopMock(page, {
		replayStatus: {
			turn_id: 'fixture-turn',
			same_workspace: true,
			revision_changed: true,
			snapshot_available: true,
			can_rerun: true,
			source_changes: [{ name: 'sales.csv', kind: 'changed', details: ['Rows changed'] }]
		}
	});
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Check freshness.');
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await expect(answer).toContainText('Scoped to general.');
	await answer.getByRole('button', { name: 'Analysis details' }).click();

	const freshness = answer.locator('[data-slot="badge"]');
	await expect(freshness).toHaveAttribute('class', /fella-ui-badge-warning/);
	await expect(freshness).toContainText('1 source changed since this answer');
	const rerun = answer.getByRole('button', { name: 'Rerun' });
	await expect(rerun).toHaveAttribute('data-slot', 'button');
	await expect(rerun).toBeEnabled();
});

test('Replay freshness lookup errors use an alert and a working retry action', async ({ page }) => {
	await installDesktopMock(page, { replayStatusError: true });
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Check freshness.');
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await expect(answer).toContainText('Scoped to general.');
	await answer.getByRole('button', { name: 'Analysis details' }).click();

	const alert = answer.getByRole('alert');
	await expect(alert).toContainText("Couldn't check freshness.");
	await expect(alert).toContainText('Replay lookup unavailable.');
	const retry = alert.getByRole('button', { name: 'Try again' });
	await expect(retry).toHaveAttribute('data-slot', 'button');
	await retry.click();
	await expect.poll(() => page.evaluate(() => window.__replayStatusCalls)).toBe(2);
});

test('Sources open-beside action uses the shared button and preserves conversation scope', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();

	const openBeside = page.getByRole('button', { name: 'Open source preview beside the active conversation' });
	await expect(openBeside).toHaveAttribute('data-slot', 'button');
	await openBeside.click();

	const companion = page.getByRole('complementary', { name: 'Companion pane' });
	await expect(companion).toBeVisible();
	await expect(companion.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
	await expect(companion).toContainText('Preview only · this file is not added to the conversation context.');
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
});

test('Companion snapshot recovery uses a shared button and reopens the current source version', async ({ page }) => {
	await installDesktopMock(page, {
		reindexedCatalog: { ...folders[0].catalog, revision: 'sales-r2' }
	});
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Open source preview beside the active conversation' }).click();

	const companion = page.getByRole('complementary', { name: 'Companion pane' });
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('/reindex');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(companion).toContainText('The workspace changed after this preview was opened.');

	const reopen = companion.getByRole('button', { name: 'Open current version' });
	await expect(reopen).toHaveAttribute('data-slot', 'button');
	await reopen.click();
	await expect(companion).not.toContainText('The workspace changed after this preview was opened.');
	await expect(companion).toContainText('Preview only · this file is not added to the conversation context.');
});

test('Chart disclosures use Collapsible and Open beside preserves the chart in the companion pane', async ({ page }) => {
	await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: 'http://127.0.0.1:1421' });
	await installDesktopMock(page, {
		chartEvidence: {
			id: 'fixture-chart',
			tool: 'make_chart',
			args: {},
			result_summary: 'Monthly revenue by month.',
			ms: 12,
			chart: {
				kind: 'line',
				title: 'Monthly revenue',
				labels: ['Jan', 'Feb', 'Mar'],
				series: [{ name: 'Revenue', values: [12, 18, 15] }],
				unit: '$',
				x_label: 'Month',
				y_label: 'Revenue',
				metadata: {
					source_label: 'sales.csv',
					fields: ['month', 'revenue'],
					aggregation: 'sum',
					filters: [],
					part_to_whole: false
				}
			}
		}
	});
	await page.goto('/');
	const composer = page.getByRole('combobox', { name: 'Ask a question' });
	await composer.fill('Show monthly revenue.');
	await composer.press('Enter');

	const chart = page.locator('.chart-card');
	await expect(chart).toContainText('Monthly revenue');
	const chartDetails = chart.getByRole('button', { name: 'Chart details' });
	await expect(chartDetails).toHaveAttribute('aria-expanded', 'false');
	await chartDetails.click();
	await expect(chartDetails).toHaveAttribute('aria-expanded', 'true');
	await expect(chart).toContainText('sales.csv');
	const exactValues = chart.getByRole('button', { name: 'Show exact values' });
	await exactValues.click();
	await expect(exactValues).toHaveAttribute('aria-expanded', 'true');
	const exactTable = chart.getByRole('table', { name: 'Monthly revenue values' });
	await expect(exactTable).toContainText('$18');

	const exportTrigger = chart.getByRole('button', { name: 'Export chart and data' });
	await exportTrigger.click();
	await page.getByRole('menuitem', { name: 'Copy chart image' }).click();
	await expect(chart.getByRole('status')).toHaveText('Chart image copied to clipboard.');
	const copiedImageTypes = await page.evaluate(async () =>
		(await navigator.clipboard.read()).flatMap((item) => item.types)
	);
	expect(copiedImageTypes).toContain('image/png');

	await exportTrigger.click();
	const pngDownloadPromise = page.waitForEvent('download');
	await page.getByRole('menuitem', { name: 'Download chart image (PNG)' }).click();
	const pngDownload = await pngDownloadPromise;
	expect(pngDownload.suggestedFilename()).toBe('Monthly-revenue.png');
	const pngPath = await pngDownload.path();
	const pngBytes = await readFile(pngPath);
	expect([...pngBytes.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);

	await exportTrigger.click();
	await page.getByRole('menuitem', { name: 'Copy data' }).click();
	await expect(chart.getByRole('status')).toHaveText('Chart data copied to clipboard.');
	expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
		'Month\tRevenue ($)\r\nJan\t12\r\nFeb\t18\r\nMar\t15'
	);

	await exportTrigger.click();
	const csvDownloadPromise = page.waitForEvent('download');
	await page.getByRole('menuitem', { name: 'Download data (CSV)' }).click();
	const csvDownload = await csvDownloadPromise;
	expect(csvDownload.suggestedFilename()).toBe('Monthly-revenue.csv');
	const csvPath = await csvDownload.path();
	expect(await readFile(csvPath, 'utf8')).toBe('\uFEFFMonth,Revenue ($)\r\nJan,12\r\nFeb,18\r\nMar,15');

	const openBeside = page.getByRole('button', { name: 'Open beside' });
	await expect(openBeside).toHaveAttribute('data-slot', 'button');
	await openBeside.click();

	const companion = page.getByRole('complementary', { name: 'Companion pane' });
	await expect(companion.getByRole('heading', { name: 'Monthly revenue' })).toBeVisible();
	await expect(page.getByLabel('Chart open beside conversation')).toContainText('Monthly revenue is open beside the conversation.');
});

test('Suggested follow-ups use shared link buttons and submit the selected question', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Summarize the workspace.');
	await page.getByRole('button', { name: 'Send' }).click();

	const firstAnswer = page.locator('.msg.assistant').last();
	await expect(firstAnswer).toContainText('Scoped to general.');
	const followup = firstAnswer.getByRole('button', { name: 'What else stands out?' });
	await expect(followup).toHaveAttribute('data-slot', 'button');
	await followup.click();

	await expect(page.locator('.msg.user').last()).toContainText('What else stands out?');
	await expect(page.locator('.msg.assistant').last()).toContainText('Scoped to general.');
});

test('Workspace starter prompts use shared outline buttons and submit the selected prompt', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	const workspace = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await workspace.click();
	await workspace.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();

	const prompts = page.locator('.examples');
	await expect(prompts).toBeVisible();
	const prompt = prompts.getByRole('button', { name: 'How did my spending change this year?' });
	await expect(prompt).toHaveAttribute('data-slot', 'button');
	await prompt.click();

	await expect(page.locator('.msg.user').last()).toContainText('How did my spending change this year?');
	await expect(page.locator('.msg.assistant').last()).toContainText('Scoped to C:\\FellaFixture\\northwind-sales.');
});

test('Companion pane close uses a shared compact button and returns to the workspace conversation', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Open source preview beside the active conversation' }).click();

	const companion = page.getByRole('complementary', { name: 'Companion pane' });
	await expect(companion).toBeVisible();
	const close = companion.getByRole('button', { name: 'Close companion pane and return to conversation' });
	await expect(close).toHaveAttribute('data-slot', 'button');
	const bounds = await close.boundingBox();
	expect(bounds).not.toBeNull();
	expect(bounds.width).toBe(30);
	expect(bounds.height).toBe(30);

	await close.click();
	await expect(companion).toHaveCount(0);
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
});
