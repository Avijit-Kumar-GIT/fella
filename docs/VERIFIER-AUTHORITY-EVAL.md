# Verifier authority conformance run

Run date: 2026-10-06

Scope: product-roadmap item 10, verifier authority and repair behavior.

## Protocol fixed before the candidate run

The behavioral oracle is the policy in [`VERIFIER-AUTHORITY.md`](./VERIFIER-AUTHORITY.md).
Expected behavior is asserted from those rules and the stated scenario goals,
not inferred from candidate answers. The controlled full-loop cases use a
scripted OpenAI-compatible endpoint so model wording and tool calls are fixed;
they measure orchestration and evidence disposition, not model intelligence
or real-provider latency.

The full-loop suite covers:

| Scenario | Expected behavior |
| --- | --- |
| Unsupported derived claim beside valid regional totals and chart | Repair the claim only; preserve the SQL result and one valid chart. |
| Query crosses an explicitly requested current/archive boundary | Exclude only the broad-scope query; retain the corrected current-scope result. |
| Independent semantic findings arrive over successive passes | Apply bounded repairs; exclude only the exact offending result; retain sibling results. |
| An implausible zero-row filter answers a nonzero dataset | Exclude the empty-result query and allow a corrected query to answer. |
| A derived comparison is stated but absent from the result | Keep the useful grouped result, compute the missing derived value, and answer. |
| A tool query fails, then the model corrects it | Keep the execution error on that call, retain the successful correction, and answer. |

Separate policy checks cover a replay mismatch’s exact evidence ID, a chart
whose plotted values do not match its source, unsupported-claim scope,
same-model disagreement, stale-workspace blocking, and successful replay.
UI behavior is checked by type-check/build plus inspection of the on-demand
Analysis Details rendering and chart filtering paths.

No gold analysis answers, scenario outcomes, or acceptance thresholds were
changed after running the candidate. Existing legacy status assertions were
left intact for audit. The max-step test uses two distinct tool operations so
that it measures the configured step cap rather than the separate duplicate-
call guard; its expected step/evidence limit is unchanged.

## Results

| Measure | Observed |
| --- | ---: |
| Full-loop scripted scenarios meeting their assertions | 6 / 6 |
| Expected final answers produced | 6 / 6 |
| Fake-model requests | 25 (4, 4, 6, 4, 4, 3) |
| Tool evidence records | 13 |
| Targeted semantic repair passes | 6 |
| Tool-error recovery passes | 1 |
| Final evidence retained as accepted | 9 |
| Exact-scope verifier exclusions | 3 |
| Tool errors retained as tool errors | 1 |
| Rust unit tests | 331 passed |
| Rust suite, with two legacy status assertions filtered | 464 passed, 2 ignored, 2 filtered |
| Unfiltered agent-loop suite | 20 passed, 2 failed, 1 ignored |
| Svelte diagnostics | 0 errors, 0 warnings |
| Production frontend build | Passed |
| Formatting and whitespace checks | Passed |

The six full-loop cases are within the 20 passing agent-loop tests. The two
unfiltered failures are retained and reported (they were not edited):
`direct_data_calls_do_not_require_a_contract` and
`unresolved_contract_defers_direct_data_tools_until_revised` both expect
`Verified`, while the runtime returns `NeedsReview` for, respectively, an
ungrounded higher-risk interpretation and an explicitly ambiguous contract.
The implementation's pre-existing `status()` policy reserves `Verified` for a
grounded interpretation with clean replay. These expectation conflicts need
owner adjudication; this run does not change them into passes.

The counts above come from fixed request scripts and asserted evidence traces;
they are not population estimates. No unexpected whole-answer block or
unsupported final answer appeared in the six controlled scenarios. That is
not a claim about real-model false-positive/false-negative rates. Latency is
not reported: local fake-provider timing does not represent production model
latency.

## Limitations and interpretation

- This is a verifier-authority conformance run, not a broad analytics-quality
  benchmark and not a before/after comparison against the pre-#10 branch.
- Scripted responses intentionally isolate harness behavior. They do not
  estimate how often a live model will select the right tool, repair a finding,
  or leave a wrong claim unresolved.
- Chart mismatch, stale workspace blocking, unsupported claims, and
  same-model disagreement have direct policy-level checks; not every one is a
  full live-model end-to-end scenario.
- No generalization claim is made from these fixtures. The FQA-Bench and a
  real-provider run remain the appropriate next measurement for answer
  coverage, unresolved unsupported claims, latency, and comparison to a
  baseline.

Any failing check remains a failure and must be investigated against the
predeclared behavior; this report is not permission to tune the oracle.
