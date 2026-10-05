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
