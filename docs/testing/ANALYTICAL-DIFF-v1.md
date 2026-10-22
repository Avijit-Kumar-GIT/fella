# Analytical diff contract v1.1

User-approved fixture corrections: keep a changed source revision consistent
with its producing trace, and capture the unchanged-artifact baseline after
report creation has intentionally marked it retained. Assertions are unchanged.
The initial v1 run remains recorded as 18 passed / 2 failed; v1.1 is a corrected
task set, not a relabelled untouched baseline.

This contract is fixed before implementation and focused test execution.
The existing numerical comparison assertions remain unchanged.

## Scope and acceptance criteria

1. A comparison retains its existing compatibility gate: canonical saved values,
   declared keys/grain, units, periods, source boundaries and complete results
   must establish safe alignment before any numerical delta is exposed. Null is
   not zero; exact integer deltas remain decimal strings; floats are labelled
   approximate. Incompatible comparisons contain no delta tables.
2. Add five human-readable dimensions, derived only from persisted records:
   meaning (question, recorded measure/definition/filter/assumption semantics),
   inputs (recorded source scope/selection and source revision), method (saved
   computations and presentation changes), results (compatible saved-value
   differences and verification), and impact (explicit consumers).
3. Missing contracts, definitions, source snapshots or method details are shown
   as unavailable/not recorded, never invented from answer prose. Changes in
   source revision describe retained values, not fresh execution or causation.
4. Revisions identify baseline/current/proposed/historical roles from artifact
   pointers only. Viewing or keeping a revision does not adopt, mutate, rerun,
   clear verification, move Studio pins or replace report snapshots.
5. Dependency discovery is workspace-scoped and deterministic. Report exact
   Studio placement/checkpoint references to the baseline revision; do not infer
   prose references, claim every consumer is indexed, or imply pinned records
   automatically update. Reports/Findings outside this dependency index are an
   explicit limitation. A comparison without dependency context says not scanned.
6. Forged/unvalidated resolved data and cross-workspace inputs expose no detailed
   semantic/method metadata or dependency identities. No model/network calls or
   live source queries are needed for comparison.
7. Shared AnalyticalDiff.svelte presents the five dimensions in a single clear
   reading order with compact responsive before/after pairs, collapsible method
   detail and scrollable exact result values. It uses Fella tokens/curvature;
   it is not five equal-weight cards or a general dashboard. Status is also
   conveyed in words, never color alone.
8. Adoption/keep/open-consumer actions are callbacks only and explicitly labelled;
   immutable candidate records remain intact. Busy/error states are host-owned.

## Focused validation

- Existing Rust artifact comparison tests plus new tests for five dimensions,
  missing metadata, untrusted records, presentation-only changes, exact/null
  values, known dependencies/pins and read-only behavior.
- Component validation is scoped to the new shared component and generated
  backend wire types. No broad app/desktop suite is run for this slice.
- Report failures and limitations; do not revise existing expectations to fit
  candidate output.

## Integration boundary

The existing compare(before, after) stays available. A new dependency-aware
backend entry point accepts the data directory and canonical resolved revisions.
Main wires that endpoint and ArtifactView; this slice does not edit ArtifactView.
Frontend types extend ArtifactComparison additively. The shared component never
invokes IPC or chooses a workspace/revision from global state.

## Validation record

- Initial Rust artifact-exchange run: 18 passed, 2 failed (fixture issues described
  above). Both failures remain part of the record.
- User-approved v1.1 setup: all 5 analytical-diff-filtered Rust tests passed.
- Shared diff UI: 8 passed, 0 failed; no model calls or execution in the fixture.
- Real Electron/Rust integration: candidate review from chat did not adopt it;
  explicit adoption changed the current pointer and left a report on revision 1.
- Production Rust release build passed. The broader artifact-exchange suite was
  not rerun after the fixture correction; only the 5 task-specific tests were.

Limits: numerical comparisons retain the existing compatibility gate. Dependency
discovery lists explicit report placements/checkpoints, not every Finding,
Question, prose citation or downloaded report. No Windows/macOS packaged run or
live-model analytical accuracy benchmark is implied by these checks.
