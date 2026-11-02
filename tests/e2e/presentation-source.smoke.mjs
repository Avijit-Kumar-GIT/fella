// Frozen UI-only smoke v1.2. Original v1 behavioral criteria unchanged.
// User-approved cell assertion correction: compare each exact cell value rather
// than whitespace-dependent concatenated row text. v1.1 failure is preserved in
// /tmp/fella-presentation-source-v1.1-M4LlLc and its script snapshot in /tmp.
// User-approved locator correction: the wrapping Region label includes option
// text; use its accessible combobox role/name instead of exact getByLabel.
// Original v1 script: /tmp/fella-presentation-source-smoke-v1.mjs. Original
// failures remain in /tmp/fella-presentation-source-v1-{FCX4mj,GFNyb4,ym16Er}.
// Actual AnalyticalAnswer, lazy PresentationSource, renderer and shared WASM
// validator. Only IPC responses are deterministic; no model quality is graded.
// Run from repository root: node tests/e2e/presentation-source.smoke.mjs
// Logs, screenshots and failure diagnostics are retained in /tmp.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtemp, writeFile } from 'node:fs/promises';
const require = createRequire(process.cwd() + '/package.json');
const { createServer } = await import(require.resolve('vite'));
const { chromium, expect } = require('@playwright/test');

const criteria = Object.freeze({
  lazy: 'No PresentationSource JS module or artifact IPC before Answer actions → View JSON; opening shows canonical JSON matching the rendered revision.',
  manual: 'Valid manual JSON revises/adopts one new presentation revision on the SAME analytical run and source revision, and updates the displayed view.',
  malformed: 'Malformed JSON is rejected visibly with zero artifact_revise or artifact_adopt calls.',
  region: 'A selected region survives adding chart/table switching; both views use the selected region and its exact saved values.',
  displayOnly: 'Manual display editing invokes no SQL, Python, ask, presentation regeneration or analytical execution.',
  narrow: 'At 320×640 the page and source dialog fit horizontally; source actions remain reachable.',
  invalidation: 'Workspace change or cancellation while a manual revision is pending prevents adoption and display-cache mutation.'
});
const version = '1.2';
const evidenceDirectory = await mkdtemp('/tmp/fella-presentation-source-v1.2-');
const logs = [];
function log(value) { const line = typeof value === 'string' ? value : JSON.stringify(value); logs.push(line); console.log(line); }
log({ version, criteria, evidenceDirectory });
await writeFile(evidenceDirectory + '/criteria-v1.2.json', JSON.stringify(criteria, null, 2));

const workspace = '/fixture/presentation-source', turn = 'source-turn', sourceRevision = 'source-r1';
const results = [{ id: 'monthly', columns: ['region', 'month', 'amount'], rows: [
  ['East', '2025-01', 10], ['East', '2025-02', 11], ['West', '2025-01', 20], ['West', '2025-02', 23]
], exact_rows: [['East', '2025-01', '10'], ['East', '2025-02', '11'], ['West', '2025-01', '20'], ['West', '2025-02', '23']], row_count: 4, complete: true }];
const authored = {
  version: 'fella-ui-1', intent: 'exploration', capabilities: ['display'],
  state: { region: { type: 'enum', options: [{ value: 'East', label: 'East' }, { value: 'West', label: 'West' }], default: 'East', persistence: 'viewer' } },
  root: { type: 'Document', id: 'document', children: [
    { type: 'Title', id: 'title', props: { content: 'Monthly regional values' } },
    { type: 'Select', id: 'region', props: { name: 'Region' }, bind: { state: 'region' } },
    { type: 'Chart', id: 'chart', props: { resultId: 'monthly', title: 'Regional chart', spec: { mark: 'line', encoding: { x: { field: 'month', type: 'ordinal' }, y: { field: 'amount', type: 'quantitative' } } } }, filters: [{ state: 'region', field: 'region' }] }
  ] }
};
const changed = structuredClone(authored);
changed.root.children[0].props.content = 'Regional values with exact view';
changed.state.view = { type: 'enum', options: [{ value: 'chart', label: 'Chart' }, { value: 'table', label: 'Exact values' }], default: 'chart', persistence: 'viewer' };
changed.root.children.splice(2, 0, { type: 'SegmentedControl', id: 'view', props: { name: 'View' }, bind: { state: 'view' } });
changed.root.children.find(node => node.id === 'chart').when = { state: 'view', equals: 'chart' };
changed.root.children.push({ type: 'Table', id: 'table', props: { resultId: 'monthly', title: 'Regional exact values', fields: ['region', 'month', 'amount'] }, filters: [{ state: 'region', field: 'region' }], when: { state: 'view', equals: 'table' } });

let entry;
const cacheDir = await mkdtemp('/tmp/fella-presentation-source-cache-');
const server = await createServer({ configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error', cacheDir,
  optimizeDeps: { noDiscovery: true, entries: [] },
  server: { host: '127.0.0.1', port: 0, strictPort: false, hmr: false, ws: false, fs: { allow: [process.cwd(), cacheDir] } },
  plugins: [{ name: 'presentation-source-v1', resolveId(id) { if (id === '/__source-entry.js') return '\0source-entry'; },
    load(id) { if (id === '\0source-entry') return entry; },
    configureServer(vite) { vite.middlewares.use('/__source-smoke', (_req, res) => {
      res.setHeader('Content-Type', 'text/html');
      res.end('<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1"><link rel="icon" href="data:,"><style>body{margin:0}#fixture{box-sizing:border-box;width:100%;max-width:760px;padding:12px}</style></head><body><main id="fixture"></main><script type="module" src="/__source-entry.js"></script></body></html>');
    }); }
  }]
});
let browser;
const failures = [];
try {
  const { validatePresentation } = await server.ssrLoadModule('/src/lib/presentation/validate.ts');
  const initial = validatePresentation(authored, results), updated = validatePresentation(changed, results);
  assert.equal(initial.valid, true, 'Fixture initial spec invalid: ' + JSON.stringify(initial.issues));
  assert.equal(updated.valid, true, 'Fixture edited spec invalid: ' + JSON.stringify(updated.issues));
  const spec = initial.spec, edited = updated.spec;
  const canonicalText = 'Canonical analytical statement remains unchanged.';
  const message = { id: 'source-message', role: 'assistant', text: canonicalText, ts: 1, pending: false,
    artifact_id: 'source-artifact', artifact_revision: 2,
    answer: { turn_id: turn, text: canonicalText, workspace: { path: workspace, revision: sourceRevision }, evidence: [], verification: [] },
    presentation_snapshot: { workspace, turn_id: turn, spec, results } };
  const revisions = [1, 2].map(revision => ({ revision, parent_revision: revision === 2 ? 1 : null, run_id: turn, spec, schema_version: 'fella-ui-1', change_note: 'Saved view', created_at_ms: 1 }));
  const artifact = { id: 'source-artifact', workspace, title: 'Regional fixture', current_revision: 2, saved: true, revisions, created_at_ms: 1, updated_at_ms: 1 };
  const output = { artifact, revision: revisions[1], run: { id: turn, question: 'Explore saved regional values.', workspace_revision: sourceRevision, result: { text: canonicalText, evidence: [], verification: [], status: 'needs_review' } }, results };
  entry = `
import {mount,unmount,tick,flushSync} from 'svelte';
import '/src/app.css';
import AnalyticalAnswer from '/src/lib/components/AnalyticalAnswer.svelte';
import {session,Conversation} from '/src/lib/session.svelte.ts';
const chat=new Conversation('source-smoke');chat.messages=[${JSON.stringify(message)}];
const message=chat.messages[0];window.testMessage=message;
const saved=${JSON.stringify(output)};window.canonical=${JSON.stringify(spec)};window.edited=${JSON.stringify(edited)};
window.calls=[];window.pending=[];window.holdRevise=false;window.adopted=null;window.persisted=0;
window.fella={invoke(command,payload){
 window.calls.push({command,payload:structuredClone(payload)});
 if(command==='artifact_resolve')return Promise.resolve(structuredClone(saved));
 if(command==='artifact_revise'){
  const candidate=structuredClone(saved);candidate.revision={...candidate.revision,revision:3,parent_revision:2,run_id:payload.runId,spec:structuredClone(payload.spec)};
  candidate.artifact.revisions.push(candidate.revision);
  window.candidate=candidate;
  return window.holdRevise?new Promise(resolve=>window.pending.push(()=>resolve(candidate))):Promise.resolve(candidate);
 }
 if(command==='artifact_adopt'){
  const adopted=structuredClone(window.candidate);adopted.artifact.current_revision=payload.revision;
  window.adopted=adopted;return Promise.resolve(adopted);
 }
 return Promise.reject(new Error('Unexpected IPC '+command));
}};
session.catalog.workspace=${JSON.stringify(workspace)};session.persist=()=>window.persisted++;
let component=mount(AnalyticalAnswer,{target:document.getElementById('fixture'),props:{message}});flushSync();await tick();
window.fixture={async release(){for(const release of window.pending.splice(0))release();await tick();},async switchWorkspace(){session.catalog.workspace='/other-workspace';await tick();},async unmount(){await unmount(component);await tick();}};
window.ready=true;`;
  await server.listen();
  browser = await chromium.launch({ headless: true });
  const url = 'http://127.0.0.1:' + server.httpServer.address().port + '/__source-smoke/';
  async function fresh(viewport = { width: 1000, height: 850 }) {
    const page = await browser.newPage({ viewport });
    page.setDefaultTimeout(10000);
    const state = { imports: 0, errors: [], navigations: 0 };
    page.on('request', request => {
      const target = new URL(request.url());
      if (request.resourceType() === 'script' && target.pathname === '/src/lib/components/PresentationSource.svelte' && target.searchParams.get('type') !== 'style') state.imports++;
    });
    page.on('pageerror', error => state.errors.push(error.message));
    page.on('framenavigated', frame => { if (frame === page.mainFrame()) state.navigations++; });
    await page.goto(url, { waitUntil: 'networkidle' });
    await page.waitForFunction(() => window.ready);
    await expect(page.getByText('Monthly regional values', { exact: true })).toBeVisible();
    assert.equal(state.imports, 0, 'Source module loaded initially');
    assert.deepEqual(await page.evaluate(() => window.calls), [], 'Unexpected initial IPC');
    return { page, state };
  }
  async function openSource(page) {
    await page.getByRole('button', { name: 'Answer actions', exact: true }).click();
    await page.getByRole('menuitem', { name: 'View JSON', exact: true }).click();
    await expect(page.getByRole('dialog', { name: 'Presentation JSON' })).toBeVisible();
    const raw = await page.getByRole('textbox', { name: 'Revision JSON', exact: true }).inputValue();
    assert.deepEqual(JSON.parse(raw), await page.evaluate(() => window.canonical));
  }
  async function fillEdit(page) {
    await page.getByRole('textbox', { name: 'Revision JSON', exact: true }).fill(await page.evaluate(() => JSON.stringify(window.edited)));
    await page.getByRole('button', { name: 'Save revision', exact: true }).click();
  }
  async function onlyDisplayCalls(page) {
    const calls = await page.evaluate(() => window.calls);
    assert.deepEqual(calls.filter(call => !['artifact_resolve', 'artifact_revise', 'artifact_adopt'].includes(call.command)), [], 'Manual edit invoked analytical work');
    return calls;
  }
  async function run(name, test) {
    let fixture;
    try { fixture = await test.setup(); await test.check(fixture); log('PASS ' + name); }
    catch (error) {
      failures.push({ name, error: error.stack ?? String(error) }); log('FAIL ' + name + ': ' + error.message);
      if (fixture) {
        await fixture.page.screenshot({ path: evidenceDirectory + '/' + name + '.png', fullPage: true }).catch(() => {});
        await writeFile(evidenceDirectory + '/' + name + '.html', await fixture.page.content()).catch(() => {});
        await writeFile(evidenceDirectory + '/' + name + '-state.json', JSON.stringify(await fixture.page.evaluate(() => ({ calls: window.calls, message: window.testMessage })), null, 2)).catch(() => {});
      }
    } finally { await fixture?.page.close(); }
  }
  await run('lazy-canonical', { setup: () => fresh(), check: async ({ page, state }) => {
    await openSource(page); assert.equal(state.imports, 1);
    assert.equal(await page.locator('.answer-surface .analytical-presentation').count(), 1);
    assert.equal(await page.evaluate(() => window.testMessage.artifact_revision), 2);
    await onlyDisplayCalls(page); assert.deepEqual(state.errors, []);
  } });
  await run('manual-edit-region-display-only', { setup: () => fresh(), check: async ({ page, state }) => {
    await page.getByRole('combobox', { name: 'Region', exact: true }).selectOption('West');
    await page.waitForFunction(() => window.testMessage.presentation_state?.['control:region'] === 'West');
    await openSource(page); await fillEdit(page);
    await expect(page.getByRole('status').filter({ hasText: 'Revision 3 saved.' })).toBeVisible();
    const calls = await onlyDisplayCalls(page), revise = calls.filter(call => call.command === 'artifact_revise'), adopt = calls.filter(call => call.command === 'artifact_adopt');
    assert.equal(revise.length, 1); assert.equal(adopt.length, 1);
    assert.equal(revise[0].payload.runId, turn); assert.equal(revise[0].payload.baseRevision, 2);
    assert.equal(adopt[0].payload.baseRevision, 2); assert.equal(adopt[0].payload.revision, 3);
    assert.deepEqual(revise[0].payload.spec, edited);
    const actual = await page.evaluate(() => ({ message: window.testMessage, adopted: window.adopted }));
    assert.equal(actual.message.artifact_id, artifact.id); assert.equal(actual.message.artifact_revision, 3);
    assert.equal(actual.adopted.run.id, turn); assert.equal(actual.adopted.run.workspace_revision, sourceRevision);
    assert.equal(actual.message.answer.text, canonicalText); assert.deepEqual(actual.message.answer.evidence, []);
    assert.deepEqual(actual.message.presentation_snapshot.spec, edited);
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(page.getByText('Regional values with exact view', { exact: true })).toBeVisible();
    await expect(page.getByRole('combobox', { name: 'Region', exact: true })).toHaveValue('West');
    await page.getByRole('button', { name: 'Exact values', exact: true }).click();
    const table = page.locator('.answer-surface').getByRole('table').filter({ has: page.getByRole('cell', { name: 'West', exact: true }) });
    await expect(table).toBeVisible(); assert.equal(await table.locator('tbody tr').count(), 2);
    assert.deepEqual(await table.locator('tbody tr').evaluateAll(rows => rows.map(row => [...row.querySelectorAll('td')].map(cell => cell.textContent.trim()))), [['West', '2025-01', '20'], ['West', '2025-02', '23']]);
    await page.getByRole('button', { name: 'Chart', exact: true }).click();
    await expect(page.getByRole('img', { name: 'Regional chart', exact: true })).toBeVisible();
    await expect(page.locator('.answer-surface .chart-target canvas, .answer-surface .chart-target svg').first()).toBeVisible();
    await page.getByText('Exact saved values', { exact: true }).click();
    const chartTable = page.locator('.answer-surface .presentation-chart table');
    assert.equal(await chartTable.getByRole('cell', { name: 'West', exact: true }).count(), 2);
    assert.equal(await chartTable.getByRole('cell', { name: 'East', exact: true }).count(), 0);
    await expect(page.getByRole('combobox', { name: 'Region', exact: true })).toHaveValue('West');
    await onlyDisplayCalls(page); assert.deepEqual(state.errors, []); assert.equal(state.navigations, 1);
  } });
  await run('malformed-zero-revise', { setup: () => fresh(), check: async ({ page, state }) => {
    await openSource(page); await page.getByRole('textbox', { name: 'Revision JSON', exact: true }).fill('{broken');
    await page.getByRole('button', { name: 'Save revision', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('Enter valid JSON before saving.');
    const calls = await onlyDisplayCalls(page); assert.equal(calls.filter(call => ['artifact_revise', 'artifact_adopt'].includes(call.command)).length, 0);
    assert.equal(await page.evaluate(() => window.testMessage.artifact_revision), 2); assert.deepEqual(state.errors, []);
  } });
  await run('narrow-fit', { setup: () => fresh({ width: 320, height: 640 }), check: async ({ page, state }) => {
    await openSource(page);
    const fit = await page.evaluate(() => { const dialog = document.querySelector('[role="dialog"]'), rect = dialog.getBoundingClientRect(); return { width: innerWidth, pageWidth: document.documentElement.scrollWidth, left: rect.left, right: rect.right, clientWidth: dialog.clientWidth, scrollWidth: dialog.scrollWidth }; });
    assert.ok(fit.pageWidth <= fit.width + 1, JSON.stringify(fit)); assert.ok(fit.left >= 0 && fit.right <= fit.width + 1, JSON.stringify(fit)); assert.ok(fit.scrollWidth <= fit.clientWidth + 1, JSON.stringify(fit));
    await expect(page.getByRole('button', { name: 'Save revision', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0); assert.deepEqual(state.errors, []);
  } });
  for (const mode of ['workspace', 'cancel']) await run('pending-' + mode, { setup: () => fresh(), check: async ({ page, state }) => {
    await openSource(page); await page.evaluate(() => window.holdRevise = true); await fillEdit(page);
    await page.waitForFunction(() => window.pending.length === 1);
    if (mode === 'workspace') await page.evaluate(() => window.fixture.switchWorkspace());
    else await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await page.evaluate(() => window.fixture.release());
    // Release settles the actual pending IPC promise; subsequent DOM checks
    // observe its continuation, not a fixed-duration synchronization sleep.
    await expect(page.getByRole('dialog')).toHaveCount(0);
    const calls = await onlyDisplayCalls(page); assert.equal(calls.filter(call => call.command === 'artifact_adopt').length, 0);
    assert.equal(await page.evaluate(() => window.testMessage.artifact_revision), 2);
    assert.deepEqual(await page.evaluate(() => window.testMessage.presentation_snapshot.spec), spec);
    await expect(page.getByText('Monthly regional values', { exact: true })).toBeVisible(); assert.deepEqual(state.errors, []);
  } });
} catch (error) { failures.push({ name: 'harness-setup', error: error.stack ?? String(error) }); log('FAIL harness-setup: ' + error.message); }
finally {
  await browser?.close(); await server.close();
  await writeFile(evidenceDirectory + '/results-v1.2.json', JSON.stringify({ version, criteria, failures }, null, 2));
  await writeFile(evidenceDirectory + '/run.log', logs.join('\n') + '\n');
  log({ result: failures.length ? 'FAIL' : 'PASS', failures: failures.length, evidenceDirectory });
}
if (failures.length) process.exitCode = 1;
