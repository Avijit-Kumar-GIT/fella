# Report design and analytical handoff acceptance

The baseline behavior is defined before candidate execution:

1. A request to overlay monthly revenue lines for different years changes the
   common month axis and preserves the year grouping, analytical run, exact
   result rows, evidence, and unrelated Report outputs.
2. Presentation changes apply directly to the open Report, persist as a new
   presentation revision, and reopen without model calls or recomputation.
3. A calculation change starts the analytical agent automatically when the
   relevant output is identified. The user stays in the Report. An ambiguous
   target prompts for selection rather than revising unrelated outputs.
4. A candidate computation returns an inline analytical comparison. The old
   Report pin stays in effect until the user accepts the candidate. Acceptance
   changes the Report pin, preserving prior Report and artifact revisions.
5. Invalid result bindings, missing components, or stale Report versions are
   rejected. A failed model call or computation leaves the saved Report usable.

Focused checks:

- `cargo test --manifest-path backend/Cargo.toml --locked --lib report_design::tests`
- `node tests/unit/javascript/test-report-design.mjs`
- Live Electron + Rust + Gemma 31b, disposable profile, Northstar sample:
  the original overlay request followed by an explicit calculation change.

Task-set revision v1.1 corrects a newly written Rust fixture's Title property
from `text` to the existing catalog's required `content`, with user approval.
Title-change, preserved-pin, and unchanged-run assertions retain their intent.
The initial v1 run had two passing cases and one invalid-fixture failure.

## Validation record

- The revised Rust contract passed four cases. The JavaScript contract passed
  saved-run reuse, preserved results and other pins, invalid binding rejection,
  and stale-version rejection.
- Initial live attempts failed while parsing the model proposal. Conflicting
  answer-envelope instructions were removed from catalog context; fenced JSON
  normalization and a single bounded validation-repair attempt were added.
- A later live run passed the overlay request but failed the analytical path:
  the model described a formula without computing revised values. The agent
  now receives an explicit, source-validated Report revision task instruction.
- The subsequent Gemma 31b Electron run passed both paths. Overlaying the years
  preserved the original run and exact result rows. The analytical request
  executed SQL for 36 monthly rows, returned an inline comparison, preserved the
  old pin before acceptance, and saved the new pin after acceptance. The model
  marked the candidate `needs_review`; this validates the workflow, not the
  correctness of every generated query or claim.
- Live checks used disposable app data and the user's Northstar sample folder.
  They are development smoke checks, not an unchanged benchmark score.
- A subsequent dev reopen check blocked analysis, design-model, direct SQL,
  reindex and filtered-query operations. The saved Report reopened with a
  rendered canvas and its correct pinned-revision label without those calls.
