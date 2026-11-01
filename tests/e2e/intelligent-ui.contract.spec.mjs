import { test as base, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';

// Browser contract v1.1: owner-approved asset/authority separation. v1.0's
// four lazy-font failures remain recorded; fixture inputs and UI assertions
// are unchanged. Bundled assets are reported and must stay same-origin GETs.

const frozen = JSON.parse(await readFile(new URL('../fixtures/intelligent-ui-v1.json', import.meta.url), 'utf8'));
const fixture = id => {
	const found = frozen.cases.find(item => item.id === id);
	if (!found) throw new Error(`Missing frozen fixture: ${id}`);
	return found;
};
function nodeIn(item, id) {
	const pending = [item.input.root];
	while (pending.length) {
		const node = pending.pop();
		if (node.id === id) return node;
		pending.push(...(node.children ?? []));
	}
	throw new Error(`Missing frozen node: ${id}`);
}

const test = base.extend({
	contractErrors: [async ({ page }, use) => {
		const errors = [];
		page.on('pageerror', error => errors.push(error.message));
		page.on('console', message => {
			if (message.type() === 'error' && /renderer|visualization|vega/i.test(message.text())) errors.push(message.text());
		});
		await use(errors);
		expect(errors, 'Uncaught browser errors and chart renderer errors').toEqual([]);
		expect((await observe(page)).queryCalls, 'Display interactions must not request query replay').toBe(0);
	}, { auto: true }]
});

async function settle(page) {
	// A bounded observation window for mount effects, local updates and timers.
	await page.evaluate(() => new Promise(resolve => {
		requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(resolve, 150)));
	}));
}

async function loaded(page) {
	await page.waitForFunction(() => window.__contractHarness?.ready === true);
	expect(await page.evaluate(() => window.__contractHarness.compileerror), 'Frozen case must compile with the shared validator').toBeNull();
	await page.waitForLoadState('networkidle', { timeout: 15_000 });
}

async function openCase(page, item, companion) {
	const params = new URLSearchParams({ case: item.id });
	if (companion) params.set('companion', companion.id);
	await page.goto(`/__fella_ui_contract?${params}`);
	await loaded(page);
}

async function observe(page) {
	return page.evaluate(() => window.__contractHarness);
}

function captureInteractionRequests(page) {
	// Attach only AFTER module loading and initial rendering. WASM, JS, CSS,
	// fonts and images are assets, not analytical authority. Neither category
	// should start a new request during these already-loaded local controls.
	const assets = [], authority = [];
	const listener = request => {
		const entry = { method: request.method(), type: request.resourceType(), url: request.url() };
		const pathname = new URL(request.url()).pathname;
		const asset = ['script', 'stylesheet', 'font', 'image'].includes(entry.type)
			|| (entry.method === 'GET' && pathname.endsWith('.wasm'));
		(asset ? assets : authority).push(entry);
	};
	page.on('request', listener);
	return {
		async verify() {
			await settle(page);
			page.off('request', listener);
			expect(authority, 'No new non-asset requests from display interactions').toEqual([]);
			expect(assets.every(asset => asset.method === 'GET' && new URL(asset.url).origin === new URL(page.url()).origin), 'Display assets must remain bundled same-origin GET requests').toBe(true);
			await test.info().attach('display-assets-v1.1', { body: JSON.stringify({ version: '1.1', assets, authority }), contentType: 'application/json' });
		}
	};
}

async function exactRows(page, item, tableExpectation) {
	const table = nodeIn(item, tableExpectation.node);
	const result = frozen.results.find(result => result.id === table.props.resultId);
	const fields = table.props.fields ?? result.columns;
	const expected = tableExpectation.row_indices.map(index => fields.map(field => result.exact_rows[index][result.columns.indexOf(field)]));
	const rendered = page.getByRole('table', { name: `Exact values from saved result ${result.id}`, exact: true });
	await expect(rendered).toBeVisible();
	await expect.poll(() => rendered.locator('tbody tr').evaluateAll(rows => rows.map(row =>
		Array.from(row.querySelectorAll('td'), cell => cell.textContent.trim())
	))).toEqual(expected);
	if (tableExpectation.exact_revenue) {
		const column = fields.indexOf('revenue');
		expect(expected.map(row => row[column])).toEqual(tableExpectation.exact_revenue);
	}
}

async function controlState(page, expected) {
	for (const [key, value] of Object.entries(expected)) {
		await expect.poll(async () => (await observe(page)).state[`control:${key}`]).toEqual(value);
	}
}

async function visibleConsumers(page, item, expected) {
	for (const id of [...(expected.visible_nodes ?? []), ...(expected.hidden_nodes ?? [])]) {
		const node = nodeIn(item, id);
		if (!['Metric', 'Table'].includes(node.type)) continue;
		const target = node.type === 'Metric'
			? page.getByText(node.props.title, { exact: true })
			: page.getByRole('table', { name: `Exact values from saved result ${node.props.resultId}`, exact: true });
		if (expected.visible_nodes?.includes(id)) await expect(target).toBeVisible();
		else await expect(target).toBeHidden();
	}
	if (expected.table) await exactRows(page, item, expected.table);
}

// Criteria come only from the frozen pre-candidate cases and exact saved rows.
// No candidate output is used to derive expected control values or event counts.
test('Select filters exact North/South rows and restores saved viewer state', async ({ page }) => {
	const item = fixture('select_filters_table');
	await openCase(page, item);
	const select = page.getByRole('combobox', { name: nodeIn(item, 'region_picker').props.name, exact: true });
	await expect(select).toHaveValue(item.expected.initial.state.region);
	await controlState(page, item.expected.initial.state);
	await exactRows(page, item, item.expected.initial.table);
	const requests = captureInteractionRequests(page);
	const change = item.expected.interactions.find(step => step.action === 'select');
	await select.selectOption(change.value);
	await expect(select).toHaveValue(change.value);
	await controlState(page, change.expect.state);
	await exactRows(page, item, change.expect.table);
	expect((await observe(page)).counters).toEqual({ statechange: 1, action: 0 });
	await requests.verify();

	// Reload is a genuine new page; its startup requests are outside the above capture.
	await page.reload();
	await loaded(page);
	const reopen = item.expected.interactions.find(step => step.action === 'reopen');
	await expect(select).toHaveValue(reopen.expect.state.region);
	await controlState(page, reopen.expect.state);
	await exactRows(page, item, reopen.expect.table);
	expect((await observe(page)).counters.action).toBe(0);

	const restore = item.expected.interactions.find(step => step.action === 'restore_viewer_state');
	const storageKey = (await observe(page)).storageKey;
	await page.evaluate(({ key, value }) => localStorage.setItem(key, JSON.stringify(value)), {
		key: storageKey, value: Object.fromEntries(Object.entries(restore.value).map(([key, value]) => [`control:${key}`, value]))
	});
	await page.reload();
	await loaded(page);
	await expect(select).toHaveValue(restore.expect.state.region);
	await controlState(page, restore.expect.state);
	await exactRows(page, item, restore.expect.table);
	expect((await observe(page)).counters.action).toBe(0);
});

test('SegmentedControl changes visible saved-result consumer', async ({ page }) => {
	const item = fixture('segmented_control_visible_consumer');
	await openCase(page, item);
	await visibleConsumers(page, item, item.expected.initial);
	const picker = nodeIn(item, 'view_picker');
	const declaration = item.input.state[picker.bind.state];
	const group = page.getByRole('group', { name: picker.props.name, exact: true });
	const initial = declaration.options.find(option => option.value === declaration.default);
	await expect(group.getByRole('button', { name: initial.label, exact: true })).toHaveAttribute('aria-pressed', 'true');
	const requests = captureInteractionRequests(page);
	const change = item.expected.interactions[0];
	const next = declaration.options.find(option => option.value === change.value);
	await group.getByRole('button', { name: next.label, exact: true }).click();
	await expect(group.getByRole('button', { name: next.label, exact: true })).toHaveAttribute('aria-pressed', 'true');
	await expect(group.getByRole('button', { name: initial.label, exact: true })).toHaveAttribute('aria-pressed', 'false');
	await controlState(page, { [picker.bind.state]: change.value });
	await visibleConsumers(page, item, change.expect);
	expect((await observe(page)).counters).toEqual({ statechange: 1, action: 0 });
	await requests.verify();
});

test('Checkbox shows and hides saved rows', async ({ page }) => {
	const item = fixture('checkbox_visible_consumer');
	await openCase(page, item);
	const checkbox = page.getByRole('checkbox', { name: nodeIn(item, 'detail_toggle').props.label, exact: true });
	await expect(checkbox).not.toBeChecked();
	await controlState(page, item.expected.initial.state);
	await visibleConsumers(page, item, item.expected.initial);
	const requests = captureInteractionRequests(page);
	for (const step of item.expected.interactions) {
		await checkbox.setChecked(step.value);
		await expect(checkbox).toBeChecked({ checked: step.value });
		await controlState(page, { show_detail: step.value });
		await visibleConsumers(page, item, step.expect);
	}
	expect((await observe(page)).counters).toEqual({ statechange: item.expected.interactions.length, action: 0 });
	await requests.verify();
});

test('Tabs click and ArrowRight/End/Home move selection and actual keyboard focus', async ({ page }) => {
	const item = fixture('tabs_derive_mechanics');
	await openCase(page, item);
	const labels = item.expected.derived.views.labels;
	const tabs = labels.map(label => page.getByRole('tab', { name: label, exact: true }));
	async function selection(index) {
		for (let i = 0; i < tabs.length; i++) {
			await expect(tabs[i]).toHaveAttribute('aria-selected', String(i === index));
			await expect(tabs[i]).toHaveAttribute('tabindex', i === index ? '0' : '-1');
		}
		await expect(page.getByRole('tabpanel', { name: labels[index], exact: true })).toBeVisible();
	}
	await selection(item.expected.initial.selected_index);
	await visibleConsumers(page, item, item.expected.initial);
	const requests = captureInteractionRequests(page);
	let selected = item.expected.initial.selected_index;
	for (const step of item.expected.interactions) {
		if (step.action === 'click_tab') await tabs[step.index].click();
		else await tabs[selected].press(step.key);
		selected = step.expect.selected_index;
		await selection(selected);
		if (step.expect.focused_tab_index !== undefined) await expect(tabs[step.expect.focused_tab_index]).toBeFocused();
		await visibleConsumers(page, item, step.expect);
	}
	expect((await observe(page)).counters).toEqual({ statechange: item.expected.interactions.length, action: 0 });
	await requests.verify();
});

for (const companionId of ['select_filters_table', 'tabs_derive_mechanics']) {
	test(`FollowUp fires only on explicit click, including ${companionId} state changes and reopen`, async ({ page }) => {
		const item = fixture('follow_up_requires_explicit_click');
		const companion = fixture(companionId);
		await openCase(page, item, companion);
		const button = page.getByRole('button', { name: nodeIn(item, 'ask').props.label, exact: true });
		await expect(button).toBeVisible();
		await settle(page);
		expect((await observe(page)).counters.action).toBe(item.expected.initial.follow_up_events);
		const requests = captureInteractionRequests(page);
		if (companionId === 'select_filters_table') {
			const step = companion.expected.interactions.find(step => step.action === 'select');
			await page.getByRole('combobox', { name: nodeIn(companion, step.node).props.name, exact: true }).selectOption(step.value);
			await exactRows(page, companion, step.expect.table);
		} else {
			const step = companion.expected.interactions.find(step => step.action === 'click_tab');
			await page.getByRole('tab', { name: companion.expected.derived.views.labels[step.index], exact: true }).click();
			await exactRows(page, companion, step.expect.table);
		}
		await settle(page);
		expect((await observe(page)).counters).toEqual({ statechange: 1, action: 0 });
		await requests.verify();

		await page.reload();
		await loaded(page);
		await settle(page);
		const reopened = item.expected.interactions.find(step => step.action === 'reopen');
		expect((await observe(page)).counters.action).toBe(reopened.expect.follow_up_events);
		const click = item.expected.interactions.find(step => step.action === 'click');
		const before = (await observe(page)).counters.action;
		await button.click();
		await settle(page);
		const after = await observe(page);
		expect(after.counters.action - before).toBe(click.expect.follow_up_events_delta);
		expect(after.actions).toHaveLength(click.expect.follow_up_events_delta);
		expect(after.actions[0]).toMatchObject({ type: 'continue', message: click.expect.message });
	});
}

for (const id of ['time_trend_chart', 'safe_chart_layer', 'safe_chart_facet', 'safe_chart_concat']) {
	test(`${id} renders a canvas without renderer errors`, async ({ page }) => {
		const item = fixture(id);
		await openCase(page, item);
		const node = nodeIn(item, item.expected.chart.node);
		const chart = page.getByRole('img', { name: node.props.title ?? 'Visualization of saved results', exact: true });
		await expect(chart.locator('canvas').first()).toBeVisible();
		await expect(page.getByRole('status').filter({ hasText: /Loading visualization|Visualization unavailable/i })).toHaveCount(0);
		expect(await chart.locator('canvas').first().evaluate(canvas => canvas.width > 0 && canvas.height > 0)).toBe(true);
		const requests = captureInteractionRequests(page);
		await page.getByText('Exact saved values', { exact: true }).click();
		const rows = item.expected.chart.row_indices
			?? Object.values(item.expected.chart.panels).flat().sort((a, b) => a - b);
		await exactRows(page, item, { node: item.expected.chart.node, row_indices: rows });
		expect((await observe(page)).counters.action).toBe(0);
		await requests.verify();
	});
}
