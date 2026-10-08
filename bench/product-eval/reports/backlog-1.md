# Backlog #1: Ask without a mounted workspace

Date: 2026-10-01

Historical path note: commands below record the repository layout at this
run; the Rust crate was later renamed from `src-tauri/` to `backend/`.

Candidate: working tree based on `c9ca8c1`; not committed.
Model: `openai/gpt-5.6-luna` through the configured OpenAI credentials.

This report covers the no-workspace route, conversation continuity across
workspace mounts, and the prompt contract. The implementation and narrow
cross-mount model smoke are complete. The smoke passed its frozen final-answer
screen, but runtime verification emitted semantic-binding warnings; this is
not a broad correctness or verified-runtime claim. Forecasting is allowed by
the current prompt contract. Its quality is separate backlog #4 work.

## Changes

- Fella's persona now covers general questions as well as file-backed analysis.
- Workspace-only instructions (SQL-only figures, inspect/plan/chart steps,
  file interpretation and directory guidance) are omitted when no workspace
  is open. The model receives no tools in that path, so it can answer directly.
- The no-workspace prompt permits stable model-knowledge answers, asks for a
  workspace only for file-dependent requests, directs mixed requests to answer
  the separable general part, and retains recent conversation context without
  treating old workspace claims as current evidence.
- Missing-file guidance explicitly asks the user to open or mount the relevant
  workspace. Current/source requests are told when no live research tool is
  available.
- In the mounted-workspace prompt, the blanket forecast refusal was replaced
  with guidance to treat forecasts as estimates and state method, assumptions,
  and uncertainty. This does not yet make the forecast path reliable.
- Conversation summaries now retain a per-turn workspace path and revision.
  Opening a different folder no longer clears every conversation's context;
  the model can still resolve references from earlier turns, while query and
  interpretation hints are only included for the exact currently mounted
  snapshot. Older or unknown scopes are labeled as non-evidence.
- Archive hydration now restores the same bounded context after a restart,
  including general-only conversations when no workspace is mounted. Per-turn
  answer snapshots, not the archive's single conversation-level workspace tag,
  determine whether analytical hints are current.
- The workspace prompt explicitly treats prior assistant prose/results as
  conversational context, not evidence. Same-snapshot SQL is labeled as a hint
  to re-run, never as a result to trust without current execution.

## Test integrity and results

The three Ask-routing cases in `../ask-routing/cases.jsonl` were written from
the roadmap acceptance behavior before their live candidate run. Their SHA-256
is `b894110f1f025d58632af2f33d930e60556aa195c97c704174e61e3404e9a47e`.
Their questions, references, and golds were not changed after seeing outputs.

| Run | Local-data request | Mixed general + local | Contextual follow-up | Result |
| --- | --- | --- | --- | --- |
| Initial, 1 iteration/case | Pass | Pass | Pass | 3/3 |
| Repeat, 1 iteration/case | Pass | **Fail** | Pass | 2/3 |
| After clarifying the general missing-folder instruction, 3 iterations/case | 3/3 | 3/3 | 3/3 | 3/3 majority; 9/9 phrase checks |
| Direct OpenAI smoke after the context change, 1 iteration/case | Pass | Pass | Pass | 3/3; 3/3 phrase checks |

The failed mixed response correctly explained year-over-year comparison and
said local rent records were unavailable, but did not explicitly offer opening
or mounting a folder. That remained a failure. The prompt was then clarified at
the general behavioral level: when any part needs local files, explicitly
offer opening/mounting the workspace; a manual-data alternative may accompany
but not replace it. The fixed suite was rerun unchanged. The final 3-iteration
result is encouraging but small and prompt-iterated, not evidence of broad
reliability.

The final responses used zero tools in all three cases. The local-data request
asked the user to mount records without returning a personal total. The mixed
responses separated the general definition from the unavailable personal
comparison; illustrative numbers, when present, were labeled as examples. The
follow-up consistently resolved “the latter” to the median from the setup turn.

The direct OpenAI smoke run used `openai/gpt-5.6-luna` and scored the fixed
three-case routing suite 3/3 once. It is a repeatability check, not a quality
estimate. The previous three-iteration routing result remains the better, though
still small, repeated sample.

The separately frozen `conversation-lifecycle/` case exercises one real
conversation across the transition: a general median/mean explanation with no
workspace, mounting `readings.csv`, a local median/mean calculation, and a
contextual follow-up. Its case file SHA-256 before the candidate run was
`b42d4dd6bb3e5b10373fff240abd77b5204ad883839c28655edd8f3bc4ba3ecf`; the
fixture SHA-256 was
`03a615bd49021d1db0a57efdfb7f3ea2c1b25334a22e41d2f3dc675d4ee9b4f3`. The
criteria and fixture were not changed after the run. The one direct-OpenAI
iteration returned the expected median (17), mean (2,013.6), and maximum
(10,000), with a correct explanation of outlier resistance. It passed the
frozen final-answer screen (1/1); setup turns returned non-empty answers.
However, the engine's verification output flagged missing semantic bindings
and unsupported median/mean/max operations. Therefore the answer screen passed,
but runtime verification did not cleanly validate the computation. This is a
single synthetic smoke, not evidence of broad reliability. The runner currently
grades only the final response and requires setup turns to be non-empty and
error-free; inspect their printed text for semantic quality.

Three new deterministic state tests cover (1) same-conversation continuity
from no workspace into a folder and across a folder switch, (2) suppressing
query hints when either the path or revision changes, and (3) restart hydration
from a transcript containing general, old-workspace, and current-workspace
turns. A fourth assertion verifies that a general conversation hydrates with no
folder open. These test context assembly and source scoping; they do not claim
the model's answer quality across a live mount transition.

The separate `general-knowledge/` suite returned 3/3 phrase checks over three
iterations after the prompt change, at about 319 prompt tokens and 3.3 seconds
mean latency per answer. Its rubric was broadened after exploratory outputs in
the earlier backlog-0 work, so that score is post-hoc and is not an unbiased
quality measure. The pre-change prompt used about 1,974 input tokens per answer
on that same small set. Treat the token reduction as a diagnostic observation,
not evidence of improved answer quality.

### Legacy forecast refusal case — policy mismatch, not a current acceptance failure

The existing `bench/folder-qa/cases.jsonl` case `fqa-refusal-spend` was run
without editing its question or `gold: "refusal"`. The direct-OpenAI result
scored 0/1 against that gold: the model estimated `$430.68` for next month's
grocery bill using the average monthly spend from 2024. The executed SQL groups
the source by month in a subquery, then averages those monthly totals. The
verifier reported: `time bucket month was not used by the query (the executed
evidence did not carry the requested time grouping)`.

The refusal expectation is now known to conflict with product intent: Fella
should be allowed to forecast, with method and uncertainty disclosed. So this
0/1 is a historical result against a pre-release policy gold, not evidence that the
model should have refused and not a current product-acceptance failure. The
SQL/verifier trace is still worth a separate technical review, but it does not
establish whether `$430.68` is a good forecast. Backlog #4 owns forecast-quality
evaluation; its task and grading criteria must be defined independently of this
candidate output. At the time of this run, the old FQA case and gold were left
unchanged. The first target-state FQA-Bench v0.1 retires that pre-release
refusal gold from active scoring and preserves this 0/1 with its original
task-set provenance.
This run is not reported as a forecast pass.

## Validation

- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` — passed.
- `cargo test --locked --lib` — **247 passed** (the first sandboxed attempt had
  8 local-mock-server bind failures; the full rerun outside the sandbox passed).
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --features eval --example agent_eval` — **23 passed**.
- `git diff --check` — passed.
- At the time of this validation, no existing FQA-Bench or visualization
  expected answers were changed. The first target-state v0.1 task definition
  is a separately versioned corpus; it does not rewrite this historical run
  record.

## Follow-up limitations

- The older general-knowledge phrase rubric was broadened after exploratory
  outputs, so its score is not an unbiased baseline. Keep that caveat visible;
  broader general-knowledge quality needs independently reviewed frozen tasks.
- The old `fqa-refusal-spend` result remains a historical 0/1 against its
  legacy refusal gold, which conflicts with current intent. It is not a current
  forecast acceptance failure. Backlog #4 should assess forecast quality using
  preregistered targets and methods; audit its verifier trace separately
  without requiring refusal.
