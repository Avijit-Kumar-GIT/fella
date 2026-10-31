# Reports framework

**Status:** Product acceptance criteria, October 2026. This document defines
the behavior Reports must provide; it is not an implementation-specific test
scorecard.

Reports are Fella's persistent, interactive analytical surfaces. Analysts use
conversation to create and revise them. Report viewers can explore them without
needing to understand queries, chart arguments, or data-type configuration.

## Terms and ownership

- An **app workspace** organizes the user's work in the Fella window. It owns a
  collection of Reports.
- A **mounted data workspace** is a selected local folder. It is a source and
  access boundary, not the owner or identity of a Report.
- A **Report** is a durable, user-organized artifact that may be created from
  Chat or an Investigation and contains references to saved analytical work.
- An **analytical artifact** is a verified or reviewable result tied to its
  producing run, source scope, and source revision.

Each app workspace can contain multiple independent Reports. Reports are not
owned by a Conversation or Investigation. Their originating conversations and
investigations remain provenance links. Deleting those records does not delete
Reports or explicitly retained supporting analysis.

## Modes and authority

- **Analyze** is the default mode for inspecting sources, defining the question,
  calculating, verifying, and saving analytical results.
- **Design** changes a Report's composition, layout, labels, and presentation
  using existing saved results. Presentation-only changes do not run analysis.
- A request that changes a metric, population, filter definition, grouping, or
  other analytical logic activates the analytical runtime, even when issued
  from Design. Fella explains the transition and records a new revision.
- If the requested change could mean either presentation or analysis and that
  ambiguity could materially affect the result, Fella asks a concise question.
- Both modes stay in the same Report and conversation. Switching modes or
  changing presentation does not reload the window or discard context.
- The model may choose how to present verified values. It cannot invent or
  change analytical values, execute queries, access files, or alter verification
  state through the presentation runtime.

## Durable Report model

A Report retains:

- Stable identity, title, app-workspace owner, and creation/update history.
- Layout and component definitions.
- Explicit component bindings to saved result/metric definitions and the
  deterministic computation needed to update those results.
- Source scope, source revisions, producing runs, evidence, and verification
  state for every bound result.
- Declared interactions, filter scopes, saved defaults, and current saved view
  state where applicable.
- A revision history that distinguishes presentation edits from analytical
  revisions.

Opening, reopening, arranging, or exporting a saved Report must not regenerate
its presentation or rerun its analysis. Source changes mark affected results as
potentially stale; they do not silently replace saved values.

## Interactive data contract

- A component that supports filtering binds to a verified query/metric contract
  and its source scope, not just the aggregate values currently drawn on screen.
- The binding retains enough information to compute valid filter combinations,
  including dimensions and grain. A displayed aggregate alone is not assumed to
  contain the data required for later filtering.
- Filter and cross-filter actions use deterministic, bounded analytical
  execution. Ordinary viewer interactions do not call a model.
- Every interaction is scoped explicitly: report-wide, component-local, or
  cross-filtered to named components. Unknown or incompatible fields are not
  silently applied.
- Fella handles dates/time zones, currencies, units, grain, missing values, and
  denominators. It asks when an unresolved choice can materially change the
  result.
- Interaction execution respects the source scope and read-only boundaries of
  the mounted data workspace associated with the result.

## Viewer state and interaction

Keep three kinds of state distinct:

1. **Saved Report:** composition, metric definitions, allowed filters, and
   default selections.
2. **Viewer state:** temporary filter selections and chart/table selections.
3. **Analytical revision:** a changed metric, population, calculation, or
   source scope.

Viewer selections do not silently modify the saved Report or metric definition.
They are transient while the Report is open; reopening restores its saved
defaults. An analyst can intentionally change those defaults in Design. The UI
makes active selections visible and allows them to be removed or reset.

Reports should remain visually calm until a user explores them:

- Show only useful, report-specific filters by default; keep secondary filters
  behind a simple Filters control.
- Hover and keyboard focus expose exact values and relevant comparisons.
  Touch interaction provides equivalent detail by tapping.
- Selecting a chart mark or table row filters only the declared related
  components. The active selection is visible and easy to clear.
- A Details view exposes the metric definition, source, evidence, and analysis
  behind a component without crowding the default canvas.
- Empty-result, unavailable-field, stale-data, and failed-computation states
  explain what happened and offer a clear recovery path.

## Authoring, revisions, and recovery

- Users can ask Fella to create, add, remove, arrange, filter, compare, or refine
  Report content in natural language. Direct selection and simple controls may
  supplement chat for quick edits.
- Selecting a component gives the prompt explicit context about the target.
- Presentation edits update in place without disturbing unrelated components.
- Analytical edits produce a candidate result and a reviewable diff. Preserve
  the previous result and Report revision; do not move a Report to the candidate
  silently.
- A diff distinguishes meaning, inputs, method, results, verification, and
  known Report impact. Withhold numeric deltas when result alignment is unsafe.
- A failed component must not make the entire Report disappear. Keep unaffected
  content visible and show a component-level error or safe fallback.
- Saving uses optimistic concurrency or an equivalent conflict check. Unsaved
  edits remain recoverable if persistence fails.

## Responsive, accessible, and local-first behavior

- Use Fella's shared component library, spacing, typography, and surface tokens.
  Avoid a grid of visually equal cards when the information has different
  importance.
- The Report fits compact desktop windows without page-level sideways scrolling.
  Secondary detail may collapse progressively, but source scope, active filters,
  stale state, and required review remain visible.
- All interactions are keyboard accessible and have visible focus, labels, and
  non-color status cues. Hover is never the only route to important data.
- Presentation changes affect neither data access nor analytical state.
  Analytical computation remains in the Rust engine and stays inside the
  explicitly mounted source scope.

## End-to-end acceptance slice

The initial complete experience must demonstrate:

1. An app workspace can contain and reopen at least two independent Reports.
2. A user can save a verified answer into a Report, retaining its exact result,
   metric definition, source lineage, and verification state.
3. In Design, the user can arrange and label saved results without invoking
   analytical computation.
4. Clicking or tapping a chart value updates declared linked components, shows
   the active filter, and can be reset. Keyboard focus exposes equivalent
   details.
5. Filter changes execute deterministically without a model call and preserve
   metric meaning.
6. Reopening the Report restores saved composition and bindings without
   regenerating the layout or rerunning the analysis.
7. Asking to change a calculation (for example, "exclude refunds") activates
   Analyze, creates a reviewable analytical revision, and updates dependent
   components only after the revision is adopted.
8. Deleting the originating Conversation or Investigation leaves the Report and
   its retained analytical records available.
9. A failure or stale source in one component does not conceal the rest of the
   Report, and the Report remains usable in a compact window.

Scheduled refresh, external sharing, and multi-page Report navigation are
follow-on product decisions; this framework does not require them for the
initial slice.

## Implementation status

This section describes the implementation on the `feat/reports-workspace`
branch. “Implemented” means the code path exists; it does not mean the full
end-to-end acceptance slice has been exercised in the desktop app.

### Implemented in this slice

- Reports are owned by the app workspace, and the picker can add saved results
  produced from any mounted folder. A Report's owner is distinct from each
  result's source scope. Legacy folder-owned Reports are migrated when loaded.
- Multiple Reports can be created and reopened. Saving a placement retains the
  referenced analytical record and pinned revision independently of its
  originating conversation.
- Opening the Northstar analyst sample idempotently creates one in-progress
  Report with saved, read-only results: a 2023–2025 monthly trend, Q3/Q4
  revenue metrics, product-category and refund-reason comparisons, and linked
  tables. Selectors and chart selections filter those saved results. The Report
  states the posted-refund date basis and that category values are before
  refunds; it does not claim those refunds can be allocated to products.
- Placements support labels and full/half widths. Layouts collapse to one
  column in narrower panes. Markdown and portable Report export retain the
  layout metadata.
- Analyze and Design are separate Report modes. The model-assisted Design
  endpoint receives Report structure and saved-output metadata only. It can
  propose a bounded set of layout operations; proposals are previews and only
  persist after the user applies them. Requests requiring analysis hand off to
  the existing conversation analysis flow.
- Saved-result presentations can declare interaction groups and linked fields.
  Chart selections, accessible table actions, and result filters share exact
  typed values across placements from the same source scope. When a result has
  saved SQL evidence, the Rust runtime can replay that read-only query with the
  selected value against the already-mounted matching source revision. Viewer
  selections are temporary; Design can explicitly save them as defaults.
- Presentation failures stay inside the affected output and preserve its
  answer and exact values. Reports show when the currently mounted source
  catalog revision differs from the revision recorded by a saved output.
- Report history stores compact snapshots of Report structure and references
  to analytical artifact revisions; it does not duplicate result datasets.
  History supports comparison and restoring an older Report version as a new
  version. Existing named checkpoints remain available.

### Implemented with limits

- SQL-backed cross-filtering replays the recorded query inside its existing
  mounted workspace, with exact typed values and source-selection checks. It is
  limited to eight linked results per presentation and 10,000 returned rows
  per replay. Dropdown choices come from the saved result rows; chart marks
  exposed by a replay can also be selected after Rust confirms the value against
  the saved source query. Python-derived results and unsupported queries fall
  back to saved rows with an explanation. Replay filters only the columns
  projected by the saved query; a structured metric/query contract is still
  needed to expose new dimensions and recompute aggregates from the full source.
- Model-directed Design supports Report naming, output labels, placement,
  order, and size. It does not author arbitrary new chart internals, visual
  encodings, or unrestricted component programs.
- Analytical changes continue through the existing conversation and artifact
  revision system. The Report stays pinned until the user selects and saves a
  newer artifact revision. Artifact comparison is available, but a single
  Report-level dependency diff that summarizes every affected output is not
  implemented.
- Source staleness can be compared with the catalog currently mounted in the
  app. There is no background refresh service or automatic recomputation.
- Report history is bounded to 100 snapshots and 8 MiB per Report. Restoring a
  snapshot reuses its artifact references and does not restore deleted source
  files or rerun analysis.

### Acceptance status and remaining work

The implementation covers the ownership and multi-Report foundation, saved
result composition, bounded viewer interactions, Design previews, and Report
history. The full acceptance slice above is **not yet verified end to end**.
In particular, interactive re-query from structured metric definitions, the
Report-level analytical review/adoption experience, deletion-retention behavior
across the full desktop flow, and compact-window/accessibility behavior need
dedicated product validation.

The next architectural slice should introduce a structured, Rust-owned metric
and query binding that can enumerate dimensions from a full source and
recompute aggregates, rather than filtering only the columns projected by a
saved SQL result. The next UX slice should make proposed analytical revisions
and their Report impact reviewable before changing any Report pin.
