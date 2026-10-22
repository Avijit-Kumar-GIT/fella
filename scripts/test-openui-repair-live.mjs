// Reliability v1 live check: real configured model, saved-run presentation only.
// Input run stays byte-identical; no analytical tools may execute. A valid
// correction must be obtained within the same two-attempt production budget.
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir, mkdtemp } from 'node:fs/promises';
import { tmpdir, homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'vite';
import { EngineClient } from '../electron/engine.mjs';
const inputs = process.argv.slice(2);
assert.ok(inputs.length, 'Pass saved analysis-turn JSON paths; originals are never modified.');
const profile = await mkdtemp(join(tmpdir(), 'fella-openui-repair-live-'));
const authPath = process.env.FELLA_E2E_AUTH_FILE || join(homedir(), '.local/share/dev.fella.app/auth.json');
const provider = process.env.FELLA_OPENUI_LIVE_PROVIDER || 'ollama-cloud';
const model = process.env.FELLA_OPENUI_LIVE_MODEL || 'gemma4:31b';
const auth = JSON.parse(await readFile(authPath, 'utf8'));
assert.ok(auth[`apikey:${provider}`], 'Configured provider credential required; never printed.');
await writeFile(join(profile, 'auth.json'), JSON.stringify({ [`apikey:${provider}`]: auth[`apikey:${provider}`] }), { mode: 0o600 });
await mkdir(join(profile, 'analysis/turns'), { recursive: true });
const engine = new EngineClient(resolve(process.env.FELLA_ENGINE_PATH || 'backend/target/release/fella'), profile);
const server = await createServer({ configFile:'vite.config.ts', appType:'custom', logLevel:'error', optimizeDeps:{ noDiscovery:true, entries:[] }, server:{ middlewareMode:true, hmr:false, ws:false } });
try {
 const { validatePresentation } = await server.ssrLoadModule('/src/lib/presentation/validate.ts');
 await engine.request('set_settings', { settings: { provider, model } });
 for (const input of inputs) {
  const original = await readFile(input, 'utf8'), run = JSON.parse(original);
  const savedPath = join(profile, 'analysis/turns', `${run.id}.json`);
  await writeFile(savedPath, original);
  const { results } = await engine.request('presentation_results', { workspaceId: run.workspace, turnId: run.id });
  let program = run.result.presentation_program ?? '', check = validatePresentation(program, results), attempts = 0;
  assert.equal(check.valid, false, 'This case must exercise a previously invalid/missing presentation.');
  const started = Date.now();
  while (!check.valid && attempts < 2) {
   attempts++;
   let timer;
   const response = await Promise.race([
    engine.request('presentation_repair', { workspaceId: run.workspace, turnId: run.id, program, issues: check.issues.map(x => x.message), model }),
    new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Presentation repair exceeded 180 seconds')), 180000); })
   ]).finally(() => clearTimeout(timer));
   program = response.program; check = validatePresentation(program, results);
  }
  await writeFile(join(profile, `${run.id}.openui`), program);
  await writeFile(join(profile, `${run.id}.validation.json`), JSON.stringify({ valid:check.valid, issues:check.issues, attempts }));
  assert.equal(await readFile(savedPath, 'utf8'), original, 'Repair must not rewrite canonical analysis.');
  assert.equal(check.valid, true, JSON.stringify(check.issues));
  console.log(`PASS ${run.id}: ${model}, ${attempts} correction(s), ${Date.now()-started}ms; canonical run unchanged`);
 }
} finally { engine.dispose(); await server.close(); console.log(`Diagnostic directory: ${profile}`); }
