# Fella Product Capability Evaluation

This is the cross-route evaluation map for Fella's product direction. It is
separate from [FQA-Bench](../fqa-bench/README.md): FQA-Bench measures analysis
over mounted folders; this map tracks user requests that may need model
knowledge, web research, local analysis, or a combination. Keep family scores
separate. A high folder-QA score must not conceal a no-folder refusal or a
missing research route.

## Capability families

| Family | What it measures | Current fixtures / status |
| --- | --- | --- |
| General knowledge | Useful, accurate answers from model knowledge when no workspace is open and no external lookup is needed | `general-knowledge/`; three seed cases, smoke runs only; initial baseline rubric was revised after exploratory outputs |
| Ask routing without a workspace | Do not demand files for general questions; request a folder only for file-dependent parts; preserve follow-up context | `ask-routing/`; three fixed cases, one prior 3×3 repeated run and one newer direct-OpenAI smoke run |
| Conversation lifecycle | Carry context from a no-workspace answer through a real folder mount, local calculation, and same-thread follow-up | `conversation-lifecycle/`; one frozen synthetic smoke trajectory, 1 iteration; useful regression signal, not a quality estimate |
| Web research | Source selection, factual support, citations, freshness, and safe handling of untrusted pages | Not implemented; no live score until the product/privacy policy and web tool exist |
| Filesystem analytics | Folder discovery, interpretation, computation, follow-up, and evidence | FQA-Bench; development suites and one draft public-data anchor |
| Forecasting and scenarios | Distinguish observation from forecast/scenario; inspect history; evaluate method against a time-ordered baseline; communicate uncertainty | FQA-Bench v0.1 target-state contract permits estimates and scenarios; draft estimate/evidence-limit cases exist, but no broad forecast-quality claim is supported yet. |
| Visualization | Correct chart choice and faithful rendering of values, units, grouping, and missingness | `bench/visualization/`; component diagnostic, not yet a fully tagged end-to-end suite |
| Hybrid questions | Correctly combine local findings with general or sourced external context without leaking local data into a web query | Not implemented; add only after both local and web routes have independently validated baselines |

Clarification, conversation context, provenance, and tool proportionality are
cross-cutting dimensions. Report them within the relevant family instead of
pretending they are separate user-facing modes.

## Conversation lifecycle smoke case

`conversation-lifecycle/` defines a fixed Fella-only trajectory: answer a
general question without a workspace, mount a folder, calculate from its file,
then answer a follow-up in the same conversation. The final answer is graded
against values computed directly from the fixture; setup turns must at least
return non-empty, error-free responses and are printed for manual review. This
is a narrow lifecycle regression, not a forecast or broad accuracy benchmark.
Its JSONL criteria and fixture are frozen before the model run.

## Seed general-knowledge suite

`general-knowledge/cases.jsonl` uses the existing `agent_eval bench` runner.
Each case declares `workspace_scope: "none"`; the Fella condition gets a new
engine with no mounted folder, rather than an empty mounted folder or leftover
workspace from a prior test. The `bare` condition calls the same configured
model once without Fella's prompt/tools. This is a small diagnostic set, not a
representative benchmark.

The deterministic `contains` checks only screen out obvious misses. Review the
answers for factual accuracy, completeness, and whether unnecessary tools or
folder instructions appeared. Do not treat the phrase checks as a complete
semantic grader, and do not add production rules to satisfy individual cases.

The initial general-knowledge gold wording was broadened after exploratory
model outputs because literal checks missed plausible paraphrases. Preserve
that history: its pass rate is post-hoc and is not an unbiased benchmark
comparison. Freeze future criteria before candidate runs; report suspected
grader defects separately. The Ask-routing and lifecycle suites provide
separate fixed smoke/regression checks; neither should be generalized into a
claim of broad model knowledge or end-to-end correctness.

Run both conditions with the same model and copied app data, saving outputs
outside the repository:

```bash
cd src-tauri
AGENT_EVAL_DATA_DIR=/tmp/fella-eval \
  EVAL_SHOW_ANSWERS=1 cargo run --release --features eval --example agent_eval -- bench \
  --dir ../bench/product-eval/general-knowledge \
  --harness fella --iters 1 --models "openai/gpt-5.6-luna" \
  --json /tmp/fella-general-fella.json

AGENT_EVAL_DATA_DIR=/tmp/fella-eval \
  EVAL_SHOW_ANSWERS=1 cargo run --release --features eval --example agent_eval -- bench \
  --dir ../bench/product-eval/general-knowledge \
  --harness bare --iters 1 --models "openai/gpt-5.6-luna" \
  --json /tmp/fella-general-bare.json
```

Run several iterations for a quality comparison. A one-iteration pass is only
a smoke/baseline capture. Record code commit, exact provider/model, date,
settings, task-set hash, outputs, correctness review, latency, and usage. Keep
provider/network failures separate from wrong or refused answers. Do not point
the evaluator at the live app data directory.

## Baseline ledger

| Date / commit | Family | Model and condition | Result | Interpretation |
| --- | --- | --- | --- | --- |
| 2026-10-01 / `c9ca8c1` | General knowledge | `openai/gpt-5.6-luna`; same-model bare vs. Fella, no workspace; 3 cases × 3 iterations | Fella 3/3 cases by majority (9/9 rubric checks); bare 3/3 cases by majority (8/9 checks) | **Exploratory only:** phrase criteria were broadened after initial model outputs. Not an unbiased baseline. Initial Fella prompt used ~1,974 tokens/answer vs. bare ~81. |
| 2026-10-01 / working tree after backlog #1 | General knowledge | `openai/gpt-5.6-luna`; Fella, no workspace; same 3 cases × 3 iterations | 3/3 cases by majority (9/9 current phrase checks) | Prompt used ~319 tokens/answer; mean latency ~3.3s. Same post-hoc rubric, so diagnostic only—not proof of an accuracy lift. |
| 2026-10-01 / working tree after backlog #1 | Ask routing without a workspace | `openai/gpt-5.6-luna`; Fella; 3 cases × 3 iterations after a prompt clarification | 3/3 cases by majority (9/9 phrase checks); a preceding 1-iteration run was 2/3 | The preceding mixed response named missing local records but omitted the explicit folder action, so it failed unchanged criteria. The final repeated run passed; see [backlog #1 report](reports/backlog-1.md). This small, prompt-iterated sample is not a broad quality claim. |
| 2026-10-01 / working tree after context change | Ask routing without a workspace | `openai/gpt-5.6-luna`; direct OpenAI; Fella; 3 cases × 1 iteration | 3/3 | Current prompt smoke passed all fixed phrase checks. One run per case is not a quality estimate; full answers and caveats are in the [backlog #1 report](reports/backlog-1.md). |
| 2026-10-01 / working tree | Conversation lifecycle | `openai/gpt-5.6-luna`; direct OpenAI; Fella; synthetic general → mount → analysis → follow-up, 1 iteration | 1/1 final-answer numeric/phrase screen | Returned median 17, mean 2,013.6, and max 10,000 with a contextually correct explanation. The setup turns also returned answers. Runtime verification emitted semantic-binding/unsupported-operation warnings, so this is an answer-quality smoke pass, not a verified-runtime pass or broad reliability claim. Criteria hash: `b42d4dd6bb3e5b10373fff240abd77b5204ad883839c28655edd8f3bc4ba3ecf`. |
| 2026-10-06 / `b50fd4d` (runtime change `6f63965`) | Conversation lifecycle | `openai/gpt-5.6-luna`; direct OpenAI; unchanged synthetic general → mount → analysis → same-revision follow-up, 1 iteration | 1/1 final-answer screen; follow-up retrieved same-revision execution | Across all three recorded turns: 6 model calls, 4 tool operations, 0 failed/duplicate calls, 16.8s summed turn latency, 41,934 input + 481 output tokens. Both workspace turns were `NeedsReview` despite all listed numerical evidence checks passing. The ordinary `agent_eval` headline omits ungraded pre-mount/setup-turn usage; episode totals were summed from persisted per-turn traces. One smoke only; see [continuity report](reports/backlog-7-continuity-2026-10-06.md). Task hash unchanged: `b42d4dd6bb3e5b10373fff240abd77b5204ad883839c28655edd8f3bc4ba3ecf`. |
| 2026-10-01 / `c9ca8c1` | Filesystem analytics | `openai/gpt-5.6-luna`; Fella, three existing FQA cases, 1 iteration each | 2/3 | Direct rent total and finished-book average passed. The cross-file rent/budget comparison inspected both tables but did not execute a comparison. Synthetic benchmark fixtures; smoke sample only. |
| 2026-10-01 / `c9ca8c1` | Visualization | `openai/gpt-5.6-luna`; Fella, category bar and monthly line, 1 iteration each | 2/2 | Both chart payloads and requested summaries matched their gold values. Does not establish broader chart correctness. |
| 2026-10-01 audit | Web research | Current Fella | Not runnable | `tools.rs` has no web-search/page-read tool |
| 2026-10-01 / working tree after context change | Forecasting | `openai/gpt-5.6-luna`; direct OpenAI; Fella; pre-release draft case `fqa-refusal-spend`, 1 iteration | 0/1 against then-active `Gold::Refusal` (retired task-policy mismatch; not a current acceptance check) | Returned a $430.68 estimate from 2024 monthly averages. The SQL groups by month inside a subquery, while the verifier flags the requested monthly bucket as absent. This run cannot establish forecast quality; the trace raises a separate verifier/contract question. The historical result is unchanged. FQA-Bench v0.1 replaces the pre-release refusal gold with estimate and evidence-limit contracts. See [backlog #1 report](reports/backlog-1.md). |
| 2026-10-03 / working tree after initial backlog #4 implementation | Forecast + scenario | `openai/gpt-5.6-luna`; direct OpenAI; unchanged UCI bike tasks; 1 iteration each | 2/2 final-answer grades; both `needs_review` | Forecast point (87,442) and 2012 what-if (1,844,618.4) matched their golds, but the forecast answer omitted a backtest metric/uncertainty limitation, verifier warnings remained, and the high-horizon sandbox regression failed. Development smoke only; see [backlog #4 report](reports/backlog-4.md). |
| 2026-10-03 / working tree after backlog #4 iterations | Forecast + scenario | `openai/gpt-5.6-luna`; direct OpenAI; unchanged UCI bike tasks; 2 iterations each | Run 7: 2/2 final-answer grades; both runtime records `failed` | Forecast point and scenario matched the unchanged answer screen, but forecast still omitted backtest/uncertainty and verification failed. Subsequent verifier unit tests cover two observed false-positive label patterns; no live rerun yet. Development smoke only, not a quality estimate; see [backlog #4 report](reports/backlog-4.md). |
| 2026-10-04 / working tree after backlog #4 verification fixes | Forecast + scenario | `openai/gpt-5.6-luna`; direct OpenAI; unchanged UCI bike tasks; 2 iterations each | Latest forecast run 19 and scenario run 18: 2/2 final-answer grades each; both runtime records `verified` | The requested arithmetic-mean forecast returned 87,442 from Jan–Jun and reported 3 rolling-origin comparisons: mean MAE 62,165 versus last-value-naive MAE 26,489 (the requested method did worse); it withheld a useful error band with only 3 errors. Scenario returned 1,844,618.4 as a hypothetical result. Run 18 had 2 failed/superseded calls across its iterations; run 19 had none. The grader previously accepted a whole-number rounding error under its 0.5% tolerance, so these smokes are not a broad correctness claim. Tasks/golds unchanged; see [backlog #4 report](reports/backlog-4.md). |
| 2026-10-04 / working tree after backlog #5 loop fixes | Visualization | `openai/gpt-5.6-luna`; direct OpenAI; 4 frozen synthetic cases; 1 iteration | Final run r8: 4/4 answer/chart golds; 3/4 runtime `Verified`, with monthly line `NeedsReview` / ambiguous | Pie and bar values were generated correctly; each had one rejected guessed evidence ID before recovering from the tool's exact-ID hint. Mean total usage was about 33.6k tokens per case. One small smoke only; high context cost, the monthly ambiguous status, the 2 unresolved `agent_loop` test expectations, and the legacy chart error-text assertion remain open. Task SHA-256: `90efba5b5ec325452d2d30afbf12352d6102fc2aa20d5016d6f5cde5d152e1e2`. See [backlog #5 report](reports/backlog-5.md). |

The detailed, reproducible snapshot—including suite hashes, per-case tool
traces, answer summaries, and review caveats—is in
[`baselines/2026-10-01.md`](baselines/2026-10-01.md). The eval runner emits
aggregate JSON to the requested output path; keep raw traces outside the repo
unless their inputs are known to be synthetic/public and safe to retain.

## Admission rules for future suites

- Author tasks from user intent and analyst capability, before inspecting a
  candidate implementation's failure.
- Keep general knowledge, web research, local file analysis, forecast quality,
  and hybrid tasks separately identifiable and separately scoreable.
- Use synthetic controlled data for exact private-data ground truth and
  licensed, pinned public data for external variation. Do not imply that either
  is a sample of customer workflows.
- Have another reviewer verify answerability, acceptable interpretations,
  expected evidence, and grading rules before a task is promoted beyond
  development.
- Score correctness, coverage, unnecessary refusal/clarification, unsupported
  claims, citation support, chart/forecast validity, and cost separately.
- Preserve raw answers/traces securely; publish aggregate results only when
  they do not expose credentials, private inputs, or personal data.
