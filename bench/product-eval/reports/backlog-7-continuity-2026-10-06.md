# Backlog #7 continuity smoke — 2026-10-06

## Run identity

- Code branch: `feat/eval-replay-refs`
- Branch head: `b50fd4d` (runtime implementation `6f63965`; the head also
  contains a documentation-only clarification)
- Provider/model: direct OpenAI, `openai/gpt-5.6-luna`
- Harness: Fella; one iteration; no bare-model comparison
- Frozen case: `lifecycle-general-mount-analysis-followup`
- Case SHA-256: `b42d4dd6bb3e5b10373fff240abd77b5204ad883839c28655edd8f3bc4ba3ecf`
- Fixture SHA-256: `03a615bd49021d1db0a57efdfb7f3ea2c1b25334a22e41d2f3dc675d4ee9b4f3`

The case and fixture were unchanged. The run used only the controlled five-row
`readings.csv` fixture, an isolated temporary app-data directory, and one live
model iteration. The temporary credential copy, app data, staging folder, and
raw result file were removed after extracting the measurements.

## Interaction trace

| Turn | Route / result | Model calls | Tools | Wall time | Tokens in / out |
| --- | --- | ---: | --- | ---: | ---: |
| General median/mean explanation, before mounting | `model_only`; non-empty answer; `insufficient_data` envelope (no local data to verify) | 1 | none | 3.726s | 466 / 154 |
| Median and mean from the mounted file | `workspace_ask`; answer returned; `needs_review` | 3 | `inspect_table` (50ms), `run_python` (279ms) | 9.152s | 24,296 / 159 |
| Follow-up asking which statistic resists the largest value | `workspace_ask`; final answer passed the frozen answer screen; `needs_review` | 2 | `read_prior_analysis` (0ms), `inspect_table` (50ms) | 3.890s | 17,172 / 168 |
| **Episode total** | **Final answer 1/1; setup turns were non-empty and error-free** | **6** | **4 successful operations; 0 failed or duplicate** | **16.768s** | **41,934 / 481** |

The follow-up record refers to the preceding analysis turn and carries the same
workspace revision. It reused prior execution evidence, then inspected the
current table for the largest value; it did not rerun SQL or Python. The final
answer contained the expected median (17), mean (2,013.6), and maximum (10,000),
and correctly identified the median as less affected by the outlier.

## What this says—and does not say

- **Continuity worked in this run:** the model selected
  `read_prior_analysis`, the runtime accepted the same-revision reference, and
  the relevant source was retained. The extra table inspection was reasonable
  for finding the maximum, although a SQL aggregate could also have answered
  that part.
- **Tool work was small compared with model work:** four local tool operations
  took a cumulative 379ms; the three agent turns took 16.768s. The model made
  six provider calls and consumed 42,415 total input/output tokens. Most of the
  observed cost was model-side context and round trips, not computation.
- **Answer grading and runtime status still disagree:** the final answer
  passed the frozen phrase/value screen, while both mounted-data turns were
  `NeedsReview`. Their recorded numerical verification checks were all `ok`,
  so the status is not explained by a failed numeric check. Treat this as an
  interpretation/verification gap to investigate, not as proof that the answer
  was wrong or that `NeedsReview` is a false positive.
- **The ordinary `agent_eval` summary is not episode-total accounting for this
  case:** it reports the final graded answer's latency and tokens, while the
  ungraded `pre_mount_turns` and `setup_turns` are omitted from those aggregates.
  The episode totals above were summed from the three canonical per-turn
  records produced in the isolated run.
- **This is a smoke, not a reliability estimate:** one model, one synthetic
  workspace, one iteration, and a phrase/value screen do not establish general
  model quality, calibration, or performance on real personal data.

Useful follow-up metrics are same-revision retrieval rate, stale-evidence
rejection rate, answer correctness by interaction family, model calls and
tokens per correct episode, cumulative provider latency versus tool time, and
the rate at which correct answers remain `NeedsReview`. Keep these separate;
no single “verified” percentage captures them all.
