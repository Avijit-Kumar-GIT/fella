# Intelligent UI contract: foundational slice

Status: foundational phase implemented. The acceptance below was frozen before
candidate execution; measured outcomes and retained failures follow at the end.
Frozen examples: [intelligent-ui-v1.json](../../tests/fixtures/intelligent-ui-v1.json),
fixture version `intelligent-ui-v1`. This slice adds a small authoring contract and
compiler over the existing catalog. It retains json-render and Fella's current
components, tokens, quiet surfaces, readable hierarchy, and responsive panes.
It is not the complete exploratory UI roadmap.

## Grounding and limits of the inspected baseline

- [Architecture](../architecture/ARCHITECTURE.md) separates presentation from
  Rust-owned execution, evidence, scope, and verification. Presentation failure
  must preserve the canonical answer and evidence.
- [schemas.ts](../../src/lib/presentation/schemas.ts),
  [manifest.ts](../../src/lib/presentation/manifest.ts), and
  [the catalog generator](../../scripts/generate-presentation-catalog.mjs) define
  component props and generate the Rust catalog. `Chart.spec` currently has a
  broad record schema; [chart.ts](../../src/lib/presentation/chart.ts) performs
  additional safety, field, composition, and budget checks.
- [agent.rs](../../backend/src/engine/agent.rs), specifically
  `presentation_composition_needed` and `question_needs_time_chart`, currently
  uses multi-row results and question/column keyword heuristics to request
  composition and charts. These are observed routing heuristics, not proof that
  a view fits a question. `presentation_sample` also reports row-count-based
  completeness; [artifacts.rs](../../backend/src/engine/artifacts.rs)
  `eligible_results` distinguishes saved analytical tables from inspection
  previews. Saved-result completeness does not establish source coverage.
- [validate.ts](../../src/lib/presentation/validate.ts) and `validate_spec` in
  `artifacts.rs` validate through separate paths today. Their acceptance is not
  assumed equivalent. [AnalyticalPresentation.svelte](../../src/lib/components/presentation/AnalyticalPresentation.svelte)
  uses `CatalogRenderer`; [runtime.ts](../../src/lib/presentation/runtime.ts)
  allows display state and explicit follow-ups. Existing linked filters can
  request a host-controlled saved SQL replay; the new `filters` below only
  select saved rows and must not activate that path.

These statements describe source inspection before the parallel implementation,
not measured behavior or benchmark results. Existing “verified” wording in
prompts does not certify presentation quality, business meaning, or full
coverage. The new contract cannot promote a run's verification status. Intent
is optional model-authored guidance, not a verified classifier or a keyword
replacement that dictates one layout.

## Author and canonical forms

The author envelope is a strict JSON object:

```text
{
  version: 'fella-ui-1',
  intent?: 'lookup'|'comparison'|'trend'|'distribution'|'relationship'|
           'exploration'|'explanation',
  capabilities: ['display'] or ['display', 'follow_up'],
  state: { KEY: StateDeclaration, ... },
  root: Node
}
StateDeclaration =
  {type:'enum', options:[{value:string,label:string}, ...],
   default:string, persistence:'viewer'} |
  {type:'boolean', default:boolean, persistence:'viewer'}
Node = {
  type: ExistingComponent, id?: StableKey, props?: NormalComponentProps,
  children?: Node[], bind?: {state:KEY},
  when?: {state:KEY, equals:string|boolean},
  filters?: [{state:KEY, field:SavedColumn}, ...]
}
```

`root.type` must be `Document`; omitted props/children normalize to `{}`/`[]`.
Enum values are unique and the default must be one of them. Booleans stay
booleans; no string/number coercion. Only `persistence:'viewer'` is supported.
Keys and IDs follow the existing bounded safe-identifier rules; reject unsafe
object keys, unknown fields/types, duplicate IDs, and existing budget overruns.

`bind` is for Select, SegmentedControl, and Checkbox. The first two require enum
state; Checkbox requires boolean state. Authors supply the usual accessible
`name` or `label`; the harness derives `stateKey`, `options`, and
`defaultValue`/`defaultChecked` from the declaration. Conflicting manually
supplied derived props are invalid. SegmentedControl retains its existing
two-to-six option limit; other current component limits still apply.

`when` names declared state and an in-domain value of the matching type. It
controls visibility only. `filters` is allowed only on Chart/Table, refers to
declared state and a column of that component's saved result, and compares
canonical exact saved keys without numeric coercion. Multiple filters combine with AND and
retain source row order. The reserved enum value `$all` explicitly requests all saved rows; it is never
inferred from a display label. No query, transform, new result, or rewritten
exact value is implied; zero matches is an honest empty
view. Filter before chart encoding/aggregation. Source rows and `exact_rows`
remain unchanged and result/coverage limitations remain visible.

Every authored display control must have a real visible or filter consumer:
`when` on content other than the control itself, or `filters` on a visible
Chart/Table. Reject an orphan control even if its declaration/default is valid.
A nominal reference on a permanently unreachable branch does not demonstrate
an effect. Tabs has its own panel visibility consumers; existing ResultFilter
must affect a compatible displayed result through its existing mechanism.
Buttons that set state must target declared, type-valid values with a consumer.
FollowUp and follow-up Buttons have an explicit request effect instead.

For author Tabs, children are TabPanel nodes whose `props.label` is a nonempty
string. The harness derives parent `labels`, a shared stable `name` from the
Tabs identity, and each panel's zero-based `index` in child order. The label is
author syntax, not a new component. Reject detached panels, non-panel children,
and conflicting derived props. Initial selection is the first panel; clicks
and existing arrow/Home/End keyboard behavior select exactly one panel.

Generated node IDs are deterministic for the same structural path; they are
not promised stable after sibling insertion/reordering. Explicit IDs preserve
identity across revisions and must not collide with explicit or generated IDs.
Do not silently rename duplicates. Compiler output remains the usual flat
`{root,elements}` with child-key references, rendered by json-render.
`elements[root].type` is Document and its `props.contract` carries
`{version,capabilities,state,intent?}`. State consumers must survive save/reopen;
their internal lowering is a harness detail, not a model-authored expression
language. Revalidating canonical output must enforce the same declarations and
consumers as author input, rather than trusting the metadata marker.

Legacy `fella-json-render-v1` flat inputs without new contract metadata remain
accepted under their existing safety/binding rules. Do not retroactively apply
typed-state or orphan-control requirements to them. New metadata opts a flat
tree into the strict contract; malformed/unknown versions fail closed rather
than falling back to legacy. The fixture includes a legacy orphan selector to
make that distinction explicit. Legacy acceptance never grants new capability.

## Shared acceptance boundary

Use one small Rust contract implementation, built natively and as WASM, for
author parsing/compilation and canonical acceptance. Native persistence and
renderer preflight must execute that same implementation. Keep filesystem,
database, model, and UI dependencies outside this crate. TypeScript adapts the
accepted output to the existing registry and viewer state; it must not become
a second authority with different acceptance rules.

Generate author JSON Schema, canonical metadata schema, and the safe chart
schema from the same typed definitions used by acceptance. Keep the existing
component catalog; wire its normal prop definitions into the contract rather
than hand-maintaining another vocabulary. `Chart.spec` must expose the bounded
safe subset structurally, including unit views, layer, facet/spec, and concat
(including hconcat/vconcat), rather than an unrestricted object. Every field,
including tooltip and nested composition fields, must exist in the bound result.
Only host-injected saved rows supply chart data. Reject authored data/datasets,
loaders/URLs, transforms, expressions/signals/events, and unsupported properties
at any depth. Preserve current size/depth/view/facet budgets and inert pinned
schema metadata handling. Compiler-generated Vega internals are not author
permission to submit executable expressions.

Only `display` is mandatory; `follow_up` is the sole optional capability.
Reject `scenario`, `query`, `code`, `network`, or any unknown capability instead
of ignoring it. Workspace policy may further restrict accepted features.
Follow-up actions require the capability and an explicit user click (including
keyboard activation of the button). Rendering, opening, restoring state,
changing a selector, or switching a tab must emit no follow-up or analytical
operation. A follow-up is a new user request through the existing host path,
not authority embedded in the view.

## Delivery phases and predetermined acceptance

1. **Freeze contract and data.** Fix fixture v1 before candidate runs.
   The saved `sales` result has four rows, two months, two regions, and matching
   `exact_rows`; it contains no implied total or forecast. Expected validity
   and interactions are data, not observations from a candidate.
2. **Shared compiler and validator.** Both native and WASM must agree on every
   fixture's `expected.valid`, preserve explicit IDs, produce semantically
   equal canonical JSON (object key order ignored), derive control/Tabs props,
   retain metadata, and reaccept valid canonical output. Generated schema must
   admit the valid author/Chart shapes and exclude the specified unsafe forms;
   saved-field/domain checks remain semantic validation. Invalid input yields
   no accepted spec and an issue identifying the offending field/node. Exact
   error prose is not graded.
3. **Viewer integration.** Run the declared interaction sequences against the
   existing json-render registry. Metric reads row 0 revenue as exact `100`;
   trend shows both monthly regional series; bar compares regions within each
   month without inventing a total. Select changes displayed Table rows from
   North to South; SegmentedControl swaps visible content; Checkbox reveals and
   hides its consumer; Tabs derives labels/indices and shows one panel. Viewer
   reopen restores valid local choices; invalid saved state resolves to the
   declaration default. No display interaction mutates saved results, evidence,
   source scope, analytical state, or verification, or invokes queries, models,
   network, or source I/O. Viewer-state persistence remains local display state.
   Follow-up emits exactly one request per activation, none on mount/restore.
   Rejected presentations leave answer/evidence accessible with an honest
   presentation limitation. Preserve current caveat and incomplete-result UI.
4. **Report separately.** Publish native/WASM acceptance parity and interaction
   outcomes, including failures/unresolved observations. Measure compile,
   validate, WASM startup, first render, interaction, and spec size separately
   from model latency, model calls/tokens, and task/model quality. Record build,
   platform, dataset size, warm/cold setup, sample count, and distributions;
   this proposal asserts no latency target or performance improvement. A valid
   spec is not a quality score. Broader model comparisons need separately
   predetermined tasks/grades for correctness, grounding, view suitability,
   interaction usefulness, accessibility, and caveat visibility.

The JSON fixture contains direct `input` objects, shared `results`, and
`expected` assertions. Interaction steps use explicit node IDs; `row_indices`
refer to the original saved result, not a filtered index. `derived` describes
observable canonical relationships, not a second executable compiler.
Rejection `reason` is a grading explanation, not a required error code.
Safe layer/facet/concat and legacy cases supplement the required lookup, trend,
category, filter, Tabs, and invalid cases. This file and its fixture are
specifications only; no test run or acceptance result is claimed here.

Freeze v1 inputs and expectations together. Report all failures, exclusions,
grader limitations, and post-hoc changes; an ambiguous grade remains unresolved
pending user adjudication. A correction requires an approved, newly versioned
fixture and comparisons rerun on that version, not silent gold-answer tuning.
Future slices may introduce equations, diagrams, richer exploration, and typed
scenarios with explicit provenance and execution policy. None is a new catalog
addition or capability in this foundational slice.


## Delivery phases and remaining product choices

Phase one delivers the shared native/WASM contract, typed chart grammar, nested
compiler, typed display state and consumers, capability enforcement, canonical
persistence/reopen/export checks, and bounded repair. The existing 25 components
and visual tokens remain. There is no new analytical engine, React dependency,
or executable presentation language. The adapter is small because json-render's
[flat spec and registry](https://json-render.dev/docs) already provide the target
representation and [Svelte 5 renderer](https://github.com/vercel-labs/json-render/tree/main/packages/svelte).
Replacing that renderer would not solve Fella's contract mismatch.

Phase two should improve exploratory affordances using the same contract:
explicit units, ordering and analytical roles from trustworthy result metadata;
reference-linked detail, richer comparison, equations and diagrams where useful;
and larger frozen task sets with human assessment of clarity and suitability.
Acceptance requires useful controls, exact grounding, responsive and keyboard
usable views, visible limitations, and no increase in presentation authority.
Intent remains guidance rather than a mandatory layout template.

Phase three may add bounded scenarios. First decide supported input types,
allowed calculation operators, dependency/cycle limits, uncertainty treatment,
and how users accept an analytical follow-up or promote a hypothetical result.
Then persist assumptions and derivation provenance separately from observations;
label scenario output and snapshot it during export. No scenario capability,
math execution, query, network, or arbitrary IPC is enabled in phase one.
These choices do not block the foundational delivery.

## Validation record — 2026-11-01

Deterministic results for contract `155031c2543f` on Linux x64 / Node 24.20.0:

- All 28 frozen native/WASM acceptance cases agree, including 16 rejections.
  All 12 valid cases pass identity, metadata, canonical reopen, immutable-result,
  state/default/filter/visibility, and exact Markdown snapshot checks.
- Seven shared Rust tests pass (including frozen acceptance, chart resource and
  repeat-scope checks, and multiple-error repair diagnostics). All 46 focused
  agent tests pass. `pnpm check` reports zero errors/warnings; production build,
  release engine build, and strict Clippy pass.
- Browser contract **v1.1: 10/10 pass**: real Select/SegmentedControl/Checkbox
  behavior, viewer restoration, tab clicks and actual keyboard focus, explicit
  follow-up lifecycle, zero query-replay tripwire calls, and canvas rendering of
  trend/layer/facet/concat. The user approved separately reporting bundled asset
  loads while requiring same-origin GETs and zero non-asset authority calls.
- A fresh-cache Vite/Electron smoke with a disposable app profile restored the
  sample folder, seeded/opened its Report, and rendered three nonzero visible
  chart canvases without renderer/Vite/no-workspace errors. This validates the
  Linux dev path, not a Windows/macOS execution claim.
- All 12 accepted final live-chart render/export checks pass through the actual
  Vega adapter at 380 and 960px. These checks establish compilation/export, not
  a visual-quality score or complete narrow-window usability.

Performance for the final contract: WASM 1,202,192 bytes / 373,175 gzip bytes;
first compile+instantiate 2.53ms; first 28-case acceptance batch 28.51ms; native
batch including startup 29.82ms. Five warm batches and instantiations are
recorded by `test-intelligent-ui-contract.mjs`. These are one-process Node
measurements with uncontrolled OS/engine caches, not desktop startup gains.
The generated schema is sizable; provider schema/token overhead and cold asset
loading remain costs to monitor.

Real-model evaluation uses the unchanged six-task
[intelligent-ui-model-v1](../../tests/fixtures/intelligent-ui-model-v1.json)
fixture, SHA-256 `ec6ea60fa0929657978cd9afa52014b815435e867bf7ecf49dfccfdb6b4579ff`.
The initial run: Gemma 4 31B **6/6**, Luna **4/6** combined acceptance/suitability.
Luna's region control failed validation after its single repair; the monthly
bar chart failed the fixed heuristic. Inspection found the generation prompt
advertised derived canonical props excluded by authoring. The implementation
now lists author props and returns several validation errors together.

The single post-fix comparison under the same frozen rubric: Gemma **6/6**,
Luna **5/6**. All final candidates pass structural acceptance; first-pass
composition is **3/4 Gemma**, **2/4 Luna**, with remaining compositions accepted
after one repair. Both models pass syntax and missing-field repair challenges
in one real request. There are zero provider failures. Luna's monthly bars still
fail the suitability heuristic because it rejects `yearmonth`; the saved output
keeps January/February distinct and renders successfully. This remains a
**failed grade with a suspected grader limitation**, not an adjusted pass.
Narrative correctness, broad model reliability, and end-to-end analytical
accuracy are ungraded. Presentation acceptance is not evidence verification.

Failures and corrections retained:

- Initial native fixture runs failed on missing derived tab name and wrong
  label location. Compiler/schema were corrected; frozen fixtures unchanged.
- One agent-test compile failed after removal of a test-only size constant;
  the constant was restored. Two stale agent expectations failed before the
  user-approved contract-v2 expectation correction. The approved revision
  replaces old mandatory/row-count/English-chart and flat-field assumptions
  with optional answers, nested authoring, and capability checks. No frozen
  model expectations were changed.
- Browser v1.0 first could not launch because Chromium was not installed. Once
  installed, **6 passed / 4 failed** on a bundled font loaded when revealing
  hidden tables. The user-approved v1.1 distinction is explicit above; all row,
  state, keyboard, and action assertions remain intact.
- The focused artifact suite is **18 passed / 2 failed**. Both failures were
  reproduced independently from the untouched starting commit `ae2f3cb`:
  `first_json_render_open_removes_legacy_artifact_rows_and_their_studio_pins`
  and `invalid_studio_pins_do_not_partially_save_or_retain_artifacts`.
  Their workspace-ownership expectations remain unresolved and unchanged.
- A diagnostic-wording regression in artifact acceptance was corrected in
  implementation. Strict Clippy also caught an existing blank line after a
  test attribute; only comment/attribute placement changed.
- Parity-run infrastructure encountered a synchronous pipe timeout and stale
  native/WASM hashes; serial JSON-lines transport and rebuilding resolved these
  without expectation changes. Localhost permission failures required running
  browser/Electron checks outside the sandbox.

Raw model candidates, timings, tokens, and both run failures remain in temporary
reports under `/tmp/fella-intelligent-ui-model-v1-I9s8z5` and
`/tmp/fella-intelligent-ui-model-v1-IENzCx`. Browser traces and asset attachments
are under `/tmp/fella-intelligent-ui-contract-results`. These are local evidence,
not bundled or committed user data. Only the scoped checks above ran; unrelated
app suites were not used as a completion gate.
