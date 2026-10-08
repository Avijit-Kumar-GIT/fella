# Release decision protocol v0.1

This is a pre-registered decision rule for future candidate comparisons; it is
not a claim that today's suites satisfy the evidence requirements. Apply it to
the same frozen tasks, workspace snapshots, rubric, model version/settings,
iteration count, and budget. Record any exception before opening candidate
outputs. Never move a threshold after observing a candidate result.

## Valid comparison

- Use at least three stochastic repetitions per episode with the same model
  version, provider, decoding settings, tool budget, split, and workspace/task
  hashes. For deterministic checks, report the exact build and fixture hash.
- The condition-specific harness, prompt, and toolset may differ when they
  are the component under evaluation; record their hashes and labels. Keep the
  shared evaluation budget, task/grader, and model-generation settings pinned.
- Pair by task ID, model, and profile. Keep invalid setup/provider/evaluator
  outcomes visible and out of the model-correctness denominator.
- Do not make a public capability claim for a family until its target evidence
  is present: four independent workspaces for that claimed family and the
  task/reviewer/holdout requirements in `../fqa-bench/coverage-v0.1.yml`.
- Have at least two reviewers independently assess each blind-holdout packet.
  Run and retain field-level agreement before mapping packets back to task,
  model, or condition. Pre-register any agreement threshold for the evaluation;
  unresolved reviewer disagreement makes that dimension inconclusive, not a
  model pass or failure.
- Use a paired interval for correctness and report the raw transition counts
  (improved, worsened, unchanged). The interval is uncertainty information,
  not a replacement for task-level review or the domain-specific metrics.

## Decision rules

1. **Integrity blockers:** any confirmed privacy-boundary violation (such as
   local file contents entering a web request without user authorization),
   unintended write, fabricated provenance, or chart presented as grounded
   while its plotted data are wrong blocks release of that behavior. Required
   count: zero confirmed critical incidents in the evaluated scope.
2. **Correctness non-inferiority:** for every family claimed as supported,
   the candidate's paired correctness interval must not establish a decrease
   from baseline. If the interval includes both meaningful improvement and
   regression, the result is inconclusive; it cannot be advertised as an
   improvement. Report small/underpowered slices as such.
3. **Helpfulness guard:** report answer behavior separately. An increase in
   `unnecessary_deferral`, `error_or_empty`, or missed necessary clarification
   is a regression even if unsupported claims fall. Do not trade a refusal
   reduction for materially more unsupported claims without an explicit
   product-risk decision.
4. **Forecast/chart gates:** do not infer these from overall analytics
   accuracy. Forecasts need their method/backtest/uncertainty rubric; charts
   need data and semantic correctness. An untested capability is not releasable
   as validated.
5. **Efficiency:** report tokens, wall time, steps, tool errors, exact
   duplicates, and cost where provider prices are pinned. Efficiency can
   justify a candidate only when correctness and helpfulness remain
   non-inferior; there is no universal token target that overrides quality.
6. **Overall decision:** say `improved`, `non-inferior`, `regressed`, or
   `inconclusive` per family and for each user-valued dimension. There is no
   weighted composite score. A candidate can be released with an explicitly
   limited capability claim only when integrity blockers are absent and the
   claimed families pass the evidence gate.

These gates intentionally use baseline-relative decisions instead of
arbitrary absolute accuracy targets. The v0.1 corpus is not yet broad enough
to justify market-wide cut scores. The full paired result and family
denominators remain part of every release decision.
