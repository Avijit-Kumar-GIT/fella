# Verifier authority conformance run

Run date: 2026-10-06. This is a historical scripted conformance report for
verifier authority and repair behavior. It measures orchestration on fixed
responses, not model intelligence, live-provider latency, or broad analytics
quality. The current verifier contract is summarized in
[`../ARCHITECTURE.md`](../ARCHITECTURE.md#the-analytical-turn).

## Predeclared scenarios

The controlled full-loop suite covered:

| Scenario | Expected behavior |
| --- | --- |
| Unsupported derived claim beside valid regional totals and chart | Repair the claim only; preserve the result and valid chart. |
| Query crosses an explicit current/archive boundary | Exclude only the broad-scope query; retain corrected evidence. |
| Independent semantic findings across repair passes | Apply bounded repairs and exclude only exact offending results. |
| Implausible zero-row filter on a nonzero dataset | Exclude the empty result and allow correction. |
| Derived comparison absent from the result | Keep useful grouped evidence and compute the missing comparison. |
| Tool query fails, then the model corrects it | Retain the first error and the successful correction. |

Separate checks covered exact evidence IDs on replay mismatch, chart/source
mismatch, unsupported-claim scope, same-model disagreement, stale workspace,
successful replay, and on-demand UI details. No answer golds or thresholds
were changed after seeing candidate output.

## Results

| Measure | Observed |
| --- | ---: |
| Full-loop scripted scenarios meeting assertions | 6 / 6 |
| Fake-model requests | 25 |
| Tool evidence records | 13 |
| Targeted semantic repair passes | 6 |
| Final evidence retained as accepted | 9 |
| Exact-scope verifier exclusions | 3 |
| Tool errors retained as tool errors | 1 |
| Svelte diagnostics / production build | 0 errors, 0 warnings / passed |

The unfiltered agent-loop suite at that time had **two failures**:
`direct_data_calls_do_not_require_a_contract` and
`unresolved_contract_defers_direct_data_tools_until_revised` expected
`Verified`, while the runtime returned `NeedsReview`. They were reported as
failures and not changed in that run. A later release-gate run passed the full
default Rust suite; that later result is recorded in
[`../TESTING.md`](../TESTING.md), not used to rewrite this historical report.

The six scripted cases are a small fixed suite, not a population estimate.
They do not establish live-model false-positive or false-negative rates. No
latency claim is made from the fake provider. The broader quality measure is
FQA-Bench, whose current v0.1 corpus remains draft.
