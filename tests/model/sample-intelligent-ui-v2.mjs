#!/usr/bin/env node
// Real native-engine evaluation. No engine/provider work without explicit --live.
import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFile, writeFile, mkdir, mkdtemp, readdir, rm, chmod, realpath, stat } from 'node:fs/promises';
import { appendFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { homedir } from 'node:os';
import { resolve, join, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const CRITERIA = new URL('./sample-intelligent-ui-v2.criteria.json', import.meta.url);
const RUBRIC = new URL('./sample-intelligent-ui-v2.md', import.meta.url);
const PIN = { criteria: '7a0fffc79bde6e8d3264b2574f267e17df82b8589629483fecff96dbbe0c95ba', rubric: 'ba61290519e336c69bf2abd197eff65268e4ff78ac16ff5824944be8ddc14e33' };
const IDS = ['plain-q4', 'monthly-2025', 'regional-explore', 'followup-visual', 'followup-exact'];
const hash = x => createHash('sha256').update(x).digest('hex');
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const ensure = (v, m) => { if (!v) throw new Error(m); };
const secrets = [];
const redact = x => secrets.reduce((s, key) => s.replaceAll(key, '[REDACTED]'), typeof x === 'string' ? x : JSON.stringify(x));
const save = (path, value) => writeFile(path, redact(JSON.stringify(value, null, 2)) + '\n', { mode: 0o600 });
const args = process.argv.slice(2);
const selected = new Set();
let live = false;
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--live') live = true;
  else if (args[i] === '--all') IDS.forEach(id => selected.add(id));
  else if (args[i] === '--case') (args[++i] ?? '').split(',').filter(Boolean).forEach(id => selected.add(id));
  else if (['--list', '--help', '-h'].includes(args[i])) {
    console.log('Opt-in native evaluation: --live --case ID [--case ID] or --live --all\n' + IDS.join('\n'));
    process.exit(0);
  } else throw new Error(`Unknown option: ${args[i]}`);
}
if (!live) { console.log('Not run. Use --list; pass --live and select cases only after native/catalog are ready.'); process.exit(0); }
ensure(selected.size, 'Choose --case or --all explicitly');
for (const id of selected) ensure(IDS.includes(id), `Unknown case: ${id}`);
const criteriaBytes = await readFile(CRITERIA), rubricBytes = await readFile(RUBRIC);
ensure(hash(criteriaBytes) === PIN.criteria && hash(rubricBytes) === PIN.rubric, 'Frozen criteria/rubric changed; do not run under this version');
const criteria = JSON.parse(criteriaBytes);
const timeoutMs = Number(process.env.FELLA_SAMPLE_TURN_MS ?? criteria.protocol.max_turn_ms);
ensure(Number.isInteger(timeoutMs) && timeoutMs > 0 && timeoutMs <= 240000, 'Turn timeout must be 1..240000 ms');
const workspace = await realpath(process.env.FELLA_SAMPLE_WORKSPACE ?? '/mnt/c/Users/aviji/Documents/Fella Analyst Sample Workspace');
const binary = await realpath(resolve(ROOT, process.env.FELLA_ENGINE_PATH ?? 'backend/target/debug/fella'));
const validator = await realpath(resolve(ROOT, process.env.FELLA_CONTRACT_ACCEPT_PATH ?? 'backend/target/debug/examples/contract_accept'));
const authSource = process.env.FELLA_SAMPLE_AUTH_FILE ?? join(homedir(), '.local/share/dev.fella.app/auth.json');
const output = process.env.FELLA_SAMPLE_EVAL_OUTPUT ? resolve(process.env.FELLA_SAMPLE_EVAL_OUTPUT) : await mkdtemp('/tmp/fella-sample-intelligent-ui-v2-');
ensure(output.startsWith('/tmp/'), 'Output must be a new directory under /tmp');
if (process.env.FELLA_SAMPLE_EVAL_OUTPUT) await mkdir(output, { mode: 0o700 });
await chmod(output, 0o700);
console.log(`REPORT_DIR=${output}`);
const report = { version: criteria.version, started_at: new Date().toISOString(), selected: [...selected],
  model: criteria.model, provider: criteria.provider, turn_timeout_ms: timeoutMs, output, workspace,
  criteria_sha256: PIN.criteria, rubric_sha256: PIN.rubric, cases: [], exclusions: criteria.excluded_from_live_score,
  limitations: ['No browser event/pixel assertion.', 'Semantic/source-policy checks require separate review.',
    'Display edit token usage is unavailable in the native response.', 'No harness retry or automatic adjudication.'] };
const reportPath = join(output, 'results.json');
await writeFile(join(output, 'frozen-criteria.json'), criteriaBytes, { mode: 0o600 });
await writeFile(join(output, 'frozen-rubric.md'), rubricBytes, { mode: 0o600 });
await save(reportPath, report);
const checkpoint = () => save(reportPath, report);
let authBytes;
async function snapshotSources() {
  const snapshot = {};
  for (const name of Object.keys(criteria.audited_source_sha256)) snapshot[name] = hash(await readFile(join(workspace, name)));
  return snapshot;
}
function assertion(group, label, ok, detail) { return { group, label, outcome: ok === null ? 'unresolved' : ok ? 'pass' : 'fail', detail }; }
function grade(result, group, label, ok, detail) { result.checks.push(assertion(group, label, ok, detail)); }
function summarize(result) {
  result.grades = {};
  for (const group of [...new Set(result.checks.map(c => c.group))]) {
    const checks = result.checks.filter(c => c.group === group);
    result.grades[group] = checks.some(c => c.outcome === 'fail') ? 'fail' : checks.some(c => c.outcome === 'unresolved') ? 'unresolved' : 'pass';
  }
}
class Native {
  constructor(data, dir) {
    this.dir = dir; this.n = 0; this.pending = new Map(); this.dead = false;
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/(API_KEY|TOKEN|SECRET|FELLA_.*(MODEL|PROMPT|PROVIDER|DATA|AUTH|BASE_URL))/.test(key)));
    this.child = spawn(binary, ['--engine-stdio', '--data-dir', data], { cwd: ROOT, env: { ...env, FELLA_MEMORY: '0' }, stdio: ['pipe', 'pipe', 'pipe'] });
    this.child.stderr.on('data', b => appendFileSync(join(dir, 'stderr.log'), redact(String(b))));
    this.lines = createInterface({ input: this.child.stdout });
    this.lines.on('line', line => {
      appendFileSync(join(dir, 'stdio.jsonl'), redact(line) + '\n');
      let msg; try { msg = JSON.parse(line); } catch { this.fail(new Error('Non-JSON native stdout')); return; }
      const pending = this.pending.get(msg.id);
      if (!pending) return;
      if (msg.event) { pending.events.push(msg.event); return; }
      clearTimeout(pending.timer); this.pending.delete(msg.id);
      if (msg.ok) pending.resolve(msg.result); else pending.reject(new Error(`Native ${pending.method}: ${redact(msg.error)}`));
    });
    this.child.on('error', e => this.fail(e));
    this.child.stdin.on('error', e => this.fail(e));
    this.child.on('exit', (code, signal) => { this.dead = true; this.fail(new Error(`Native exited: ${code ?? signal}`)); });
  }
  fail(error) { for (const p of this.pending.values()) { clearTimeout(p.timer); p.reject(error); } this.pending.clear(); }
  async call(method, params = {}, events = [], limit = timeoutMs) {
    ensure(!this.dead, 'Native process already stopped');
    const id = ++this.n, started = Date.now();
    appendFileSync(join(this.dir, 'requests.jsonl'), redact({ id, method, params, at: new Date().toISOString() }) + '\n');
    try {
      return await new Promise((resolveCall, reject) => {
        const timer = setTimeout(() => { const e = new Error(`${method} timed out at ${limit} ms; not retried`); e.timeout = true; this.fail(e); this.stop(); }, limit);
        this.pending.set(id, { resolve: resolveCall, reject, timer, events, method });
        this.child.stdin.write(JSON.stringify({ id, method, params }) + '\n');
      });
    } finally { appendFileSync(join(this.dir, 'timings.jsonl'), JSON.stringify({ id, method, elapsed_ms: Date.now() - started }) + '\n'); }
  }
  stop() { this.lines.close(); if (!this.dead) this.child.kill('SIGKILL'); this.fail(new Error('Native stopped')); }
}
async function profile(name, fn) {
  const dir = join(output, name); await mkdir(dir, { mode: 0o700 });
  const data = join(dir, 'data'); await mkdir(data, { mode: 0o700 });
  const auth = JSON.parse(authBytes); const key = auth['apikey:ollama-cloud'];
  ensure(typeof key === 'string' && key.length > 0, 'Local auth lacks Ollama Cloud credential');
  await save(join(data, 'profile-metadata.json'), { isolated: true, live_db_copied: false });
  // Never send credentials through IPC/logging. Only the native private profile reads this file.
  await writeFile(join(data, 'auth.json'), JSON.stringify({ 'apikey:ollama-cloud': key }), { mode: 0o600 });
  const engine = new Native(data, dir);
  try {
    await engine.call('set_settings', { settings: { provider: criteria.provider, base_url: criteria.base_url, model: criteria.model } });
    const settings = await engine.call('get_settings');
    ensure(settings.has_credential && settings.provider === criteria.provider && settings.model === criteria.model, 'Isolated provider settings or auth unavailable');
    await save(join(dir, 'settings.json'), settings);
    await engine.call('open_workspace', { path: workspace });
    return await fn(engine, dir, data);
  } finally { engine.stop(); await rm(join(data, 'auth.json'), { force: true }); }
}
async function accept(spec, results, dir, label) {
  const started = Date.now();
  const child = spawnSync(validator, [], { cwd: ROOT, input: JSON.stringify({ spec, results, charts_enabled: true }) + '\n', encoding: 'utf8', timeout: 30000, maxBuffer: 32 * 1024 * 1024 });
  ensure(!child.error && child.status === 0, `Shared native validator unavailable: ${child.error?.message ?? child.status}`);
  const value = JSON.parse(child.stdout.trim());
  await save(join(dir, `${label}-acceptance.json`), { ...value, elapsed_ms: Date.now() - started });
  ensure(value.contractHash === report.catalog_contract_hash, 'Validator/catalog mismatch; parent must rebuild before evaluation');
  return value;
}
function reachable(spec, state = {}) {
  if (!spec?.elements?.[spec.root]) return [];
  const result = [], seen = new Set();
  function visit(id) {
    if (seen.has(id)) return; seen.add(id);
    const node = spec.elements[id]; if (!node || node.visible === false) return;
    if (node.visible && typeof node.visible === 'object') {
      const key = node.visible.$state?.replace(/^\/(?:control:)?/, '');
      if (Object.hasOwn(node.visible, 'eq') ? state[key] !== node.visible.eq : !state[key]) return;
    }
    result.push({ id, ...node });
    const children = node.type === 'Tabs' ? (node.children ?? []).slice(0, 1) : node.children ?? [];
    children.forEach(visit);
  }
  visit(spec.root); return result;
}
function stateDefaults(spec) { return Object.fromEntries(Object.entries(spec?.elements?.[spec.root]?.props?.contract?.state ?? {}).map(([key, decl]) => [key, decl.default])); }
function regionCheck(spec, results) {
  const declarations = spec?.elements?.[spec.root]?.props?.contract?.state ?? {};
  for (const [key, decl] of Object.entries(declarations)) {
    const domain = (decl.options ?? []).map(o => o.value).filter(v => v !== '$all').sort();
    if (decl.type !== 'enum' || decl.persistence !== 'viewer' || !same(domain, criteria.expected.regions)) continue;
    const options = (decl.options ?? []).map(o => o.value);
    if (!options.includes(decl.default)) continue;
    const defaults = stateDefaults(spec);
    const control = reachable(spec, defaults).find(n => n.type === 'Select' && n.props.stateKey === key && same((n.props.options ?? []).map(o => o.value), options) && n.props.defaultValue === decl.default);
    if (!control) continue;
    const selections = [];
    for (const region of domain) {
      const state = { ...defaults, [key]: region };
      const consumers = reachable(spec, state).filter(n => ['Table', 'Chart'].includes(n.type));
      const matches = [];
      for (const node of consumers) {
        const result = results.find(r => r.id === node.props.resultId);
        if (!result?.complete) continue;
        for (const binding of node.props.filters ?? []) {
          const col = result.columns.indexOf(binding.field);
          if (binding.stateKey !== key || col < 0) continue;
          const rows = result.exact_rows ?? result.rows;
          const values = [...new Set(rows.map(r => String(r[col])))];
          if (!domain.every(v => values.includes(v))) continue;
          const selected = rows.map((r, i) => ({ r, i })).filter(({ r }) => String(r[col]) === region).map(({ i }) => i);
          if (selected.length && selected.length < rows.length) matches.push({ node: node.id, result_id: result.id, field: binding.field, rows: selected });
        }
      }
      selections.push({ region, matches });
    }
    if (selections.every(s => s.matches.length)) return { ok: true, key, declaration: decl, control: control.id, selections };
  }
  return { ok: false, reason: 'No supported reachable Select/viewer enum/actual saved-row consumer for every geographic region' };
}
function chartLeaves(value, inherited = {}) {
  if (!value || typeof value !== 'object' || value.repeat) return [];
  const encoding = { ...inherited, ...value.encoding };
  const nested = value.layer ?? value.concat ?? value.hconcat ?? value.vconcat ?? (value.spec ? [value.spec] : null);
  return nested ? nested.flatMap(s => chartLeaves(s, encoding)) : [{ mark: typeof value.mark === 'string' ? value.mark : value.mark?.type, encoding }];
}
function month(v) {
  const s = String(v); const m = s.match(/^2025[-/](0?[1-9]|1[0-2])(?:[-/T ].*)?$/);
  return m ? `2025-${m[1].padStart(2, '0')}` : null;
}
function numeric(v) { const s = String(v).replace(/[$,]/g, '').trim(); return /^[-+]?\d+(?:\.\d+)?(?:e[-+]?\d+)?$/i.test(s) ? Number(s) : NaN; }
function closeMoney(v, expected) { return Math.abs(numeric(v) - Number(expected)) <= criteria.protocol.numeric_serialization_tolerance_usd; }
function monthlyMatches(results) {
  const matches = [];
  for (const result of results) {
    const rows = result.exact_rows ?? result.rows;
    if (!result.complete || rows?.length !== 12 || result.row_count !== 12) continue;
    for (let mi = 0; mi < result.columns.length; mi++) {
      const dates = rows.map(r => month(r[mi]));
      if (new Set(dates).size !== 12 || dates.includes(null)) continue;
      for (let ni = 0; ni < result.columns.length; ni++) {
        if (!/(net|revenue)/i.test(result.columns[ni]) || /(gross|before|pre.refund)/i.test(result.columns[ni])) continue;
        const differences = rows.flatMap((row, i) => {
          const expected = criteria.expected.monthly.find(r => r.period === dates[i]).net_revenue;
          return closeMoney(row[ni], expected) ? [] : [{ month: dates[i], actual: row[ni], expected }];
        });
        matches.push({ result_id: result.id, month_field: result.columns[mi], net_field: result.columns[ni], differences });
      }
    }
  }
  return matches;
}
function monthlyChart(spec, results, matches, lineOnly = false) {
  for (const node of reachable(spec, stateDefaults(spec)).filter(n => n.type === 'Chart')) {
    const result = results.find(r => r.id === node.props.resultId);
    if (!result?.complete) continue;
    for (const leaf of chartLeaves(node.props.spec)) {
      if (!(lineOnly ? ['line'] : ['line', 'area']).includes(leaf.mark)) continue;
      const x = leaf.encoding.x, y = leaf.encoding.y;
      const rows = result.exact_rows ?? result.rows;
      const field = x?.field, ci = result.columns.indexOf(field);
      if (ci < 0 || !y?.field || !result.columns.includes(y.field)) continue;
      const chronological = x.type === 'temporal' || Array.isArray(x.sort) && same(x.sort.map(month), criteria.expected.monthly.map(r => r.period));
      if (!chronological || !rows.every(r => month(r[ci]))) continue;
      if (matches && !matches.some(m => m.result_id === result.id && m.month_field === field && m.net_field === y.field)) continue;
      return { ok: true, node: node.id, result_id: result.id, mark: leaf.mark, x, y, usd_axis: /USD|\$/i.test(JSON.stringify([node.props.title, x.title, y.title, y.axis])) };
    }
  }
  return { ok: false };
}
async function turns(data) {
  try { return (await readdir(join(data, 'analysis/turns'))).filter(n => n.endsWith('.json')).sort(); }
  catch (e) { if (e.code === 'ENOENT') return []; throw e; }
}
async function fresh(engine, dir, id) {
  const item = { id, kind: 'fresh_ask', checks: [], started_at: new Date().toISOString() }; report.cases.push(item); await checkpoint();
  const events = [], started = Date.now();
  try {
    const answer = await engine.call('ask', { workspaceId: workspace, conversationId: `sample-v2-${id}-${randomUUID()}`, question: criteria.questions[id], model: criteria.model }, events);
    item.elapsed_ms = Date.now() - started;
    await save(join(dir, 'answer.json'), answer); await save(join(dir, 'events.json'), events);
    const saved = await engine.call('presentation_results', { workspaceId: workspace, turnId: answer.turn_id });
    await save(join(dir, 'saved.json'), saved);
    const { run, results } = saved, spec = answer.presentation_spec;
    item.turn_id = answer.turn_id; item.usage = answer.usage ?? null; item.trace = answer.trace;
    grade(item, 'execution', 'real native model call trace', (answer.trace?.model_calls ?? []).some(c => c.success && c.model.includes(criteria.model)), { model: answer.trace?.model, calls: answer.trace?.model_calls });
    grade(item, 'execution', 'saved canonical run matches answer', run.id === answer.turn_id && run.question === criteria.questions[id], run.id);
    if (id === 'plain-q4') {
      const count = results.some(r => r.complete && r.row_count === 1 && r.columns.some((c, i) => /count|orders/i.test(c) && numeric((r.exact_rows ?? r.rows)[0]?.[i]) === 102));
      grade(item, 'correctness', 'Q4 distinct order count 102 in saved scalar and answer', count && /\b102\b/.test(answer.text));
      grade(item, 'presentation', 'plain answer has no presentation spec', spec == null);
    } else if (id === 'monthly-2025') {
      const candidates = monthlyMatches(results), matches = candidates.filter(m => !m.differences.length);
      grade(item, 'correctness', 'complete 12-month audited net series', matches.length > 0, candidates);
      const claims = JSON.stringify([answer.text, ...results.map(r => [r.columns, r.exact_rows ?? r.rows])]).replaceAll(',', '');
      for (const [label, value] of [['Q3', criteria.expected.q3_net], ['Q4', criteria.expected.q4_net], ['increase', criteria.expected.q4_minus_q3]]) {
        grade(item, 'correctness', `${label} explicitly reported`, new RegExp(`(?<![\\d.])${value.replace('.', '\\.')}0*(?![\\d.])`).test(claims), value);
      }
      grade(item, 'policy_review', 'definitions, dedup, refund posting, order/AOV scope', null, 'Review saved evidence/SQL and text against frozen rubric; correct totals alone do not establish policy adherence.');
      grade(item, 'policy_review', 'orphan 64.25 validity and sensitivity disclosed', null, { answer_mentions_amount: /64[.]25/.test(answer.text), required: 'Literal-policy total versus matched-only sensitivity; source-owner validity uncertain.' });
      if (spec) {
        const acceptance = await accept(spec, results, dir, id);
        grade(item, 'presentation', 'native shared acceptance', acceptance.valid, acceptance.issues);
        const trend = monthlyChart(spec, results, matches);
        grade(item, 'presentation', 'saved-result-bound monthly trend', trend.ok, trend);
        grade(item, 'presentation', 'exact saved monthly values available as Table', reachable(spec, stateDefaults(spec)).some(n => n.type === 'Table' && matches.some(m => m.result_id === n.props.resultId)));
      } else grade(item, 'presentation', 'monthly trend and exact-value view', false, 'No presentation_spec');
    } else {
      if (spec) {
        const acceptance = await accept(spec, results, dir, id); grade(item, 'presentation', 'native shared acceptance', acceptance.valid, acceptance.issues);
        const regional = regionCheck(spec, results); item.region = regional;
        grade(item, 'presentation', 'region Select/state/consumer over saved domain', regional.ok, regional);
        const regionalRows = results.filter(r => r.complete && r.columns.some(c => /region/i.test(c))).map(r => ({ id: r.id, rows: r.row_count }));
        grade(item, 'correctness', 'complete monthly regional saved coverage', results.some(r => {
          const rows = r.exact_rows ?? r.rows; const ri = r.columns.findIndex(c => /region/i.test(c));
          return r.complete && ri >= 0 && r.columns.some((_, mi) => criteria.expected.regions.every(region => new Set(rows.filter(row => String(row[ri]) === region).map(row => month(row[mi])).filter(Boolean)).size === 12));
        }), regionalRows);
      } else grade(item, 'presentation', 'region Select/state/consumer', false, 'No presentation_spec');
      grade(item, 'policy_review', 'regional orphan treatment and no invented allocation', null, 'Inspect saved query/text; regional arithmetic is not a frozen refund-allocation gold.');
    }
    item.status = 'completed'; summarize(item); await checkpoint();
    return { answer, saved, item };
  } catch (e) { item.status = e.timeout ? 'timeout' : 'error'; item.error = redact(e.stack ?? String(e)); item.elapsed_ms = Date.now() - started; await save(join(dir, 'events.json'), events); summarize(item); await checkpoint(); return null; }
}
async function edit(engine, dir, data, base, artifact, id) {
  const item = { id, kind: 'saved_json_edit', checks: [], started_at: new Date().toISOString() }; report.cases.push(item); await checkpoint();
  const events = [], started = Date.now();
  try {
    const beforeTurns = await turns(data), canonicalBefore = await engine.call('analysis_turn_load', { turnId: base.answer.turn_id });
    const oldSpec = artifact.revision.spec, oldRegion = regionCheck(oldSpec, artifact.results);
    const response = await engine.call('artifact_presentation_edit', { workspaceId: workspace, artifactId: artifact.artifact.id, baseRevision: artifact.artifact.current_revision, instruction: criteria.questions[id], model: criteria.model, routeFirst: true }, events);
    await save(join(dir, `${id}-response.json`), response); await save(join(dir, `${id}-events.json`), events);
    const canonicalAfter = await engine.call('analysis_turn_load', { turnId: base.answer.turn_id });
    const afterTurns = await turns(data);
    grade(item, 'isolation', 'zero new analytical runs', same(beforeTurns, afterTurns), { before: beforeTurns, after: afterTurns });
    grade(item, 'isolation', 'canonical run/evidence/trace unchanged', same(canonicalBefore, canonicalAfter), { before_sha256: hash(JSON.stringify(canonicalBefore)), after_sha256: hash(JSON.stringify(canonicalAfter)) });
    grade(item, 'isolation', 'zero new analytical tool/evidence events', !events.some(e => ['tool_start', 'tool_end'].includes(e.kind)), events.map(e => e.kind));
    grade(item, 'presentation', 'display-only accepted JSON edit returned', response.requires_analysis === false && !!response.artifact);
    if (response.artifact) {
      const next = response.artifact; await save(join(dir, `${id}-artifact.json`), next);
      grade(item, 'isolation', 'same artifact and canonical run', next.artifact.id === artifact.artifact.id && next.revision.run_id === base.answer.turn_id && next.run.id === base.answer.turn_id);
      grade(item, 'isolation', 'saved result IDs/values/rows unchanged', same(next.results, artifact.results));
      grade(item, 'presentation', 'revision increments and current JSON changes', next.revision.revision === artifact.revision.revision + 1 && !same(next.revision.spec, oldSpec));
      const accepted = await accept(next.revision.spec, next.results, dir, id);
      grade(item, 'presentation', 'native shared acceptance', accepted.valid, accepted.issues);
      const region = regionCheck(next.revision.spec, next.results);
      grade(item, 'presentation', 'preserved region state declaration/domain with connected consumer', oldRegion.ok && region.ok && region.key === oldRegion.key && same(region.declaration, oldRegion.declaration), { before: oldRegion, after: region });
      if (id === 'followup-visual') {
        const trend = monthlyChart(next.revision.spec, next.results, null, true);
        grade(item, 'presentation', 'monthly line chart and USD axis', trend.ok && trend.usd_axis, trend);
      } else grade(item, 'presentation', 'exact saved Table with regional filter', region.ok && reachable(next.revision.spec, stateDefaults(next.revision.spec)).some(n => n.type === 'Table' && (n.props.filters ?? []).some(f => f.stateKey === region.key)));
      artifact = next;
    }
    item.status = 'completed';
  } catch (e) { item.status = e.timeout ? 'timeout' : 'error'; item.error = redact(e.stack ?? String(e)); await save(join(dir, `${id}-events.json`), events); }
  item.elapsed_ms = Date.now() - started; summarize(item); await checkpoint(); return artifact;
}
try {
  report.binary = { path: binary, sha256: hash(await readFile(binary)), mtime: (await stat(binary)).mtime.toISOString() };
  const catalog = await readFile(join(ROOT, 'backend/resources/presentation-catalog.json'));
  report.catalog_sha256 = hash(catalog); report.catalog_contract_hash = JSON.parse(catalog).hash;
  report.validator_sha256 = hash(await readFile(validator));
  report.git_head = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: ROOT, encoding: 'utf8' }).stdout.trim();
  report.git_dirty = spawnSync('git', ['status', '--short'], { cwd: ROOT, encoding: 'utf8' }).stdout.trim().split('\n');
  report.sources_before = await snapshotSources();
  ensure(same(report.sources_before, criteria.audited_source_sha256), 'Audited source snapshot differs; blocked without changing frozen criteria');
  authBytes = await readFile(authSource, 'utf8');
  const auth = JSON.parse(authBytes); ensure(auth['apikey:ollama-cloud'], 'Missing local Ollama Cloud credential');
  secrets.push(...Object.values(auth).filter(v => typeof v === 'string' && v.length > 8));
  report.auth_source_sha256 = hash(authBytes); // Never persist credential content.
  await checkpoint();
  for (const id of ['plain-q4', 'monthly-2025']) if (selected.has(id)) {
    await profile(id, (engine, dir) => fresh(engine, dir, id));
  }
  if (['regional-explore', 'followup-visual', 'followup-exact'].some(id => selected.has(id))) {
    await profile('regional-sequence', async (engine, dir, data) => {
      const base = await fresh(engine, dir, 'regional-explore');
      if (base) base.item.prerequisite = !selected.has('regional-explore');
      const followups = ['followup-visual', 'followup-exact'].filter(id => selected.has(id));
      if (!followups.length) return;
      if (!base?.saved.run.result.presentation_spec || !base.item.region?.ok) {
        for (const id of followups) report.cases.push({ id, status: 'blocked', reason: 'Fresh regional presentation lacks required Select/state/consumer; no fabricated seed or repair substituted.', checks: [] });
        await checkpoint(); return;
      }
      let artifact;
      try {
        const record = await engine.call('artifact_create', { workspaceId: workspace, turnId: base.answer.turn_id, title: 'Sample intelligent UI v2 regional', spec: base.saved.run.result.presentation_spec });
        artifact = await engine.call('artifact_resolve', { workspaceId: workspace, artifactId: record.id });
        await save(join(dir, 'artifact-before-edits.json'), artifact);
      } catch (e) {
        for (const id of followups) report.cases.push({ id, status: 'blocked', reason: `Could not save actual regional spec: ${redact(String(e))}`, checks: [] });
        await checkpoint(); return;
      }
      for (const id of followups) {
        artifact = await edit(engine, dir, data, base, artifact, id);
        if (engine.dead) break;
      }
    });
  }
} catch (e) { report.infrastructure_error = redact(e.stack ?? String(e)); }
finally {
  try { report.sources_after = await snapshotSources(); report.sources_unchanged = same(report.sources_before, report.sources_after); }
  catch (e) { report.source_check_error = redact(String(e)); }
  if (authBytes) report.original_auth_unchanged = hash(await readFile(authSource)) === hash(authBytes);
  report.finished_at = new Date().toISOString(); await checkpoint();
  const summaries = report.cases.map(c => `${c.id}: ${c.status}; ${JSON.stringify(c.grades ?? {})}`);
  const summary = [`Report: ${reportPath}`, ...summaries, 'Unresolved policy reviews are not passes.', ...(report.infrastructure_error ? [`Infrastructure: ${report.infrastructure_error}`] : [])].join('\n');
  await writeFile(join(output, 'SUMMARY.txt'), redact(summary) + '\n', { mode: 0o600 }); console.log(redact(summary));
}
process.exitCode = report.infrastructure_error || report.sources_unchanged !== true || report.cases.some(c => c.status !== 'completed' || Object.values(c.grades ?? {}).includes('fail')) ? 1 : report.cases.some(c => Object.values(c.grades ?? {}).includes('unresolved')) ? 2 : 0;
