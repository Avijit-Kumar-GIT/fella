// Focused browser layout regression v1. Acceptance criteria are fixed before
// running the fixture:
// - a cached AnalyticalAnswer with artifact_id and a valid presentation_snapshot
//   renders synchronously on initial mount and reopen, without artifact IPC,
//   a "Preparing" state, or a second page navigation;
// - a real Metric/Grid/Card/Chart/Table presentation has no document-level or
//   scrollable horizontal overflow at 320px and 1440px;
// - the metric grid collapses to one column at 320px and expands at desktop;
// - Vega renders a non-empty canvas at both viewport sizes; a real point hover
//   displays a bounded, Fella-styled tooltip with result-bound values.
// Screenshots are retained in a unique /tmp directory. Vite uses a separate
// unique /tmp cache to avoid invalidating another browser script's modules.

import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'vite';
import { chromium } from '@playwright/test';

const spec = { root: 'document', elements: {
	document: { type: 'Document', props: {}, children: ['title', 'caption', 'metrics', 'card'] },
	title: { type: 'Title', props: { content: 'Quarterly sales overview' }, children: [] },
	caption: { type: 'Caption', props: { content: 'Saved result · four quarters' }, children: [] },
	metrics: { type: 'Grid', props: { columns: 3 }, children: ['q4-revenue', 'q4-orders', 'q3-revenue'] },
	'q4-revenue': { type: 'Metric', props: { resultId: 'sales', field: 'revenue', row: 3, title: 'Q4 revenue', unit: 'USD' }, children: [] },
	'q4-orders': { type: 'Metric', props: { resultId: 'sales', field: 'orders', row: 3, title: 'Q4 orders', unit: 'orders' }, children: [] },
	'q3-revenue': { type: 'Metric', props: { resultId: 'sales', field: 'revenue', row: 2, title: 'Q3 revenue', unit: 'USD' }, children: [] },
	card: { type: 'Card', props: { title: 'Saved analysis', detail: 'Exact values remain attached to the source result', tone: 'plain' }, children: ['chart', 'table'] },
	chart: { type: 'Chart', props: { resultId: 'sales', title: 'Revenue by quarter', spec: { mark: { type: 'line', point: true }, encoding: { x: { field: 'period', type: 'ordinal' }, y: { field: 'revenue', type: 'quantitative' }, tooltip: [{ field: 'period', type: 'ordinal' }, { field: 'revenue', type: 'quantitative' }] } } }, children: [] },
	table: { type: 'Table', props: { resultId: 'sales', title: 'Quarterly detail', fields: ['period', 'revenue', 'orders'], columnFormats: { revenue: { label: 'Net revenue', format: 'currency', unit: 'USD', decimals: 2 }, orders: { label: 'Orders', format: 'number', decimals: 0 } } }, children: [] }
} };

const result = {
	id: 'sales',
	columns: ['period', 'revenue', 'orders'],
	rows: [
		['Q1 2025', 2890.1, 104],
		['Q2 2025', 3144.25, 111],
		['Q3 2025', 3392.8, 126],
		['Q4 2025', 3616.38, 140]
	],
	exact_rows: [
		['Q1 2025', '2890.10', '104'],
		['Q2 2025', '3144.2500', '111'],
		['Q3 2025', '3392.80', '126'],
		['Q4 2025', '3616.3799999999999', '140']
	],
	row_count: 4,
	complete: true
};

const workspace = '/fixture/quarterly-sales';
const turnId = 'cached-layout-turn';
const message = {
	id: 'cached-layout-answer',
	role: 'assistant',
	text: 'Saved quarterly analysis.',
	ts: 1,
	artifact_id: 'cached-layout-artifact',
	artifact_revision: 1,
	pending: false,
	answer: {
		turn_id: turnId,
		text: 'Saved quarterly analysis.',
		workspace: { path: workspace, revision: 'fixture-revision' },
		evidence: [],
		verification: []
	},
	presentation_snapshot: { workspace, turn_id: turnId, spec, results: [result] }
};

const entry = `
import { mount, tick, unmount } from 'svelte';
import '/src/app.css';

window.__fellaInvocations = [];
window.fella = {
	invoke(command, payload) {
		window.__fellaInvocations.push({ command, payload });
		return Promise.reject(new Error('Unexpected desktop IPC in cached presentation fixture'));
	}
};

const { default: AnalyticalAnswer } = await import('/src/lib/components/AnalyticalAnswer.svelte');
const target = document.getElementById('fixture');
const message = ${JSON.stringify(message)};
let component;
let mountCount = 0;

async function openSavedAnswer() {
	if (component) await unmount(component);
	component = mount(AnalyticalAnswer, { target, props: { message } });
	mountCount++;
	window.__immediatePresentation = Boolean(target.querySelector('.analytical-presentation'));
	await tick();
	window.__mountCount = mountCount;
}

window.__presentationFixture = { reopen: openSavedAnswer };
await openSavedAnswer();
window.__fixtureReady = true;
`;

const html = `<!doctype html>
<html lang="en" data-color-mode="dark">
<head>
	<meta charset="utf-8">
	<meta name="viewport" content="width=device-width, initial-scale=1">
	<title>Fella presentation layout fixture</title>
	<style>
		html, body { width: 100%; min-width: 0; min-height: 100%; margin: 0; background: var(--bg); color: var(--text); }
		#fixture { width: 100%; max-width: 100%; min-width: 0; margin: 0 auto; padding: 16px; }
		@media (min-width: 700px) { #fixture { max-width: 1080px; padding: 28px; } }
	</style>
</head>
<body><main id="fixture"></main><script type="module" src="/__presentation-layout-entry.js"></script></body>
</html>`;

const cacheDir = await mkdtemp(join(tmpdir(), 'fella-presentation-layout-cache-'));
const screenshotDir = await mkdtemp(join(tmpdir(), 'fella-presentation-layout-shots-'));
const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	cacheDir,
	optimizeDeps: { noDiscovery: true, entries: [] },
	server: { host: '127.0.0.1', port: 0, strictPort: false, hmr: false, ws: false, fs: { allow: [process.cwd(), cacheDir] } },
	plugins: [{
		name: 'fella-presentation-layout-fixture',
		resolveId(id) { if (id === '/__presentation-layout-entry.js') return '\0presentation-layout-entry.js'; },
		load(id) { if (id === '\0presentation-layout-entry.js') return entry; },
		configureServer(vite) {
			vite.middlewares.use('/__presentation-layout', (_request, response) => {
				response.setHeader('Content-Type', 'text/html; charset=utf-8');
				response.end(html);
			});
		}
	}]
});

let browser;
let succeeded = false;
let navigations = 0;
try {
	await server.listen();
	const address = server.httpServer.address();
	assert.ok(address && typeof address === 'object');
	const baseUrl = `http://127.0.0.1:${address.port}/__presentation-layout/`;
	browser = await chromium.launch({ headless: true });
	const page = await browser.newPage({ viewport: { width: 320, height: 900 }, deviceScaleFactor: 1 });
	page.on('framenavigated', frame => { if (frame === page.mainFrame()) navigations++; });
	const pageErrors = [];
	page.on('pageerror', error => pageErrors.push(error.message));
	await page.goto(baseUrl, { waitUntil: 'networkidle' });
	await page.waitForFunction(() => window.__fixtureReady === true);
	await page.waitForFunction(() => {
		const canvas = document.querySelector('.presentation-chart-target canvas');
		return Boolean(canvas && canvas.width > 0 && canvas.height > 0);
	}, null, { timeout: 30_000 });
	await page.waitForFunction(() => !document.querySelector('.presentation-chart [role="status"]')?.textContent?.trim(), null, { timeout: 30_000 });

	assert.equal(navigations, 1, 'fixture should have only its initial navigation');
	assert.equal(await page.locator('.analytical-presentation').count(), 1, 'cached answer should mount through the json-render catalog');
	assert.equal(await page.locator('.presentation-metric').count(), 3, 'all three metric cards should render');
	assert.equal(await page.locator('.presentation-card').count(), 1, 'the composed card should render');
	assert.equal(await page.getByRole('heading', { name: 'Quarterly detail' }).count(), 1, 'the requested Table component should render');
	assert.equal(await page.locator('.presentation-table-title').evaluate(heading => heading.nextElementSibling?.querySelectorAll('tbody tr').length ?? 0), 4, 'the requested Table should show all four saved rows');
	assert.equal(await page.getByText(/Preparing/i).count(), 0, 'cached answer should not show a preparing state');
	assert.deepEqual(await page.evaluate(() => window.__fellaInvocations), [], 'cached artifact should not invoke IPC, including artifact_resolve');
	assert.equal(await page.evaluate(() => window.__mountCount), 1);
	assert.equal(await page.evaluate(() => window.__immediatePresentation), true, 'presentation must exist immediately after mount');

	const chartCanvas = page.locator('.presentation-chart-target canvas');
	await chartCanvas.scrollIntoViewIfNeeded();
	const chartBounds = await chartCanvas.boundingBox();
	assert.ok(chartBounds, 'the chart canvas should be available for an interactive hover');
	const plottedPixel = await chartCanvas.evaluate(canvas => {
		const context = canvas.getContext('2d');
		if (!context) return null;
		const chart = canvas.closest('.analytical-document');
		const color = getComputedStyle(chart).getPropertyValue('--chart-violet').trim();
		const channels = color.match(/[\da-f]{2}/gi)?.map(channel => parseInt(channel, 16));
		if (!channels || channels.length < 3) return null;
		const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
		for (let y = 0; y < canvas.height; y++) {
			for (let x = 0; x < canvas.width; x++) {
				const offset = (y * canvas.width + x) * 4;
				if (pixels[offset + 3] > 180 && channels.every((channel, index) => Math.abs(channel - pixels[offset + index]) < 24)) return { x, y };
			}
		}
		return null;
	});
	assert.ok(plottedPixel, 'the chart should paint its first Fella series color for hover targeting');
	await page.mouse.move(chartBounds.x + plottedPixel.x + 0.5, chartBounds.y + plottedPixel.y + 0.5);
	let chartTooltip = page.locator('.fella-chart-tooltip');
	assert.equal(await chartTooltip.isVisible(), true, 'hovering a plotted result point should show a data tooltip');
	assert.ok(await chartTooltip.locator('.fella-chart-tooltip-row').count() >= 1, 'tooltip should contain result-bound field/value rows');
	const tooltipStyle = await chartTooltip.evaluate(element => ({
		borderRadius: getComputedStyle(element).borderRadius,
		maxWidth: getComputedStyle(element).maxWidth,
		right: element.getBoundingClientRect().right,
		chartRight: element.parentElement.getBoundingClientRect().right
	}));
	assert.notEqual(tooltipStyle.borderRadius, '0px', 'tooltip should use Fella surface curvature');
	assert.ok(tooltipStyle.right <= tooltipStyle.chartRight + 1, 'tooltip should stay within the chart bounds');

	async function inspectViewport(width, height, label) {
		await page.setViewportSize({ width, height });
		await page.waitForTimeout(120);
		const metrics = await page.evaluate(() => {
			const root = document.documentElement;
			const grid = document.querySelector('.presentation-grid');
			const canvas = document.querySelector('.presentation-chart-target canvas');
			const scrollableOverflow = [...document.querySelectorAll('body *')]
				.filter(element => {
					const style = getComputedStyle(element);
					return ['auto', 'scroll'].includes(style.overflowX) && element.scrollWidth > element.clientWidth + 1;
				})
				.map(element => ({ tag: element.tagName, className: String(element.className), scrollWidth: element.scrollWidth, clientWidth: element.clientWidth }));
			let paintedPixels = 0;
			if (canvas) {
				const context = canvas.getContext('2d');
				if (context) {
					const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
					for (let index = 3; index < pixels.length; index += 4) if (pixels[index] > 0) paintedPixels++;
				}
			}
			return {
				viewportWidth: root.clientWidth,
				documentWidth: root.scrollWidth,
				gridColumns: grid ? getComputedStyle(grid).gridTemplateColumns.split(' ').length : 0,
				presentationLeft: document.querySelector('.analytical-presentation')?.getBoundingClientRect().left ?? null,
				presentationRight: document.querySelector('.analytical-presentation')?.getBoundingClientRect().right ?? null,
				canvasWidth: canvas?.width ?? 0,
				canvasHeight: canvas?.height ?? 0,
				paintedPixels,
				scrollableOverflow
			};
		});
		assert.ok(metrics.documentWidth <= metrics.viewportWidth + 1, `${label}: document horizontal overflow ${metrics.documentWidth}px > ${metrics.viewportWidth}px`);
		assert.ok(metrics.presentationLeft !== null && metrics.presentationLeft >= -1, `${label}: presentation must stay within the viewport on the left`);
		assert.ok(metrics.presentationRight <= metrics.viewportWidth + 1, `${label}: presentation must stay within the viewport on the right`);
		assert.deepEqual(metrics.scrollableOverflow, [], `${label}: a child should not introduce horizontal scrolling`);
		assert.ok(metrics.canvasWidth > 0 && metrics.canvasHeight > 0, `${label}: Vega canvas must have dimensions`);
		assert.ok(metrics.paintedPixels > 100, `${label}: Vega canvas must contain drawn pixels, got ${metrics.paintedPixels}`);
		if (width === 320) assert.equal(metrics.gridColumns, 1, '320px layout should use one metric column');
		else assert.ok(metrics.gridColumns >= 2, 'wide layout should use multiple metric columns');
		await page.screenshot({ path: join(screenshotDir, `presentation-${label}.png`), fullPage: true });
		console.log(`PASS ${label} layout: ${JSON.stringify(metrics)}`);
	}

	await inspectViewport(320, 900, '320px');
	await inspectViewport(1440, 1000, 'wide');

	await page.evaluate(() => window.__presentationFixture.reopen());
	assert.equal(await page.evaluate(() => window.__mountCount), 2, 'reopen should remount the cached answer');
	assert.equal(await page.evaluate(() => window.__immediatePresentation), true, 'reopened cached answer should appear synchronously');
	assert.equal(await page.locator('.analytical-presentation').count(), 1, 'reopen should not duplicate the answer surface');
	assert.deepEqual(await page.evaluate(() => window.__fellaInvocations), [], 'reopening the cached artifact should still avoid IPC resolution');
	assert.equal(navigations, 1, 'reopening the cached presentation should not reload the page');
	assert.equal(await page.getByText(/Preparing/i).count(), 0, 'reopening should not show a preparing state');
	assert.deepEqual(pageErrors, [], 'fixture should not emit browser errors');
	console.log('PASS cached AnalyticalAnswer mount/reopen: synchronous snapshot, no resolve IPC, no preparing state, no page reload');
	console.log(`Presentation layout regression v1: passed. Screenshots: ${screenshotDir}`);
	succeeded = true;
} finally {
	try { await browser?.close(); }
	finally {
		await server.close();
		if (succeeded) await rm(cacheDir, { recursive: true, force: true });
		else console.error(`Isolated Vite cache retained for diagnosis: ${cacheDir}`);
	}
}
