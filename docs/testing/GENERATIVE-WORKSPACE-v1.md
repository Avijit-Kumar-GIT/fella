# Generative analytical workspace: acceptance contract v1

Defined 2026-10-21 before candidate implementation. Baseline: `eb60f75` on
`explore/composable-interface`. Scope follows
[the implementation plan](../OPENUI-ANALYTICAL-WORKSPACE-PLAN.md).

The frozen persistence/export/desktop contract below remains v1. Its executable
task set is now v1.2; the v1.1 results below remain historical and unchanged.

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
