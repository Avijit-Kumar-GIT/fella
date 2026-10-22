# Generative analytical workspace: acceptance contract v1

Defined 2026-10-21 before candidate implementation. Baseline: `eb60f75` on
`explore/composable-interface`. Scope follows
[the implementation plan](../OPENUI-ANALYTICAL-WORKSPACE-PLAN.md).

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

## Test layers and independent grading

- Rust tests: storage, canonical binding, scope, excluded/partial evidence,
  revisions, retention, document conflicts, comparisons, import, and rollback.
- JS/Svelte tests: real OpenUI parsing, bindings, visual-spec safety, exact
  numeric serialization, fallback, Markdown conversion, and state validation.
- Real Electron plus Rust: rich answer to Studio, edits, tab changes, restart,
  deterministic export, and chat deletion/retention, using disposable profiles.
- Provider evaluation: distinct opt-in stage. Freeze task versions and usage
  budget first; record first-attempt validity, repair/fallback rate, analytical
  correctness, presentation appropriateness, latency, and token cost separately.
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

- Slice 0: acceptance contracts and baseline recorded; implementation pending.
- Slice 1 (persistence): stable artifacts, immutable candidate revisions,
  optimistic adoption, canonical result bindings, Studio documents/checkpoints,
  and user-approved saved-work retention. `cargo test --manifest-path
  backend/Cargo.toml --lib artifacts --locked`: 17 passed. The backend worker
  additionally ran the eight existing analysis-store tests successfully.
  No existing expectations changed. No UI/model/platform claim is made by
  this library slice. A small rusqlite metadata store reuses the bundled
  SQLite dependency to atomically commit document pins and retention markers;
  canonical runs and result payloads remain in their existing store.
