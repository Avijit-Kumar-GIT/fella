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
consequence of asking a question. The accepted policy is **use web research
when needed and make it visible**: stable questions can use model knowledge;
the model may search when the user asks for sources/current information or
freshness materially affects the answer. Show that research is happening and
show the sources used. Search only with a minimal, generalized query; never
include mounted-file contents, rows, snippets, hidden workspace memory, or
credentials. For hybrid tasks, keep local analysis and public research
separate, then combine their conclusions in Fella. This policy is recorded in
[`DECISIONS.md`](DECISIONS.md).

## What the current implementation tells us

The repo already has substantial pieces to build on: a shared Rust runtime,
local SQL/Python/text tools, a workspace analytical model, conversation state,
chart rendering, and verification. The problem is partly how these pieces are
currently composed and described:

- The no-workspace branch in `src-tauri/src/engine/agent.rs` supplies no tools
  and currently directs the model toward asking the user to open a folder for
  data/file requests, while its fallback wording narrows normal answers.
- The active runtime prompt permits forecasts and scenarios and asks the model
  to state method, assumptions, and uncertainty. The observed forecast mismatch
  came from a pre-release benchmark gold that required refusal, not a blanket
  refusal in the shipped prompt. Forecast-method quality, calibration, and
  clear separation of observed values from projected ones remain unfinished.
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

- [x] **Write the Ask routing and privacy decision.** Stable questions can
  use model knowledge; web research is allowed when the user asks for sources
  or current information, or freshness materially affects the answer. Make
  research visible and show sources. Use a minimal generalized query and
  never send mounted-file contents, rows, snippets, workspace memory, or
  credentials to the search service. Keep workspace analysis local and combine
  it with public findings inside Fella. Recorded in `DECISIONS.md`; runtime
  implementation remains a later backlog item.
- [x] **Reconcile the core product documents.** Updated `VISION.md`,
  `PRINCIPLES.md`, `NON-GOALS.md`, `ARCHITECTURE.md`, `WHY.md`, and `README.md`
  to distinguish the Ask-led product direction from the current
  workspace-focused release. Detailed help pages still describe current
  shipped behavior and should be updated only when the corresponding feature
  ships.
- [x] **Capture a before-change baseline.** Ran the same pinned model/provider,
  prompts, settings, and task set against the current branch. Save question,
  route/tool trace, response, latency, token/cost data when available, and
  correctness/source judgments. The evaluation runner now creates genuine
  no-workspace cases. Captured the seed general-knowledge suite against both
  bare-model and Fella conditions, plus representative current file and chart
  routes; web research is not implemented, and forecast quality has not yet
  been benchmarked. The small snapshot
  is a smoke baseline, not a representative quality claim; see
  [`bench/product-eval/baselines/2026-10-01.md`](../bench/product-eval/baselines/2026-10-01.md).
- [x] **Separate evaluation families in the methodology.** FQA-Bench remains
  scoped to filesystem analysis. Added a product-wide evaluation map for
  general knowledge, web research, filesystem analytics, forecasting,
  visualizations, and hybrid tasks; each is scored independently. Fixtures
  and substantial family baselines will be curated in their own follow-up
  work, before runtime behavior changes.

**Complete when:** the product/privacy policy is explicit, conflicting docs no
longer contradict it, and a reproducible baseline exists for each currently
implemented route. Baseline breadth and repeat-run coverage remain limited and
must be expanded before drawing product-quality conclusions.

### 1. Make Ask useful without a mounted folder

- [x] **Remove the accidental folder prerequisite for general questions.**
  Revise the no-workspace prompt and route so that a missing folder blocks
  only questions that actually require local files. A user asking “What is
  Rust?” or “How are steps counted on Apple Health?” should get a direct
  answer without being told to mount a folder first.
- [x] **Give the model a clear no-tool path.** Let it answer stable, ordinary
  background questions from its configured model knowledge. Do not require a
  workspace inspection, SQL plan, verification pass, or clarification when
  none is needed. If its knowledge may be stale or the user asks for sources,
  let it select web research instead.
- [x] **Handle mixed intent.** When a question contains both a general
  explanation and a workspace-dependent request, answer the part that can be
  answered, inspect the workspace for the rest, and distinguish the two
  results. Do not turn one missing file/source into a refusal of the entire
  question.
- [x] **Keep the conversation coherent.** A general answer, a later question
  about a mounted folder, and a follow-up remain in the same conversation.
  Per-turn workspace path/revision travels with the compact conversation
  memory; switching folders preserves prior wording for reference resolution,
  while old query/interpretation hints are withheld unless that exact snapshot
  is mounted. Archive hydration applies the same rule after restart, including
  when no workspace is open. Regression tests cover these transitions.
- [x] **Remove contradictory prompt instructions.** The no-workspace
  greeting-only rule was replaced with
  route-aware guidance. Forecasts are described as estimates and must include
  method, assumptions, and uncertainty. The pre-release draft `fqa-refusal-spend`
  case expected refusal, which conflicted with this product behavior. Its
  recorded **0/1 against that legacy gold** is preserved as a historical policy
  mismatch, not evidence that a forecast should have been refused or that the
  current forecast is correct. The old refusal expectation is retired from
  active scoring in the first target-state FQA-Bench v0.1 and replaced by
  estimate, partial-answer, clarification, and evidence-limit contracts.
  Forecast quality remains a separate workstream in backlog #4.
  Prompt guidance describes task judgment rather than example phrases or
  filenames.

**Acceptance checks:** after a general prompt clarification, the fixed
no-workspace routing cases passed 3/3 by majority over three iterations (9/9
phrase checks). An earlier unchanged-suite run scored 2/3 because the mixed
answer did not explicitly offer mounting a folder; that failure was recorded,
then the unchanged suite was rerun. A separate direct-OpenAI smoke run on the
same three cases scored 3/3 (one run per case); this is not a broad quality
claim. The general-knowledge suite passed 3/3 over three iterations, but its
phrase rubric was broadened after exploratory outputs, so it is not independent
evidence. Deterministic regression tests now cover context across mount,
revision, and restart boundaries. A separately frozen live trajectory case
passed its final-answer screen 1/1 across a general answer, folder mount, local
calculation, and follow-up. Runtime verification still emitted unsupported
semantic-operation warnings, so this is not a verified-runtime or broad
reliability claim. The unchanged legacy forecast-refusal case scored **0/1
against obsolete refusal expectations**; it is not an in-scope acceptance
check for the current policy. Its trace also flags the requested monthly
bucket, which needs separate technical review under backlog #4. See
[`bench/product-eval/reports/backlog-1.md`](../bench/product-eval/reports/backlog-1.md).

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

- [x] **Remove blanket forecast rejection.** Allow the model to inspect the
  available series and decide whether the data can support a forecast, a
  scenario, or neither. Keep the user-facing distinction between observed
  facts, model estimates, and hypothetical scenarios. Prompt policy now
  permits estimation; forecast quality remains unbenchmarked beyond the
  draft development diagnostics in the first target-state FQA-Bench v0.1.
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

- [ ] **Decouple chart data from SQL.** Define a typed visualization
  specification that can consume validated tabular output from SQL, Python,
  a forecast, or a scenario. Keep computation and rendering separate so a
  chart is not silently recomputed with different semantics.
- [ ] **Add analytical forms based on evaluation need.** At minimum, assess
  scatter plots for relationships, histograms for distributions, box/quantile
  plots for spread and outliers, stacked/area forms for composition over
  time, heatmaps for two-dimensional patterns, and observed-versus-forecast
  charts with uncertainty bands. Preserve existing bar and line charts.
- [ ] **Validate chart meaning.** Check that selected fields exist, numeric and
  categorical roles are compatible, units/aggregation are coherent, series
  and category cardinality are manageable, and missing values are handled
  explicitly. Do not silently drop rows that materially affect the displayed
  result.
- [ ] **Preserve inspectability.** Let users see the plotted values and
  definitions, including time range, aggregation, filters, units, and missing
  data treatment. The chart should be an additional presentation of the
  result, not a replacement for the answer or its provenance.
- [ ] **Make charts readable and accessible.** Define responsive sizing,
  legible labels, keyboard/screen-reader descriptions, color-safe series
  palettes, and light/dark behavior. Avoid arbitrary generated HTML/SVG or
  model-supplied executable markup.
- [ ] **Test visual correctness independently.** Verify the rendered chart
  against the underlying result table/spec, not merely that a chart object
  exists. Add snapshots or structural checks for labels, axes, series,
  missingness, and uncertainty bands.

**Acceptance checks:** at least one non-SQL chart and each prioritized chart
family can be generated from a clear dataset; the displayed marks match the
underlying values; users can inspect how the chart was constructed.

### 6. Make mounted folders easier to understand and analyze

- [ ] **Make source discovery progressive and folder-wide.** On mount, create
  a useful inventory of supported files, formats, sizes, likely tabular
  sheets/tables, date/number patterns, and parse errors. Let the model inspect
  summaries first and request targeted samples instead of stuffing entire
  folders into prompt context.
- [ ] **Preserve raw data and its lineage.** Record which source, sheet/table,
  row or page, parser, and normalization produced each analytical field.
  Normalization must not erase the original value or make a corrected value
  appear source-authored.
- [ ] **Improve general normalization.** Support common date/time and numeric
  conventions, units, missing-value markers, headers, repeated headers,
  duplicate records, and inconsistent categorical values using inspectable
  evidence. Treat guesses as hypotheses with confidence and provenance, not
  silent universal conversions.
- [ ] **Expose semantic candidates to the model.** Provide compact source
  profiles and candidate field meanings/relationships, including evidence
  and counterevidence. Let the model select or ask rather than relying only
  on lexical matches or a rigid precomputed contract.
- [ ] **Make clarification a real loop transition.** When two plausible
  meanings, populations, units, or time scopes materially change a result,
  ask one focused question in the conversation, preserve completed inspection,
  then resume after the user answers. Do not restart the entire analysis or
  create an empty/duplicate conversation.
- [ ] **Learn user corrections within the right scope.** Persist user-authored
  definitions and corrections at the intended workspace/project scope, show
  where they apply, let users edit/remove them, and avoid treating an
  accidental one-off answer as a global fact.
- [ ] **Publish the actual format/scale support boundary.** Test CSV/TSV,
  spreadsheets, JSON/JSONL, text/Markdown, PDFs, and nested/mixed folders as
  supported by the current stack. Show unsupported or partially parsed files
  clearly rather than silently omitting them. Avoid adding a parser until its
  cost and maintenance case are understood.

This work should be coordinated with `WorkspaceModel`, revision tracking,
definitions/value semantics, and broader chart invariants in the
[Analytical Computer Roadmap](ANALYTICAL-COMPUTER-ROADMAP.md), rather than
duplicated as a second ingestion architecture.

**Acceptance checks:** a user can see what was and was not ingested; the
model can discover relevant data from an unfamiliar folder; normalized values
are traceable to source values; material ambiguity can be resolved and the
same analysis resumed.

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

## Delivery order and dependencies

1. Complete **0** first: settle routing/privacy policy and capture the
   baseline so later changes are measurable.
2. Implement **1** next; it is a prompt/routing correction with high product
   impact and does not depend on web access.
3. Build **3** alongside the next small slice of **7**. Honest provenance is
   needed before adding more routes, so the UI can represent their different
   evidence without abusing `Verified`.
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
   capability ready for users.

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
