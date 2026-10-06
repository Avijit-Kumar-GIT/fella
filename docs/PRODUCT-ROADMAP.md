# Fella Product Roadmap

This is the product backlog for making Fella a capable, model-driven analytics
companion rather than an app that only answers questions when they fit one
local SQL path. It turns the current product direction into work that can be
implemented, evaluated, and accepted. It complements the [Analytical Computer
Roadmap](ANALYTICAL-COMPUTER-ROADMAP.md), which tracks the deeper Rust runtime
and workspace-model work.

This document is a plan, not a claim that these capabilities already exist.
The items are deliberately phrased around user-visible behavior and evidence
of completion. Keep the existing app and its useful working paths; change the
parts that are limiting capability, and avoid adding rules for individual test
examples.

## Product contract

Fella's Ask surface should be able to choose the appropriate way to help:

1. **Use model knowledge** for stable, general questions that do not need
   external sources or the user's files. “What is Rust?” should work without a
   mounted folder.
2. **Research the web** when the user asks for research, current information,
   or sourced claims. A general question may use research whether or not it is
   about the mounted files. “How are steps counted on Apple Health?” should
   receive a useful explanation; when current product details matter, it
   should also be possible to research and cite sources.
3. **Analyze workspace data** when the answer depends on mounted files. The
   model should inspect the available data, decide what it needs to learn,
   decompose the request when helpful, clarify material ambiguity, then run
   suitable read-only analysis.
4. **Combine routes** when a question genuinely needs both external knowledge
   and local analysis. The answer should make clear which conclusions came
   from the files, which came from external sources, and which are inference.

The harness should guide the model toward careful analysis, not prevent a
plausible useful answer merely because it cannot be labeled “verified” by one
specific execution path. Preserve the read-only workspace boundary. Do not
send file contents, rows, or attachments to a web service as an implicit
consequence of asking a question. Decide and document the exact web-query
privacy/default policy before enabling web access; make any outbound research
visible to the user.

## What the current implementation tells us

The repo already has substantial pieces to build on: a shared Rust runtime,
local SQL/Python/text tools, a workspace analytical model, conversation state,
chart rendering, and verification. The problem is partly how these pieces are
currently composed and described:

- The no-workspace branch in `src-tauri/src/engine/agent.rs` supplies no tools
  and currently directs the model toward asking the user to open a folder for
  data/file requests, while its fallback wording narrows normal answers.
- The same prompt includes a broad instruction to decline forecasts and future
  questions. This rules out useful analysis before the model can inspect the
  data or explain the limits of a forecast.
- `src-tauri/src/engine/tools.rs` offers local workspace tools but no web
  search or page-reading tool. The current local search is lexical, which is
  useful for locating text but is not by itself semantic interpretation.
- Chart creation is currently SQL-centered and supports a small set of chart
  forms. That blocks charts whose natural result comes from Python or a
  forecast, even when the data and computation are available.
- `src-tauri/src/engine/verify.rs` treats replayable computation as the
  strongest route to `Verified`. Useful answers grounded in prose, general
  knowledge, or web references do not have an equivalent provenance path, so
  the status can describe the tool route rather than the quality of the
  answer.
- Existing roadmap work remains relevant: user-authored definitions/value
  semantics, broader chart invariants, a richer trace inspector, and
  cost/quality dashboards are still tracked in the [runtime roadmap](ANALYTICAL-COMPUTER-ROADMAP.md).

Treat these as starting observations to re-check before each implementation
slice. Do not preserve a restriction just because a test currently expects
it, and do not delete a useful safety check without replacing its actual
purpose.

## Ordered backlog

### 0. Establish the product rules and baseline

- [ ] **Write the Ask routing and privacy decision.** Specify when Fella uses
  model knowledge, web research, workspace tools, or a combination; whether
  web access is off by default, enabled by an explicit user setting, or
  activated only by a clear request; what query text may leave the device;
  how the user can see and stop an outbound request; and how credentials are
  stored. The policy must state that workspace file contents and attachments
  are never silently uploaded to web search.
- [ ] **Reconcile the product documents.** Update `VISION.md`, `PRINCIPLES.md`,
  `NON-GOALS.md`, and `ARCHITECTURE.md` so they agree that Ask can handle
  general knowledge, web research, local analytics, and justified hybrids,
  while Fella remains opinionated about careful analytics and read-only data
  work. Remove claims that all questions must concern mounted files or that
  web access is categorically out of scope if those no longer express the
  product.
- [ ] **Capture a before-change baseline.** Run the same pinned model/provider,
  prompts, settings, and task set against the current branch. Save question,
  route/tool trace, response, latency, token/cost data when available, and
  independent correctness/source judgments. Mark this as the baseline rather
  than silently comparing results from different model versions.
- [ ] **Separate evaluation families.** Keep FQA-Bench focused on filesystem
  question answering. Add separately reported slices for general knowledge,
  web research, forecasting, visualization, clarification, and hybrid
  file-plus-research questions. Define tasks before implementation changes;
  do not convert observed failures into special-case production rules.

**Complete when:** the product/privacy policy is explicit, conflicting docs no
longer contradict it, and a reproducible baseline exists for each route that
is in scope.

### 1. Make Ask useful without a mounted folder

- [ ] **Remove the accidental folder prerequisite for general questions.**
  Revise the no-workspace prompt and route so that a missing folder blocks
  only questions that actually require local files. A user asking “What is
  Rust?” or “How are steps counted on Apple Health?” should get a direct
  answer without being told to mount a folder first.
- [ ] **Give the model a clear no-tool path.** Let it answer stable, ordinary
  background questions from its configured model knowledge. Do not require a
  workspace inspection, SQL plan, verification pass, or clarification when
  none is needed. If its knowledge may be stale or the user asks for sources,
  let it select web research instead.
- [ ] **Handle mixed intent.** When a question contains both a general
  explanation and a workspace-dependent request, answer the part that can be
  answered, inspect the workspace for the rest, and distinguish the two
  results. Do not turn one missing file/source into a refusal of the entire
  question.
- [ ] **Keep the conversation coherent.** A general answer, a later question
  about a mounted folder, and a follow-up should remain in the same
  conversation. Preserve relevant prior assumptions and results without
  treating all prior assistant prose as established fact.
- [ ] **Remove contradictory prompt instructions.** In particular, replace
  “only greetings/Fella questions” behavior and blanket forecast refusals
  with route-aware guidance. Prompt changes should express task judgment,
  not enumerate example phrases or file names.

**Acceptance checks:** no-folder general question succeeds; no-folder
file-dependent question clearly asks for or offers a folder; mixed question
answers the answerable portion and identifies the missing dependency; a
follow-up uses prior conversation context correctly.

### 2. Add bounded, source-grounded web research

- [ ] **Define a small provider-neutral research interface.** Give the model
  tools to search and open/read a result, return title, URL, publisher,
  retrieval time, and relevant passages, and report errors/limits. Keep the
  interface small enough to audit; do not expose unrestricted shell, browser
  automation, or arbitrary network access as a substitute.
- [ ] **Apply the privacy policy at the tool boundary.** Never attach local
  files, extracted rows, or file snippets to a web request by default. Make
  outbound research visible. If hybrid research would require disclosing
  workspace-derived details, ask first or have the model generalize the query
  without those details according to the approved policy.
- [ ] **Treat web pages as untrusted evidence.** Keep page text separate from
  system/developer instructions; clearly label it as quoted external content;
  ignore instructions found inside pages; limit fetched size, redirects, and
  timeouts; block local/private-network targets and unsafe schemes; and do
  not execute downloaded content.
- [ ] **Make citations useful.** Store enough provenance to render links near
  the claims they support. Show when research was retrieved. Do not claim a
  source supports a statement if the supporting passage was not returned or
  inspected. If sources disagree, tell the user rather than silently
  selecting one.
- [ ] **Support ordinary and current questions distinctly.** General model
  knowledge should remain available for stable definitions. Use the web when
  the user requests research/citations or when the answer depends on current
  facts. The model may explain that it has not checked current sources.
- [ ] **Test offline and failure behavior.** Cover no results, stale pages,
  blocked domains, timeouts, malformed pages, contradictory sources, unsafe
  page instructions, and cancellation. Search failure should not erase a
  useful model-knowledge answer where one is appropriate.

**Acceptance checks:** a web-sourced answer has inspectable citations;
web content cannot issue tools or override instructions; no local file data is
sent; the user can tell that research happened; failed research degrades
gracefully.

### 3. Replace one binary “Verified” label with honest provenance

- [ ] **Inventory every producer and consumer of verification status.** Map
  status generation, persistence, APIs, chat rendering, evidence/detail
  surfaces, and tests across the supported Tauri/Electron paths before
  changing semantics.
- [ ] **Represent the basis of an answer directly.** Introduce a typed
  provenance/report model that can describe, as applicable: model knowledge;
  web sources and retrieval time; workspace documents and passages; executed
  SQL/Python with inputs and result references; user-confirmed definitions or
  assumptions; and estimates/forecasts with method and uncertainty. These
  are evidence facets, not competing labels on one scale.
- [ ] **Reserve computation replay for computational claims.** A result
  derived from local data should record the source revision, selected data,
  transformation or query, and execution result sufficiently to inspect or
  replay it. A cited explanation should not be forced through SQL merely to
  earn a status badge.
- [ ] **Use language that matches the evidence.** Distinguish “calculated
  from…”, “the document says…”, “web sources report…”, “general explanation,”
  “estimate,” and “forecast.” Do not imply that citations prove truth or that
  a replay proves the user's interpretation was correct.
- [ ] **Make uncertainty informative, not suppressive.** If uncertainty is
  material, state what is known, what is assumed, and what would change the
  result. Ask a focused clarification when the ambiguity materially changes
  the answer. Otherwise give a qualified answer rather than refusing solely
  because a secondary checker disagreed.
- [ ] **Audit hard gates and brittle checks.** Review numeric-token comparison,
  second-opinion disagreement, and checker failure behavior. A hard stop is
  justified for a safety, execution, or irrecoverable data-integrity problem;
  a soft quality signal should trigger a better explanation/review, not
  automatically discard a likely useful answer.

**Acceptance checks:** general knowledge, cited research, document
interpretation, replayed calculations, and forecasts each display an
appropriate basis; no category is mislabeled “Verified” merely because of its
tool route; low-confidence output remains useful and appropriately qualified.

### 4. Support forecasts and forward-looking analysis responsibly

- [ ] **Remove blanket forecast rejection.** Allow the model to inspect the
  available series and decide whether the data can support a forecast, a
  scenario, or neither. Keep the user-facing distinction between observed
  facts, model estimates, and hypothetical scenarios.
- [ ] **Inspect before fitting.** For a time-series request, establish the
  relevant measure, time field, grain, date coverage, gaps, duplicates,
  outliers, and aggregation before selecting a method. Use the workspace
  inspection/clarification loop where the series or scope is ambiguous.
- [ ] **Evaluate forecast quality rather than just producing a line.** Where
  history permits, compare against a simple baseline and use chronological
  holdout or rolling-origin evaluation. Never use future observations to fit
  a past prediction. Report the evaluation window and an interpretable error
  measure when meaningful.
- [ ] **Represent uncertainty and limitations.** Provide prediction intervals
  when the selected method supports them; say when data is too short, too
  irregular, or structurally changed for a defensible interval. Avoid false
  precision and avoid portraying extrapolation as a certainty.
- [ ] **Support scenarios separately from forecasts.** Let a user ask “what
  if…” and specify assumptions. Show which inputs were changed and calculate
  the scenario without presenting it as a prediction of what will happen.
- [ ] **Choose reusable statistical capabilities.** Audit current Python/Rust
  execution capabilities and packaging constraints; select an extensible,
  testable set of methods and document unsupported ones. Do not add
  file-name-, domain-, or wording-specific forecast logic.

**Acceptance checks:** a supported forecast includes method, history window,
backtest/baseline where possible, and uncertainty; an unsupported forecast
explains the actual limitation and offers a useful alternative; observed and
predicted values are never conflated.

### 5. Expand visual analysis beyond basic bars and lines

- [x] **Decouple chart data from SQL.** Define a typed visualization
  specification that can consume validated tabular output from SQL, Python,
  a forecast, or a scenario. Keep computation and rendering separate so a
  chart is not silently recomputed with different semantics.
- [x] **Add analytical forms based on evaluation need.** At minimum, assess
  scatter plots for relationships, histograms for distributions, box/quantile
  plots for spread and outliers, stacked/area forms for composition over
  time, heatmaps for two-dimensional patterns, and observed-versus-forecast
  charts with uncertainty bands. Preserve existing bar and line charts.
- [x] **Validate chart meaning.** Check that selected fields exist, numeric and
  categorical roles are compatible, units/aggregation are coherent, series
  and category cardinality are manageable, and missing values are handled
  explicitly. Do not silently drop rows that materially affect the displayed
  result.
- [x] **Preserve inspectability.** Let users see the plotted values and
  definitions, including time range, aggregation, filters, units, and missing
  data treatment. The chart should be an additional presentation of the
  result, not a replacement for the answer or its provenance.
- [x] **Make charts readable and accessible.** Define responsive sizing,
  legible labels, keyboard/screen-reader descriptions, color-safe series
  palettes, and light/dark behavior. Avoid arbitrary generated HTML/SVG or
  model-supplied executable markup.
- [x] **Test visual correctness independently.** Verify the rendered chart
  against the underlying result table/spec, not merely that a chart object
  exists. Add snapshots or structural checks for labels, axes, series,
  missingness, and uncertainty bands.

**Acceptance checks:** at least one non-SQL chart and each prioritized chart
family can be generated from a clear dataset; the displayed marks match the
underlying values; users can inspect how the chart was constructed.

**Implementation update (2026-10-04):** the typed engine and UI render all
prioritized chart families, and automated data/render checks are in place.
Product acceptance remains qualified: the unchanged 25-case live run scored
20/25, three failures have potential gold-specification ambiguities, a full
post-fix run has not been performed, and desktop light/dark visual review is
still pending. See `bench/product-eval/reports/backlog-5.md`; preserve those
failures until they are adjudicated.

### 6. Make mounted folders easier to understand and analyze

- [x] **Establish a fair mount-scale baseline.** Add a reproducible, mixed
  folder fixture with nested directories, varied file sizes, duplicate
  basenames, messy supported files, unsupported files, and realistic parse
  errors. Use 5,000 files / 10 GB as a performance target—not a product limit—and
  include larger runs where practical. Before changing the implementation,
  record time to first inventory, first query-ready source, and full readiness;
  peak memory, temporary-disk use, files/rows actually available, and any
  omissions. Keep quality expectations independent of candidate output.
- [ ] **Make the inventory complete, fast, and addressable.** Recursively
  discover the mounted tree without a silent default depth cutoff. Classify
  against Fella's supported-format allowlist before opening file contents;
  do not follow symlinks by default. Give every source and skip a stable
  workspace-relative path so same-named files cannot collide. Record size,
  modification state, format, and explicit readable / unsupported / ignored /
  failed status; summarize large skip lists without hiding their details.
- [ ] **Pipeline preparation behind the inventory.** Publish useful file
  discovery promptly, then profile and prepare supported files as they are
  found, overlapping traversal with a bounded number of workers. Tune traversal
  and parsing concurrency independently; use bounded queues and backpressure
  (including limits on in-flight bytes), stream large inputs into the
  disk-backed analytical store, and cancel stale work when a mount is replaced.
  Publish source readiness atomically and pin each analysis to a stable
  workspace/source snapshot, never a half-mutated index. A folder boundary is
  not a semantic boundary: avoid one model call per directory or file, and
  allow cross-folder relationships to emerge from the complete catalog. Do not
  impose arbitrary aggregate file/byte/row caps or silently truncate a source
  and label it complete. If actual memory, disk, or parser constraints prevent
  completion, show the exact partial source and reason so the user can narrow
  scope or retry.
- [ ] **Build source profiles with traceable normalization.** For each file,
  expose compact schema/sheet/table information, row counts, representative
  values, distributions and anomalies (including missingness, date/number
  patterns, units, repeated headers, and inconsistent categories). Preserve
  source values and record the parser, normalization, and row/page lineage
  behind derived fields. Treat uncertain interpretations as candidates with
  supporting and conflicting evidence, not silent corrections.
- [ ] **Let the model investigate through the existing analytics loop.** Give
  it targeted tools to navigate the catalog, search text, inspect profiles and
  bounded samples, read relevant passages, and run SQL or Python over prepared
  data. Keep the prompt compact; generate richer explanations lazily for
  relevant sources rather than summarizing every file on mount. Make pending,
  complete, partial, and failed preparation visible to the model and user, so
  it can inspect more, work from available sources, or ask one focused
  clarification when an ambiguity materially changes the answer. Resume with
  the user's clarification without discarding completed inspection. Persist
  user-authored definitions only at their chosen workspace scope and make them
  editable/removable.
- [ ] **Refresh incrementally without trusting notifications as truth.** Reuse
  prepared results for unchanged files; invalidate and reprocess changed,
  added, or removed sources, and bind each answer to the exact catalog/data
  revision it used. Filesystem watchers may accelerate refresh, but reconcile
  against the mounted tree because platform watchers can lose events or be
  unavailable on some filesystems. Surface stale or incomplete state instead
  of silently mixing revisions.
- [ ] **Choose traversal, search, and storage components by measured fit.**
  Benchmark a parallel, filterable walker against the current traversal, with
  walker and parser concurrency measured independently;
  evaluate FFF only for repeated path/text discovery (not parsing or analytics),
  accounting for its resident index and eligibility rules. Compare streaming
  ingestion/query paths with the current SQLite default and optional DuckDB
  path on the same messy fixtures, including schema/normalization parity,
  binary/dependency cost, peak memory, and large-file behavior. Adopt a
  component only when it improves end-to-end mount or analysis quality without
  narrowing Fella's supported-file coverage.

This work should be coordinated with `WorkspaceModel`, revision tracking,
definitions/value semantics, and broader chart invariants in the
[Analytical Computer Roadmap](ANALYTICAL-COMPUTER-ROADMAP.md), rather than
duplicated as a second ingestion architecture.

**Acceptance checks:** a 5,000-file / 10-GB mixed-folder run is a measured
performance target, not a mount rejection threshold. The benchmark reports
time-to-inventory, time-to-first-queryable-data, time-to-complete-preparation,
peak memory, temporary storage, and exact source/row coverage against the
predeclared fixture. The user can see what is ready, partial, unsupported, or
failed; large supported files are not silently dropped or truncated; the model
can find relevant material across nested folders; normalized values retain
source lineage; and a clarification can resume the same analysis against the
same workspace revision. Set numeric latency/resource budgets from the recorded
baseline before comparing candidate implementations.

**Implementation progress (2026-10-05):** the default depth cutoff has been
removed and skipped files now retain workspace-relative paths, so duplicate
basenames remain distinct. The 5,000-file inventory test observed 17 ms before
and 16–17 ms after this change in local single runs; these are metadata-only
datapoints, not a general speed claim or the 10-GB acceptance test. CSV/TSV,
JSON arrays/objects, and NDJSON now profile in one streaming pass and load in a
second pass into SQLite, preserving full-file type inference without retaining
all records or silently truncating by the former 2-million-row / 256-MiB
defaults. Malformed NDJSON and non-object records are explicitly noted. The
focused completeness tests and streaming-vs-reference type inference tests
pass. Remaining constraints: XLSX currently materializes sheets; `open_workspace`
publishes the replacement catalog atomically after full preparation, although
both desktop shells now run the mount on a blocking worker and stream
scan/preparation phase and count updates to the UI. The user sees progress, not
a partially queryable inventory. Document search streams text but makes two
workspace-wide passes per query. These are facts to measure and address, not
acceptance criteria to preserve.

The baseline probe fixture contains 5,000 nested table sources (CSV, TSV, JSON
arrays, and NDJSON), 59,900 rows, 100 readable Markdown notes, and 100 visible
unsupported DOCX-shaped files. Its expected inventory is explicit: all 5,000
tables and 100 notes load, every table's row count is accounted for, and all
100 unsupported files are reported. Two back-to-back exploratory debug runs
measured scan completion at 85/90 ms, complete-catalog
readiness at 7,815/7,681 ms, first sample-query execution at 53/52 ms, first
inventory page at 4/4 ms, exact path lookup at 3/4 ms, temporary SQLite size at
27,340,352 bytes in both runs, and Linux process peak RSS (`VmHWM`) at
56,438,784/56,324,096 bytes. These are not controlled cold/warm comparisons or
a product latency claim. Scan completion is not when users can browse the
Sources page; metadata walking is a small part of this fixture's 7.7-second
mount, so overlapping the walk with preparation alone is unlikely to
materially improve this 5,000-file case. The path lookup still scans the
in-memory catalog rather than using a persistent index. The RSS figure is the
test process high-water mark, not a packaged-app or Windows process-tree
measurement. The ignored probe cleans its generated workspace and temporary
database automatically and has no timing pass/fail threshold. These debug
runs did not include the later 10-GiB release diagnostic below, and no
controlled benchmark series has been collected. Earlier
mount timings (46,983 / 20,566 / 6,961 ms) were from a simpler all-CSV fixture
and are not directly comparable. A separate 250,000-row, 5,027,795-byte CSV
loaded completely in about 2.5 seconds in a local debug run.

A one-run serialization diagnostic measured the full `Catalog` response at
2,865,584 JSON bytes and 90 ms of `serde_json` serialization for the same
fixture. This is the serialized payload size—not measured Tauri/Electron
transport latency or WebView parse/render time. Its test-process peak RSS was
59,068,416 bytes while the response buffer was live, so it is not directly
comparable to the earlier mount-only RSS readings.

Code review found avoidable per-cell work in the streaming delimited profiler:
ordinary labels and numeric values were sent through date parsing, and already
numeric values were reparsed by the tolerant-number parser. The profiler now
uses date-shape and numeric fast paths while retaining the same complete-file
inference behavior; the existing full-reference parity tests for numeric/date
types still pass. Exploratory debug runs of the same 5,000-small-source fixture
plus one complete large CSV measured 32 MiB / 860,369 rows at 23,024 ms before
and 17,661 ms after, and 64 MiB / 1,720,739 rows at 38,691 ms before and 27,407
ms after. The post-change Linux process high-water marks were 59,584,512 and
59,326,464 bytes; SQLite scratch used 89,885,624 and 156,180,336 bytes. These
are single-run comparisons, not controlled or release-build claims. The
coverage fixture has since added two malformed supported files; the probe
asserts all 102 skipped paths are visible and each malformed input has a reason.
A separate mount-wide SQLite transaction prototype had no observed speed
improvement (7,805 ms vs. 7,804 ms on one 5,000-source run) and increased live
scratch usage from 27,340,352 to 46,781,744 bytes, so that experiment was
discarded.

The probe can add one complete large CSV through `FELLA_MOUNT_SCALE_LARGE_MIB`
and opt-in per-pass instrumentation through `FELLA_INGEST_TIMING=1`. A
256-MiB (268,435,431-byte), 6,882,959-row file mounted alongside the 5,000
small sources in 90,168 ms in a debug run; test-process peak RSS was 59,883,520
bytes and SQLite scratch was 564,719,688 bytes. It passed exact source/row
coverage; it is one debug-build datapoint, not a latency budget or a basis for
linear extrapolation to 10 GB or release builds. Splitting a 64-MiB,
1,720,739-row one-file run showed
11,412 ms profiling / 9,475 ms loading before the per-cell conversion and
reader-buffer changes. Two runs with the current changes measured 9,901–10,366
ms profiling, 8,002–8,038 ms loading, and 17,991–18,419 ms for the full mount;
the Linux process high-water mark was 23,592,960–24,117,248 bytes and scratch
was 132,601,792 bytes. This one-source measurement is not directly comparable
to the 5,000-source fixture. A current no-large-file 5,000-source run completed
in 7,329 ms with 59,731,968-byte peak RSS and 27,340,352-byte scratch. All
large-file runs removed their generated source tree and database automatically.

Two release-mode diagnostics provide an initial optimized-build comparison.
A single 64-MiB CSV (1,720,739 rows) profiled in 720 ms and loaded in 1,372 ms;
the complete mount took 2,104 ms, peak RSS was 13,238,272 bytes, and SQLite
scratch was 132,601,792 bytes. The full nested fixture plus a 256-MiB CSV
(268,435,431 bytes; 6,882,959 rows) completed in 11,920 ms: 5,001 tables,
6,942,859 total rows, 100 documents, and all 102 expected skips were present;
inventory was ready in 55 ms, first sample query in 52 ms, peak RSS was
48,848,896 bytes, and scratch was 564,719,688 bytes. These are single-run
Linux release-build observations with the fixture's 5,000 small sources, not
controlled benchmarks or a 10-GiB result. The release probe removed its input
tree and generated database automatically.

The first 10-GiB-scale release diagnostic mounted a 10,737,418,215-byte CSV
(275,318,415 rows) alongside the 5,000-source mixed-format fixture. It loaded
all 5,001 tables and 275,378,315 total rows, retained the 100 documents, and
reported all 102 expected skips. Mount readiness took 603,809 ms; inventory
discovery completed in 100 ms (the catalog was not published until preparation
finished), the first sample query took 53 ms, and catalog serialization took
4 ms for 3,106,021 bytes. Test-process peak RSS was
70,656,000 bytes and SQLite scratch peaked at 22,283,271,808 bytes (excluding
the generated input tree). This is one Linux release-build run of one very
large CSV plus the mixed small-source fixture—not a controlled benchmark,
multi-large-format profile, packaged-app measurement, or Windows result. The
complete generated input and scratch directory were removed after the test.
The long readiness time is a concrete optimization target; it is not evidence
that overlapping directory walking would help: inventory discovery completed
in 100 ms, but the product still waits for the complete catalog before it can
show those entries as ready.

The default SQLite WAL path adds a checkpoint to a large source commit; SQLite
documents that its automatic checkpoint runs after a commit crosses the WAL
page threshold ([WAL checkpoint behavior](https://www.sqlite.org/wal.html#automatic_checkpoint)). The staged workspace database has no concurrent readers while it is
built, so the SQLite backend now uses `journal_mode=DELETE` rather than WAL.
This retains rollback transactions while avoiding a large WAL/checkpoint copy
before publication ([journal-mode guarantees](https://www.sqlite.org/pragma.html#pragma_journal_mode)).

The exact 10-GiB fixture was rerun unchanged: mount readiness fell from
603,809 to 577,731 ms, scratch from 22,283,271,808 to 11,109,777,408 bytes,
and process high-water RSS from 70,656,000 to 49,025,024 bytes. All 5,001
table paths, 275,378,315 rows, 100 documents, and 102 skip entries still
matched expectations. The 1-GiB single-file run measured 40,286 ms /
2,208,314,824 scratch bytes with WAL and 38,014 ms / 1,099,853,824 bytes with
rollback journaling. The 5,000-source + 256-MiB run was 11,920 ms /
564,719,688 bytes with WAL and 12,112 ms / 292,741,120 bytes with rollback
journaling. These are single-run comparisons, not a controlled series or a
performance guarantee; the mixed small-source case showed no mount-time gain,
while large-file scratch use was about halved. Regression validation passed
320 library tests, 22 workspace integration tests, and the full 10-GiB
coverage probe. Windows and packaged-shell measurements remain open.

Large CSV/TSV files (64 MiB and above) now report bounded byte progress during
both streaming passes: full-file profiling and analytical-store loading. The
existing mount event carries the workspace-relative path, stage, bytes read,
and source size; the UI shows the active filename and percentage. Updates are
sampled around every 64 MiB (with a bounded row-count polling interval), not
emitted for every record. The 5,000-source + 256-MiB release probe passed its
new assertions for monotonic progress from zero through EOF in both stages;
it found 5,001 tables, 6,942,859 rows, 100 documents, and all 102 expected
skips. That run took 16,569 ms, used 292,741,120 bytes of scratch, and reached
48,398,336 bytes process high-water RSS. This is one functional probe, not a
controlled speed comparison, and its timing is not attributed to progress
reporting. Per-file byte progress is implemented for SQLite-backed CSV, TSV,
JSON arrays/objects, and NDJSON; focused tests verify both JSON reader stages.
XLSX and optional DuckDB ingestion still report mount-level counts without
within-file progress. Publication also remains atomic at whole-workspace
completion, so the user cannot query an early-ready source during a long mount.
The scale probe can now add a streamed 64-MiB JSON array: the 5,000-source
fixture plus that source loaded all 5,001 tables, 849,417 rows, 100 documents,
and 102 expected skips in 6,056 ms; the large file contributed 789,517 rows.
Scratch was 53,547,008 bytes and process high-water RSS was 48,365,568 bytes.
This is a single functional release run, not a controlled timing comparison.
JSON insertion also binds directly into a reused SQLite parameter buffer
instead of allocating two intermediate vectors per row; a reference-parity
test checks its conversions across missing, numeric, date, boolean, and text
values. No speedup claim is made without an A/B run on the same fixture.

A 32-MiB SQLite page-cache candidate was compared on an 8-source fixture plus
the same 256-MiB CSV (6,882,959 rows). Two rollback-journal baseline runs loaded
the CSV in 5,092 / 5,050 ms, with 12,845,056 / 12,976,128 bytes process
high-water RSS. The candidate loaded it in 5,041 ms but raised high-water RSS
to 45,481,984 bytes; profiling and total mount time were also not better than
the baseline range. This is too small a timing difference to distinguish from
noise for roughly 32 MiB more transient memory, so the candidate was reverted.
The default SQLite page cache remains unchanged.

A separate `synchronous=OFF` trial on the 5,000-source / 59,900-row release
fixture measured 3,003 ms mount readiness and 48,582,656 bytes process
high-water RSS. Two default-setting runs were 2,948 / 3,065 ms and
48,349,184 / 48,226,304 bytes RSS, with identical source, row, document, skip,
and scratch coverage. The candidate did not beat the baseline range, so it was
reverted; SQLite documents that OFF can leave a database corrupt after an OS
crash or power loss ([synchronous modes](https://sqlite.org/pragma.html#pragma_synchronous)).
The default synchronous setting is retained.

The current implementation also avoids mount-time exact distinct/null/range
profiles on larger workspaces and computes them when a table is inspected;
relationship-hint discovery uses an inverted field-name index and retains at
most 128 candidates, explicitly disclosing truncation. The model's `list_files`
inventory is now bounded to 50 entries / 12,000 characters per call, supports
path search and kind filters (including skipped files), and the Sources page
renders 100 rows at a time. This bounds tool output and DOM row count, but not
the full catalog sent over desktop IPC or retained in memory: the UI still
receives every source record. Mounts now run away from the Tauri/Electron
command executor, and both shells show scan/preparation phase and count
updates; the existing workspace remains available until the full new snapshot
is ready. Existing analysis runs hold a workspace read permit through
finalization, and publication waits for them before swapping revisions; new
analyses queue behind a pending publication, preventing one answer from
mixing data from two mounts. Incremental refresh and progressively queryable
sources remain open.
XLSX sheet loading still materializes sheet data, and document search still
makes two workspace-wide passes per query. These remain open work, as do
incremental refresh, progressively queryable sources, controlled repeated
large/mixed-format runs, multi-large-file coverage, and packaged
process-tree peak-memory measurements.

**Research informing the design:** Rust's [`ignore::WalkBuilder`](https://docs.rs/ignore/latest/ignore/struct.WalkBuilder.html)
offers a parallel recursive walker and filtering, but its file-size and ignore
policies must not silently narrow Fella's supported data. DuckDB's
[CSV reader](https://duckdb.org/docs/current/data/csv/overview) supports
parallel reads, sampling/sniffing, and explicit rejected-row reporting; its
[multi-file reader](https://duckdb.org/docs/current/data/multiple_files/overview)
can query file lists directly, while repeated sniffing can add overhead for
many small files ([performance notes](https://www.duckdb.org/docs/current/guides/performance/file_formats)).
The [`notify` crate documents](https://docs.rs/notify/latest/notify/) dropped
events for very large directories and missing events on some network filesystems,
so watching is an optimization, not the source of truth. [FFF](https://github.com/dmtrKovalenko/fff)
is a candidate for long-lived, repeated path/text search; its in-memory index
and file-eligibility behavior mean it should be benchmarked separately from
Fella's analytical ingestion and must not define which supported data is read.

### 7. Make the agent loop cohesive across routes

- [ ] **Use one traceable conversation loop with route-specific tools.** The
  same turn/session model should cover direct model answers, research,
  workspace inspection, computation, clarification, and combinations. Do not
  implement a separate “web agent” that loses conversation context or an
  analytics sub-agent that cannot use the user's answer to a clarification.
- [ ] **Keep the analyst loop available without making it mandatory for every
  request.** For data analysis, the model should inspect, form/decompose a
  question, probe or clarify where useful, execute, review, and communicate.
  For a simple definition, answer directly. Let the model stop when it has a
  good answer; retain any early-stop nudge only if evaluation shows it does
  not increase incomplete or incorrect answers.
- [ ] **Make context and memory participate in decisions.** Separate
  conversation history, workspace facts/definitions, retrieved source
  evidence, and derived results. Label each by origin and freshness. A
  follow-up should reuse prior result references or rerun against a changed
  source revision, not invent continuity from a summary.
- [ ] **Use clarification instead of forced semantic guessing.** Ask only
  about a decision that changes the result; offer concise choices plus a
  free-text response; keep the question and answer in the transcript; resume
  the paused turn with the inspection already done.
- [ ] **Instrument every route.** Record model/tool steps, source revision,
  prompts/model identifiers where safe, elapsed time, token/cost estimates,
  research citations, clarifications, errors, and provenance. Redact secrets
  and provide a clear local retention/deletion policy.
- [ ] **Do not add multi-agent orchestration by default.** First identify a
  measurable quality or latency limitation that parallel/specialist agents
  solve, then compare it against one model-directed loop with better tools
  and source inspection.

**Acceptance checks:** a multi-step hybrid question retains one coherent
trace and conversation; context used in the answer can be inspected; simple
questions do not incur unnecessary analytics stages; clarification resumes
the same work.

### 8. Build a disciplined quality and capability evaluation

- [ ] **Keep task families separately reportable.** Report filesystem
  analytics, general knowledge, web research, forecasting, chart correctness,
  clarification, and hybrid tasks independently as well as in an overall
  summary. A strong score in one family must not hide a broken family.
- [ ] **Use whole-workspace tasks, not implementation-shaped prompts.** Include
  multiple unrelated profiles and folder structures, messy human-created
  files, varied formats, missingness, conflicting notes, and realistic
  follow-ups. The runner should not hand the model only the file known to
  contain the answer unless the task specifically tests retrieval after
  discovery.
- [ ] **Write tasks independently of the implementation.** Specify user
  intent, acceptable answer elements, source of truth, known ambiguity, and
  scoring rubric before looking at a new implementation's behavior. Have an
  independent reviewer author or audit a portion of the tasks and expected
  answers.
- [ ] **Evaluate the interaction, not only final text.** Score whether the
  model inspected the right material, selected an appropriate route, asked a
  necessary clarification, computed correctly, cited/supports claims, and
  communicated limits. Include complete successful traces and failure
  diagnoses.
- [ ] **Measure helpfulness as well as restraint.** Track answer correctness,
  coverage/answer rate, false-refusal or unnecessary-clarification rate,
  unsupported-claim rate, citation support, forecast error/calibration, chart
  correctness, time, tool/model calls, and cost. A refusal is not a success
  simply because it avoids a wrong number.
- [ ] **Compare fairly.** Pin model/version and settings; use paired runs for
  baseline versus candidate; separate deterministic fixture tests from live
  model/web runs; record variance and failed/incomplete runs. Do not use a
  test threshold as a reason to hardcode a task-specific rule.
- [ ] **Pre-register release gates.** Establish acceptable thresholds for
  correctness, answer coverage, false refusal, provenance, and cost from the
  baseline and product risk. Set them before evaluating a candidate; block
  release on regressions that matter, not on a single cherry-picked example.
- [ ] **Use an iterative slice size.** After a systemic change, rerun the
  affected family and a representative cross-family regression slice. Run
  the full suite at integration/release checkpoints, not after every prompt
  edit.

**Acceptance checks:** results can be reproduced and independently reviewed;
each supported capability has a visible score and failure taxonomy; changes
show whether they improve the quality/coverage tradeoff rather than merely
changing the wording of outputs.

### 9. Finish user experience, privacy, and release integration

- [ ] **Make Ask's scope obvious without making it feel like a file-only
  utility.** The empty state should accept general questions and make folder
  analysis/research discoverable without forcing a workspace selection.
- [ ] **Present evidence without duplicating the answer.** Put compact source,
  calculation, or forecast details adjacent to the relevant answer and make
  deeper provenance inspectable on demand. Do not repeat the same “checked
  workspace/details” block under every message.
- [ ] **Make modality visible.** Use restrained, consistent treatments for
  model explanation, web-cited claims, file-backed findings, computed values,
  and projections in both light and dark themes. Keep the language more
  important than badges.
- [ ] **Review permissions and threat boundaries.** Document local file
  access, provider transmission, web research, credentials, telemetry, and
  retention. Test prompt injection from both local documents and web pages.
- [ ] **Verify supported shells.** Run the same representative functionality
  checks against Tauri and Electron where both are maintained: streaming,
  tool approval/cancellation as applicable, chart display, citations,
  clarification/resume, settings, and persisted conversations.
- [ ] **Check performance and packaging.** Measure startup, idle memory,
  folder-inspection responsiveness, and packaged size. Ensure indexing and
  source profiling do not make ordinary Ask noticeably slower or require a
  cloud service.

**Acceptance checks:** users understand what Fella used, web access is not
surprising, private files remain local unless a separately approved feature
explicitly changes that boundary, and both maintained shells pass the same
capability suite.

### 10. Bound verifier authority and make its decisions inspectable

The verifier is a quality-control part of the harness, not a second agent
with an opaque veto over the whole answer. It should catch concrete problems
and help the model repair them without discarding sound analysis because of a
disagreement, a brittle comparison, or a failure in an unrelated output.

- [ ] **Define verifier authority explicitly.** Separate informational
  findings, repair requests, and hard gates. A hard gate must name the exact
  affected claim or artifact and cite an objective reason, such as failed or
  stale execution, invalid result structure, a safety boundary, or a chart
  whose plotted values do not match its source result. Semantic disagreement
  or a checker error alone is not a whole-answer hard gate.
- [ ] **Keep verification scoped to what failed.** A chart problem should not
  erase valid prose or tabular results; an unsupported sentence should not
  invalidate independent calculations. Preserve accepted SQL/Python results
  and charts unless the verifier identifies a concrete defect in those
  artifacts. Any removal or supersession must identify the specific item and
  reason in the trace.
- [ ] **Make findings useful to the model.** Return structured findings that
  identify the affected claim/artifact, the failed check, the supporting
  evidence, and a possible repair direction. The harness can then revise,
  qualify, omit only the unsupported claim, ask for material clarification,
  or retain a supported answer with an appropriate caveat.
- [ ] **Bound repair and preserve work.** Set a small, explicit repair budget;
  do not repeat equivalent tool calls or clear valid evidence to force a
  restart. If repair cannot resolve one finding, keep the rest of the answer
  and report the unresolved part precisely.
- [ ] **Expose the decision without clutter.** Let users inspect what was
  checked, what passed or failed, and what effect the finding had. Keep this
  detail available on demand rather than adding a success/failure badge or a
  repeated verifier report to every message. Do not offer a blanket “ignore
  verification” switch for safety or execution-integrity gates.
- [ ] **Evaluate verifier errors as product failures.** Add independently
  specified cases for false-positive rejection, checker disagreement,
  valid calculations with an unsupported prose claim, valid prose with a
  broken chart, stale/failed execution, and a genuinely unsupported result.
  Track false-positive blocks, false negatives, answer coverage, evidence
  retained, repair/tool-call count, and latency through the full agent loop.
  Declare expected behavior before running candidates; preserve failures for
  review instead of tuning expectations to observed output.

**Acceptance checks:** a verifier finding cannot silently erase unrelated
valid work; every hard gate is scoped and justified; supported portions of a
partial answer remain available; genuine execution, safety, and artifact
integrity failures still block the affected output; and evaluation shows both
reduced false-positive blocking and no material increase in unsupported
claims.

## Delivery order and dependencies

1. Complete **0** first: settle routing/privacy policy and capture the
   baseline so later changes are measurable.
2. Implement **1** next; it is a prompt/routing correction with high product
   impact and does not depend on web access.
3. Build **3** and **10** alongside the next small slice of **7**. Honest
   provenance and bounded verifier authority belong together: the UI should
   represent evidence without abusing `Verified`, and a checker should not
   veto unrelated supported work.
4. Add **2** after the privacy decision and provenance shape are agreed.
   Use deterministic mocked search/page tools for most tests, then a small,
   separately reported live-web evaluation.
5. Develop **4** and **5** on the shared execution and visualization
   interfaces. Backtest forecasts and compare chart output with source result
   tables.
6. Continue **6** using the existing workspace-model roadmap; prioritize
   source discovery, lineage, clarification/resume, and user-authored meaning
   over adding a long list of file-specific heuristics.
7. Run **8** at each systemic milestone and complete **9** before calling a
   capability ready for users. Treat **10** as a release-critical part of
   verification, not as permission to disable concrete safety or integrity
   checks.

Each delivery slice should be reviewable on its own and include implementation,
tests, docs, and evaluation evidence. Keep changes on the active feature branch
until the corresponding capability passes its agreed acceptance checks.

## Explicitly not the goal

- Do not hardcode the wording, filenames, fields, categories, or expected
  answers of benchmark tasks into production routing or analytics logic.
- Do not replace the current app or stable working analytics paths wholesale.
- Do not remove read-only protections or grant general shell/file-write
  capability to make analytics feel more flexible.
- Do not claim that a citation, a deterministic calculation, or a green
  benchmark alone guarantees truth.
- Do not make every question go through SQL/Python, a multi-step plan, web
  search, or a verification gate.
- Do not optimize for fewer model/tool calls at the cost of a complete,
  correct, clearly supported answer.
