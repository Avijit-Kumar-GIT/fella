# Generative analytical workspace: acceptance contract v1

Defined 2026-10-21 before candidate implementation. Baseline: `eb60f75` on
`explore/composable-interface`. Scope follows
[the implementation plan](../OPENUI-ANALYTICAL-WORKSPACE-PLAN.md).

The frozen persistence/export/desktop contract below remains v1. Its executable
task set is now v1.10; earlier v1.1–v1.9 results below remain historical and
unchanged.

## Frozen expected behavior

1. Existing analytical runs remain canonical and immutable. Artifact creation
   accepts a saved run in the explicitly named workspace; another workspace's
   run, excluded evidence, invented references, and incomplete-result claims
   are rejected. Stored result values are never supplied by the renderer.
2. Presentation syntax uses OpenUI's actual parser. Unknown components,
   oversized programs, unsafe visualization sources/expressions, and invalid
   references cannot acquire engine, shell, filesystem, or network authority.
   A failure preserves the original answer and exact results.
3. The model may choose different layouts, field mappings, marks, facets,
   layers, and annotations. New presentations do not require a new ChartKind
   per visualization. Legacy charts continue to open.
4. Artifacts have stable workspace identity and immutable revisions. A stale
   base revision cannot overwrite newer adopted work. A display-only revision
   reuses results; an analytical revision points to a new saved run. Original
   questions, traces, definitions, and checks stay unchanged.
5. Studio documents pin revisions, autosave user narrative/order, preserve
   selections, support keyboard controls, and reopen without model calls or
   live source queries. Rapid navigation cannot change an action's target.
6. Chat deletion cannot silently delete or orphan saved analytical work. The
   explicit retention choice is documented before changing deletion semantics.
7. Comparisons use saved values with compatible fields, keys, units, and
   completeness; zero is not null. Incompatible results explain why no delta
   is shown. Adopting/restoring revisions never erases later history.
8. Markdown export preserves exact canonical values, units, caveats, evidence,
   selected display state, and component reading order. Unknown content is
   disclosed, not silently omitted. Export needs no model/source query.
9. Reopenable packages validate version, paths, payload sizes, references,
   and contents. No imported program executes arbitrary code or remote loads.
10. General chat, ordinary analysis, saved Question runs, clarification,
    original-request fidelity, Tools tabs, and compact layouts retain their
    existing behavior. Presentation failures do not downgrade answer coverage.

## Presentation-generation acceptance addition — task set v1.2

Defined before validating the new presentation-generation path. This adds
criteria; it does not revise v1/v1.1 assertions or claim a live model result.

11. An explicit design pass receives the saved analytical question, contract,
    answer/verification status, and bounded profiles of saved result rows.
    Profiles describe field roles, missingness, cardinality, numeric shape,
    temporal ordering, completeness, and a small representative sample. They
    are labeled as design hints, not new evidence. Identifier, high-cardinality
    text, and long-text cell values are omitted from the representative sample.
12. The design decision is free-form; it is not a closed chart-family enum or a
    required KPI/bar/table template. The composition pass receives the decision
    and can choose prose, exact values, tables, declarative charts, or useful
    combinations from the registered OpenUI components. A chart/control is
    optional. OpenUI parsing, saved-result binding, and no-execution checks stay
    mandatory. Invalid-program repair reuses the original design decision.
13. Visual variety is not itself the quality metric. Provider evaluation must
    judge appropriateness against the task and available data: scalar, direct
    two-period comparison, time trend, ranking, distribution, and exact-record
    retrieval should not all be forced into one generic composition. Repeating
    a suitable design for equivalent inputs is acceptable; novelty without
    analytical fit is not.

The deterministic v1.2 checks cover prompt-template removal, plan parsing and
bounds, profile behavior/privacy, OpenUI safety/binding, and the existing
desktop persistence contract. They do **not** prove that a live provider
follows the plan or produces different appropriate views. That claim requires
an opt-in provider evaluation with a frozen, varied task set, blinded human
appropriateness grading, first-attempt validity, exact-value binding, latency,
and token-cost reporting. No quality score is inferred from unit tests.

## Primary-answer contract — task set v1.3

Defined 2026-10-21 before validating the candidate that makes OpenUI the
default workspace answer. This supersedes the two-pass design-plan/composition
architecture described in v1.2; those v1.2 test results remain historical.

14. A completed workspace answer is returned in the analysis turn as one
    `fella_openui_answer` containing both the canonical verified text and the
    OpenUI Lang program. The program is the primary visible answer, not a
    later model pass, optional preference, or user-triggered enhancement.
15. The transcript does not show the canonical prose answer or legacy
    `make_chart` output before the OpenUI presentation. The canonical answer is
    retained in the run for verification, history, and a secondary details
    disclosure. If OpenUI is missing, rejected, or unavailable, show that
    canonical answer as an explicit fallback.
16. On answer completion, validate the supplied program against the saved
    evidence, create or revise the persistent artifact automatically, and
    reopen it without a model call. There is no “Create interactive view”
    action and no separate `presentation_propose` model call.
17. A workspace answer with no reusable table still receives a substantive
    text-first OpenUI document; presentation output is not conditional on a
    chartable result. General, non-workspace chat remains ordinary conversation.
18. Presentation validation and rendering cannot execute analysis or invent
    values. Existing canonical analysis, verification, provenance, and legacy
    records remain readable; legacy chart records are not the active answer
    surface for new workspace turns.

The v1.3 deterministic checks can establish the same-turn output contract,
automatic renderer routing, validation, persistence semantics, and removal of
the second-pass IPC surface. They do **not** establish live-provider adherence,
answer-specific visual quality, or that views differ appropriately across
questions. Those require a separately frozen provider evaluation and blinded
grading; no such quality claim is inferred from these tests.

## Presentation-language validation — task set v1.4

Defined 2026-10-22 after a real-model OpenUI run exposed the mismatch between
OpenUI's expression AST and Fella's static-prop validator. Literal-only numeric
arithmetic and string concatenation are folded deterministically before prop
schema validation. They do not read results or execute application code.
Variables, state, runtime references, member/index access, builtins, queries,
mutations, actions, and non-constant expressions remain unsupported. Existing
result/field binding, chart safety, exact-value display, and fallback behavior
remain required. The task set adds positive literal-expression coverage while
retaining negative cases for runtime expressions and builtins.

Validation record: the deterministic analytical-presentation suite passed 51
checks, and the real Electron/OpenAI acceptance test passed twice on a synthetic
eight-row workspace. Both runs persisted an automatically generated answer;
the final run also exercised the normalized renderer. An earlier real-model
characterization returned an invalid facet shape (`columns` nested inside
`facet`) and was rejected; the model-facing instructions were clarified and
the test was rerun against a rebuilt bundle. These are operational acceptances,
not a quality or diversity score. Generated answers were not graded for
analytical accuracy.

## OpenUI static-expression compatibility — task set v1.5

Defined 2026-10-22 after the user reported a generated answer rejected by the
presentation validator and pointed to OpenUI Lang's Svelte integration. The
validator now accepts local named references and OpenUI expression forms only
when they resolve entirely from static values declared in the same program.
Constant arithmetic, string operations, conditionals, comparisons, and safe
member/index access are folded into ordinary literal props before the Svelte
renderer sees them. This follows OpenUI's composition syntax without allowing
runtime state, query/mutation results, built-in execution, or action plans.

Existing binding, schema, chart-source, artifact, and output-safety assertions
remain in force. The task set adds forward-reference and expression fixtures,
while keeping explicit rejections for unresolved values, component-as-data,
reactive state, and executable built-ins. No prior expected result is weakened.

The opt-in real-model suite covers `openai/gpt-5.6-luna` and
`ollama-cloud/gemma4:31b`. The Gemma run exercises Fella's configured Ollama
Cloud provider, not a local inference server. Both latest Electron acceptance
runs passed (OpenAI: 11.2s; Gemma: 15.3s) on the synthetic workspace. Each
generated answer validated, rendered as the primary response, and persisted
automatically. Gemma's output used reusable named OpenUI component references.
These runs establish compatibility for these prompts/providers, not analytical
accuracy or general model quality.

## Question-led visual choice — task set v1.6

Defined 2026-10-22 to test the product requirement that OpenUI selects a useful
presentation from the question and evidence, rather than waiting for the user
to request a chart or prescribing a chart family. The live-model task now asks
for a comparison of regional quarter-by-quarter revenue and order volume but
does not ask for a visualization. Acceptance requires a valid, persisted
same-turn OpenUI answer and a chart component; it does not prescribe the chart
mark, layout, or composition. The deterministic prompt contract also checks
for positive guidance to visualize meaningful relationships without adding
ornamental or redundant content. Existing safety and provenance assertions are
unchanged. This is a newly versioned task set, not a post-hoc adjustment of the
v1.5 result.

The first v1.6 live-model attempt passed on Gemma but failed on OpenAI: its
program selected a suitable line chart, then referenced an undeclared
`trend_section`, and Fella correctly rejected the unresolved reference. This
failure is retained here; the task's criteria were not weakened.

## OpenUI reference completeness — task set v1.7

Defined 2026-10-22 after the v1.6 OpenAI run exposed a missing named statement.
The Fella prompt now explicitly requires every component/value reference to
resolve exactly once and every non-root statement to be reachable from root.
The OpenUI validator continues to reject missing, duplicate, and unused
statements. The question, chart criterion, and other v1.6 expectations are
unchanged. The initial v1.6 failure remains separately recorded; v1.7 reruns
the same task after this prompt-only change.

The v1.7 live rerun passed on both OpenAI and Ollama Cloud Gemma 4 31B (2/2,
31.2 seconds total). Both selected and rendered a line-chart view without an
explicit visualization request, and both answers were persisted as artifacts.
Their compositions differed; the test does not grade analytical accuracy,
visual quality, or whether a line chart is always the best choice.

Scope limitation: this proves question-led selection within Fella's current
registered components. It does not prove an unrestricted UI vocabulary: Fella
still has a finite native component registry and applies a safety-constrained
Vega-Lite subset that binds chart fields to saved results and excludes
arbitrary code, remote resources, and unrecorded data transforms. Broadening
that declarative vocabulary without weakening provenance is a separate design
and implementation gap.

## OpenUI grammar compatibility — task set v1.8

Defined 2026-10-22 before changing the validator. OpenUI accepts grouping
parentheses, multiline expressions, line comments, and fenced model output; the
validator must apply the same preprocessing and expression grammar before
enforcing Fella's static-only boundary. Grouping delimiters must not be
mistaken for unparsed AST tokens. Valid literals, arrays, objects, local
references, component calls, member/index access, unary/binary operators,
ternaries, and legal trailing commas must validate.

The complete expression must be consumed. Inputs the upstream recovery parser
silently skips or partially parses—unknown source characters, malformed
operators, missing separators/colons, and trailing tokens—must not be accepted
as a different presentation. Static evaluation must match OpenUI's primitive
operator behavior and short-circuit/conditional branch selection, within
Fella's finite-number and safe-object limits. Runtime state, query/mutation
references, builtins/actions, unknown components, unsafe keys, invalid result
bindings, and executable behavior remain rejected.

This adds compatibility and rejection fixtures; no existing expected result is
weakened. Live Gemma and OpenAI runs remain separate provider checks and cannot
by themselves prove grammar coverage or analytical quality.

## OpenUI static coercion parity — task set v1.9

Defined 2026-10-22 after a source-level comparison found that Fella's static
evaluator disagreed with OpenUI for loose equality and native string coercion
of literal arrays/objects. Static expressions must preserve OpenUI's `==`, `!=`,
and `+` behavior for values the model can construct from literals. This is a
presentation-semantics check; it does not grant builtins, actions, queries, or
runtime state access. The task set adds positive coercion cases and confirms
that actual `@`-prefixed OpenUI builtins remain rejected by Fella's policy.

## Final-answer syntax correction — task set v1.10

Defined 2026-10-22 after a live model returned a chart call with an unclosed
props object. Before accepting the finalizer tool call, the engine must reject
mismatched/unclosed delimiters outside strings and line comments. Its existing
finalizer-only correction turn must receive that syntax error, allowing the
model to repair the presentation without rerunning analysis. Delimiter-like
characters in strings and comments remain ordinary text. This check is an
early syntax guard, not a replacement for the full OpenUI parser/schema
validation in the renderer.

## Test layers and independent grading

- Rust tests: storage, canonical binding, scope, excluded/partial evidence,
  revisions, retention, document conflicts, comparisons, import, and rollback.
- JS/Svelte tests: real OpenUI parsing, bindings, visual-spec safety, exact
  numeric serialization, fallback, Markdown conversion, and state validation.
- Real Electron plus Rust: rich answer to Studio, edits, tab changes, restart,
  deterministic export, and chat deletion/retention, using disposable profiles.
- Provider evaluation: distinct opt-in stage. Freeze task versions and usage
  budget first; record first-attempt validity, repair/fallback rate, analytical
  correctness, task/data fit, unnecessary-template rate, presentation
  appropriateness, latency, and token cost separately.
  Deterministic fixtures do not establish live model compatibility.
- Visual review: screenshots in ignored test-results, dark/light, compact and
  expanded views, keyboard, exact-values access, no nested-card dashboard chrome.

Do not modify existing assertions to fit observed output. Keep failed runs,
classify product/setup/grader failures, and obtain approval for expectation
corrections. At most two repeats of one unchanged setup/hypothesis.

## Baseline observations

- `pnpm check`: 0 errors / 0 warnings.
- `cargo test --manifest-path backend/Cargo.toml --lib investigation --locked`:
  35 passed, 351 filtered (focused evidence, not a full Rust suite).
- `pnpm build`: passed; Vite reports 14.05s total, 5.58s client. Largest client
  page JS: 1,146.53 kB raw / 313.89 kB gzip. Existing large-chunk warning.
- No startup, model, packaged-platform, or memory measurement is claimed yet.

Preserve initial-load splitting: new presentation, Studio, and visualization
engines must be lazy-loaded. Report raw/gzip chunks and relative startup/memory
measurements before claiming a performance improvement. Reopen, reorder,
promotion, saved-result comparisons, and exports have zero-model-call and
zero-live-source-query behavioral budgets.

## Delivery ledger

Each slice records its commit, checks, failures, and exclusions here or in a
linked follow-up. This document specifies behavior; it is not proof of delivery.

- Slice 0: acceptance contract and pre-implementation baseline recorded at
  `7b4867b`.
- Slice 1 (persistence): stable artifacts, immutable candidate revisions,
  optimistic adoption, canonical result bindings, Studio documents/checkpoints,
  and user-approved saved-work retention. `cargo test --manifest-path
  backend/Cargo.toml --lib artifacts --locked`: 17 passed. The backend worker
  additionally ran the eight existing analysis-store tests successfully.
  No existing expectations changed. No UI/model/platform claim is made by
  this library slice. A small rusqlite metadata store reuses the bundled
  SQLite dependency to atomically commit document pins and retention markers;
  canonical runs and result payloads remain in their existing store. Commit:
  `4248c86`.

### Slice 2 — initial local end-to-end vertical slice

This is a bounded implementation across the first OpenUI answer, Studio, and
portable-report path; it does not complete the entire plan. The implementation
preserves the existing Rust analytical engine. Presentation generation uses
the existing provider transport as a separate bounded operation, receives
authorized saved-result context, and cannot invoke analysis tools. Vega-Lite
adds a broader declarative rendering surface, but generated JavaScript/custom
code remains unsupported. Commits: `271bf48` (OpenUI runtime/library),
`5c2fc39` (artifact and Studio integration), and `76cd3ff` (Electron/Rust E2E).

- OpenUI Svelte integration uses pinned package versions and Fella-owned
  components, schema/prompt generation, parser and result-binding validation,
  safe Vega-Lite binding/rendering, and readable fallback. Legacy charts remain
  supported. Presentation/Studio code is lazy-loaded.
- Saved artifacts and immutable revisions refer to canonical runs/results;
  Studio documents pin revisions and support ordering, revision-aware writes,
  checkpoints, restoration, and workspace-scoped views. Conversation deletion
  discloses and retains saved Studio outputs and their supporting analysis.
- Markdown generation is deterministic and uses saved exact values/evidence.
  Versioned portable JSON report packages validate input and reopen read-only;
  imported programs are not executed.

Validation evidence for the implementation candidate:

- `cargo test --manifest-path backend/Cargo.toml --lib --locked`: 417 passed,
  0 failed. `cargo check --manifest-path backend/Cargo.toml --locked` passed.
- `pnpm test:analytical-presentation`: contract v1, task set v1.1 — 46 passed,
  0 failed. The original v1 run was 45/46; its lone failure came from two test
  fixtures calling OpenUI's reserved `Filter` built-in where Fella's
  result-bound component is `ResultFilter`. On 2026-10-21 the user approved
  changing only those fixture calls and versioning the task set. Assertions and
  behavior under test were unchanged. The v1.1 result is post-correction, not
  an untouched v1 baseline.
- `pnpm test:studio-report`: 3 test groups passed, covering exact values/state,
  invalid/mismatched content disclosure, deterministic output, and no source or
  model access.
- Real Electron plus Rust E2E, executable task set v1.1:
  `pnpm test:e2e:generative` — 7 passed, 0 failed. It covers immutable and
  workspace-owned artifacts, optimistic writes/checkpoints, saved-work
  retention on conversation deletion, invalid presentation fallback/no
  authority, Studio edits/restart, Markdown export, and read-only package
  reopen. A stale-write journey initially hung because `?` in the Rust IPC
  dispatcher exited before serializing an error response; the dispatcher was
  fixed to return command errors through the normal response path, and the
  focused stale-write journey then passed.
- The first E2E attempt was invalid as product evidence: its observer used a
  dynamic import from Playwright's Electron evaluation context, which lacks an
  import callback. After replacing that test harness path, the v1 run had 4/7
  passing. The delete test looked for retention text in an icon tooltip instead
  of the visible confirmation dialog, and the presentation fixture used the
  reserved OpenUI `Filter` built-in. The user approved the correction on
  2026-10-21; v1.1 asserts the dialog before confirming deletion and uses
  `ResultFilter`, preserving the intended checks. This is a documented
  post-hoc task-set correction, not an untouched baseline pass.
- `pnpm check`: 0 errors, 0 warnings. `pnpm electron:build` passed. Vite still
  emits its large-chunk warning: the main page bundle is about 820.86 kB raw /
  217.36 kB gzip; separate lazy Vega-Lite and Vega chunks are about
  253.56/81.49 kB and 486.25/172.23 kB respectively. These are bundle sizes,
  not startup, latency, or memory measurements.
- Linux Electron loading of `fella://app/` was exercised by the real desktop
  test. A user-reported Windows `ERR_FAILED (-2)` startup was not reproduced
  or verified on Windows; cross-platform packaged startup remains untested.

### Slice 3 — data-aware presentation design (task set v1.2)

The repeated-template failure was in the generation context, not the OpenUI
renderer: the composer had been shown the same quarterly-revenue metric/bar/
table example and only three leading result rows. The design pass now receives
the saved question, analysis contract, answer/verification status, result
metadata, and deterministic bounded profiles. Profiles sample at most 1,024
saved rows, 32 fields per result, and 16 result tables, disclose omissions,
summarize null/cardinality/numeric shape/temporal ordering, and include up to
eight evenly spaced representative rows. Identifier, high-cardinality, and
long-text values are redacted from those rows. Profile summaries are marked as
design hints, never analytical evidence.

The model first selects a free-form visual thesis/form—without a chart-family
enum—then composes an OpenUI program from registered components and the saved
bindings. The quarterly-revenue example has been removed from the model-facing
prompt. A validation repair carries the same plan forward. Rust still enforces
the existing component, result-binding, and visualization-safety checks. The
saved analytical run is not rerun. Diagnostics separate planner/composer token
usage and latency. This adds a model call in the normal case, so token and
latency costs are expected to rise; no cost or quality improvement is claimed.

Validation: the analytical presentation contract v1.2 passed 47/47; Studio
report checks passed; `cargo test --manifest-path backend/Cargo.toml --lib
--locked` passed 425/425; `pnpm check` reported zero errors/warnings; and
`pnpm electron:build` passed (existing large-chunk warning remains). The
generative Electron suite first failed before assertions because sandboxed
Electron could not create its Linux sandbox host (`Operation not permitted`).
It was rerun outside the sandbox without changing expectations and passed
7/7. No live provider was called: actual view appropriateness, cross-question
variation, first-attempt validity, and added token/latency costs remain to be
evaluated against the v1.2 rubric above.

No hosted/live model evaluation was run. The deterministic fixtures do not
establish model quality, first-attempt validity, token cost, or provider
compatibility. Startup time, answer latency, memory, macOS/Windows packaging,
visual accessibility review, and production-scale result storage remain
unmeasured. The code and tests intentionally do not claim the entire product
plan is complete.

### Slice 4 — OpenUI as the automatic workspace answer (task set v1.3)

Implementation commit: `072043e`.

The v1.2 two-pass designer/composer is historical and is superseded by the
same-turn `fella_openui_answer` contract. New workspace turns return a
canonical answer plus an OpenUI program together; the transcript renders only
the OpenUI answer as its primary surface and automatically persists it as an
artifact. The prose answer remains for verification and a secondary details
disclosure, with an explicit fallback if OpenUI cannot be validated or shown.
The standalone “Create interactive view” action, rich-answer preference,
separate presentation IPC/model pass, and active chat `make_chart` presentation
path are removed. Old chart evidence and records remain readable for
compatibility; this is not a deletion of analytical history.

Validation completed on 2026-10-21:

- `pnpm test:analytical-presentation`: 49 passed, 0 failed. This includes the
  same-turn primary-answer contract, transcript suppression of canonical prose,
  automatic artifact creation, OpenUI parsing/binding/safety, exact-value
  rendering/export, and fallback disclosure.
- `pnpm test:studio-report`: 3 groups passed; deterministic saved-value export,
  mismatch disclosure, and no recomputation.
- `cargo test --manifest-path backend/Cargo.toml --lib --locked`: 419 passed,
  0 failed.
- `pnpm check`: 0 errors, 0 warnings.
- `pnpm electron:build`: passed. Vite retains the existing large-chunk warning;
  the largest app chunk is 815.02 kB raw / 215.64 kB gzip. No startup or memory
  improvement is claimed.
- `pnpm test:e2e:generative`: 8 passed, 0 failed against the freshly rebuilt
  Electron/Rust app. The new local mock-provider journey inspects the workspace,
  returns `fella_openui_answer` in the same analysis turn, verifies OpenUI is
  the only primary transcript surface, and checks automatic artifact persistence
  with exactly one `ask` and no `presentation_propose` call. It also asserts the
  active system prompt no longer directs the model to `make_chart`.
- The first mock-provider E2E run used a stale local Rust sidecar built before
  the finalizer code. The captured tool exchange showed that binary treating
  `fella_openui_answer` as an unknown tool. After rebuilding with
  `pnpm electron:build`, the targeted journey and full suite passed. This was a
  stale build-artifact issue; no test expectation was changed.
- The first E2E launch inside the restricted runner failed because Electron's
  Linux sandbox host was denied (`Operation not permitted`). The suite was run
  outside that restriction; no test was excluded and no assertion was altered.

No live provider was used. These checks prove the integration and persistence
path against a deterministic mock, not provider compliance, view diversity,
answer-specific design quality, or model latency/token cost. Legacy chart data
and renderers remain available for previously saved analytical history; new
workspace answers do not expose that chart-generation path.
