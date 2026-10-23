# Fella generative analytical workspace

Fella's core interaction is a conversation grounded in a user's local workspace.
The agent explores sources, plans and runs analysis through the existing Rust
engine, verifies the resulting evidence, and returns an answer as a structured,
interactive presentation. A presentation is not the analysis itself: it is a
safe view over a canonical answer and its saved results.

## Product model

- **Ask** is the default conversational entry point for a focused question.
- **Investigate** keeps a larger objective, its Questions, runs, findings, and
  unresolved coverage connected over time.
- **Studio** organizes saved analytical artifacts into reusable reports and
  workspaces. It does not own or rerun the analysis engine.

These surfaces share the same evidence, source revisions, and persisted
analytical artifacts. A conversation can be deleted without deleting
explicitly saved analytical work.

## Presentation architecture

The model's finalizer returns a `json-render` spec: a flat object containing a
root key and keyed component elements, with children represented by references.
Fella's component catalog defines each component's props and composition slots.
The generated catalog is consumed by both the Rust engine's finalizer contract
and the Svelte renderer, reducing schema drift.

The Svelte application renders the validated tree with `CatalogRenderer` and
Fella-owned components. Components cover narrative, metrics, tables, responsive
layout, locally interactive controls, and flexible Vega-Lite visualizations.
The model chooses the composition for the specific question and supplied
results; the catalog is a vocabulary, not a required template.

Analytical authority remains separate from presentation. Metrics, tables,
filters, and charts bind to saved result IDs and fields. The presentation cannot
fetch files, run SQL/Python, invoke application commands, author replacement
datasets, or change verification status. Rust validates the bounded tree and
persists the original JSON spec with the analysis record. The frontend validates
component props and result bindings before rendering. A rejected view falls
back to the canonical answer and exact saved results.

## Persistent analytical artifacts

An artifact is a reusable reference to a saved analysis run, its result data,
and a versioned presentation spec. Its identity survives conversation changes.
Each revised presentation is an immutable candidate; adopting it advances the
artifact's current revision without erasing pinned reports or history. Source
revision and evidence remain attached to the original run.

Portable report export is deterministic and uses persisted artifacts. It
converts supported components into Markdown, includes exact saved result values,
and discloses when a visual cannot be represented faithfully. Export does not
query source files or call a model.

## Current implementation status

### Implemented

- Same-turn model finalizer for the canonical answer and JSON presentation spec.
- Shared generated component catalog and Svelte `CatalogRenderer` integration.
- Fella components for structured narrative, responsive layouts, metrics,
  result tables, locally controlled tabs/filters, and Vega-Lite charts.
- Result binding, chart field validation, safe local state, and fallback to saved
  analytical values when a view cannot be displayed.
- Persistent versioned artifacts, workspace ownership checks, pinned Studio
  placements, checkpoints, immutable revisions, and chat-deletion retention.
- Deterministic Markdown report conversion and read-only portable report
  snapshots.

### Remaining product work

- Evaluate real-model chart choice and layout quality across a blind, diverse
  task set; measure correctness separately from presentation quality.
- Improve the richness of generated views without turning the catalog into a
  rigid list of chart templates or granting the renderer data authority.
- Complete analytical diffs for meaning, inputs, method, results, and downstream
  impact, with review and rollback of proposed revisions.
- Expand deterministic report conversion, including faithful snapshots for
  visualizations that cannot be rendered as text.
- Measure latency, bundle size, memory, and render cost for large saved results.

## Delivery principles

1. Keep the Rust analysis and verification runtime authoritative.
2. Keep presentation JSON declarative, bounded, and grounded in saved results.
3. Extend the shared catalog and existing artifact persistence rather than
   creating parallel component or analytical systems.
4. Treat model-produced layouts as answer-specific; evaluate across different
   question types instead of optimizing to one fixture.
5. Preserve the canonical answer, exact result values, evidence, and revisions
   when a presentation is unavailable or a report conversion is partial.
6. Validate focused component, renderer, persistence, export, desktop, and real
   model paths separately; never infer broad model quality from mock fixtures.

## Smallest end-to-end proof

In a mounted workspace, ask a question that requires a multi-period comparison.
Fella should inspect the local source, compute and verify the exact result,
return one same-turn answer containing a relevant chart and narrative, and save
that spec as a reusable artifact. Reopening the conversation must render the
saved view without a page reload or another model call. The user should be able
to promote it to Studio, review an immutable analytical revision, and export a
Markdown report that preserves exact values and discloses any visual loss.
