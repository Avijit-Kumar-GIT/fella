# Backlog #4: Forecasting development evaluation

Date: 2026-10-04

Candidate: uncommitted working tree on `feat/eval-replay-refs`.
Model: `openai/gpt-5.6-luna`, through the configured OpenAI credentials.
Suite: `bench/fqa-bench/suites/uci-bike-sharing`.
The initial one-iteration snapshot below predates later runtime and
verification changes. Follow-up runs use the same frozen task set and grading
keys. Runs 8–19 continued the same diagnostic smoke without changing tasks,
golds, grading criteria, or exclusions; runs 18 and 19 are the latest scenario
and forecast traces respectively.

This initial two-task, one-iteration snapshot is a development smoke, not a
quality estimate or acceptance claim. The tasks and grading keys were not
changed. Their hashes for this run were:

- `tasks.jsonl`: `2c37a874011fa4aa3b313b4e9d01488712f1c760328afde605696d80366de302`
- `answer-keys.jsonl`: `c358ab45961c4ca3ba0af955662f84089465b2b31a7cbabe506df14fd52a6422`
- `cases.jsonl`: `aeaa0cca8b013e19c657b350c353863fe99e76d2397a0e3f0eebfa2daa69345a`

## Initial snapshot: run 3 (one iteration per task)

| Frozen task | Final-answer grade | Runtime status | Notes |
| --- | --- | --- | --- |
| `bike-forecast-july-2011-mean-baseline` | Pass; predicted July total `87,442` rentals from the Jan–Jun arithmetic mean | `needs_review`; interpretation `ambiguous`; 1 failed/superseded tool call; 11.4 s | The answer correctly labeled the estimate and stated its method and observed window. It did not state a rolling-origin metric or an uncertainty limitation. A Python tool call ran, but this evaluator output does not expose its source/stdout, so helper use and a completed backtest cannot be confirmed. |
| `bike-scenario-2012-ten-percent-lower` | Pass; scenario total `1,844,618.4` rentals | `needs_review`; interpretation `grounded`; 2 failed/superseded tool calls; 9.6 s | The answer clearly labeled the result as a scenario and distinguished it from an observed total. Verification complained that the user-provided `10%` assumption was absent from returned data evidence; that is a verifier/provenance limitation, not a changed gold or a reason to mark the answer verified. |

The unchanged final-answer screen passed 2/2, with deterministic closeness
0.80 on each. There was no independent judge and only one iteration per task.
This supports only that these two point answers were produced; it does not
establish forecast quality, calibrated uncertainty, or reliable verification.
The raw evaluator JSON was kept outside the repository at
`/tmp/fella-forecast-eval-run-3.json`.

## Follow-up frozen-task smokes (runs 4–7)

All follow-up runs used the same two tasks, answer keys, and grading
methodology. No task, gold, grader, or exclusion was changed after observing
candidate output.

| Run | Iterations | Answer screen | Runtime status | Observed result |
| --- | ---: | --- | --- | --- |
| 4 | 2 per task | Forecast 1/2; scenario 2/2 | Both needs-review; interpretation ambiguous | No explicit source selector in the contract; the scenario factor appeared as an unresolved data field. |
| 5 | 2 per task | Forecast rate 1/2; scenario 2/2 | Forecast needs-review/grounded; scenario failed/ambiguous | Source selection began grounding the forecast table, but filter/operation checks warned and the scenario contract still had an unresolved pseudo-field. |
| 6 | 2 per task | 2/2 | Both needs-review/grounded | Contracts selected sources and preserved the stated 10% scenario input. The forecast still omitted backtest evidence; scenario verification retained a numeric-input warning. |
| 7 | 2 per task | 2/2 | Both failed/grounded | Forecast stayed SQL-only and omitted backtest/uncertainty. Verification misread a formula input as the forecast result label and treated a year in prose as a column label. |

Run 7 did not establish a verified answer even though the unchanged answer
grader passed both outputs. Generic unit tests now cover those two false-label
patterns, but they were added after run 7; their effect on a live trace remains
unconfirmed. Raw evaluator outputs are outside the repository at
/tmp/fella-forecast-eval-run-4.json through
/tmp/fella-forecast-eval-run-7.json.

Across these runs the model repeatedly returned the correct arithmetic-mean
point forecast, but did not call rolling_origin_backtest or report its metric
and uncertainty. The scenario result was also repeatedly correct while runtime
verification failed or requested review. Prompt and contract changes alone
have not made the model reliably follow the forecast-evaluation instruction.

## Follow-up verification smokes (runs 8–19)

These remain two-task development diagnostics, with two iterations per task;
they are not a broad quality estimate. The answer screen, verification rules,
and frozen task/gold files were unchanged. Iteration is not an independent
sample, and the outputs below must not be read as a 100% accuracy claim.

| Latest trace | Final-answer screen | Runtime result | What it establishes / does not establish |
| --- | --- | --- | --- |
| Run 18, scenario | 2/2; scenario total `1,844,618.4` matched the frozen expected result | `verified`, grounded, accepted; `hard_fail=false`; 2 failed/superseded calls across the two iterations | Scenario assumption remained distinct from observed data; current verification no longer treats failed/superseded query rows as support. The extra calls show the route still sometimes revises work. |
| Run 19, forecast | 2/2; July point estimate `87,442` matched the frozen expected result | `verified`, accepted; 0 failed tool calls | The user explicitly requested the Jan–Jun arithmetic mean, so Fella retained that method. Across three one-month rolling origins, its MAE was 62,165 versus 26,489 for last-value naive; the requested method performed worse than this baseline, a comparison worth surfacing rather than silently switching estimators. With only three errors, Fella withheld a useful empirical error band and stated that limitation. |

The run-15 scenario trace exposed a grading weakness: an answer rounded
`1,844,618.4` to `1,844,619`, yet the existing numeric screen accepted it
under its 0.5% closeness tolerance. The benchmark was not changed. A separate
precision-aware evidence matcher was added so runtime verification checks the
precision actually displayed in the answer; the unchanged runner grade is
still not precise enough to catch every small rounding error by itself.
Run 17 also exposed a year-adjacency false positive in verifier labeling;
the generic year exclusion was added and the later run-18 trace passed. These
are disclosed post-run implementation changes, not post-hoc benchmark edits.

Latest raw JSON is outside the repository at
`/tmp/fella-forecast-eval-run-18.json` and
`/tmp/fella-forecast-eval-run-19.json`. The earlier run files 8–17 are also
retained there. Frozen task and answer-key hashes remain the values listed
above.

## Implementation and regression status

The candidate adds reusable, dependency-free Python forecast methods
(`naive`, `mean`, `drift`, `linear_trend`, and `seasonal_naive`), expanding
chronological backtests against a selected baseline, and empirical error bands
that are withheld when calibration history is too short or degenerate. The
forecast preamble is loaded only when generated Python references its API.
Prompt guidance asks the model to inspect temporal scope and data quality,
avoid leakage, report method and evidence-proportional uncertainty, keep
scenarios distinct, and provide a useful estimate rather than refusing merely
because the request concerns the future. Tool documentation distinguishes a
selected time range from a multi-period output bucket.

Checks on the current working tree:

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: **271 passed**.
- `cargo test --manifest-path src-tauri/Cargo.toml --test python_tool`:
  **15 passed**, including the 501-origin integration case (167.07 s).
- `cargo test --manifest-path src-tauri/Cargo.toml --test workspace`:
  **22 passed**.
- `cargo test --manifest-path python-sandbox/Cargo.toml`: **3 passed**.
- `pnpm check`: passed with 0 errors and 0 warnings.
- `git diff --check`: passed.

The separate `agent_loop` integration target is **failing**: 7 passed, 10
failed, and 1 was ignored. Nine failures expected `Verified` but received
`NeedsReview`; one sequential-context test asserts the old literal phrase
`Earlier in this conversation`, while the prompt now says
`Earlier conversation for reference resolution only`. These failures were
preserved and are not counted as passes or waived by this forecast work. They
indicate broader verification/prompt-test alignment still needs attention.

The 501-origin failure was observed before moving the numerical rolling-origin
loop into compiled Rust within the WASM guest. The preserved integration test
now passes; this resolves that specific execution-budget regression, not the
separate question of whether the model consistently chooses and reports a
backtest.

The latest live smokes are runs 18 and 19 above: both unchanged answer screens
passed 2/2 and both tasks reached runtime `verified`. The forecast now reports
its evaluation metric and explicitly withholds an empirical band because the
available rolling-origin errors are too few. This demonstrates the requested
behavior on these two frozen tasks only; it does not establish reliable
uncertainty calibration or general forecast quality. The unchanged answer
grader's rounding tolerance is a known limitation, and the broader
`agent_loop` suite remains red as described above.

## Status

Backlog #4's **implementation is complete for the currently supported
forecast and scenario path**: the unchanged frozen forecast and scenario tasks
both produced expected answers, method/scope and evaluation details were
surfaced, weak uncertainty was withheld rather than overstated, and both
runtime records were verified in the latest traces. This is a narrow
development acceptance check, not a claim of broad or best-in-class forecast
quality. Unsupported forecast shapes and broader uncertainty calibration
remain unestablished; the full product should continue to describe concrete
limitations and offer a useful alternative where the series cannot support
the requested method. The separate `agent_loop` failures remain open and are
not hidden by this scoped completion. No benchmark task, gold, criterion,
threshold, or exclusion was changed after observing candidate output.
