import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer } from 'vite';
import { chromium } from '@playwright/test';

// Answer/disclosure regression v1. Expected behavior fixed before execution:
// - generic needs_review without an actionable check is not a warning badge;
// - informational/not-run checks do not become failures; real flagged/legacy
//   failures and unresolved semantics remain visible and inspectable;
// - failed/incomplete statuses remain warnings and backend records never change;
// - a live, completed, new, or reopened timeline starts closed; only a user
//   click opens it, and later progress/completion cannot undo that choice.
const harnessScript = `
import { mount, unmount, tick } from 'svelte';
import RunTimeline from '/src/lib/components/RunTimeline.svelte';
import { session, Conversation } from '/src/lib/session.svelte.ts';
let component;
const target = document.getElementById('fixture');
window.answerHarness = {
  async mountMessage(message, busy) {
    if (component) await unmount(component);
    const chat = new Conversation('answer-disclosure-test');
    chat.messages = [message]; chat.busy = busy;
    session.conversations = [chat]; session.activeConversationIndex = 0;
    component = mount(RunTimeline, { target, props: {
      get message() { return chat.messages[0]; }, bodyId: 'fixture-evidence'
    }});
    this.chat = chat; await tick();
  },
  async complete(answer) {
    this.chat.messages[0].answer = answer;
    this.chat.messages[0].pending = false;
    this.chat.busy = false; await tick();
  },
  async replace(message) { this.chat.messages = [message]; this.chat.busy = true; await tick(); },
  async progress() { this.chat.activity = 'Reading another source'; await tick(); },
  async mountTurn(turn) {
    if (component) await unmount(component);
    component = mount(RunTimeline, { target, props: { turn, bodyId:'saved-evidence' }}); await tick();
  }
};`;
const source = '<!doctype html><html><head><link rel="icon" href="data:,"></head><body><main id="fixture"></main><script type="module" src="/__answer-disclosure-entry.js"></script></body></html>';

// Concurrent Vite scripts must not invalidate one another's optimized modules.
// This is setup isolation only; regression v1's assertions are unchanged.
const cacheDir = await mkdtemp(join(tmpdir(), 'fella-answer-disclosure-cache-'));
const server = await createServer({
	configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error',
	cacheDir,
	optimizeDeps: { noDiscovery: true, entries: [] },
	server: { host: '127.0.0.1', port: 0, strictPort: false, hmr: false, ws: false, fs: { allow: [process.cwd(), cacheDir] } },
	plugins: [{ name: 'answer-disclosure-fixture',
		resolveId(id) { if (id === '/__answer-disclosure-entry.js') return '\0answer-disclosure-entry.js'; },
		load(id) { if (id === '\0answer-disclosure-entry.js') return harnessScript; },
		configureServer(server) {
		server.middlewares.use('/__answer-disclosure', (_request, response) => {
			response.setHeader('Content-Type', 'text/html'); response.end(source);
		});
	} }]
});
let browser;
let passed = 0;
let succeeded = false;
async function check(name, run) { await run(); passed++; console.log(`PASS ${name}`); }

try {
	await server.listen();
	const { render } = await server.ssrLoadModule('svelte/server');
	const { default: Message } = await server.ssrLoadModule('/src/lib/components/Message.svelte');
	const { default: EvidenceBlock } = await server.ssrLoadModule('/src/lib/components/EvidenceBlock.svelte');
	const { answerNeedsReview } = await server.ssrLoadModule('/src/lib/components/EvidenceSummary.svelte');
	const base = {
		text:'Revenue was 40.', status:'needs_review',
		trace:{id:'trace',turn_id:'turn',mode:'workspace_ask',steps:[{id:'query',operation:'run_sql',duration_ms:1,success:true,sources:['sales.csv']}]},
		evidence:[{id:'query',tool:'run_sql',args:{},result_summary:'One result',columns:['revenue'],rows:[[40]],row_count:1,ms:1}],
		verification:[{label:'Query replay matched',ok:true,outcome:'passed',finding:{code:'replay_match',effect:'informational',target:'answer'}}]
	};
	const message = answer => ({id:'answer',role:'assistant',text:answer.text,ts:1,answer});
	const markup = answer => render(Message,{props:{message:message(answer)}}).body;
	await check('clean needs_review is quiet without falsely claiming Verified', () => {
		const original = JSON.stringify(base);
		assert.equal(answerNeedsReview(base),false);
		const html = markup(base);
		assert.doesNotMatch(html,/data-testid="answer-review-status"|Verified under recorded checks/);
		assert.match(html,/aria-expanded="false"/);
		assert.doesNotMatch(html,/class="evidence-body/);
		assert.equal(JSON.stringify(base),original);
	});
	await check('informational and not-run checks retain detail without creating an alert', () => {
		const answer = {...base,verification:[...base.verification,
			{label:'Interpretation is not independently certified',ok:false,outcome:'flagged',finding:{code:'semantic_caveat',effect:'informational',target:'answer'}},
			{label:'Optional replay not run',ok:false,outcome:'not_run'}]};
		assert.equal(answerNeedsReview(answer),false);
		const html = render(EvidenceBlock,{props:{answer,expanded:true,bodyId:'limits'}}).body;
		assert.match(html,/data-recorded-verification-status="needs_review"/);
		assert.match(html,/Interpretation is not independently certified/);
		assert.match(html,/Optional replay not run/);
		assert.doesNotMatch(html,/At least one check raised a concern/);
	});
	await check('real and archived unscoped failures still get one review badge', () => {
		for (const check of [
			{label:'Replay differed',ok:false,outcome:'flagged',finding:{code:'replay_mismatch',effect:'block_answer',target:'answer'}},
			{label:'Legacy failed check',ok:false},
			{label:'Explicit advisory',ok:false,outcome:'flagged',finding:{code:'advisory',effect:'informational',target:'answer'}}
		]) {
			const answer = {...base,verification:[check]};
			assert.equal(answerNeedsReview(answer),true);
			assert.equal([...markup(answer).matchAll(/data-testid="answer-review-status"/g)].length,1);
			assert.match(render(EvidenceBlock,{props:{answer,expanded:true,bodyId:'flagged'}}).body,new RegExp(check.label));
		}
	});
	await check('explicit semantic gaps remain actionable with discoverable reasons', () => {
		const answer = {...base,contract:{interpretation:'ambiguous',unresolved:['Does net revenue exclude refunds?']}};
		assert.equal(answerNeedsReview(answer),true);
		assert.match(markup(answer),/data-testid="answer-review-status"/);
		const html = render(EvidenceBlock,{props:{answer,expanded:true,bodyId:'gap'}}).body;
		assert.match(html,/aria-label="Unresolved interpretation"/);
		assert.match(html,/Does net revenue exclude refunds\?/);
	});
	await check('failed and incomplete answers keep their visible limitation warnings', () => {
		assert.match(markup({...base,status:'failed'}),/Verification failed/);
		assert.match(markup({...base,status:'insufficient_data'}),/Evidence is incomplete/);
		const clarification = {...base,clarification:{question:'Which revenue definition?',options:[]}};
		assert.equal(answerNeedsReview(clarification),false);
	});

	// Actual client-side component: SSR alone cannot establish effect/click state.
	const address = server.httpServer.address();
	assert.ok(address && typeof address === 'object');
	browser = await chromium.launch({headless:true});
	const page = await browser.newPage();
	const errors = []; page.on('pageerror',error => errors.push(error.message));
	await page.goto(`http://127.0.0.1:${address.port}/__answer-disclosure`);
	await page.waitForFunction(() => !!window.answerHarness,undefined,{timeout:15_000});
	const host = page.locator('#fixture');
	const running = {id:'running-answer',role:'assistant',text:'',ts:2,pending:true,run_state:'executing',run_steps:[{id:'step',label:'Read source',state:'running',started_at_ms:1}]};
	const show = () => host.getByRole('button',{name:'Show analysis steps',exact:true});
	const hide = () => host.getByRole('button',{name:'Hide analysis steps',exact:true});
	await check('running steps start closed and completion does not open them', async () => {
		await page.evaluate(message => window.answerHarness.mountMessage(message,true),running);
		await show().waitFor(); assert.equal(await show().getAttribute('aria-expanded'),'false');
		assert.equal(await host.getByRole('list',{name:'Recorded steps'}).count(),0);
		await page.evaluate(answer => window.answerHarness.complete(answer),base);
		assert.equal(await show().getAttribute('aria-expanded'),'false');
	});
	await check('user-opened steps persist across updates; user close is respected', async () => {
		await page.evaluate(message => window.answerHarness.mountMessage(message,true),running);
		await show().click(); assert.equal(await hide().getAttribute('aria-expanded'),'true');
		await page.evaluate(() => window.answerHarness.progress());
		assert.equal(await hide().getAttribute('aria-expanded'),'true');
		await page.evaluate(answer => window.answerHarness.complete(answer),base);
		assert.equal(await hide().getAttribute('aria-expanded'),'true');
		await hide().click(); assert.equal(await show().getAttribute('aria-expanded'),'false');
	});
	await check('a different question and a reopened saved turn start closed', async () => {
		await show().click();
		await page.evaluate(message => window.answerHarness.replace(message),{...running,id:'next-answer'});
		assert.equal(await show().getAttribute('aria-expanded'),'false');
		await page.evaluate(turn => window.answerHarness.mountTurn(turn),{
			id:'saved-turn',state:'needs_review',trace:base.trace,plan:{strategy:'direct_tools',steps:[]},result:{text:base.text,status:base.status,evidence:base.evidence,verification:base.verification}
		});
		assert.equal(await show().getAttribute('aria-expanded'),'false');
	});
	assert.deepEqual(errors,[],'the focused client regression must not mask renderer errors');
	console.log(`Answer/disclosure regression v1: ${passed} passed.`);
	succeeded = true;
} catch (error) {
	const evidence = join(cacheDir, 'failure.json');
	await writeFile(evidence, JSON.stringify({ passed, cacheDir, error: String(error), stack: error.stack }, null, 2));
	console.error(`Focused regression failed; isolated cache and failure evidence retained at ${evidence}`);
	throw error;
} finally {
	try { await browser?.close(); }
	finally {
		await server.close();
		if (succeeded) await rm(cacheDir, { recursive: true, force: true });
	}
}
