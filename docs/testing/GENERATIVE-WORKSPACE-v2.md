# Generative workspace contract v2.1

Behavior contract for generated analytical presentations. Values are fixed
from fixtures, not inferred from model output or from the candidate renderer.
This document describes intended behavior; its automated test implementation
is not included in the public repository.

## Acceptance criteria

1. **Answer independent of presentation.** A workspace answer remains complete
   when no JSON view is useful or composition fails. Preserve the canonical
   answer independently of presentation validation; repeated tool calls get one
   plain-language, no-tools recovery attempt instead of terminating with a
   renderer error.
2. **Same-turn rich answer when useful.** When the model returns an interactive
   presentation, its canonical answer and complete JSON component tree come
   from the same analysis turn. The view is primary; there is no secondary
   “create view” action or presentation-proposal model call.
3. **Interactive local state.** A component such as Tabs changes the displayed
   panel in place. Its state is saved with the conversation and restored when
   reopening it without rerunning analysis.
4. **Correction without recomputation.** If the finalizer returns an
   unregistered component, the engine reports the specific contract failure
   and allows a finalizer-only correction. The source inspection and analysis
   run are not repeated.
5. **Result-bound rendering.** The frontend rejects missing result IDs and
   fields. It keeps the canonical answer and exact saved result values visible
   and does not provide execution authority to presentation components.
6. **Artifact and report integrity.** Artifacts retain immutable revision
   history and workspace ownership. Report conversion preserves exact saved
   values, source/evidence context, reading order, and selected local state.
7. **Portable reports.** Export and import preserve the report's saved records;
   imported snapshots are read-only and cannot query their original sources.
8. **No hidden work.** Opening, switching, revising, rendering, exporting, and
   reopening saved work do not call a model or rerun analytics unless the
   specific user action requests it.

This version adds the user-approved fail-open answer contract after model
evaluation showed that presentation-generation failures could erase or block a
useful answer.

## Independent value oracle

The contract's reference values include `Q3 = 10`, `Q4 = 20`, explicit zero
and null values, and identifiers beyond JavaScript's safe integer range.
Reports should preserve exact saved string values, not rounded display
numbers.

## Evaluation notes

The model benchmark task sets live under `bench/`. They evaluate model
behavior and are separate from the presentation contract described here.
Keep benchmark task versions and observed failures intact when reporting a
change.
