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

## Starting-point implementation notes

At the `origin/main` baseline recorded for this work, Fella already has saved
analytical artifacts, result revisions, Studio document checkpoints, Markdown
and portable snapshot export, and a JSON-render component library. The current
Studio document's `workspace` field is also used as its mounted-folder key;
placements assume their artifact belongs to that same folder. Report-level
query bindings and linked-dashboard filter execution are not yet a durable
contract. These are the main ownership and interaction gaps this framework
must close.
