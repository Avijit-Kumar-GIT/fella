// Opt-in real provider evaluation. Never builds artifacts or edits benchmark inputs.
// Freeze tasks/rubric BEFORE calls; a changed benchmark needs a new fixture version.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdtemp, chmod } from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const fixturePath = join(root, 'tests/fixtures/intelligent-ui-model-v1.json');
const nativePath = join(root, `backend/target/debug/examples/contract_accept${process.platform === 'win32' ? '.exe' : ''}`);
const manifestPath = join(root, 'backend/resources/presentation-catalog.json');
// Pinned after authoring, before the first run. Do not update after observing output.
const FROZEN_FIXTURE_SHA256 = 'ec6ea60fa0929657978cd9afa52014b815435e867bf7ecf49dfccfdb6b4579ff';
const sha256 = value => createHash('sha256').update(value).digest('hex');
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const milliseconds = started => Math.round(performance.now() - started);
const ensure = (condition, message) => { if (!condition) throw new Error(message); };
let secrets = [];
function redact(value) {
  let text = typeof value === 'string' ? value : JSON.stringify(value);
  for (const secret of secrets) text = text.replaceAll(secret, '[REDACTED]');
  return text;
}
const errorText = error => redact(error instanceof Error ? error.message : String(error));

function responseSchema(authorSchema) {
  function rebase(value) {
    if (Array.isArray(value)) return value.map(rebase);
    if (!object(value)) return value;
    return Object.fromEntries(Object.entries(value).map(([key, child]) => [key,
      key === '$ref' && typeof child === 'string' && child.startsWith('#')
        ? `#/properties/presentation_spec/anyOf/0${child.slice(1)}` : rebase(child)]));
  }
  const schema = { type: 'object', properties: {
    presentation_spec: { anyOf: [rebase(authorSchema), { type: 'null' }] }
  }, required: ['presentation_spec'], additionalProperties: false };
  function check(value) {
    if (!value || typeof value !== 'object') return;
    if (typeof value.$ref === 'string') {
      ensure(value.$ref.startsWith('#/'), 'Only local schema references are supported');
      const target = value.$ref.slice(2).split('/').reduce((node, key) =>
        node?.[key.replaceAll('~1', '/').replaceAll('~0', '~')], schema);
      ensure(target !== undefined, `Unresolved response schema reference: ${value.$ref}`);
    }
    Object.values(value).forEach(check);
  }
  check(schema);
  return schema;
}

function nativeAccept(spec, context) {
  const started = performance.now();
  const child = spawnSync(nativePath, [], {
    cwd: root, input: `${JSON.stringify({ spec, results: context.results, charts_enabled: true })}\n`,
    encoding: 'utf8', timeout: 30000, maxBuffer: 16 * 1024 * 1024, windowsHide: true
  });
  ensure(!child.error && child.status === 0,
    `Native CLI failed: ${child.error?.message ?? child.signal ?? child.status}; ${child.stderr ?? ''}`);
  const lines = child.stdout.trim().split(/\r?\n/);
  ensure(lines.length === 1, 'Native CLI must return one JSON line per request');
  const output = JSON.parse(lines[0]);
  ensure(output.contractHash === context.manifest.hash, 'Native CLI/manifest hash mismatch; parent must rebuild native artifact');
  ensure(typeof output.valid === 'boolean' && Array.isArray(output.issues), 'Malformed native acceptance response');
  ensure(output.valid ? object(output.spec?.elements) : output.spec === null && output.issues.length > 0,
    'Native acceptance response lacks canonical spec or rejection diagnostics');
  return { ...output, duration_ms: milliseconds(started) };
}

function accept(content, context) {
  // Same optional Markdown-fence removal and null handling as agent.rs.
  let text = content.trim().replace(/^```(?:json|JSON)?/, '').trim();
  if (text.endsWith('```')) text = text.slice(0, -3).trim();
  let envelope;
  try { envelope = JSON.parse(text); }
  catch (error) { return { valid: false, stage: 'syntax', errors: [
    { path: 'response', message: `Presentation response was not JSON: ${error.message}` }
  ] }; }
  if (!object(envelope) || !Object.hasOwn(envelope, 'presentation_spec')) return {
    valid: false, stage: 'envelope', envelope, errors: [
      { path: 'presentation_spec', message: 'Presentation response did not include presentation_spec.' }
    ]
  };
  if (envelope.presentation_spec === null) return { valid: true, stage: 'null', envelope, spec: null, errors: [] };
  const native = nativeAccept(envelope.presentation_spec, context);
  return { valid: native.valid, stage: 'native', envelope, spec: native.spec,
    errors: native.issues, native };
}

function messagesFor(task, context) {
  const marker = '\nAvailable result bindings (metadata only; the result rows themselves are provided separately as evidence):\n';
  const metadata = context.results.map(({ id, columns, row_count, complete }) => ({ id, columns, row_count, complete }));
  const prefix = context.manifest.prompt.split(marker)[0];
  const system = "You compose Fella's primary analytical presentation after the answer and analysis are complete. Return only a JSON object with one property, `presentation_spec`, matching the supplied response schema. You may return presentation_spec:null when the canonical answer already explains the result clearly without a separate interface. Do not change, repeat, or add claims to the canonical answer. Use the exact result IDs and field names below; charts and tables must bind to those saved results, never embed invented data. When the question asks for a trend across ordered periods, make a result-bound line or area chart the primary view; for category rankings use bars. For exact records use a table. Add concise interpretation and caveats without repeating values as unbound literals. Keep the view responsive and avoid duplicate content.\n\n";
  const samples = context.results.map(({ id, columns, row_count, complete, rows }) => ({
    id, columns, row_count, complete, sample_rows: rows.slice(0, 24)
  }));
  return [{ role: 'system', content: `${system}${prefix}${marker}${JSON.stringify(metadata)}` },
    { role: 'user', content: `Original user question:\n${task.question}\n\nCanonical answer, already completed and verified; preserve it as the authority:\n${task.answer}\n\nAccepted saved result bindings and exact row samples (charts render from the full saved result, not only this sample):\n${JSON.stringify(samples)}` }];
}

function retryMessages(messages, candidate, errors, schema) {
  return [...messages, ...(candidate === null ? [] : [{ role: 'assistant', content: candidate }]),
    { role: 'user', content: `The previous candidate above failed the shared acceptance contract: ${JSON.stringify(errors)}. Return a corrected object with \`presentation_spec\` (or null for a plain answer). The response schema is: ${JSON.stringify(schema)}. Use one of the exact result IDs and field names already supplied; remove unsupported properties. Preserve the canonical answer and analysis.` }];
}

function requestBody(provider, messages, mode, context) {
  const budget = context.fixture.protocol.output_tokens;
  if (provider.id === 'ollama-cloud') {
    const approx = Math.floor(Buffer.byteLength(JSON.stringify(messages)) / 4);
    const numCtx = Math.min(32768, Math.max(8192, Math.ceil((Math.floor(approx * 5 / 4) + budget) / 2048) * 2048));
    return { model: provider.model, messages, stream: true, keep_alive: '30m', think: false,
      options: { temperature: context.fixture.protocol.temperature, num_ctx: numCtx, num_predict: budget },
      format: mode === 'structured' ? context.schema : 'json' };
  }
  return { model: provider.model, messages, stream: true, stream_options: { include_usage: true },
    reasoning_effort: 'none', max_completion_tokens: budget,
    response_format: mode === 'structured'
      ? { type: 'json_schema', json_schema: { name: 'fella_presentation_answer', strict: false, schema: context.schema } }
      : { type: 'json_object' } };
}

function decodeResponse(raw, provider) {
  const trimmed = raw.trim();
  let frames;
  if (provider.id === 'ollama-cloud') {
    frames = trimmed.split(/\r?\n/).filter(Boolean).map(line => JSON.parse(line));
  } else if (trimmed.startsWith('data:') || trimmed.startsWith(':') || trimmed.startsWith('event:')) {
    frames = trimmed.split(/\r?\n\r?\n/).flatMap(event => {
      const data = event.split(/\r?\n/).filter(line => line.startsWith('data:')).map(line => line.slice(5).trim()).join('\n');
      return !data || data === '[DONE]' ? [] : [JSON.parse(data)];
    });
  } else frames = [JSON.parse(trimmed)];
  let content = '', tokens = null, finishReason = null;
  for (const frame of frames) {
    if (frame.error) throw new Error(`Provider error: ${JSON.stringify(frame.error)}`);
    if (provider.id === 'ollama-cloud') {
      content += frame.message?.content ?? '';
      if (frame.done) { tokens = { input: frame.prompt_eval_count ?? null, output: frame.eval_count ?? null,
        total: Number.isFinite(frame.prompt_eval_count) && Number.isFinite(frame.eval_count)
          ? frame.prompt_eval_count + frame.eval_count : null, raw: frame };
        finishReason = frame.done_reason ?? 'stop'; }
    } else {
      const choice = frame.choices?.[0];
      content += choice?.delta?.content ?? choice?.message?.content ?? '';
      if (choice?.delta?.refusal || choice?.message?.refusal) throw new Error('Provider refused the composition request');
      finishReason = choice?.finish_reason ?? finishReason;
      if (frame.usage) tokens = { input: frame.usage.prompt_tokens ?? null, output: frame.usage.completion_tokens ?? null,
        total: frame.usage.total_tokens ?? null, raw: frame.usage };
    }
  }
  ensure(frames.length > 0 && finishReason !== null, 'Provider stream ended without a completion marker');
  return { content, tokens, finish_reason: finishReason };
}

async function callProvider(provider, messages, mode, context) {
  const started = performance.now();
  const attempt = { mode, origin: 'live_provider', request: requestBody(provider, messages, mode, context),
    duration_ms: 0, tokens: null, candidate: null, provider_failure: null };
  try {
    const response = await fetch(provider.endpoint, {
      method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${context.credentials[provider.credential]}` },
      body: JSON.stringify(attempt.request), redirect: 'error', signal: AbortSignal.timeout(context.fixture.protocol.timeout_ms)
    });
    attempt.http_status = response.status;
    attempt.raw_response = await response.text();
    ensure(response.ok, `Provider HTTP ${response.status}: ${attempt.raw_response}`);
    const decoded = decodeResponse(attempt.raw_response, provider);
    attempt.candidate = decoded.content;
    attempt.tokens = decoded.tokens;
    attempt.finish_reason = decoded.finish_reason;
  } catch (error) { attempt.provider_failure = errorText(error); }
  attempt.duration_ms = milliseconds(started);
  // Native/setup failures are infrastructure errors, not grounds for a model retry.
  if (attempt.provider_failure === null) {
    try { attempt.acceptance = accept(attempt.candidate, context); }
    catch (error) { attempt.infrastructure_failure = errorText(error); }
  }
  return attempt;
}

function declarations(spec) { return spec.elements[spec.root].props.contract?.state ?? {}; }
function defaultState(spec) { return Object.fromEntries(Object.entries(declarations(spec)).map(([key, def]) => [key, def.default])); }
function visibleNodes(spec, state) {
  const nodes = [];
  function visit(id) {
    const node = spec.elements[id];
    const condition = node.visible;
    if (condition === false) return;
    if (object(condition)) {
      const key = condition.$state?.replace(/^\/control:/, '');
      if (state[key] !== condition.eq) return;
    }
    nodes.push({ id, ...node });
    const children = node.type === 'Tabs' ? node.children.slice(0, 1) : node.children;
    children.forEach(visit);
  }
  visit(spec.root);
  return nodes;
}
function savedRows(node, state, context) {
  const result = context.results.find(result => result.id === node.props.resultId);
  if (!result) return [];
  return result.rows.flatMap((row, index) => (node.props.filters ?? []).every(binding => {
    const value = state[binding.stateKey];
    return value === '$all' || result.exact_rows[index][result.columns.indexOf(binding.field)] === value;
  }) ? [{ index, values: Object.fromEntries(result.columns.map((field, i) => [field, row[i]])) }] : []);
}
function chartLeaves(spec, inherited = {}, facets = []) {
  if (spec.repeat) return []; // Explicit frozen limitation; never silently call it a pass.
  const encoding = { ...inherited, ...spec.encoding };
  const fields = value => object(value) ? typeof value.field === 'string' ? [value.field] : Object.values(value).flatMap(fields) : [];
  const nextFacets = [...facets, ...fields(spec.facet), ...fields(encoding.row), ...fields(encoding.column), ...fields(encoding.facet)];
  const branches = spec.layer ?? spec.concat ?? spec.hconcat ?? spec.vconcat ?? (spec.spec ? [spec.spec] : null);
  if (branches) return branches.flatMap(child => chartLeaves(child, encoding, nextFacets));
  return [{ mark: typeof spec.mark === 'string' ? spec.mark : spec.mark?.type, encoding, facets: nextFacets }];
}
const rawField = encoding => object(encoding) && typeof encoding.field === 'string' && !encoding.aggregate && !encoding.bin;
const positionalFields = leaf => ['x', 'y', 'xOffset', 'yOffset'].flatMap(key => rawField(leaf.encoding[key]) ? [leaf.encoding[key].field] : []).concat(leaf.facets);
function chartGrade(spec, context, kind) {
  const state = defaultState(spec), covered = new Set(), evidence = [];
  for (const node of visibleNodes(spec, state).filter(node => node.type === 'Chart' && node.props.resultId === 'sales')) {
    const rows = savedRows(node, state, context);
    for (const leaf of chartLeaves(node.props.spec)) {
      const { encoding: enc } = leaf;
      const measureKey = ['x', 'y'].find(key => rawField(enc[key]) && enc[key].field === 'revenue' && enc[key].type === 'quantitative');
      if (!measureKey || !rows.length) continue;
      const measure = enc[measureKey];
      let suitable = false;
      if (kind === 'temporal_trend') {
        const time = enc[measureKey === 'x' ? 'y' : 'x'];
        const series = ['color', 'fill', 'stroke', 'detail'].some(key =>
          (Array.isArray(enc[key]) ? enc[key] : [enc[key]]).some(value => rawField(value) && value.field === 'region')) || leaf.facets.includes('region');
        suitable = ['line', 'area'].includes(leaf.mark) && rawField(time) && time.field === 'month' && time.type === 'temporal'
          && (!time.timeUnit || ['month', 'yearmonth', 'yearmonthdate', 'date'].includes(time.timeUnit)) && series
          && !['normalize', 'center'].includes(measure.stack) && (leaf.mark !== 'area' || measure.stack === null || measure.stack === false || leaf.facets.includes('region'));
      } else {
        const positions = positionalFields(leaf);
        const distinguished = ['month', 'region'].every(field => positions.includes(field) || new Set(rows.map(row => row.values[field])).size === 1);
        const noBinnedPositions = ['x', 'y', 'xOffset', 'yOffset'].every(key => !enc[key]?.bin && !enc[key]?.aggregate && !enc[key]?.timeUnit);
        suitable = leaf.mark === 'bar' && distinguished && noBinnedPositions && measure.scale?.zero !== false
          && !['normalize', 'center'].includes(measure.stack);
      }
      if (suitable) { rows.forEach(row => covered.add(row.index)); evidence.push({ node: node.id, leaf, row_indices: rows.map(row => row.index) }); }
    }
  }
  return { pass: same([...covered].sort(), [0, 1, 2, 3]), evidence };
}

function filterGrade(spec, context) {
  const defaults = defaultState(spec), states = declarations(spec), evidence = [];
  for (const control of visibleNodes(spec, defaults).filter(node => ['Select', 'SegmentedControl'].includes(node.type))) {
    const key = control.props.stateKey, def = states[key];
    const options = def?.options?.map(option => option.value) ?? [];
    if (def?.type !== 'enum' || def.persistence !== 'viewer' || !options.includes(def.default)
      || !options.includes('North') || !options.includes('South') || options.some(value => !['North', 'South', '$all'].includes(value))) continue;
    for (const node of Object.entries(spec.elements).map(([id, node]) => ({ id, ...node }))) {
      if (!['Table', 'Chart'].includes(node.type) || node.props.resultId !== 'sales'
        || !node.props.filters?.some(filter => filter.stateKey === key && filter.field === 'region')) continue;
      const exposed = node.type === 'Table' ? node.props.fields ?? ['month', 'region', 'revenue']
        : chartLeaves(node.props.spec).flatMap(leaf => Object.values(leaf.encoding).flatMap(value =>
          (Array.isArray(value) ? value : [value]).map(value => value?.field))).concat(chartLeaves(node.props.spec).flatMap(leaf => leaf.facets));
      if (!['month', 'revenue'].every(field => exposed.includes(field))) continue;
      const rows = ['North', 'South'].map(value => {
        const state = { ...defaults, [key]: value };
        const visible = visibleNodes(spec, state).some(item => item.id === node.id);
        return visible ? savedRows(node, state, context).map(row => row.index) : [];
      });
      evidence.push({ control: control.id, consumer: node.id, state: key, North: rows[0], South: rows[1] });
      if (same(rows, [[0, 2], [1, 3]])) return { pass: true, evidence };
    }
  }
  return { pass: false, evidence };
}

function viewGrade(task, accepted, record, context) {
  const checks = [];
  for (const criterion of task.criteria) {
    if (!accepted?.valid) { checks.push({ criterion, pass: false, reason: 'Structural acceptance failed; suitability unestablished' }); continue; }
    const spec = accepted.spec;
    let result;
    if (criterion === 'author_envelope') {
      const author = accepted.envelope.presentation_spec;
      result = { pass: same(Object.keys(accepted.envelope), ['presentation_spec']) && (author === null ||
        author.version === 'fella-ui-1' && object(author.root) && author.root.type === 'Document') };
    } else if (criterion === 'bounded_repair') {
      result = { pass: record.seed_expected_failure === true && record.attempts.length === 1
        && record.attempts[0].mode === 'json' && record.attempts[0].provider_failure === null && spec !== null };
    } else if (criterion === 'concise_lookup') {
      const nodes = spec ? Object.values(spec.elements) : [];
      const prose = nodes.reduce((count, node) => count + String(node.props.content ?? '').length + String(node.props.detail ?? '').length, 0);
      result = { pass: spec === null || nodes.length <= context.fixture.rubric.limits.lookup_max_nodes
        && prose <= context.fixture.rubric.limits.lookup_max_prose_characters
        && nodes.every(node => ['Document', 'Metric', 'Text', 'Caption', 'Title', 'Section', 'Card'].includes(node.type))
        && nodes.filter(node => node.type === 'Metric').every(node => node.props.resultId === 'sales' && node.props.field === 'revenue' && node.props.row === 0),
        nodes: nodes.length, prose_characters: prose };
    } else if (spec === null) result = { pass: false, reason: 'This task requires a bound presentation' };
    else if (criterion === 'bound_lookup_metric') {
      const metrics = visibleNodes(spec, defaultState(spec)).filter(node => node.type === 'Metric');
      result = { pass: metrics.length > 0 && Object.values(spec.elements).filter(node => node.type === 'Metric').every(node =>
        node.props.resultId === 'sales' && node.props.field === 'revenue' && node.props.row === 0)
        && context.results.find(result => result.id === 'sales').exact_rows[0][2] === '100' };
    } else if (criterion === 'region_filter_consumer') result = filterGrade(spec, context);
    else if (['temporal_trend', 'within_month_bars'].includes(criterion)) result = chartGrade(spec, context, criterion);
    else throw new Error(`Unknown frozen rubric criterion ${criterion}`);
    checks.push({ criterion, ...result });
  }
  return { type: 'heuristic_only', narrative_accuracy: 'not_evaluated', pass: checks.every(check => check.pass), checks };
}

async function runTask(provider, task, context, record) {
  const messages = messagesFor(task, context);
  let previousCandidate = null, errors = null;
  if (task.kind === 'repair') {
    record.injected_seed = { origin: 'frozen_repair_challenge', candidate: task.failed_candidate,
      acceptance: accept(task.failed_candidate, context) };
    const seed = record.injected_seed.acceptance;
    record.seed_expected_failure = !seed.valid && (task.seed_error === 'syntax' ? seed.stage === 'syntax'
      : seed.stage === 'native' && /profit/.test(JSON.stringify(seed.errors)));
    ensure(record.seed_expected_failure, 'Frozen repair challenge did not fail its expected acceptance check');
    previousCandidate = task.failed_candidate;
    errors = seed.errors;
  } else {
    const first = await callProvider(provider, messages, 'structured', context);
    record.attempts.push(first);
    ensure(!first.infrastructure_failure, first.infrastructure_failure);
    if (first.acceptance?.valid) return;
    previousCandidate = first.candidate;
    errors = first.acceptance?.errors ?? [{ path: 'provider', message: first.provider_failure }];
  }
  record.attempts.push(await callProvider(provider, retryMessages(messages, previousCandidate, errors, context.schema), 'json', context));
  ensure(!record.attempts.at(-1).infrastructure_failure, record.attempts.at(-1).infrastructure_failure);
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes('--help')) {
    console.log('Usage: node tests/evals/intelligent-ui.live.mjs --live [--provider all|ollama-cloud|openai]\nWait for parent confirmation that the shared manifest and native CLI are final. Without --live: skip, no credentials read or provider calls. Full reports: /tmp only.');
    return;
  }
  if (!args.includes('--live')) {
    console.log(JSON.stringify({ status: 'skipped', reason: 'Live model evaluation requires --live; awaiting parent readiness.' }));
    return;
  }
  const providerIndex = args.indexOf('--provider');
  const selection = providerIndex < 0 ? 'all' : args[providerIndex + 1];
  ensure(['all', 'ollama-cloud', 'openai'].includes(selection), 'Unknown provider selection');
  const recognized = new Set(['--live', ...(providerIndex < 0 ? [] : ['--provider', selection])]);
  ensure(args.every(arg => recognized.has(arg)), 'Unknown argument');
  const directory = await mkdtemp(join(tmpdir(), 'fella-intelligent-ui-model-v1-'));
  await chmod(directory, 0o700);
  const report = { fixture_version: 'intelligent-ui-model-v1', started_at: new Date().toISOString(),
    status: 'running', tasks: [], exclusions: [], post_hoc_changes: [], report_directory: directory };
  const save = () => writeFile(join(directory, 'report.json'), redact(JSON.stringify(report, null, 2)) + '\n', { mode: 0o600 });
  try {
    const fixtureText = await readFile(fixturePath, 'utf8');
    ensure(sha256(fixtureText) === FROZEN_FIXTURE_SHA256, 'Frozen task/rubric fixture changed; use a new version instead of changing expectations after a run');
    const fixture = JSON.parse(fixtureText);
    const sourceText = await readFile(join(root, fixture.results_fixture), 'utf8');
    const source = JSON.parse(sourceText);
    ensure(source.fixture_version === fixture.results_version, 'Unexpected frozen result fixture version');
    // Freeze exact inputs to a private /tmp artifact before any candidate request.
    const manifestText = await readFile(manifestPath, 'utf8'), manifest = JSON.parse(manifestText);
    ensure(object(manifest.authorSchema) && typeof manifest.prompt === 'string' && typeof manifest.hash === 'string', 'Final generated manifest is missing author schema, prompt or hash');
    const context = { fixture, results: source.results, manifest, schema: responseSchema(manifest.authorSchema) };
    report.inputs = { fixture_sha256: sha256(fixtureText), results_sha256: sha256(JSON.stringify(source.results)),
      source_fixture_sha256: sha256(sourceText), manifest_sha256: sha256(manifestText), contract_hash: manifest.hash,
      native_sha256: sha256(await readFile(nativePath)), agent_sha256: sha256(await readFile(join(root, 'backend/src/engine/agent.rs'))),
      runner_sha256: sha256(await readFile(fileURLToPath(import.meta.url))) };
    report.limitations = fixture.rubric.limitations;
    await writeFile(join(directory, 'frozen-inputs.json'), JSON.stringify({ ...report.inputs, fixture,
      results: source.results, manifest, response_schema: context.schema }, null, 2) + '\n', { mode: 0o400 });
    const sales = source.results.find(result => result.id === 'sales');
    ensure(same(sales?.columns, ['month', 'region', 'revenue']) && same(sales.exact_rows,
      [['2026-01-01', 'North', '100'], ['2026-01-01', 'South', '80'], ['2026-02-01', 'North', '120'], ['2026-02-01', 'South', '90']]), 'Frozen saved rows differ from the predeclared rubric');
    ensure(fixture.protocol.max_retries === 1, 'Only one retry is authorized');
    report.preflight = nativeAccept(source.cases.find(item => item.id === 'simple_lookup_metric').input, context);
    ensure(report.preflight.valid, 'Native CLI rejected the existing frozen lookup fixture');
    let credentials = {};
    try { credentials = JSON.parse(await readFile(join(homedir(), '.local/share/dev.fella.app/auth.json'), 'utf8')); }
    catch { report.credentials_error = 'Credential store unavailable or invalid; provider tasks will fail without calls'; }
    secrets = Object.values(credentials).filter(value => typeof value === 'string' && value.length > 0);
    context.credentials = credentials;
    await save();
    for (const provider of fixture.providers.filter(provider => selection === 'all' || provider.id === selection)) {
      for (const task of fixture.tasks) {
        const started = performance.now();
        const record = { provider: provider.id, model: provider.model, task: task.id, kind: task.kind, attempts: [] };
        report.tasks.push(record);
        try {
          ensure(typeof credentials[provider.credential] === 'string' && credentials[provider.credential].length > 0, 'Required provider credential is unavailable');
          await runTask(provider, task, context, record);
        } catch (error) { record.failure = errorText(error); }
        const first = record.attempts[0]?.acceptance, final = record.attempts.at(-1)?.acceptance;
        record.structural_acceptance = { first: first?.valid ?? false, final: final?.valid ?? false,
          null_presentation: final?.valid === true && final.spec === null };
        record.view_suitability = viewGrade(task, final, record, context);
        record.provider_failures = record.attempts.filter(attempt => attempt.provider_failure !== null).length;
        record.pass = !record.failure && record.provider_failures === 0 && final?.valid === true && record.view_suitability.pass;
        record.duration_ms = milliseconds(started);
        record.tokens = { input: 0, output: 0, total: 0, missing_usage_calls: 0 };
        for (const attempt of record.attempts) {
          if (!attempt.tokens) record.tokens.missing_usage_calls++;
          for (const key of ['input', 'output', 'total']) record.tokens[key] += attempt.tokens?.[key] ?? 0;
        }
        await save();
        console.log(JSON.stringify({ provider: record.provider, model: record.model, task: record.task, pass: record.pass,
          structural: record.structural_acceptance, suitable: record.view_suitability.pass, provider_failures: record.provider_failures,
          duration_ms: record.duration_ms, tokens: record.tokens }));
      }
    }
    ensure(sha256(await readFile(fixturePath)) === report.inputs.fixture_sha256 && sha256(await readFile(manifestPath)) === report.inputs.manifest_sha256
      && sha256(await readFile(nativePath)) === report.inputs.native_sha256 && sha256(await readFile(join(root, fixture.results_fixture))) === report.inputs.source_fixture_sha256,
      'Evaluation inputs changed during the run; results are invalidated');
    report.status = report.tasks.every(task => task.pass) ? 'passed' : 'failed';
  } catch (error) { report.status = 'failed'; report.setup_or_integrity_failure = errorText(error); }
  report.finished_at = new Date().toISOString();
  await save();
  console.log(JSON.stringify({ status: report.status, fixture: report.fixture_version,
    passed: report.tasks.filter(task => task.pass).length, failed: report.tasks.filter(task => !task.pass).length,
    total: report.tasks.length, setup_or_integrity_failure: !!report.setup_or_integrity_failure,
    narrative_accuracy: 'not_evaluated', report: join(directory, 'report.json') }));
  if (report.status !== 'passed') process.exitCode = 1;
}

main().catch(() => { console.log(JSON.stringify({ status: 'failed', reason: 'Runner setup failed' })); process.exitCode = 1; });
