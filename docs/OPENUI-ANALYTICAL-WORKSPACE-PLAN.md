# Fella: generative analytical workspace plan

Status: the initial interactive OpenUI/Studio/report vertical slice is
implemented; the broader product architecture remains in progress. This
document is a target architecture, not a claim that the complete product vision
is shipped. Written 2026-10-21 against `d719208` on
`explore/composable-interface`; status below was updated 2026-10-22.

### Implementation status — 2026-10-22

The first local vertical slice now includes interactive OpenUI answers, saved
artifacts, Studio, deterministic Markdown, and a validated portable report
package. OpenUI is the primary workspace answer in the same analysis turn; the
earlier separate design/composition passes are historical and no longer run.
The Svelte host supports local state and bounded presentation calculations over
saved results while preserving the existing Rust analytics harness and provider
transport. The current Gemma 4 31B compatibility run rendered and persisted a
composed answer, but one run does not establish analytical accuracy, general
model compatibility, or presentation quality. Exact validation evidence is in
the
[delivery ledger](testing/GENERATIVE-WORKSPACE-v1.md).

Delivered in this slice:

- The pinned OpenUI Svelte integration and Fella component library, generated
  versioned prompt/schema assets, OpenUI Lang parser/evaluator, a reactive
  Svelte host, result-bound values, local controls, bounded display functions,
  and a validated declarative Vega-Lite renderer. Legacy chart output remains
  readable. The model chooses composition from Fella's registered vocabulary;
  this does not provide arbitrary executable visualization code.
- Same-turn OpenUI finalization as the primary workspace answer, automatic
  artifact persistence, exact-result bindings, and a readable fallback. A
  bounded finalizer correction can repair malformed syntax or a directly
  hard-coded Metric value without rerunning analytical tools. The renderer
  remains the authoritative full parser/schema/binding check.
- Workspace-owned artifact identities and immutable revisions, canonical saved
  result references, Studio documents/placements, revision-aware writes,
  checkpoints, and retention of saved work after conversation deletion.
- Deterministic Markdown export and versioned portable JSON report packages;
  imported packages are validated and opened read-only without executing their
  saved presentation program.
- Electron/Rust end-to-end coverage for persistence, saved-work retention,
  restart/reopen, export, and portable-package journeys.

Partially delivered or still open: adaptive multi-question Investigation
orchestration; complete review/adopt and downstream-impact workflows for
analytical candidate revisions; large-result adaptive layouts/virtualization;
direct mdxcn interoperability and faithful SVG/PNG fallback for unsupported
figures; cross-workspace reuse with explicit authorization; and full
visual/accessibility review across compact, expanded, dark/light, and
keyboard-only states.

Not established by this slice: live-provider quality or model portability,
startup/latency/memory improvement, packaged Windows/macOS/Linux behavior,
historical source replay, or arbitrary custom visualization execution. The
current production build reports a large-chunk warning; bundle splitting and
performance budgets still need measurement. See the ledger for exact results
and test-set changes.

## 1. Outcome and central decision

Fella should turn questions about user-owned information into analytical work
that can be inspected, refined, compared, reused, and shared. Conversation is
the easiest way to direct the work; it is not the canonical analytical record.

The central presentation decision is:

> Retire the current chart specification as the mandatory ceiling on new
> presentations. Keep deterministic execution, result integrity, source
> boundaries, provenance, and historical compatibility.

OpenUI must not become a wrapper around the existing `ChartKind` enumeration
that leaves the model restricted to the same prescribed outputs. The target is
model-directed analytical composition, with a path to visualizations that the
current renderer cannot express. Freedom of presentation does not grant
authority to invent analytical values or access additional data.

The product remains one Electron/Svelte application and one Rust-owned
analytical harness. BI and personal intelligence use the same objects and
execution boundaries; domain-specific sources and definitions supply meaning.

### End-state user journey

1. Open a folder and ask a question without first configuring a dashboard or
   choosing an analytical mode.
2. Fella inspects relevant sources, interprets the question, computes, and
   checks the result using the existing harness.
3. Receive a suitable presentation: perhaps prose, perhaps a composed
   interactive analysis with charts, tables, metrics, and annotations.
4. Open or save useful output beside the conversation in Studio.
5. Refine it directly or through natural language. Analytical changes create
   reviewable revisions; cosmetic changes do not rerun the analysis.
6. Compare results, retain decisions, and reuse selected work across the same
   workspace without copying the underlying evidence.
7. Export a readable report from the saved work without asking a model to
   recreate it or rerunning source queries.

## 2. Verified baseline: reuse, extend, and add

This table describes the pre-implementation repository inspected at `d719208`;
the implementation status above and test ledger below describe what has since
changed. The original observations were based on source inspection, not a new
functional or performance test run. Existing documentation includes historical board,
environment, and external-terminal descriptions that do not all match the
current UI. Those descriptions must not drive the new architecture.

| Area | Existing implementation | Change required |
| --- | --- | --- |
| Execution | Model-directed agent loop, read-only SQL, sandboxed Python, document tools, clarification, bounded verification | Extend orchestration and presentation finalization; do not replace the loop |
| Canonical runs | `AnalysisTurn` stores request, source snapshot, interpretation, trace, evidence, checks, and lineage outside chat archives | Reference runs from reusable outputs; preserve existing records |
| Investigation | Durable identity, title, conversation association, source scope, Questions, definitions, Findings and their history | Connect artifact references and adaptive multi-question continuation |
| Results | Evidence contains bounded SQL results, Python-published tables, charts, and provenance | Separate complete reusable results from previews; introduce durable references where needed |
| Charts | Fixed Rust `ChartKind` / `VisualizationSpec`, `make_chart`, Svelte/D3 renderer | Keep as legacy/fallback support; remove as the mandatory new presentation contract |
| UI composition | Chat plus optional Tools pane, independently keyed tabs, file/chart previews, expansion | Add Studio documents and artifact targeting without restoring retired workspace modes |
| Prior analysis | Retrieval is conversation- and revision-scoped | Add explicit authorized artifact reuse; do not silently widen existing retrieval |
| Knowledge | Workspace guide, typed semantic facts with authority/freshness/conflicts, versioned Investigation definitions | Link reusable definitions to artifact revisions and calculation dependencies |
| Handoff | Deterministic Markdown from saved Investigation records, chart-value tables, chart exports | Add presentation export adapters and portable report snapshots |
| Deletion | Conversation deletion deletes its canonical analysis runs | Resolve retained-artifact dependencies before promising independence from chat |
| Reproducibility | Source metadata/revision comparisons, retained outputs, rerun lineage | Distinguish historical viewing from exact historical recomputation and crash recovery |
| Generative UI / Studio | No OpenUI integration or first-class Studio document | New incremental capabilities, not completed features |

Primary implementation anchors:

- [Runtime records](../backend/src/engine/runtime.rs),
  [agent loop](../backend/src/engine/agent.rs),
  [state and lifecycle](../backend/src/engine/state.rs).
- [Turn persistence](../backend/src/engine/analysis_store.rs),
  [evidence](../backend/src/engine/evidence.rs),
  [Investigation records](../backend/src/engine/investigation.rs).
- [Tools](../backend/src/engine/tools.rs),
  [chart contract](../backend/src/engine/analytics/chart.rs),
  [catalog freshness](../backend/src/engine/catalog.rs).
- [Message presentation](../src/lib/components/Message.svelte),
  [chart renderer](../src/lib/components/Chart.svelte),
  [Tools pane](../src/lib/components/WorkspaceView.svelte),
  [session state](../src/lib/session.svelte.ts).
- [Report builder](../src/lib/investigation-report.ts),
  [semantic memory](../backend/src/engine/semantic_memory.rs),
  [typed IPC](../src/lib/ipc.ts), [stdio dispatch](../backend/src/stdio.rs).

## 3. Product and UX model

### Layout clarification — Chat and Notebook

The mounted **Workspace** remains the folder/data/context boundary. **Chat** and
**Notebook** name two organizations of the same application window, not new
Workspaces or execution modes. Chat prioritizes conversation; Notebook places
the analytical document beside its conversation with navigation on the left.
At compact widths, document and chat are switchable without losing their state.
Switching layouts does not create work, mount a folder or invoke the model.

The historical "Studio" UI is now presented as **Reports**: ordered documents
that reference saved results and pinned revisions. Its persistence/API names
remain compatible. Investigations are ongoing work; results are reusable
outputs; reports communicate selected outputs. Opening a result does not
require adding it to a report. Advanced planning/source controls remain
available from the Investigation notebook rather than defining its default
six-screen interaction.

Analytical diff v1 compares recorded meaning, inputs, method and results and
lists indexed report/checkpoint pins. It preserves current/proposed versions
and does not automatically update reports. The dependency index does not claim
complete Finding/Question/prose/exported-report coverage. See
[the diff contract](testing/ANALYTICAL-DIFF-v1.md) and
[the layout contract](testing/CHAT-NOTEBOOK-v1.md).

Ask, Investigate, and Studio describe ways of working, not three mutually
exclusive application modes, engines, or mandatory top-level navigation tabs.

| Experience | User interaction | Authoritative state |
| --- | --- | --- |
| Ask | Ask, clarify, follow up, select output, request changes | Conversation references runs and artifact revisions |
| Investigate | Define an objective, inspect progress, revise Questions/definitions, review findings, continue work | Existing Investigation, Questions, definitions, Findings, and runs |
| Studio | Open saved output, arrange sections, compare revisions, edit narrative, export | Workspace-owned Studio documents reference artifacts and pinned revisions |

Keep chat as the default. Preserve the left sidebar's workspace and file/chat
organization and the optional right Tools pane. Studio can open as a Tools
tab and expand into the primary working surface; do not add an outer board or
require another workspace/environment layer.

Start Studio as an ordered, notebook-like document, not a free-positioned
dashboard canvas. A user may reorder, resize where meaningful, annotate, and
group analytical sections without writing executable cells. The model drives
the investigation; an empty section or absent chart never invalidates work.

Use contextual instructions such as "Break this down by region" against the
selected artifact. Show the target briefly and make it removable. Do not send
the same instruction to whichever Investigation happens to be globally active.

Preserve Fella's Mac-like curvature, Notion-like hierarchy, design tokens,
custom menus, readable typography, quiet chrome, and progressive disclosure.
Generated content does not control the sidebar, titlebar, permission dialogs,
verification chrome, or application navigation. Avoid wrapping every metric,
paragraph, and chart in its own bordered card. Exact values, caveats, and source
inspection must remain accessible at compact widths and through the keyboard.

General conversation remains available without a folder. The first artifact
reuse slice is workspace-scoped; unbound chat must not acquire the selected
folder's authority implicitly. General/model-knowledge prose is not labelled
as a verified data result merely because it has a rich presentation.

## 4. Presentation freedom without a new analytical engine

### 4.1 What changes, and what stays

| Current behavior or constraint | Target treatment |
| --- | --- |
| Fixed chart enum as the required path | Legacy adapter only; new presentations use the OpenUI library and broader visualization capabilities |
| A label column followed by a prescribed series shape | Bind named result fields and declare mappings; do not reshape analysis to satisfy a display template |
| Fixed dimensions and one prescribed answer arrangement | Responsive, model-composed sections, annotations, comparisons, and linked views |
| Small hard category/series ceilings used as readability rules | Adaptive height, scrolling, faceting, series selection, or explicitly disclosed aggregation; retain measured resource ceilings |
| Resource and execution limits | Retain and measure; visualization freedom does not imply unlimited memory or DOM nodes |
| Unsupported/malformed presentation | Preserve analysis; show readable fallback with a retry/reformat action |
| Missing values, source scope, evidence exclusions | Preserve their semantics in every renderer and export |

Do not delete `Chart.svelte`, archived specs, or existing verification before
new paths have compatibility and quality coverage. Do not continue requiring
every new presentation to call `make_chart` as a prerequisite.

### 4.2 Three presentation levels

**Level A: composable analytical components.** OpenUI arranges narrative,
tables, metrics, comparisons, charts, annotations, sections, and controls.
These use Fella-owned Svelte components and resolve data from saved results.
This is the first supported route, not a single hard-coded answer template.

**Level B: broader declarative visualization.** A visualization surface
supports field mappings, marks, layers, scales, facets, annotations, and
interaction composition beyond the old chart enum. Evaluate an established
declarative renderer against the existing D3 approach before choosing one.
Do not build a general visualization grammar from scratch or import multiple
large chart engines by default. A spike must demonstrate outputs unavailable
through the old contract, not just new component names for old charts.

**Level C: isolated custom visualizations.** If a real analysis requires
something Level B cannot express, provide a separately approved custom
SVG/canvas/code surface. This is a scheduled security/design work package,
not an implied permission to run generated JavaScript in the app renderer.
No arbitrary executable route ships before its isolation and export contract
passes review. Until then, unsupported custom requests receive an honest
fallback; do not claim unlimited visual expressiveness.

Any renderer has a vocabulary. The goal is to remove arbitrary product
templates as the ceiling, not promise that every possible visualization will
work without an implementation or security boundary.

### 4.3 Data, calculations, and interactions

- Data components receive authorized result references and field/formatting
  choices, not model-authored numbers that become canonical on rendering.
- The host resolves bindings from existing evidence or durable result payloads.
  A result reference is not a file path, SQL command, or access capability.
- Preserve nulls, dates, units, ordering, uncertainty, row completeness, and
  exact canonical representations. Chart pixel coordinates and rounded axis
  labels are never the export's numerical source of truth.
- Local hover, selection, sort, layout, and complete-result display filters
  require no model call or source query. Persist user-visible filter state
  where reopening/export depends on it.
- Display transforms are explicit and reproducible. Derived measures or
  changed populations that become analytical claims use the Rust computation
  path and record lineage; arbitrary browser expressions do not silently
  create evidence. Do not forbid ordinary visual calculations such as scales.
- A filter over a sample must say it filters a sample. It cannot imply a new
  population total. Analytical scope changes create a new run where necessary.
- Natural-language edits include the artifact ID, base revision, workspace,
  requested change, and relevant selection. Rust revalidates all references.
- Keep structural validity, binding validity, numerical reconciliation,
  semantic interpretation, answer coverage, and visual quality separate.
  Valid OpenUI does not prove that an answer or chart is correct.

## 5. Architecture and trust boundaries

```text
User question / targeted analytical change
                    |
       Existing Rust analytical harness
       inspect -> compute -> check -> persist run
                    |
        Canonical result/evidence references
                    |
      Optional design -> composition phases
      existing model transport; no new data tools
                    |
        Stored OpenUI program + bindings
                    |
          Artifact revision and manifest
          /             |               \
    Chat answer    Studio placement    Report snapshot
          \             |               /
           Same pinned results and provenance
```

### Rust authority

Rust continues to own workspace identity, source selection, tool permissions,
execution budgets, result persistence, reference authorization, verification,
and artifact/revision lifecycle. New allowlisted IPC commands expose bounded
operations; a generic "execute generated action" command is not acceptable.

Do not port the OpenUI parser to Rust merely to duplicate a maintained parser.
Use the established JS parser/schema machinery for presentation syntax. Rust
independently authorizes every result/artifact reference and permitted
analytical operation. Passing frontend schema validation grants no access.
Persisted programs remain untrusted presentation input when reopened.

### Presentation library and Svelte

Define the component library once, generate its model prompt/schema from that
definition, and version both. Ship the matching prompt/schema with the app
through a reproducible build step rather than trusting an arbitrary prompt
supplied by the renderer on every request. Keep generated programs tied to a
language version, library version, and manifest hash.

Use the native Svelte runtime if the compatibility spike passes. Keep model
transport, conversation state, and artifact state in Fella; no React chat
stack, Next server, Vercel AI SDK, OpenUI gateway, or second agent framework is
required simply to use the renderer.

### Generation and failure lifecycle

Initially finalize analytical work before optional presentation generation.
Use the existing selected provider through Rust with a bounded generation
budget and a compact manifest of reusable results. Do not put the entire
workspace or large result arrays into the presentation prompt.

A presentation request may add a model call. Record its tokens, time, model,
and failures separately from analytical execution. Allow at most one bounded
repair attempt in the initial policy; fall back after failure. Freeze this
policy before comparing candidates. Simple answers need not incur it.

Stream incomplete presentation nodes as provisional, non-authoritative UI;
disable consequential actions while incomplete. Commit the final program only
after syntax/binding validation. Preserve the saved analytical answer if
streaming stops or generation fails. A presentation failure does not change a
Question's coverage to Partial or invalidate unrelated evidence.

Reopening uses saved output, not regeneration. A presentation-only edit may
need a model call but not a new source computation. Presentation repair must
not rerun analysis as a hidden recovery step. A future same-turn optimization
can be evaluated after the separated path establishes a baseline.

### Generated actions and custom-code isolation

Application-owned handlers distinguish local view changes, artifact actions,
and requests for a new analytical run. Validate targets, current workspace,
base revisions, payload size, and permitted action types; keep cancellation
and late events scoped to their owning operation. No action receives shell
access, arbitrary IPC, file access, model credentials, or a network proxy.

Level C needs an opaque-origin isolation boundary, restrictive CSP, no remote
dependencies, a small typed message channel, bounded payloads, time/memory
limits, cleanup on close, and accessible/exportable fallbacks. Test external
requests and abuse through generated markup, URLs, messages, and nested
frames. A screenshot fallback does not make executable content safe or prove
its numbers. Exact bound data and provenance remain independently inspectable.

## 6. Canonical objects and storage

Extend existing persistence rather than introduce a parallel run store.

| Object | Responsibility and proposed contents |
| --- | --- |
| AnalysisTurn, Evidence, Finding, Definition | Existing authoritative records; do not replace or duplicate their ownership |
| Artifact | Stable ID, workspace, human title, originating references, adopted revision pointer, lifecycle metadata |
| Artifact revision | Immutable revision ID, parent, producing run/evidence references, result bindings, definition/Finding references, OpenUI program/version, verification references, change metadata |
| Result reference | Existing run/evidence location or a durable payload reference; explicit schema, completeness, row count, types, and provenance |
| Studio document | Durable workspace-owned identity, title, ordered placements, user narrative, document revision and layout preferences |
| Placement | Artifact ID plus pinned revision, local display state, size/order; moving a placement does not modify its artifact |
| Report snapshot | Pinned artifact/definition/Finding revisions, ordered content, exported interaction state, renderer/export versions, attribution, payload/asset manifest |

Query/code and the original question remain authoritative in their producing
run; artifacts reference them. Evidence IDs are namespaced by turn. A
presentation revision can reuse the same result bindings; an analytical
revision points to a new run. Avoid treating every text node as an artifact or
building an untyped graph that owns all analytical concepts.

Recommended initial persistence is Rust-owned atomic records alongside the
current app-data stores, with a small rebuildable workspace/reference index.
Do not introduce a new service or database solely for artifacts. Extract large
result payloads once, with hashes/size checks and references, when required;
do not duplicate legacy payloads on migration, promotion, or every revision.
Keep user data confidential; hashes are not an authorization mechanism.

Use revision-aware writes for document edits and adopted pointers. A stale
tab cannot overwrite a newer revision; report the conflict and preserve both
drafts where necessary. Persist promotion idempotently, and serialize lifecycle
changes so a concurrent deletion cannot orphan a newly saved artifact.

### Retention and deletion

Recommended policy: deleting a conversation removes its transcript but retains
explicitly saved artifacts and the minimal supporting records they need.
The user approved this behavior on 2026-10-21. The current delete confirmation
discloses that saved Studio outputs, supporting analysis records, and revisions
remain in the workspace. A separate dependency-aware permanent-deletion path
for saved work is still needed. Workspace removal and permanent deletion must
disclose their effect; source folders remain untouched unless the user
explicitly authorizes otherwise.

Do not allow destructive operations to leave saved references broken or imply
that all saved work is removed when some is retained. Garbage collection must
start as an inspectable reference audit, not an aggressive background deletion
job. Preserve legacy archives and Projects; they are not silently converted
into or deleted in favor of Studio.

### Freshness and reproducibility

Separate source freshness, definition freshness, artifact revision selection,
and verification. An unrelated file change can conservatively mark freshness
unknown until dependency checks resolve it; it should not rewrite the result.
Use recorded source dependencies where available, and disclose unknown
lineage instead of promising precise invalidation.

The current catalog revision is a metadata-based freshness marker, not an
archived dataset or cryptographic content proof. Distinguish:

1. Reopen the exact saved result/presentation: supported by retained outputs.
2. Recompute using current source files: a new run with new lineage.
3. Recompute against historical inputs: only guaranteed when those inputs
   were retained or can be supplied and validated.

Add content hashes or opt-in snapshots only through a measured storage/privacy
design; do not copy entire workspaces implicitly. A changed source marks an
affected output potentially outdated, not automatically numerically wrong.

## 7. Adaptive Investigation and shared state

Use the existing Investigation as the durable work aggregate. Broader agentic
work may propose/edit a Question plan, perform runs, ask for a material user
decision, record Findings, and select artifacts. It is not a fixed checklist
or required sequence of screens. Supported partial work remains useful.

Keep one canonical Investigation conversation. Ordinary messages remain
ordinary messages; explicitly associated runs name their Question. Do not
combine all sibling Questions into the active request or silently claim that
a freeform follow-up completed a saved Question.

Keep view state separate from domain state: selected tab, artifact, revision,
conversation, Investigation section, scroll, and local filters. Tab switching
restores its own references and never rebinds one tab to another Investigation.
Opening a saved artifact need not switch conversations. Deliberately opening
another Investigation retains its existing conversation-switching contract.

Resume from persisted results and pending decisions after restart. Exact
mid-tool crash continuation is a separate capability; end-of-run persistence
and clarification support do not establish it. Interrupted operations must be
visible and restartable without pretending an unfinished computation passed.
Use a small work-state journal only where the adaptive multi-run workflow
actually needs recovery; do not create another private-reasoning event log.

## 8. Revisions, comparisons, and checkpoints

An analytical change targets an artifact and its base revision. Create a
candidate using the current harness; keep the adopted revision visible until
the change is ready to compare. Concurrent candidates remain distinct.

The review separates:

- Meaning: definitions, population, denominator, assumptions, units, time.
- Method: query/computation and declared transformations.
- Inputs: source selection and revisions.
- Results: exact before/after values, absolute/relative differences where valid.
- Impact: Findings, Studio placements, and reports referencing old work.

Deterministic comparisons require compatible units, grain, keys, periods, and
completeness. Changed definitions can make results intentionally different;
show that difference without implying identical semantics. Do not compare row
positions as entities, fill missing rows with zero implicitly, or infer causal
explanations from numerical deltas. When incompatible, show the two results
with the reason instead of manufacturing a delta.

Adopt changes the selected revision pointer. Keep previous leaves the prior
pointer unchanged and retains the candidate in history. Neither action upgrades
verification or removes caveats. Low-risk local layout changes require no
review modal. Reports pin versions and are not rewritten when a metric changes.

A checkpoint is a small named set of revision references and relevant
document/definition state, not a whole-workspace copy. Restoring appends a
restoration event or revision; it does not erase later history, edit source
files, or delete conversation messages. Preserve the original request.

## 9. Reporting and mdxcn

Export from the persisted presentation plus its resolved artifact bindings,
not model-written numerical prose or screenshot pixels. Use a transient
normalized traversal for export, not a second independently editable report
schema that duplicates analytical truth.

mdxcn is a source-distributed family of Markdown-friendly components and text
figures, not a universal OpenUI converter. Its Markdown, MDX, and
Comark/Knap routes differ. A Fella adapter is required. Reuse suitable
deterministic text-rendering helpers and conventions after license/dependency
review; do not embed React/MDX execution in the Svelte app just for export.

| Presentation content | Deterministic portable output |
| --- | --- |
| Narrative and headings | Escaped Markdown in reading order |
| Metrics and comparisons | Exact canonical values, units, context, and compatible deltas |
| Tables | Markdown table or disclosed data attachment for large results |
| Supported visualizations | mdxcn-compatible text figure plus exact values |
| Figure not faithfully representable as text | SVG/PNG snapshot with explanation, data, and attribution |
| Filters, selectors, hidden series, tabs | Explicit exported state; disclose omitted content and offer inclusion |
| Caveats, uncertainty, checks, provenance | Legible notes and stable references in the report/package |
| Unsupported component | Visible warning or explicit export failure; never silent omission |

Text charts approximate visual length/position; exact numeric tables remain
authoritative. Numeric values must not be recreated by the model or rounded
through the plotting layer. Include selected state and renderer/export
versions. Do not fetch remote fonts, images, code, or source data implicitly
when exporting/reopening.

Provide two clearly described outputs:

- Portable Markdown: readable without Fella; figures/data assets use relative
  links when included. State the limits of single-file portability.
- Reopenable report package: Markdown, manifest, pinned necessary result
  payloads and figures; reconstruct the saved analysis view in Fella without
  the originating chat or live sources. It need not contain raw source files.

The export preview must disclose included data and local paths that could be
sensitive. Reopening a package treats it as untrusted input, validates paths,
sizes, IDs, and versions, and cannot execute arbitrary MDX or generated code.
Content inspection and numerical export should not require a model connection.

## 10. Phased work packages and acceptance gates

The IDs below are planning identifiers, not newly created GitHub issues. Convert
approved near-term packages into buildable issues later. Phases are dependency
order, not calendar estimates; no release date is claimed. Parallelize only
across boundaries with agreed contracts, and prove one slice before broadening.

### Phase 0 — compatibility, contracts, and measurement

- **P0.1:** Freeze the result-binding, action, revision, fallback, and retention
  contracts and reconcile relevant outdated architecture/roadmap descriptions.
- **P0.2:** Spike native OpenUI Svelte streaming, nested composition, actions,
  state restore, validation, and broad visualization options. Compare actual
  installed packages, not React demo promises. Record bundle and dependencies.
- **P0.3:** Freeze representative fixtures, independent grading criteria,
  baseline hardware/build/provider settings, and performance/cost budgets.
- **P0.4:** Threat-model generated presentation, cross-workspace references,
  sensitive exports, and the separately gated custom visualization path.

Gate: a supported package/version combination and rendering strategy are
demonstrated, limitations are explicit, baseline results are preserved, and
the first slice has a test contract before its implementation. If Svelte is
inadequate, decide between a small maintainable adapter/upstream work and
deferring the unsupported behavior; do not silently force React into the app.

### Phase 1 — durable, data-bound OpenUI answers

- **P1.1:** Add minimal Rust-owned artifact/revision/presentation persistence,
  authorized result references, completeness/type metadata, and legacy adapters.
- **P1.2:** Register Fella-native composable components and the chosen data-bound
  visualization surface; generate matching prompt/schema assets.
- **P1.3:** Add optional Rust-controlled presentation generation, streaming,
  bounded repair, cancellation, and readable fallback to the existing answer.
- **P1.4:** Render stored rich answers in chat, persist relevant local state,
  expose exact values/evidence, and record separate presentation diagnostics.

Gate: at least two materially different compositions from the same verified
results; values reconcile without model transcription; invalid programs and
bindings retain a usable answer; reload makes no model call or source query;
ordinary chat, clarification, and legacy charts still work. This is an
incremental integration milestone, not completion of visualization freedom.

### Phase 2 — remove the visualization ceiling

- **P2.1:** Deliver broader mappings, composition, layers/facets/annotations,
  and linked displays beyond the existing chart enum.
- **P2.2:** Replace readability-only caps with adaptive layouts, selection,
  virtualization, or disclosed aggregation while retaining execution budgets.
- **P2.3:** Define/persist meaningful display transforms and interaction state;
  route new analytical calculations through existing Rust tools.
- **P2.4:** Establish the Level C prototype/security/export design and a go/no-go
  decision. Keep executable custom output disabled until the gate passes.

Gate: representative waterfall, small-multiple, annotated comparison, or other
new compositions cannot depend on adding a `ChartKind` for each example.
Cases above the former category/series limits remain useful without silent
truncation. Unsuitable mappings, missing values, and unsupported outputs are
honestly handled. Demo success alone does not establish chart correctness.

### Phase 3 — Studio and reusable analytical work

- **P3.1:** Implement the approved dependency-aware retention policy and
  workspace artifact discovery; ensure promotion is idempotent and atomic.
- **P3.2:** Add durable ordered Studio documents and pinned placements using
  the existing Tools tab/expand interaction.
- **P3.3:** Support selection, reorder, resize where useful, narrative editing,
  multiple documents, and revision-aware autosave without navigation modals.
- **P3.4:** Add explicit artifact references to conversations and targeted
  natural-language edits without widening implicit prior-history access.

Gate: save from chat, restart, open from the same workspace, arrange, reuse in
another conversation, and delete the source chat under the approved policy
without losing the artifact. Promotion, opening, and rearrangement cause no
model call or analytical rerun. Rapid tab switching cannot retarget actions.

### Phase 4 — analytical revisions, diffs, and checkpoints

- **P4.1:** Create candidate analytical and presentation revisions with explicit
  targets, base revisions, source/definition dependencies, and concurrency checks.
- **P4.2:** Compare meaning, method, inputs, and compatible result values; show
  downstream impact without duplicating results or claiming causal proof.
- **P4.3:** Add adopt/keep-previous/history interactions with provenance and
  unresolved checks intact; avoid approval fatigue for display-only changes.
- **P4.4:** Add small reference-based checkpoints and non-destructive restore.

Gate: "exclude refunded orders" produces a new run and comparison; formatting
does not. A concurrent edit cannot overwrite a revision. Incompatible grains
or units do not receive bogus deltas. Restore preserves intervening history,
the original request, and pinned report snapshots.

### Phase 5 — deterministic portable reporting

- **P5.1:** Traverse stored presentations and bindings in reading order with
  explicit adapters, export-state capture, and unsupported-node handling.
- **P5.2:** Implement Markdown/table/metric export and suitable local mdxcn
  text figures; maintain exact-value and precision coverage.
- **P5.3:** Add SVG/PNG fallback and a portable asset/payload manifest where
  text cannot preserve meaning; disclose limitations and sensitive contents.
- **P5.4:** Add report snapshots, package import validation, and offline reopen.

Gate: export needs zero model calls and zero source queries; values, units,
nulls, selections, caveats, and attribution match saved work. The Markdown is
readable outside Fella. A package reopens after moving/deleting its originating
chat and losing access to live source files, subject to its declared contents.
Unknown components do not disappear silently.

### Phase 6 — deeper Investigations, definitions, and intelligence

- **P6.1:** Extend the existing model-directed workflow to propose/adapt plans,
  run associated questions, continue after decisions, and recover interrupted
  multi-run work without imposing a rigid notebook checklist.
- **P6.2:** Strengthen reusable analytical definitions: scope, authority, units,
  population, grain, bindings, versioning, and observable calculation effects.
- **P6.3:** Add bounded, explicit retrieval of artifacts and document evidence
  across a workspace with dependency/freshness checks and conflict handling.
- **P6.4:** Connect definition/source changes to impacted outputs and suggested
  refreshes; preserve prior results and leave recomputation user-visible.

Gate: analogous BI and personal-information tasks work through the same runtime;
another Investigation's assumption cannot silently become global truth; old
prose is not new evidence; a changed definition shows its affected work and
never silently rewrites historical results. Resume behavior states exactly
what was recovered rather than claiming unsupported mid-tool replay.

### Phase 7 — optional custom rendering and release hardening

- **P7.1:** If approved after P2.4, implement isolated custom visualization with
  bounded verified inputs, no application authority, and explicit trust labels.
- **P7.2:** Provide independently inspectable exact values, accessible fallback,
  deterministic snapshot/export behavior where supported, and clear limits.
- **P7.3:** Complete packaged Windows/macOS/Linux lifecycle, offline, compact,
  keyboard, streaming, memory, cancellation, and hostile-content journeys.
- **P7.4:** Roll out gradually, retain legacy rendering for historical output,
  and remove obsolete new-generation paths only after measured coverage.

Gate: approved security tests show no access to the Electron bridge, filesystem,
credentials, shells, arbitrary network, or other workspaces. Closing tabs cleans
up workers/frames. Unsupported exact reproduction is disclosed. No claim of
arbitrary safe code execution follows solely from passing finite tests.

### Continuous gates, not a final cleanup phase

Every phase includes model compatibility, analytical regression, accessibility,
Fella-native visual review, startup/bundle/memory measurement, and test-integrity
reporting. Existing engine/source isolation remains a gate even for UI-only
work. Fix an underlying failure rather than weaken its assertion to fit output.

## 11. First vertical slice and its grading contract

The first thin end-to-end path through P1, a minimal P3 document, and basic P5
export is implemented. It is an explicit slice of those phases, not a claim
that all their packages or the whole product vision are complete. The contract
and test evidence are versioned in the delivery ledger.

Use a small, reviewable fixture with Q3/Q4 revenue, refunds, multiple categories,
missing values, and independently specified expected totals. Include a second
unrelated workspace to test isolation. The fixture vocabulary is test data,
not production routing logic.

1. Ask for a Q3/Q4 comparison. Existing tools inspect, compute, verify, and save.
2. Generate an OpenUI answer with narrative, bound metrics, a comparison
   visualization, and exact-value table; allow another valid arrangement.
3. Save the composed artifact into an ordered Studio document and reopen it
   after restart. No regeneration or source query is permitted for reopening.
4. Reorder/expand the saved output without changing the artifact's analytical
   meaning or source scope.
5. Export Markdown containing exact values, selected state, caveats, and
   provenance. Existing deterministic report primitives may serve this thin
   export; full mdxcn coverage is not required to prove the initial pipeline.
6. Inject malformed output, an invented result reference, cross-workspace
   reference, missing payload, and interrupted generation. Each retains a
   readable answer or explicit recoverable state, without unauthorized access.

Before executing the candidate, freeze fixture version, expected values,
acceptable presentations, invalid-input outcomes, and operation counters.
Grade analytical values, evidence, UI behavior, and visual suitability
separately; never require an exact component sequence to force a chosen chart.

This slice does not claim broad visualization freedom, full revisions, custom
code isolation, historical source replay, or complete Investigation autonomy.
The next slice is "exclude refunded orders and compare the result"; it adds
a targeted revision and compatible deterministic diff before generalizing.

## 12. Evaluation and performance plan

Reuse the existing [testing layers](TESTING.md),
[product evaluation framework](../bench/product-eval/README.md),
[FQA-Bench methodology](../bench/fqa-bench/methodology.md), and
[performance audit](testing/PERFORMANCE-AUDIT-2026-10-16.md).
New work needs versioned cases; existing scores do not establish OpenUI quality.

### Required fixture families

- Scalar/metric, comparison, long tables, categorical and temporal data,
  distributions, layered/faceted views, and mixed document/data findings.
- Negative values, zero versus null, currencies/decimals, large integers,
  duplicate labels, dates/time zones, empty results, uncertainty, and missingness.
- Complete versus sampled/truncated results; changing sources and definitions;
  mixed units/grains; failed checks and withheld/excluded evidence.
- Malformed/oversized/truncated OpenUI; unknown components; forged actions and
  references; hostile labels, document instructions, and imported packages.
- Reload, promotion, source-chat deletion, missing files, offline reopen,
  rapid switching, concurrent edits, cancelled generation, and late events.
- Keyboard-only use, screen-reader equivalents, dark/light themes, text zoom,
  compact tools panes, and expanded documents.

### Measurements and release decisions

| Dimension | Measurement |
| --- | --- |
| Analytical quality | Independent answer/coverage and evidence grading; compare against the unchanged harness on the same tasks/models |
| Presentation fidelity | Bound values and transformations reconcile with canonical results; missingness/units/uncertainty survive |
| Visual usefulness | Reviewed appropriateness, hierarchy, readability, and interaction outcomes; not only DOM presence |
| Model portability | First-attempt validity, repair frequency, fallback rate, quality, and failures across representative supported models |
| Cost | Analytical tokens/calls and presentation tokens/calls separately; cost per correctly completed task, not only tokens per UI |
| Latency | Cold/warm startup, workspace readiness, first answer, first rich content, completion, reopen, tab switch, export; p50/p95 |
| Resources | Initial versus lazy-loaded bundle, peak/retained memory, parser/render work, DOM size, CPU, payload storage and duplication |
| Integrity | Unauthorized-reference/action attempts, silent omissions, corrupted outputs, cancellation/cleanup, and version compatibility |

Freeze numerical startup/memory/latency budgets using P0 baselines before
candidate runs. This plan does not invent measured gains or a release-grade
score. Collect repeated comparable runs with identical hardware, fixtures,
provider/model settings, and build type; distinguish provider variation from
local overhead. Lazy-load presentation/Studio/export capability when feasible.

Hard behavioral gates independent of hardware:

- Reopen, promotion, layout, exact saved-value comparison, and deterministic
  export require no model calls. They require no live source queries; purely
  local deterministic processing of saved outputs is allowed.
- No silent value fabrication, preview-as-complete substitution, revision
  overwrite, cross-workspace binding, or unsupported-node omission.
- Invalid presentation cannot erase a supported saved analytical answer.
- All model/provider calls use existing user configuration and are accounted
  for; live evaluations require an authorized usage budget.

Report failures, exclusions, untested platforms, grader limitations, and any
approved expectation changes. A successful render is not an analytical pass;
an aesthetically pleasing screenshot is not evidence of reproducibility.

## 13. Decisions and non-goals

Direction established in the discussion: preserve the engine, remove the old
presentation ceiling, use shared durable artifacts, keep a simple chat-first
desktop, and deliver incrementally. The initial OpenUI/Studio/report slice is
implemented. It does not authorize generated code execution or unreviewed
changes to source/data access. The table distinguishes decisions already
implemented from remaining gates.

| Decision gate | Recommended default | Needed before |
| --- | --- | --- |
| Exact OpenUI/library/visualization dependency | Pinned native Svelte OpenUI integration and validated Vega-Lite surface are implemented; evaluate additional grammar/features before expanding | Broader P2 coverage and performance review |
| Saved-work retention and deletion wording | Retain explicitly saved outputs/supporting records and disclose them; a separate permanent-deletion path remains | Initial policy approved and implemented; permanent-deletion UX remains |
| Studio interaction | Ordered documents in existing Tools tabs with expansion; no new mandatory app mode | Initial P3 slice implemented; broader authoring/reuse remains |
| Custom executable presentation | Separate opt-in implementation only after security/export review; no app authority | Level C / P7 |
| Historical input retention | Saved results first; explicit opt-in source snapshots only when needed | Any claim of exact historical recomputation |
| Artifact storage | Rust app-data records and canonical saved-result references; explicit export for portability | Initial P1 persistence implemented; lifecycle/recovery hardening remains |

These choices do not block discussing or approving the first spike, but must
be resolved at their stated boundaries rather than guessed during rollout.

Not part of this plan by implication: connectors, hosted sharing, enterprise
governance, arbitrary source-file editing, shell access for the model, a new
database service, multi-agent orchestration, a React migration, a full notebook
kernel, a freeform dashboard builder, or replacing the analytics engine.
Do not resurrect retired Projects/environments to implement Studio, and do not
erase their saved user data as cleanup.

## 14. External research and known compatibility limits

Research checked 2026-10-22. Pin actual package versions and recheck before
upgrades; repository `main`, package publication, and documentation versions
can differ.

- [OpenUI Svelte API](https://www.openui.com/docs/api-reference/svelte-lang):
  supplies component registration, prompt generation, streaming rendering,
  simple actions, and form state. It documents missing expression/reactive
  query/action-plan features and the need to handle parser failures in the host.
- [Svelte package source](https://raw.githubusercontent.com/thesysdev/openui/main/packages/svelte-lang/package.json):
  inspected source declares `0.4.2`, Svelte 5 peers, and core `^0.4.1`.
  Do not combine it blindly with the React demo's 1.0 dependencies.
- [OpenUI Lang introduction](https://github.com/thesysdev/openui/blob/main/docs/content/docs/openui-lang/index.mdx)
  describes the line-oriented, positional language, model-facing component
  library prompt, and client-side composition. [The Svelte adapter README](https://github.com/thesysdev/openui/blob/main/packages/svelte-lang/README.md)
  documents `defineComponent`, `createLibrary`, prompt generation, parser,
  streaming renderer, actions, and Svelte 5 support. Its current README says
  adapter `0.4.x` targets Lang Core `0.4.x` / OpenUI Lang 0.5; Lang 1.0 support
  currently requires the React renderer. Fella pins `@openuidev/lang-core`
  `0.4.1` with `@openuidev/svelte-lang` `0.4.2` rather than mixing the React
  1.x surface into its Svelte application.
- [OpenUI 1.0 compatibility](https://www.openui.com/docs/openui-lang/whats-new):
  explains the language/runtime version distinction. The plan does not assume
  the Svelte package already runs all the demo's behaviors.
- [Open Intelligent UI](https://github.com/thesysdev/open-intelligent-ui) and
  [its component library](https://raw.githubusercontent.com/thesysdev/open-intelligent-ui/main/src/lib/library.ts):
  useful composition reference, not a drop-in Svelte application or a source
  of analytical authority. Its broad drawing/HTML capabilities need separate
  local-first and security decisions.
- [Open Intelligent UI prompt options](https://raw.githubusercontent.com/thesysdev/open-intelligent-ui/main/src/lib/prompt-options.ts)
  demonstrates the product pattern behind the expressive behavior: the model
  composes layout, content, data, and input primitives, while task-specific
  rules and local state/pure functions make controls useful. Its current
  library is React-based; the whole component set and query/action behavior are
  not a drop-in Svelte runtime.
- [OpenUI renderer source](https://raw.githubusercontent.com/thesysdev/openui/main/packages/svelte-lang/src/lib/Renderer.svelte):
  accumulated-response parsing and callback-driven state require measurement
  and app-owned persistence/fallback.
- [OpenUI telemetry](https://www.openui.com/docs/openui-lang/telemetry):
  documents default installation telemetry, optional runtime telemetry, and
  opt-outs. Audit the chosen version and preserve Fella's local-control policy.
- [mdxcn documentation](https://www.mdxcn.dev/docs),
  [registry](https://raw.githubusercontent.com/shadcn-labs/mdxcn/main/registry.json),
  and [ASCII filter source](https://github.com/shadcn-labs/mdxcn/blob/main/registry/default/graph-knap/filters.ts):
  Markdown/MDX/text figure conventions and deterministic drawing paths inform
  an adapter. They do not supply an arbitrary OpenUI converter.
- [Cursor agent/checkpoint documentation](https://cursor.com/docs/agent/overview):
  reference for reviewable work and restoration, not a requirement to clone an
  IDE or treat a checkpoint as permanent version history.

Fella uses the official Svelte adapter to define its component library and
generate prompts, and OpenUI Lang Core for parsing and expression evaluation.
A Fella-owned Svelte host evaluates supported local state and bounded display
functions, resolves data only from authorized saved analysis results, and
renders Fella components. This host provides Fella's result/provenance and
persistence boundary; presentation programs do not receive live query, shell,
filesystem, arbitrary HTML/JavaScript, network, or general engine-action
authority. Canonical persisted OpenUI calls are positional. A narrow adapter
accepts named props for registered Fella components when models emit them,
normalizes them before validation/persistence, and leaves already-positional
programs unchanged. Declarative Vega-Lite remains subject to Fella's binding and
resource limits; arbitrary executable/custom visual code stays disabled until
the separate Level C security/export gate passes.

## 15. Completion and stop conditions

Do not declare the plan delivered because OpenUI renders one attractive chart.
Completion means new work can escape the old chart ceiling, reuse trustworthy
saved results, survive its chat, support reviewable refinement, export without
recomputation, and remain legible and responsive in the desktop.

Stop and reassess a slice when it requires a parallel engine, a second source
of analytical truth, unreviewed executable authority, unjustified dependency
growth, weakened correctness checks, or a return to cluttered navigation.
Keep earlier functioning slices available behind a reversible rollout while
the next gate is evaluated. Do not use a fallback as an excuse to leave the
model permanently restricted to the old chart contract.
