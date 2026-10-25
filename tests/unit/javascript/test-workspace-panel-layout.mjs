// Workspace panel layout contract v1 is documented in
// docs/WORKSPACE-PANEL-LAYOUT-RESPONSIVE-v1.md before running this fixture.
// This focused browser check mounts the real layout with stateful snippets;
// it does not load the parent route or touch session state.

import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'vite';
import { chromium } from '@playwright/test';

const harnessSource = `
<script>
	import { onMount, tick } from 'svelte';
	import WorkspacePanelLayout from '/src/lib/components/WorkspacePanelLayout.svelte';
	let open = $state(true);
	let notebook = $state(false);
	let expanded = $state(false);
	let compact = $state(false);
	let conversationValue = $state('conversation persists');
	let documentValue = $state('document persists');
	onMount(() => {
		window.__panelRefs = {
			conversation: document.querySelector('.fixture-conversation'),
			document: document.querySelector('.fixture-document')
		};
		window.__panelHarness = {
			async setNotebook(value) { notebook = value; await tick(); },
			async setExpanded(value) { expanded = value; await tick(); },
			async setOpen(value) { open = value; await tick(); }
		};
		window.__panelReady = true;
	});
</script>

<WorkspacePanelLayout {open} {notebook} bind:expanded bind:compact>
	{#snippet conversation()}
		<div class="fixture-conversation">
			<label>Conversation state <input aria-label="Conversation state" bind:value={conversationValue}></label>
		</div>
	{/snippet}
	{#snippet tools()}
		<div class="fixture-document">
			<label>Document state <input aria-label="Document state" bind:value={documentValue}></label>
		</div>
	{/snippet}
</WorkspacePanelLayout>
`;

const html = `<!doctype html>
<html lang="en" data-color-mode="dark">
<head>
	<meta charset="utf-8">
	<meta name="viewport" content="width=device-width, initial-scale=1">
	<title>Workspace panel layout contract</title>
	<style>
		html, body { width: 100%; min-width: 0; height: 100%; margin: 0; overflow: hidden; background: var(--bg); color: var(--text); }
		#fixture { display: flex; width: 100vw; height: 100vh; min-width: 0; min-height: 0; }
		.fixture-conversation, .fixture-document { display: flex; width: 100%; height: 100%; min-width: 0; align-items: flex-start; padding: 20px; }
		.fixture-conversation label, .fixture-document label { width: 100%; min-width: 0; }
		.fixture-conversation input, .fixture-document input { display: block; width: 100%; max-width: 100%; margin-top: 8px; }
	</style>
</head>
<body><main id="fixture"></main><script type="module" src="/__workspace-panel-layout-entry.js"></script></body>
</html>`;

const entry = `
import '/src/app.css';
import { mount } from 'svelte';
import Harness from '/__workspace-panel-layout-harness.svelte';
mount(Harness, { target: document.getElementById('fixture') });
`;

const cacheDir = await mkdtemp(join(tmpdir(), 'fella-workspace-layout-cache-'));
const virtualHarnessPath = join(process.cwd(), 'tests', 'unit', 'javascript', '__WorkspacePanelLayoutHarness.svelte');
const server = await createServer({
	configFile: 'vite.config.ts',
	appType: 'custom',
	logLevel: 'error',
	cacheDir,
	optimizeDeps: { noDiscovery: true, entries: [] },
	server: { host: '127.0.0.1', port: 0, strictPort: false, hmr: false, ws: false, fs: { allow: [process.cwd(), cacheDir] } },
	plugins: [{
		name: 'workspace-panel-layout-contract-fixture',
		resolveId(id) {
			if (id === '/__workspace-panel-layout-entry.js') return '\0workspace-panel-layout-entry.js';
			if (id === '/__workspace-panel-layout-harness.svelte') return virtualHarnessPath;
		},
		load(id) {
			if (id === '\0workspace-panel-layout-entry.js') return entry;
			if (id === virtualHarnessPath) return harnessSource;
		},
		configureServer(vite) {
			vite.middlewares.use('/__workspace-panel-layout', (_request, response) => {
				response.setHeader('Content-Type', 'text/html; charset=utf-8');
				response.end(html);
			});
		}
	}]
});

let browser;
let succeeded = false;
try {
	await server.listen();
	const address = server.httpServer.address();
	assert.ok(address && typeof address === 'object');
	browser = await chromium.launch({ headless: true });
	const page = await browser.newPage({ viewport: { width: 1200, height: 800 }, deviceScaleFactor: 1 });
	const pageErrors = [];
	page.on('pageerror', error => pageErrors.push(error.message));
	await page.goto(`http://127.0.0.1:${address.port}/__workspace-panel-layout/`, { waitUntil: 'networkidle' });
	await page.waitForFunction(() => window.__panelReady === true);
	await page.waitForFunction(() => document.querySelector('.workspace-chat-layout')?.dataset.panelMode === 'split');

	const paneMetrics = () => page.evaluate(() => {
		const host = document.querySelector('.workspace-chat-layout');
		const conversation = document.querySelector('.workspace-conversation-pane');
		const tools = document.querySelector('.workspace-tools-pane');
		const rect = element => {
			const bounds = element.getBoundingClientRect();
			return { left: bounds.left, right: bounds.right, width: bounds.width, hidden: element.hidden };
		};
		return {
			host: rect(host),
			conversation: rect(conversation),
			tools: rect(tools),
			mode: host.dataset.panelMode,
			notebook: host.dataset.notebook,
			scrollWidth: host.scrollWidth,
			clientWidth: host.clientWidth
		};
	});
	const assertNoHorizontalOverflow = async (label) => {
		const dimensions = await page.evaluate(() => ({
			viewport: document.documentElement.clientWidth,
			document: document.documentElement.scrollWidth,
			host: document.querySelector('.workspace-chat-layout')?.scrollWidth,
			client: document.querySelector('.workspace-chat-layout')?.clientWidth,
			scrollable: [...document.querySelectorAll('.workspace-chat-layout *')].filter(element => {
				const style = getComputedStyle(element);
				return ['auto', 'scroll'].includes(style.overflowX) && element.scrollWidth > element.clientWidth + 1;
			}).map(element => String(element.className))
		}));
		assert.ok(dimensions.document <= dimensions.viewport + 1, `${label}: document horizontal overflow`);
		assert.ok((dimensions.host ?? 0) <= (dimensions.client ?? 0) + 1, `${label}: workspace panel horizontal overflow`);
		assert.deepEqual(dimensions.scrollable, [], `${label}: horizontally scrollable child`);
	};
	const assertSamePaneNodes = async (label) => {
		assert.deepEqual(await page.evaluate(() => ({
			conversation: window.__panelRefs.conversation === document.querySelector('.fixture-conversation'),
			document: window.__panelRefs.document === document.querySelector('.fixture-document')
		})), { conversation: true, document: true }, `${label}: snippet DOM identity should remain stable`);
	};

	console.log('Acceptance criteria: docs/WORKSPACE-PANEL-LAYOUT-RESPONSIVE-v1.md');

	// Legacy default: conversation left, tools right, existing default allocation.
	let metrics = await paneMetrics();
	assert.equal(metrics.notebook, 'false');
	assert.ok(metrics.conversation.left < metrics.tools.left, 'default layout keeps conversation on the left');
	assert.ok(Math.abs(metrics.tools.width - 480) <= 1, 'default tools width remains 480px');
	let separator = page.getByRole('separator', { name: 'Resize workspace tools' });
	await separator.focus();
	await separator.press('ArrowLeft');
	assert.ok(Math.abs((await paneMetrics()).tools.width - 512) <= 1, 'legacy keyboard resizing remains unchanged');
	assert.ok(await page.evaluate(() => Number(localStorage.getItem('fella:tools-width')) >= 512), 'legacy split persists to its original preference');

	// Switching to notebook mode changes only layout, not either snippet instance.
	await page.evaluate(() => window.__panelHarness.setNotebook(true));
	await page.waitForFunction(() => document.querySelector('.workspace-chat-layout')?.dataset.notebook === 'true');
	await assertSamePaneNodes('notebook mode');
	metrics = await paneMetrics();
	assert.ok(metrics.tools.left < metrics.conversation.left, 'notebook puts document/tools left of conversation');
	assert.ok(metrics.tools.width >= 400, 'notebook document pane meets 400px minimum when split');
	assert.ok(metrics.conversation.width >= 320, 'notebook conversation pane meets 320px minimum when split');
	assert.ok(Math.abs(metrics.tools.width / metrics.host.width - 0.62) < 0.03, 'notebook defaults to approximately 62% document width');
	separator = page.getByRole('separator', { name: 'Resize document pane' });
	await separator.focus();
	const widthBeforeResize = (await paneMetrics()).tools.width;
	await separator.press('ArrowRight');
	assert.ok(Math.abs((await paneMetrics()).tools.width - widthBeforeResize - 32) <= 1, 'keyboard resize grows the left document pane to the right');
	assert.ok(await page.evaluate(() => Number(localStorage.getItem('fella:notebook-document-width')) > 0), 'notebook width uses its own persisted preference');
	assert.ok(await page.evaluate(() => Number(localStorage.getItem('fella:tools-width')) >= 512), 'notebook resize leaves the legacy split preference intact');
	await separator.dblclick();

	// Existing compact behavior remains for the default layout, while notebook
	// mode keeps a split until its own 760px responsive breakpoint.
	await page.setViewportSize({ width: 820, height: 800 });
	await page.evaluate(() => window.__panelHarness.setNotebook(false));
	await page.waitForFunction(() => document.querySelector('.workspace-chat-layout')?.dataset.panelMode === 'expanded');
	assert.equal((await paneMetrics()).conversation.hidden, true, 'legacy compact layout still expands tools alone');
	await page.evaluate(() => window.__panelHarness.setNotebook(true));
	await page.waitForFunction(() => document.querySelector('.workspace-chat-layout')?.dataset.panelMode === 'split');
	metrics = await paneMetrics();
	assert.ok(metrics.tools.width >= 400 && metrics.conversation.width >= 320, 'notebook preserves both pane minima at 820px despite legacy compact state');
	await assertNoHorizontalOverflow('820px notebook split');

	// Narrow notebook: accessible tab selection hides the other pane without
	// removing either snippet. The bindings retain each pane's entered state.
	await page.setViewportSize({ width: 640, height: 800 });
	await page.waitForFunction(() => document.querySelector('.workspace-chat-layout')?.dataset.panelMode === 'mobile');
	const tablist = page.getByRole('tablist', { name: 'Workspace panes' });
	assert.equal(await tablist.isVisible(), true);
	const documentTab = page.getByRole('tab', { name: 'Document' });
	const chatTab = page.getByRole('tab', { name: 'Chat' });
	assert.equal(await documentTab.getAttribute('aria-selected'), 'true', 'Document is selected first on narrow screens');
	assert.equal(await page.locator('#workspace-tools-pane').evaluate(element => element.hidden), false);
	assert.equal(await page.locator('#workspace-conversation-pane').evaluate(element => element.hidden), true);
	await page.getByRole('textbox', { name: 'Document state' }).fill('document state retained');
	await chatTab.click();
	assert.equal(await chatTab.getAttribute('aria-selected'), 'true');
	assert.equal(await page.locator('#workspace-conversation-pane').evaluate(element => element.hidden), false);
	assert.equal(await page.locator('#workspace-tools-pane').evaluate(element => element.hidden), true);
	await page.getByRole('textbox', { name: 'Conversation state' }).fill('chat state retained');
	await chatTab.press('Home');
	assert.equal(await documentTab.getAttribute('aria-selected'), 'true', 'Home moves focus/selection to the first tab');
	assert.equal(await documentTab.evaluate(element => document.activeElement === element), true);
	await documentTab.press('ArrowRight');
	assert.equal(await chatTab.getAttribute('aria-selected'), 'true', 'arrow keys move between pane tabs');
	await chatTab.press('End');
	assert.equal(await chatTab.evaluate(element => document.activeElement === element), true);
	await documentTab.click();
	assert.equal(await page.getByRole('textbox', { name: 'Document state' }).inputValue(), 'document state retained');
	await chatTab.click();
	assert.equal(await page.getByRole('textbox', { name: 'Conversation state' }).inputValue(), 'chat state retained');
	await assertSamePaneNodes('mobile tab switches');
	await assertNoHorizontalOverflow('640px notebook tabs');

	// At phone width the same two panes remain switchable and no child offers a
	// sideways scroll region.
	await page.setViewportSize({ width: 320, height: 800 });
	await page.waitForFunction(() => document.querySelector('.workspace-chat-layout')?.dataset.panelMode === 'mobile');
	await assertNoHorizontalOverflow('320px notebook tabs');
	await assertSamePaneNodes('320px notebook tabs');

	// Expanded mode remains a document-only surface and does not replace slots.
	await page.evaluate(() => window.__panelHarness.setExpanded(true));
	await page.waitForFunction(() => document.querySelector('.workspace-chat-layout')?.dataset.panelMode === 'expanded');
	assert.equal(await tablist.isVisible(), false);
	assert.equal((await paneMetrics()).conversation.hidden, true);
	assert.equal((await paneMetrics()).tools.hidden, false);
	await assertSamePaneNodes('expanded notebook');
	assert.deepEqual(pageErrors, [], 'focused fixture should not emit browser errors');

	console.log('PASS WorkspacePanelLayout responsive contract v1');
	succeeded = true;
} finally {
	try { await browser?.close(); }
	finally {
		await server.close();
		if (succeeded) await rm(cacheDir, { recursive: true, force: true });
		else console.error(`Isolated Vite cache retained for diagnosis: ${cacheDir}`);
	}
}
