# Repository guidance for coding agents

## Branch delivery

- When requested work is complete and proportionate validation has run, commit
  the task's changes and push the commit to the current branch by default.
- Do not switch branches, push to a different branch, or open a pull request
  unless the user asks. Respect an explicit request to keep changes local or
  not push.
- Stage only files belonging to the current task. Preserve unrelated or
  pre-existing work; if task changes cannot be safely isolated, ask before
  committing.
- Report the commit and push result, including the commit hash and branch.

## Evaluation and test integrity

- Define expected behavior and grading criteria before running the candidate
  implementation. Do not change, delete, weaken, or exclude a failing test
  because of the observed output.
- Keep test investigation bounded. Do not rerun the same failing case under the
  same setup and hypothesis more than twice. Every additional attempt must test
  a stated, materially different hypothesis or verify a relevant code change.
- After repeated failure, stop and inspect the available evidence once: the
  assertion, logs, app state, and relevant implementation. Distinguish a
  demonstrated product failure from a test synchronization, locator,
  environment, or specification problem. A DOM state or passing assertion is
  evidence only for what it directly establishes; do not infer more.
- If the intended behavior may be working but the test cannot establish it,
  preserve the failing result and report it as unresolved. Use a distinct,
  proportionate validation method only when it adds evidence; do not keep
  cycling the same test or alter its assertions merely to obtain a pass.
- Report failing tests as failures. If a test appears ambiguous, incorrectly
  specified, or poorly graded, preserve its result and flag the concern
  separately for user review; do not silently convert it to a pass.
- Change a test expectation only when the user approves a correction grounded
  in the intended behavior—not to fit the implementation or a model response.
  Document the reason, version the benchmark/task set, and rerun comparisons
  under the same revised version.
- Keep benchmark criteria implementation-independent. A suspected grader
  false negative is an adjudication question, not permission to tune the gold
  answer after seeing candidate output.
- In reports, include failures, exclusions, grader limitations, and any
  post-hoc changes. Never present a post-hoc-adjusted score as an untouched
  baseline.
