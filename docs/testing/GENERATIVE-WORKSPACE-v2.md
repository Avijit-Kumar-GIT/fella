# Generative workspace contract v2.0

Defined before running the migrated implementation. Values are fixed from the
local fixture, not inferred from model output or from the candidate renderer.
The deterministic suite uses real Electron and the Rust sidecar but a local
provider fixture; it makes no claim about real-model analytical quality.

## Acceptance criteria

1. **Same-turn primary answer.** A workspace answer returns canonical answer
   text and one complete JSON component tree from the same analysis turn. The
   generated view is primary; there is no secondary “create view” action or
   presentation-proposal model call.
2. **Interactive local state.** A component such as Tabs changes the displayed
   panel in place. Its state is saved with the conversation and restored when
   reopening it without rerunning analysis.
3. **Correction without recomputation.** If the finalizer returns an
   unregistered component, the engine reports the specific contract failure
   and allows a finalizer-only correction. The source inspection and analysis
   run are not repeated.
4. **Result-bound rendering.** The frontend rejects missing result IDs and
   fields. It keeps the canonical answer and exact saved result values visible
   and does not provide execution authority to presentation components.
5. **Artifact and report integrity.** Artifacts retain immutable revision
   history and workspace ownership. Report conversion preserves exact saved
   values, source/evidence context, reading order, and selected local state.
6. **Portable reports.** Export and import preserve the report's saved records;
   imported snapshots are read-only and cannot query their original sources.
7. **No hidden work.** Opening, switching, revising, rendering, exporting, and
   reopening saved work do not call a model or rerun analytics unless the
   specific user action requests it.

## Independent value oracle

The test fixture includes `Q3 = 10`, `Q4 = 20`, explicit zero and null values,
and identifiers beyond JavaScript's safe integer range. Export assertions use
the exact saved string values, not rounded display numbers.

## Real-model acceptance

`tests/e2e/json-render-answer.live.spec.mjs` exercises configured hosted models
against a local quarterly dataset. It checks for a same-turn JSON component
tree, a result-bound Chart for a multi-period comparison, successful rendering,
and persistence of the same spec. A passing case establishes only that task
and model run; it is not a general quality score. Failures remain visible and
must not be repaired by changing the expected output after observing it.

## Focused commands

- `pnpm test:json-render` — catalog, schema, renderer, binding, state, and export
  contract.
- `node scripts/test-presentation-layout.mjs` — cached synchronous render,
  responsive layout, chart mount, and horizontal-overflow check.
- `pnpm test:studio-report` — deterministic saved-report conversion.
- `pnpm test:e2e:generative` — Electron/Rust artifact, answer, persistence, and
  report journeys.
- `pnpm exec playwright test --config=playwright.live.config.mjs tests/e2e/json-render-answer.live.spec.mjs --workers=1 --retries=0`
  — opt-in real-model acceptance; requires credentials in the app auth file
  and a built Electron engine sidecar.

Failures, unsupported visual conversions, and model differences are reported
as such. Mock-provider results and real-model results are never combined into
one score.
