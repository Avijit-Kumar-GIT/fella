# Backlog #5: Visualization implementation and validation

Date: 2026-10-04
Candidate: working tree on `feat/eval-replay-refs` (not committed)
Live model: `openai/gpt-5.6-luna`, OpenAI BYOK, one iteration per case
Frozen suite: `bench/visualization/cases.jsonl`
SHA-256: `cf92d192a17f9901ad5ac1205413a2b89014a9733a9f641dd8cb79dae2dd56b8`

Status: chart families and rendering paths are implemented. Validation is
qualified, not a clean acceptance: the untouched 25-case live run scored 20/25,
and five cases failed. Focused replays diagnosed and fixed general engine/tool
issues, but three benchmark outcomes still need human adjudication. No task or
gold was changed after observing model output.

## Implementation completed

The typed engine/UI path supports bar, line, pie, donut, scatter, histogram,
box plot, area, stacked area, heatmap, and forecast charts. Chart generation can
reuse SQL, Python-published tables, or forecast results. This pass also:

- preserves pre-binned histogram labels/counts instead of binning counts again;
- respects an explicit y/value field rather than adding numeric helper columns
  as unintended series;
- orders recognized monthly/date labels chronologically;
- verifies raw-observation operations and grounds figures from specialized
  scatter, box, heatmap, forecast, and histogram payloads;
- guides the model on box-plot grouping, histogram inputs, and forecast bands;
- avoids demanding a second backtest when a chart simply renders forecast
  values already present in a mounted table;
- provides SSR checks for chart marks, accessibility names, exact-value tables,
  light/dark graphical contrast, and responsive overflow.

## Evaluation integrity

The 25 tasks and expected outcomes were frozen before the first live model run.
The SHA-256 above remained unchanged for all runs. No case was removed,
excluded, weakened, or edited to match generated output. Diagnostic replays
were restricted to previously failing cases and are reported separately; they
are not combined into a post-fix suite score.

## Untouched 25-case live run (r1)

| Measure | Result |
| --- | ---: |
| Answer/chart gold | 20/25 |
| Runtime status | 6 `Verified`, 19 `NeedsReview` |
| Tool rounds | 80 |
| Failed tool calls | 9 |
| Prompt / completion tokens | 854,668 / 9,500 |
| Model wall time | 172 seconds |
| Estimated list-price cost | about $18.25 total |

The five gold failures were `viz-missing-value-fallback`,
`viz-histogram-distribution`, `viz-boxplot-spread`, `viz-area-time-series`,
and `viz-forecast-band-render`. The high `NeedsReview` count is a separate
runtime signal and is not hidden by answer-gold passes.

## Focused diagnosis and replays

| Case | Latest observed outcome | Diagnosis / remaining issue |
| --- | --- | --- |
| `viz-area-time-series` | Pass in r6 | Chronological month ordering and explicit `y_field` now produce only the requested revenue series. This replay had no failed tool calls. |
| `viz-forecast-band-render` | Pass in r5 | The final chart preserved observed/forecast/lower/upper values and did not invoke a second forecast/backtest. It recovered from one initially guessed, unavailable evidence ID. |
| `viz-histogram-distribution` | Still fails gold | The final chart contains the correct two intervals and counts (4 and 2), but the grader requires the exact label strings `10–25` and `25–40`; the model used equivalent explicit labels such as “10 to less than 25 ms” and “25 to 40 ms.” The task’s endpoint convention and label equivalence are not encoded clearly. Three intermediate tool attempts were rejected during the baseline replay. Failure preserved. |
| `viz-boxplot-spread` | Still fails gold | The model now groups A/B correctly and identifies the outlier. The runtime’s documented interpolated quartiles yield B Q1/Q3 = 4/8; the gold expects 3/9 (median-of-halves). The task does not state a quartile convention, while the existing unit test specifies interpolation. This is an adjudication issue, not a pass. Failure preserved. |
| `viz-missing-value-fallback` | Still fails gold | Fella produced a line chart with the missing observation represented as a real gap and no invented value. The question asks to keep the missing reading visible, but its gold requires `no_chart`. That appears inconsistent with the requested behavior; the recorded result remains a failure pending review. |

The r2–r6 focused runs are diagnostic only. No consolidated post-fix score is
claimed; the full 25-case suite was not rerun after the implementation fixes.

## Deterministic and UI validation

- `cargo test --all-targets --features eval --no-fail-fast`: all targets passed
  except `agent_loop`, which had 15 passed, 2 failed, 1 ignored. The two
  unchanged failures are `direct_data_calls_do_not_require_a_contract` and
  `unresolved_contract_defers_direct_data_tools_until_revised`; both expected
  `Verified` while runtime returned `NeedsReview`. No tests were changed.
- `cargo test --lib engine::analytics::`: 130 passed, 0 failed.
- `cargo test --test chart_tool`: 16 passed, 0 failed.
- `cargo test --lib engine::analytics::verify::tests`: 74 passed in a focused
  run before the final added cases; the final broader analytics run above
  includes the added tests.
- `pnpm build`: passed.
- `pnpm check`: passed with zero errors and warnings.
- `pnpm test:chart-renderer`: passed for all 11 chart kinds. Vite printed a
  non-fatal sandbox `EPERM` while binding its optional HMR WebSocket; renderer
  assertions completed successfully.
- `cargo fmt --check` and `git diff --check`: passed.

Automated SSR/theme checks are not a substitute for manually inspecting the
desktop WebView in light and dark mode. That visual review remains outstanding.

## Acceptance still open

The implementation now covers every prioritized chart family, but the live
benchmark is not an unqualified pass. Preserve the three gold failures above
for user adjudication; do not change them to fit the model. After the intended
quartile convention, interval-boundary wording, and missing-gap expectation
are reviewed, version any approved benchmark correction and run the same
suite under that new version. A full post-fix 25-case run and desktop light/dark
review are still needed before claiming backlog #5 fully accepted.

## Follow-up: transcript crash and repeated tool calls

After the Windows visual pass, a chart response crashed the transcript with
`Cannot read properties of undefined (reading 'length')`, while the same
request appeared to repeat many tool steps. Both problems were reproduced
without changing the benchmark or its golds:

- The renderer failed when chart metadata omitted empty `fields` and `filters`.
  Rust had been configured to omit those empty vectors, while the Svelte chart
  renderer assumed they were always present. The backend now serializes both
  arrays consistently, and the UI also tolerates older stored chart objects
  where either array is absent.
- Repeated tool calls in one model response were not deduplicated; cached calls
  on later responses still created evidence/UI steps and the loop kept
  advertising tools. Calls are now coalesced by operation arguments (excluding
  the descriptive `note`), cached-only retries are answered once with tools
  disabled, and duplicate results are not represented as new evidence. A
  semantic repair clears the memo so an invalidated query can genuinely rerun.

The new renderer and scripted agent-loop regression tests failed against the
old behavior, then passed after the fixes. Follow-up checks: 302 Rust library
tests passed; 16 chart-tool tests passed; the agent-loop target had 17 passed,
2 failed, and 1 ignored. The unchanged failures are
`direct_data_calls_do_not_require_a_contract` and
`unresolved_contract_defers_direct_data_tools_until_revised` (both expect
`Verified`, but runtime returns `NeedsReview`). `pnpm build`, `pnpm check`, and
the chart-renderer SSR checks passed. The SSR runner prints a non-fatal Vite
HMR socket `EPERM` in this sandbox. No post-fix live-model rerun or manual
Windows light/dark review has been performed yet.

## Follow-up: false join repair and superseded chart rendering

A subsequent visual test exposed a verifier-induced loop: a query correctly
aggregated one table by a dimension whose name also appeared in another table.
The schema prompt called matching column names "JOIN or align" hints, and a
separate lexical verifier treated a shared non-generic column mentioned in the
question as proof that a join was required. That warning triggered semantic
repairs, marked the valid query and its chart as superseded, and repeatedly
asked the model to rebuild them. The transcript UI then rendered every chart
record, including superseded chart evidence, so the user saw three charts.

The runtime no longer treats a shared column name as join intent; that alone
cannot prove a join is needed. Matching names are presented as candidate
relationships, and explicit grounded contract joins continue to be checked
against executed SQL. Repair guidance now reuses successful evidence and asks
for only the work needed to address the failed check. The answer view renders
charts and derives source labels only from successful evidence; superseded
chart records remain available in analysis details for audit.
The pre-existing lexical join-helper unit test was left unchanged, as required
by the evaluation policy, but its helper is now test-only; it no longer defines
runtime behavior and should be reviewed in the next test audit.

Regression coverage was defined before the implementation change. A scripted
two-table loop test requires one grouped query from the relevant table, one
bar chart sourced from that query, correct labels and values, no failed or
superseded evidence, and no extra semantic-repair round trip. An SSR UI test
supplies two superseded charts and one accepted chart and asserts exactly one
chart is visible. Both failed before the fix and pass afterward. Also passed:
303 Rust library tests, `pnpm check`, `pnpm build`, and the chart renderer
checks. The full agent-loop suite reports 18 passed, 2 failed, and 1 ignored;
the unchanged failures are `direct_data_calls_do_not_require_a_contract` and
`unresolved_contract_defers_direct_data_tools_until_revised` (both expect
`Verified` but runtime returns `NeedsReview`). The Vite SSR test prints a
non-fatal HMR socket `EPERM` in this sandbox. One BYOK diagnostic run of
`viz-category-bar` with `openai/gpt-5.6-luna` passed with the expected single
bar chart and values (Rent 3600, Groceries 350, Dining 180, Transport 60).
It used four tool calls: inspection, grouped SQL, one failed chart-source
reference (`evidence:latest` instead of `evidence-2`), and the corrected chart.
That recovered in one step; it is a single explicit-chart smoke, not evidence
of general model quality or multi-source behavior. Estimated usage cost was
about one cent. No benchmark cases or golds were changed.

A second BYOK diagnostic used the unchanged `viz-budget-join-two-series` case
with `openai/gpt-5.6-luna`. It passed the fixed answer and chart gold. The final
answer correctly reported Rent as $600 over budget and explicitly noted that
the available expense rows cover January–March, not a full year. The single
accepted two-series bar chart had labels Rent, Dining, Groceries, Transport;
actual values 3600, 180, 350, 60; annual-budget values 3000, 100, 300, 50.
The exact numeric chart payload and all 12 verification checks were inspected
from the persisted analysis trace; the chart cited the final SQL evidence.

This live run used 5 tool executions across 4 tool-call rounds: two table
inspections, two SQL executions, and one chart. There were no failed calls,
exact duplicate calls, or duplicate visible charts. However, the first compiled
SQL summed annual budget after joining it to expense rows, inflating budget
values (for example Rent 9000); the model then issued a second SQL query that
correctly kept one budget per category, and the final answer/chart used that
result. This is an incorrect intermediate result the current checks did not
reject. The extra query is observed overhead, though not a confirmed useless
call without trajectory-level attribution. The evaluator reports 4 rounds for
this case. Across the two post-fix live samples we have 4 and 5 tool
executions (mean 4.5, n=2); this small mixed-case mean is not a stable suite
average, and the first run's round count was not retained. In the reported
pre-fix user trace, the listed operations amount to 10 tool executions, of
which 6 are explicitly superseded; the second inspection is also likely
redundant. The reported 14+ UI steps are not interchangeable with tool-call
rounds.

A Windows manual rerun remains outstanding. No benchmark task or gold was
changed in either diagnostic.
