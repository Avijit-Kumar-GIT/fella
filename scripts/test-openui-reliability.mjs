// Reliability v1: requirements frozen in docs/testing/OPENUI-RELIABILITY-v1.md.
import assert from 'node:assert/strict';
import { createServer } from 'vite';
const server = await createServer({ configFile: 'vite.config.ts', appType: 'custom', logLevel: 'error', optimizeDeps: { noDiscovery: true, entries: [] }, server: { middlewareMode: true, hmr: false, ws: false } });
try {
 const { cachePresentation, transcriptPresentation } = await server.ssrLoadModule('/src/lib/presentation/snapshot.ts');
 const { ensureAnalyticalOutput, outputJobs } = await server.ssrLoadModule('/src/lib/analytical-output.svelte.ts');
 const results = [{ id: 'r', columns: ['amount'], rows: [[12.5]], exact_rows: [['12.50']], row_count: 1, complete: true }];
 const program = 'root = Document([Metric("r", "amount", 0, "Revenue", "USD")])';
 const message = (id) => ({ id, role: 'assistant', text: 'Revenue', ts: 1, answer: { turn_id: id, text: 'Revenue', workspace: { path: '/workspace', revision: 'r1' }, evidence: [], verification: [] } });
 const cached = message('cache'); cachePresentation(cached, program, results);
 assert.equal(transcriptPresentation(cached)?.program, program);
 cached.answer.workspace.path = '/other'; assert.equal(transcriptPresentation(cached), null);
 console.log('PASS synchronous cache and workspace isolation');
 let repairs = 0, creates = 0;
 const target = message('repair');
 const resolved = { artifact: { id: 'a', workspace: '/workspace', revisions: [] }, revision: { revision: 1, program }, run: { id: 'repair' }, results };
 const bridge = {
  presentationResults: async () => ({ run: { question: 'Revenue?' }, results }),
  presentationRepair: async (_workspace, turn, failed, issues) => { assert.equal(turn, 'repair'); assert.equal(failed, ''); assert.ok(issues.length); repairs++; return { program }; },
  artifactList: async () => [], artifactCreate: async () => { creates++; return resolved.artifact; }, artifactResolve: async () => resolved,
 };
 assert.equal(await ensureAnalyticalOutput(target, '/workspace', bridge, () => {}), resolved);
 assert.equal(repairs, 1); assert.equal(creates, 1);
 assert.equal(transcriptPresentation(target)?.program, program);
 console.log('PASS missing program repaired once against saved results and cached');
 const broken = message('bounded'); let attempts = 0;
 assert.equal(await ensureAnalyticalOutput(broken, '/workspace', { ...bridge, presentationRepair: async () => { attempts++; return { program: 'invalid(' }; } }, () => {}), null);
 assert.equal(attempts, 2); assert.match(outputJobs.bounded.error, /could not be validated/);
 assert.equal(creates, 1, 'Invalid presentations must not create artifacts');
 console.log('PASS repair bounded to two calls; invalid output remains a failure');
} finally { await server.close(); }
